import { join, resolve, relative, extname } from "path";
import { existsSync, readdirSync, statSync, readFileSync } from "fs";
import { NataApiClient, SaveConflictError } from "../core/api";
import { getLocalViewConfig, saveLocalViewConfig, type LocalViewConfig } from "../core/config";
import { logger, colors } from "../core/logger";
import { compileAllEs5 } from "../core/es5";

export interface SaveOptions {
  dir?: string;
  viewId?: number | string;
  api?: string;
  token?: string;
  message?: string;
  dryRun?: boolean;
  force?: boolean;
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

/** Assets the server stores as text; uploading them through `readFileSync(..., "utf-8")` corrupts them. */
const BINARY_EXTENSIONS = new Set([
  ".png", ".jpg", ".jpeg", ".gif", ".webp", ".avif", ".ico", ".bmp",
  ".woff", ".woff2", ".ttf", ".otf", ".eot",
  ".mp3", ".mp4", ".webm", ".wav",
  ".pdf",
]);

const MAX_FILE_BYTES = 2 * 1024 * 1024;

export function isBinaryFile(path: string, data: Buffer): boolean {
  return BINARY_EXTENSIONS.has(extname(path).toLowerCase()) || data.includes(0);
}

/**
 * Tracked paths (from the last clone/save) that are gone from disk. A file that still exists,
 * even one that is skipped from upload, is never reported. With no tracked list nothing is deleted.
 */
export function computeDeleted(
  dir: string,
  tracked: LocalViewConfig["files"] | undefined,
  sentPaths: Set<string>
): string[] {
  if (!tracked) return [];
  return tracked
    .map((f) => f.path)
    .filter((p) => !sentPaths.has(p) && !existsSync(join(dir, p)));
}

export interface ScannedFile {
  path: string;
  content: string;
  size: number;
}

/** Files to upload. Binary and oversized files go to `skipped` and stay as they are on the server. */
export function collectProjectFiles(
  dir: string,
  baseDir: string = dir,
  skipped: string[] = []
): ScannedFile[] {
  const result: ScannedFile[] = [];
  if (!existsSync(dir)) return result;

  const entries = readdirSync(dir, { withFileTypes: true });

  for (const entry of entries) {
    const fullPath = join(dir, entry.name);
    const relPath = relative(baseDir, fullPath).replace(/\\/g, "/");

    if (entry.isDirectory()) {
      if (IGNORED_DIRS.has(entry.name)) continue;
      result.push(...collectProjectFiles(fullPath, baseDir, skipped));
    } else if (entry.isFile()) {
      if (IGNORED_FILES.has(entry.name)) continue;
      // Never push environment secrets to cloud
      if (entry.name === ".env" || entry.name.startsWith(".env.")) continue;
      if (IGNORED_EXTENSIONS.has(extname(entry.name).toLowerCase())) continue;

      try {
        const stats = statSync(fullPath);
        if (stats.size > MAX_FILE_BYTES) {
          skipped.push(relPath);
          continue;
        }

        const data = readFileSync(fullPath);
        if (isBinaryFile(relPath, data)) {
          skipped.push(relPath);
          continue;
        }
        result.push({
          path: relPath,
          content: data.toString("utf-8"),
          size: stats.size,
        });
      } catch {
        // Skip unreadable files
      }
    }
  }

  skipped.sort((a, b) => a.localeCompare(b));
  return result.sort((a, b) => a.path.localeCompare(b.path));
}

/**
 * Records what the server holds after a save: the tracked file list (for deletions) and its
 * `updated_at` (sent back as `baseUpdatedAt`). Without it the next save reports a false conflict.
 */
export function recordSave(
  dir: string,
  localConfig: LocalViewConfig,
  files: ScannedFile[],
  deleted: string[],
  remoteUpdatedAt: string | undefined
) {
  const deletedSet = new Set(deleted);
  const tracked = new Map<string, LocalViewConfig["files"][number]>();
  for (const f of localConfig.files || []) {
    if (!deletedSet.has(f.path)) tracked.set(f.path, f);
  }
  for (const f of files) {
    tracked.set(f.path, { path: f.path, size: f.size });
  }
  saveLocalViewConfig(dir, {
    ...localConfig,
    files: [...tracked.values()].sort((a, b) => a.path.localeCompare(b.path)),
    remoteUpdatedAt: remoteUpdatedAt ?? localConfig.remoteUpdatedAt,
    lastSyncedAt: new Date().toISOString(),
  });
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

  const client = new NataApiClient(options.api || localConfig?.apiUrl, options.token);
  const apiUrl = client.baseUrl;

  logger.hero();
  console.log(`\n  ${colors.bold}${colors.indigo}◆ SAVING VIEW #${viewId} TO CLOUD${colors.reset}\n`);

  // Step 1: Scan local source files
  const spinner = logger.spinner("Scanning and packaging local source tree...");
  const skipped: string[] = [];
  const files = collectProjectFiles(targetDir, targetDir, skipped);
  const totalBytes = files.reduce((acc, f) => acc + f.size, 0);
  const sentPaths = new Set(files.map((f) => f.path));
  const deleted = computeDeleted(targetDir, localConfig?.files, sentPaths);

  if (files.length === 0 && deleted.length === 0) {
    spinner.stop(false, "No project files found to save");
    logger.error("The project directory contains no readable source files.");
    process.exit(1);
  }

  spinner.stop(true, `Packaged ${files.length} files (${(totalBytes / 1024).toFixed(1)} KB)`);

  if (skipped.length > 0) {
    logger.info(`${skipped.length} binary/large file(s) not uploaded (any copy already on the server is kept unchanged):`);
    for (const p of skipped) {
      console.log(`    ${colors.darkGray}○ ${p}${colors.reset}`);
    }
  }
  if (deleted.length > 0) {
    logger.info(`${deleted.length} file(s) removed locally will be deleted on the server:`);
    for (const p of deleted) {
      console.log(`    ${colors.red}- ${p}${colors.reset}`);
    }
  }

  if (options.dryRun) {
    logger.info("Dry-run mode active. No changes pushed to server.");
    for (const f of files) {
      console.log(`    ${colors.cyan}›${colors.reset} ${f.path} ${colors.darkGray}(${(f.size / 1024).toFixed(1)} KB)${colors.reset}`);
    }
    return;
  }

  if (!client.hasToken) {
    logger.error("Not logged in.", "Run `com login` first, or pass `--token <token>`.");
    process.exit(1);
  }

  // Step 2: Upload to Backend via NataApiClient
  const pushSpinner = logger.spinner(`Pushing ${files.length} files to Com.AI.VN Cloud (${apiUrl})...`);
  const startTime = Date.now();

  try {
    const failed: Array<{ path: string; message: string }> = [];
    const rawFilesPayload = compileAllEs5(
      files.map((f) => ({ path: f.path, content: f.content })),
      failed
    );

    const res = await client.saveViewRawFiles(viewId, rawFilesPayload, {
      name: localConfig?.name,
      mode: "merge",
      deleted,
      baseUpdatedAt: localConfig?.remoteUpdatedAt,
      force: options.force,
    });

    const elapsed = ((Date.now() - startTime) / 1000).toFixed(2);
    pushSpinner.stop(true, `Saved to Cloud in ${elapsed}s`);

    if (failed.length > 0) {
      logger.warn(`${failed.length} file(s) did not compile and will not run in the published app:`);
      for (const f of failed) {
        console.log(`    ${colors.red}✖ ${f.path}${colors.reset} ${colors.darkGray}${f.message.split("\n")[0]}${colors.reset}`);
      }
    }
    if (res.mode !== "merge") {
      logger.warn("This server does not support merge saves: it replaced every file with the ones just uploaded.");
    }
    if (res.ws6_snapshot_revision_id) {
      logger.warn(
        `Unsaved edits open in the realtime editor were overwritten; they are kept as revision #${res.ws6_snapshot_revision_id}.`
      );
    }
    if (res.realtime === "unavailable") {
      logger.warn("The realtime editor could not be refreshed; open editors may still show the old files.");
    }

    // Step 3: Track what the server now holds, so the next save can detect deletions and conflicts
    if (localConfig) recordSave(targetDir, localConfig, files, deleted, res.updated_at);

    // Step 4: Display Summary
    logger.card("CLOUD SAVE SUMMARY", [
      { label: "View Name", value: localConfig?.name || `View #${viewId}`, color: colors.bold + colors.white },
      { label: "View ID", value: String(viewId), color: colors.sky },
      { label: "Files Saved", value: `${files.length} files (${(totalBytes / 1024).toFixed(1)} KB)` },
      { label: "Files Deleted", value: String(deleted.length) },
      { label: "Cloud Endpoint", value: apiUrl, color: colors.yellow },
      { label: "Status", value: "Saved (not published)", color: colors.emerald },
      { label: "Timestamp", value: new Date().toLocaleTimeString(), color: colors.darkGray },
    ]);

    logger.nextSteps([
      { cmd: "com publish", desc: "Publish the saved version" },
      { cmd: "com dev", desc: "Start local preview server" },
      { cmd: `com clone ${viewId}`, desc: "Clone this updated view on another machine" },
    ]);
  } catch (error: any) {
    pushSpinner.stop(false, "Failed to save view to Cloud");
    if (error instanceof SaveConflictError) {
      logger.error(
        "The view was changed on the server since your last clone/save.",
        `Compare first with \`com clone ${viewId} --dir <other-dir>\`, or overwrite with \`com save --force\`.`
      );
      process.exit(1);
    }
    logger.error(
      error.message || String(error),
      "Check network connectivity or authenticate using `com login`"
    );
    process.exit(1);
  }
}
