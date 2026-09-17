import { existsSync, mkdirSync, readdirSync, readFileSync, statSync } from "fs";
import { join, resolve, relative } from "path";
import { logger, colors } from "../core/logger";
import { findDatabaseUrl, introspectAndGenerateSchema } from "./db";
import { getLocalViewConfig, saveLocalViewConfig } from "../core/config";
import { generateAgentsGuide } from "../templates/ai_guidelines";
import { scanFilesForDependencies, createPackageManifest, createTsConfigManifest } from "../package-manager";
import { ensureRuntimeEnvironment } from "../core/runtime";

export interface SyncOptions {
  dir?: string;
  env?: string;
  dbUrl?: string;
  install?: boolean;
  json?: boolean;
}

export interface SyncResult {
  projectName: string;
  viewId?: number;
  dbSynced: boolean;
  tableCount: number;
  typesPath?: string;
  schemaPath?: string;
  agentsPath: string;
  packageJsonPath: string;
  tsconfigPath: string;
  detectedPackages: string[];
  totalFilesScanned: number;
}

/**
 * Recursively scans project directory for source code files
 */
export function scanProjectFiles(
  projectDir: string,
  currentSubDir = ""
): Array<{ path: string; content?: string }> {
  const result: Array<{ path: string; content?: string }> = [];
  const fullCurrentDir = join(projectDir, currentSubDir);

  if (!existsSync(fullCurrentDir)) return result;

  const ignoreDirs = new Set([
    "node_modules",
    ".git",
    ".next",
    "dist",
    "target",
    ".nata",
    ".gemini",
    ".agents",
    "coverage",
    "tests",
    "__tests__",
  ]);

  const entries = readdirSync(fullCurrentDir);

  for (const entry of entries) {
    if (ignoreDirs.has(entry)) continue;

    const relPath = currentSubDir ? `${currentSubDir}/${entry}` : entry;
    const fullPath = join(projectDir, relPath);

    try {
      const stat = statSync(fullPath);
      if (stat.isDirectory()) {
        result.push(...scanProjectFiles(projectDir, relPath));
      } else if (
        stat.isFile() &&
        /\.(tsx?|jsx?|json|css|sql|md)$/i.test(entry) &&
        !/\.(test|spec)\.(tsx?|jsx?)$/i.test(entry)
      ) {
        result.push({ path: relPath });
      }
    } catch {}
  }

  return result;
}

/**
 * Main command handler for `com sync` / `com db sync`
 * Synchronizes Database schema, types/db.d.ts, schema.sql, AGENTS.md, package.json and tsconfig.json
 */
