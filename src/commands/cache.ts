import { existsSync, lstatSync, readdirSync, rmSync } from "fs";
import { homedir } from "os";
import { join, resolve } from "path";
import { logger, colors } from "../core/logger";

/** `~/.com` (or `COM_HOME`), as the engine's framework_react.rs `com_home` */
export function comHome(): string {
  return process.env.COM_HOME || join(homedir(), ".com");
}

/** Browser builds of dependencies shared by every project on this machine (deps.rs `shared_cache_path`) */
export function sharedDepsDir(): string {
  return join(comHome(), "cache", "deps");
}

/** The project's links to those builds (deps.rs `cache_path`) */
export function projectDepsDir(projectDir: string): string {
  return join(projectDir, ".nata", "deps");
}

/** Bytes on disk under `dir`, each hard-linked file counted once */
export function diskUsage(dir: string, seen = new Set<string>()): number {
  let total = 0;
  let entries: import("fs").Dirent[];
  try {
    entries = readdirSync(dir, { withFileTypes: true });
  } catch {
    return 0;
  }
  for (const entry of entries) {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) {
      total += diskUsage(path, seen);
      continue;
    }
    try {
      const stat = lstatSync(path);
      const key = `${stat.dev}:${stat.ino}`;
      if (seen.has(key)) continue;
      seen.add(key);
      total += stat.size;
    } catch {}
  }
  return total;
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
  return `${(bytes / 1024 / 1024 / 1024).toFixed(2)} GB`;
}

export interface CacheOptions {
  action?: string;
  dir?: string;
  /** Only the machine-wide cache */
  global?: boolean;
  /** Only this project's cache */
  project?: boolean;
  json?: boolean;
}

/**
 * `com cache`: where dependency builds are cached and how much space they take.
 * `com cache clean`: deletes them (both by default, `--global` or `--project` for one). Nothing breaks:
 * a project's files are hard links, and whatever is missing is rebuilt on the next request.
 */
export async function cacheCommand(options: CacheOptions = {}) {
  const action = options.action || "info";
  const projectDir = resolve(options.dir || process.cwd());
  const shared = sharedDepsDir();
  const project = projectDepsDir(projectDir);
  const hasProject = existsSync(project);

  if (action === "info" || action === "dir" || action === "ls") {
    // Files of the project that are links to the shared cache take no space of their own
    const seen = new Set<string>();
    const sharedBytes = diskUsage(shared, seen);
    const projectBytes = hasProject ? diskUsage(project, seen) : 0;
    if (options.json) {
      console.log(JSON.stringify({ shared: { dir: shared, bytes: sharedBytes }, project: { dir: project, bytes: projectBytes } }, null, 2));
      return;
    }
    logger.card("Dependency cache", [
      { label: "Shared (all projects)", value: `${shared}  ${formatBytes(sharedBytes)}`, color: colors.cyan },
      { label: "This project", value: hasProject ? `${project}  ${formatBytes(projectBytes)} (+ links to the shared cache)` : "none", color: colors.sky },
    ]);
    logger.info("Free it with `com cache clean` (--global or --project for one of them).");
    return;
  }

  if (action === "clean" || action === "clear" || action === "rm") {
    const both = !options.global && !options.project;
    const targets = [
      ...(both || options.global ? [{ label: "shared cache", dir: shared }] : []),
      ...((both || options.project) && hasProject ? [{ label: "project cache", dir: project }] : []),
    ];
    let freed = 0;
    const seen = new Set<string>();
    // Removing only the project's links frees nothing the shared cache still holds
    if (!targets.some((t) => t.dir === shared)) diskUsage(shared, seen);
    for (const target of targets) {
      if (!existsSync(target.dir)) continue;
      freed += diskUsage(target.dir, seen);
      rmSync(target.dir, { recursive: true, force: true });
      logger.success(`Removed ${target.label}: ${target.dir}`);
    }
    if (targets.length === 0) logger.info("Dependency cache is already empty.");
    else logger.success(`Freed ${formatBytes(freed)}. Dependencies are rebuilt on the next \`com dev\` request.`);
    return;
  }

  logger.error(`Unknown cache action: "${action}"`, "Usage: com cache [info|clean] [--global|--project] [--dir <path>]");
  process.exit(1);
}
