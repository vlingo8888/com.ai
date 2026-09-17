import { join, dirname, resolve, relative } from "path";
import { existsSync, mkdirSync, readdirSync } from "fs";
import { NataApiClient } from "../core/api";
import { saveLocalViewConfig, LocalViewConfig } from "../core/config";
import { setupProjectEnvironment } from "../package-manager";
import { generateAgentsGuide } from "../templates/ai_guidelines";
import { logger, colors } from "../core/logger";

export interface CloneOptions {
  dir?: string;
  api?: string;
  token?: string;
  force?: boolean;
  install?: boolean;
}

export async function cloneView(viewIdArg: string | number, options: CloneOptions = {}) {
  const viewId = typeof viewIdArg === "string" ? parseInt(viewIdArg.replace(/^view_/, ""), 10) : viewIdArg;

  if (!viewId || isNaN(viewId)) {
    logger.error("Please provide a valid viewId", "Usage: com clone <viewId>");
    process.exit(1);
  }

  logger.hero();
  console.log(`\n  ${colors.bold}${colors.indigo}◆ INITIALIZING PROJECT FROM VIEW #${viewId}${colors.reset}\n`);

  const client = new NataApiClient(options.api, options.token);

  // Step 1: Fetch metadata & source files from Backend
  const spinner = logger.spinner(`Connecting and downloading source files for View #${viewId}...`);
  let rawData;
  let viewDetails;

  const fetchStart = Date.now();
  try {
    try {
      viewDetails = await client.getViewDetails(viewId);
    } catch {}
    rawData = await client.getViewRawFiles(viewId);
    spinner.stop(true, `Successfully fetched project metadata & source tree (${((Date.now() - fetchStart) / 1000).toFixed(2)}s)`);
  } catch (error: any) {
    spinner.stop(false, `Failed to load View #${viewId}`);
    logger.error(error.message, "Verify network connectivity or authenticate using `com login`");
    process.exit(1);
  }

  const files = rawData.files || [];
  if (files.length === 0) {
    logger.warn(`View #${viewId} currently contains no files on the remote server.`);
  }

  // Step 2: Prepare target directory
  const defaultDirName = viewDetails?.name 
    ? viewDetails.name.toLowerCase().replace(/[^a-z0-9_-]/g, "-") + `-${viewId}`
    : `view-${viewId}`;
    
  const targetDir = resolve(process.cwd(), options.dir || defaultDirName);
  const relTargetDir = relative(process.cwd(), targetDir) || "./";

  if (existsSync(targetDir)) {
    const existingItems = readdirSync(targetDir);
    if (existingItems.length > 0 && !options.force) {
      logger.warn(`Target directory "${relTargetDir}" already exists and is not empty.`);
      console.log(`    ${colors.darkGray}› Use the ${colors.bold}--force${colors.reset}${colors.darkGray} flag to overwrite: ${colors.cyan}com clone ${viewId} --force${colors.reset}\n`);
      process.exit(1);
    }
  } else {
    mkdirSync(targetDir, { recursive: true });
  }

  // Step 3: Write files to disk
  console.log(`\n  ${colors.bold}${colors.cyan}Extracting files & building directory tree:${colors.reset}`);

  const fileConfigs: LocalViewConfig["files"] = [];
  let totalBytes = 0;

  for (let i = 0; i < files.length; i++) {
    const file = files[i];
    let cleanPath = (file.path || "index.tsx").replace(/^\.?\//, "");
    if (!cleanPath) cleanPath = "index.tsx";

    const fullFilePath = join(targetDir, cleanPath);
    const parentDir = dirname(fullFilePath);

    if (!existsSync(parentDir)) {
      mkdirSync(parentDir, { recursive: true });
    }

    const content = file.content || "";
    await Bun.write(fullFilePath, content);

    const size = Buffer.byteLength(content, "utf-8");
    totalBytes += size;
    fileConfigs.push({
      path: cleanPath,
      size,
    });

    const isLast = i === files.length - 1;
    logger.fileItem(cleanPath, size, isLast);
  }

  // Step 4: Auto-detect imports, generate manifests (package.json, tsconfig.json) & optional install
  await setupProjectEnvironment(
    targetDir,
    viewDetails?.name || `view-${viewId}`,
    files,
    { install: options.install }
  );

  // Step 5: Save local config & auxiliary files
  saveLocalViewConfig(targetDir, {
    viewId,
    apiUrl: options.api || "https://base.myworkbeast.com",
    name: viewDetails?.name || `View ${viewId}`,
    dbCode: viewDetails?.db_code,
    clonedAt: new Date().toISOString(),
    files: fileConfigs,
  });

  const gitignorePath = join(targetDir, ".gitignore");
  if (!existsSync(gitignorePath)) {
    await Bun.write(gitignorePath, "node_modules\n.DS_Store\n.env.local\n.nata\n");
  }

  const envPath = join(targetDir, ".env");
  if (!existsSync(envPath) && viewDetails?.db_code) {
    const dbUrl = `postgresql://postgres:postgres@27.71.25.205:5433/${viewDetails.db_code}`;
    await Bun.write(envPath, `DATABASE_URL="${dbUrl}"\n`);
  }

  const readmePath = join(targetDir, "README.md");
  if (!existsSync(readmePath)) {
    const readmeContent = `# ${viewDetails?.name || `View ${viewId}`}\n\nCloned from **Com.AI.VN View #${viewId}**.\n\n## Development\n\`\`\`bash\n# Start the local development & preview server (Zero-Install ESM)\ncom dev\n\n# Sync local changes back to the cloud\ncom push\n\`\`\`\n\n## AI Architecture Guide\nSee [AGENTS.md](./AGENTS.md) for full project architecture, \`core\` database helpers, and \`modules/\` structure for AI coding assistants.\n`;
    await Bun.write(readmePath, readmeContent);
  }

  // Generate .antigravityrules & AGENTS.md (Comprehensive AI Assistant Guidelines & Architecture Guide)
  const antigravityRulesPath = join(targetDir, ".antigravityrules");
  const agentsGuidePath = join(targetDir, "AGENTS.md");
  const agentsGuideContent = generateAgentsGuide(viewDetails?.name || `View #${viewId}`, viewId);
  await Bun.write(antigravityRulesPath, agentsGuideContent);
  await Bun.write(agentsGuidePath, agentsGuideContent);

  // Auto-introspect database schema if DATABASE_URL is available
  let dbStatusLabel = "No database configured";
  if (viewDetails?.db_code) {
    const dbUrl = `postgresql://postgres:postgres@27.71.25.205:5433/${viewDetails.db_code}`;
    try {
      const { introspectAndGenerateSchema } = await import("./db");
      const res = await introspectAndGenerateSchema(targetDir, dbUrl);
      dbStatusLabel = `${res.tableCount} tables synced (types/db.d.ts)`;
    } catch {
      dbStatusLabel = "Configured (.env) - Run `com db pull` to sync types";
    }
  }

  // Summary Card
  logger.card("PROJECT SUMMARY", [
    { label: "View Name", value: viewDetails?.name || `View #${viewId}`, color: colors.bold + colors.white },
    { label: "View ID", value: String(viewId), color: colors.sky },
    { label: "Files Extracted", value: `${files.length} files (${(totalBytes / 1024).toFixed(1)} KB)` },
    { label: "Runtime Mode", value: options.install ? "Local node_modules" : "ESM Zero-Install (Instant)", color: colors.emerald },
    { label: "Database Schema", value: dbStatusLabel, color: colors.yellow },
    { label: "AI Guide", value: ".antigravityrules (Created)", color: colors.bold + colors.cyan },
    { label: "Target Directory", value: relTargetDir, color: colors.green },
    { label: "Environment Status", value: "Ready for development", color: colors.emerald },
  ]);

  // Next steps call to action
  logger.nextSteps([
    { cmd: `cd ${relTargetDir}`, desc: "Navigate into the project directory" },
    { cmd: "com dev", desc: "Start local preview server with live reload" },
    { cmd: "com push", desc: "Deploy & synchronize changes back to Cloud" },
  ]);
}
