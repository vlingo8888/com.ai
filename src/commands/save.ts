import { join, resolve, relative, extname } from "path";
import { existsSync, readdirSync, statSync, readFileSync } from "fs";
import { NataApiClient } from "../core/api";
import { getLocalViewConfig, saveLocalViewConfig } from "../core/config";
import { logger, colors } from "../core/logger";

export interface SaveOptions {
  dir?: string;
  viewId?: number | string;
  api?: string;
  token?: string;
  message?: string;
  dryRun?: boolean;
}

const IGNORED_DIRS = new Set([
  "node_modules",
  ".git",
  ".nata",
  ".next",
  "dist",
  "build",
  "target",
  ".turbo",
  ".cache",
  ".vercel",
  ".idea",
  ".vscode",
]);

const IGNORED_FILES = new Set([
  ".DS_Store",
  "Thumbs.db",
]);

const IGNORED_EXTENSIONS = new Set([
  ".db",
  ".rwal",
  ".wal",
  ".shm",
  ".sqlite",
  ".sqlite3",
  ".log",
  ".zip",
  ".tar",
  ".gz",
  ".exe",
  ".bin",
]);

export interface ScannedFile {
  path: string;
  content: string;
  size: number;
}

export function collectProjectFiles(
  dir: string,
  baseDir: string = dir
): ScannedFile[] {
  const result: ScannedFile[] = [];
  if (!existsSync(dir)) return result;

  const entries = readdirSync(dir, { withFileTypes: true });

  for (const entry of entries) {
    const fullPath = join(dir, entry.name);
    const relPath = relative(baseDir, fullPath).replace(/\\/g, "/");

    if (entry.isDirectory()) {
      if (IGNORED_DIRS.has(entry.name)) continue;
      result.push(...collectProjectFiles(fullPath, baseDir));
    } else if (entry.isFile()) {
      if (IGNORED_FILES.has(entry.name)) continue;
      // Never push environment secrets to cloud
      if (entry.name === ".env" || entry.name.startsWith(".env.")) continue;
      if (IGNORED_EXTENSIONS.has(extname(entry.name).toLowerCase())) continue;

      try {
        const stats = statSync(fullPath);
        // Skip files larger than 2MB
        if (stats.size > 2 * 1024 * 1024) continue;

        const content = readFileSync(fullPath, "utf-8");
        result.push({
          path: relPath,
          content,
          size: stats.size,
        });
      } catch {
        // Skip unreadable files
      }
    }
  }

  return result.sort((a, b) => a.path.localeCompare(b.path));
}

export async function saveCommand(options: SaveOptions = {}) {
  const targetDir = resolve(process.cwd(), options.dir || ".");
  const localConfig = getLocalViewConfig(targetDir);

  const viewIdArg = options.viewId || localConfig?.viewId;

  if (!viewIdArg) {
    logger.hero();
    logger.error(
      "No View ID found in this directory.",
      "Ensure you are inside a cloned view project (with .nata/view.json) or specify viewId:\n    com save --view <viewId>\n    com push --view <viewId>"
    );
    process.exit(1);
  }

  const viewId = typeof viewIdArg === "string" ? parseInt(viewIdArg.replace(/^view_/, ""), 10) : viewIdArg;

  if (!viewId || isNaN(viewId)) {
    logger.error("Please provide a valid viewId", "Usage: com save [options]");
    process.exit(1);
  }

  const apiUrl = options.api || localConfig?.apiUrl || "https://base.myworkbeast.com";
  const token = options.token;

  logger.hero();
  console.log(`\n  ${colors.bold}${colors.indigo}◆ SYNCHRONIZING VIEW #${viewId} TO CLOUD${colors.reset}\n`);

  // Step 1: Scan local source files
  const spinner = logger.spinner("Scanning and packaging local source tree...");
  const files = collectProjectFiles(targetDir);
  const totalBytes = files.reduce((acc, f) => acc + f.size, 0);

  if (files.length === 0) {
    spinner.stop(false, "No project files found to save");
    logger.error("The project directory contains no readable source files.");
    process.exit(1);
  }

  spinner.stop(true, `Packaged ${files.length} files (${(totalBytes / 1024).toFixed(1)} KB)`);

  if (options.dryRun) {
    logger.info("Dry-run mode active. No changes pushed to server.");
    for (const f of files) {
      console.log(`    ${colors.cyan}›${colors.reset} ${f.path} ${colors.darkGray}(${(f.size / 1024).toFixed(1)} KB)${colors.reset}`);
    }
    return;
  }

  // Step 2: Upload to Backend via NataApiClient
  const client = new NataApiClient(apiUrl, token);
  const pushSpinner = logger.spinner(`Pushing ${files.length} files to Com.AI.VN Cloud (${apiUrl})...`);
  const startTime = Date.now();

  try {
    const rawFilesPayload = files.map((f) => ({
      path: f.path,
      content: f.content,
    }));

    await client.saveViewRawFiles(viewId, rawFilesPayload, {
      name: localConfig?.name,
    });

    const elapsed = ((Date.now() - startTime) / 1000).toFixed(2);
    pushSpinner.stop(true, `Successfully synchronized to Cloud in ${elapsed}s`);

    // Step 3: Update local config timestamp
    if (localConfig) {
      saveLocalViewConfig(targetDir, {
        ...localConfig,
        lastSyncedAt: new Date().toISOString(),
      });
    }

    // Step 4: Display Summary
    logger.card("CLOUD SYNC SUMMARY", [
      { label: "View Name", value: localConfig?.name || `View #${viewId}`, color: colors.bold + colors.white },
      { label: "View ID", value: String(viewId), color: colors.sky },
      { label: "Files Saved", value: `${files.length} files (${(totalBytes / 1024).toFixed(1)} KB)` },
      { label: "Cloud Endpoint", value: apiUrl, color: colors.yellow },
      { label: "Sync Status", value: "Synchronized & Published", color: colors.emerald },
      { label: "Timestamp", value: new Date().toLocaleTimeString(), color: colors.darkGray },
    ]);

    logger.nextSteps([
      { cmd: "com dev", desc: "Start local preview server" },
      { cmd: `com clone ${viewId}`, desc: "Clone this updated view on another machine" },
    ]);
  } catch (error: any) {
    pushSpinner.stop(false, "Failed to synchronize view to Cloud");
    logger.error(
      error.message || String(error),
      "Check network connectivity or authenticate using `com login`"
    );
    process.exit(1);
  }
}