export async function syncCommand(options: SyncOptions = {}): Promise<SyncResult> {
  const projectDir = resolve(process.cwd(), options.dir || ".");
  const localView = getLocalViewConfig(projectDir);

  const projectName =
    localView?.name ||
    projectDir.split("/").filter(Boolean).pop() ||
    "com-project";
  const viewId = localView?.viewId;

  if (!options.json) {
    logger.hero();
    logger.section("SYNCHRONIZING PROJECT ENVIRONMENT & AI CONTEXT");
  }

  const spinner = options.json
    ? null
    : logger.spinner("Scanning project structure, dependencies & database schema...");

  const scannedFiles = scanProjectFiles(projectDir);

  // Ensure .nata/core.ts & node_modules/core are up to date with latest @pglite/core engine
  try {
    ensureRuntimeEnvironment(projectDir);
  } catch {}

  // 1. Database Schema Introspection & Sync (types/db.d.ts, schema.sql, AGENTS.md injection)
  const dbUrl = options.dbUrl || findDatabaseUrl(projectDir, options.env);
  let dbSynced = false;
  let tableCount = 0;
  let typesPath: string | undefined;
  let schemaPath: string | undefined;

  if (dbUrl) {
    try {
      const dbResult = await introspectAndGenerateSchema(projectDir, dbUrl, { silent: true });
      dbSynced = true;
      tableCount = dbResult.tableCount;
      typesPath = dbResult.typesPath;
      schemaPath = dbResult.schemaPath;
    } catch (err: any) {
      if (!options.json) {
        logger.warn(`Database sync skipped: ${err.message || String(err)}`);
      }
    }
  }

  // 2. Ensure .agents/rules, .agent/rules, .antigravityrules & AGENTS.md exist & are up to date with full guidelines
  const agentsDir = join(projectDir, ".agents", "rules");
  const agentLegacyDir = join(projectDir, ".agent", "rules");
  const agentsRulePath = join(agentsDir, "architecture.md");
  const agentLegacyRulePath = join(agentLegacyDir, "architecture.md");
  const antigravityRulesPath = join(projectDir, ".antigravityrules");
  const agentsPath = join(projectDir, "AGENTS.md");

  let currentAgents = existsSync(agentsRulePath)
    ? readFileSync(agentsRulePath, "utf-8")
    : (existsSync(antigravityRulesPath)
      ? readFileSync(antigravityRulesPath, "utf-8")
      : (existsSync(agentsPath) ? readFileSync(agentsPath, "utf-8") : ""));

  if (!currentAgents || !currentAgents.includes("CRITICAL INVIOLABLE MANDATES") || !currentAgents.includes("@pglite/core")) {
    const fullTemplate = generateAgentsGuide(projectName, viewId || 1);
    currentAgents = fullTemplate;
  }

  // Inject or update live database schema into AI rules if DB was synced
  if (dbUrl && dbSynced) {
    try {
      const { introspectPostgres, updateAgentsMarkdown } = await import("./db");
      const schema = await introspectPostgres(dbUrl);
      currentAgents = updateAgentsMarkdown(currentAgents, schema);
    } catch {}
  }

  // Primary Antigravity native workspace rules (.agents/rules & .agent/rules)
  try {
    if (!existsSync(agentsDir)) mkdirSync(agentsDir, { recursive: true });
    if (!existsSync(agentLegacyDir)) mkdirSync(agentLegacyDir, { recursive: true });
    await Bun.write(agentsRulePath, currentAgents);
    await Bun.write(agentLegacyRulePath, currentAgents);
  } catch {}

  await Bun.write(antigravityRulesPath, currentAgents);
  await Bun.write(agentsPath, currentAgents);

  // Mirror guidelines to standard files for other AI editors (Claude, Cursor, Copilot)
  try {
    const claudePath = join(projectDir, "CLAUDE.md");
    const cursorRulesPath = join(projectDir, ".cursorrules");
    const githubDir = join(projectDir, ".github");
    const copilotPath = join(githubDir, "copilot-instructions.md");

    await Bun.write(claudePath, currentAgents);
    await Bun.write(cursorRulesPath, currentAgents);
    if (!existsSync(githubDir)) {
      mkdirSync(githubDir, { recursive: true });
    }
    await Bun.write(copilotPath, currentAgents);
  } catch {}

  // 3. Scan & Sync Dependencies (package.json & tsconfig.json)
  const scanResult = scanFilesForDependencies(scannedFiles, projectDir);
  const packageJsonPath = join(projectDir, "package.json");
  const tsconfigPath = join(projectDir, "tsconfig.json");

  // Merge or create package.json
  if (existsSync(packageJsonPath)) {
    try {
      const existing = JSON.parse(readFileSync(packageJsonPath, "utf-8"));
      const mergedDeps = { ...existing.dependencies, ...scanResult.dependencies };
      const mergedDevDeps = { ...existing.devDependencies, ...scanResult.devDependencies };

      existing.dependencies = mergedDeps;
      existing.devDependencies = mergedDevDeps;
      await Bun.write(packageJsonPath, JSON.stringify(existing, null, 2) + "\n");
    } catch {}
  } else {
    const manifest = createPackageManifest(projectName, scanResult);
    await Bun.write(packageJsonPath, JSON.stringify(manifest, null, 2) + "\n");
  }

  // Ensure tsconfig.json exists
  if (!existsSync(tsconfigPath)) {
    const tsconfig = createTsConfigManifest();
    await Bun.write(tsconfigPath, JSON.stringify(tsconfig, null, 2) + "\n");
  }

  // 4. Update local view config timestamp if available
  if (localView) {
    saveLocalViewConfig(projectDir, {
      ...localView,
      lastSyncedAt: new Date().toISOString(),
      files: scannedFiles.map((f) => ({ path: f.path })),
    });
  }

  if (spinner) {
    spinner.stop(true, "Synchronization completed successfully");
  }

  const result: SyncResult = {
    projectName,
    viewId,
    dbSynced,
    tableCount,
    typesPath,
    schemaPath,
    agentsPath,
    packageJsonPath,
    tsconfigPath,
    detectedPackages: scanResult.detectedPackages,
    totalFilesScanned: scannedFiles.length,
  };

  if (options.json) {
    console.log(JSON.stringify(result, null, 2));
    return result;
  }

  // Summary Card
  logger.card("SYNCHRONIZATION SUMMARY", [
    {
      label: "Project / View",
      value: viewId ? `${projectName} (View #${viewId})` : projectName,
      color: colors.bold + colors.white,
    },
    {
      label: "Files Scanned",
      value: `${scannedFiles.length} source files`,
      color: colors.cyan,
    },
    {
      label: "Database Schema",
      value: dbSynced
        ? `● Synced (${tableCount} tables -> types/db.d.ts & schema.sql)`
        : "○ Database not configured or offline",
      color: dbSynced ? colors.bold + colors.emerald : colors.darkGray,
    },
    {
      label: "AI Guidelines",
      value: "AGENTS.md (Updated & live synced)",
      color: colors.bold + colors.sky,
    },
    {
      label: "Dependencies",
      value: `${scanResult.detectedPackages.length} packages discovered (package.json & tsconfig.json synced)`,
      color: colors.emerald,
    },
    {
      label: "Runtime Mode",
      value: "⚡ ESM Zero-Install Active",
      color: colors.yellow,
    },
  ]);

  logger.success("Project is fully synchronized and ready for development & AI coding!");

  return result;
}
