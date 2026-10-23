/**
 * `com pkg`: the machine-wide package store (src/store).
 *
 *   com pkg install            link the project's packages from the store (com dev does it too)
 *   com pkg add <pkg…> [-D]    add to package.json + bun.lock, then link
 *   com pkg remove <pkg…>      remove, then link
 *   com pkg search <name>      versions in the store, the projects using them, the latest on npm
 *   com pkg ls                 what the store holds
 *   com pkg why <name>         which dependencies pull a package into the project
 *   com pkg gc [--dry-run]     delete what no project uses
 *   com pkg link               node_modules → the store links, for editors and tsc
 */

import { existsSync } from "fs";
import { join, resolve } from "path";
import { logger, colors } from "../core/logger";
import { diskUsage } from "./cache";
import {
  editDependencies,
  entryDir,
  farmDir,
  gc,
  installedVersion,
  linkNodeModules,
  listEntries,
  storeRoot,
  usage,
  usesStore,
  why,
} from "../store";
import { ensurePackages, storeBun } from "../store/ensure";

export interface PkgOptions {
  dir?: string;
  dev?: boolean;
  dryRun?: boolean;
  offline?: boolean;
  json?: boolean;
}

function formatBytes(bytes: number): string {
  if (bytes >= 1024 ** 3) return `${(bytes / 1024 ** 3).toFixed(1)} GB`;
  if (bytes >= 1024 ** 2) return `${(bytes / 1024 ** 2).toFixed(1)} MB`;
  return `${Math.round(bytes / 1024)} KB`;
}

function needStore(projectDir: string): boolean {
  if (usesStore(projectDir)) return true;
  logger.error("This project has its own node_modules", "Delete node_modules to use the shared store (or run `bun add` as usual)");
  return false;
}

/** Latest version and recent versions from the registry (abbreviated metadata) */
async function registryInfo(name: string): Promise<{ latest?: string; versions: number } | null> {
  const base = process.env.COM_NPM_REGISTRY || process.env.npm_config_registry || "https://registry.npmjs.org";
  try {
    const res = await fetch(`${base.replace(/\/$/, "")}/${name.replace("/", "%2f")}`, {
      headers: { accept: "application/vnd.npm.install-v1+json" },
      signal: AbortSignal.timeout(5000),
    });
    if (!res.ok) return null;
    const json: any = await res.json();
    return { latest: json["dist-tags"]?.latest, versions: Object.keys(json.versions ?? {}).length };
  } catch {
    return null;
  }
}

export async function pkgCommand(action: string | undefined, args: string[], options: PkgOptions = {}): Promise<number> {
  const projectDir = resolve(process.cwd(), options.dir || ".");
  switch (action) {
    case undefined:
    case "install":
    case "i": {
      if (!needStore(projectDir)) return 1;
      await ensurePackages(projectDir);
      return 0;
    }

    case "add":
    case "remove":
    case "rm": {
      if (!needStore(projectDir)) return 1;
      if (args.length === 0) {
        logger.error(`Usage: com pkg ${action} <package…>`);
        return 1;
      }
      const command = action === "add" ? "add" : "remove";
      const spinner = logger.spinner(`${command === "add" ? "Adding" : "Removing"} ${args.join(", ")}...`);
      try {
        await editDependencies(projectDir, storeBun(), command, [...(options.dev && command === "add" ? ["-d"] : []), ...args]);
        spinner.stop(true, `package.json and bun.lock updated`);
      } catch (err: any) {
        spinner.stop(false, err.message);
        return 1;
      }
      await ensurePackages(projectDir);
      if (command === "add") {
        for (const spec of args) {
          const name = spec.startsWith("@") ? `@${spec.slice(1).split("@")[0]}` : spec.split("@")[0];
          const version = installedVersion(projectDir, name);
          if (version) console.log(`   ${colors.green}+${colors.reset} ${name}@${version}`);
        }
      }
      return 0;
    }

    case "search": {
      const name = args[0];
      if (!name) {
        logger.error("Usage: com pkg search <name>");
        return 1;
      }
      const used = usage();
      const versions = new Map<string, string[]>();
      for (const entry of listEntries().filter((e) => e.name === name || e.name.includes(name))) {
        const key = `${entry.name}@${entry.version}`;
        versions.set(key, [...(versions.get(key) ?? []), ...(used.get(entry.entry) ?? [])]);
      }
      const remote = options.offline ? null : await registryInfo(name);
      if (options.json) {
        console.log(JSON.stringify({ store: Object.fromEntries([...versions].map(([k, v]) => [k, [...new Set(v)]])), registry: remote }, null, 2));
        return 0;
      }
      if (versions.size === 0) {
        console.log(`   ${colors.dim}Not in the store:${colors.reset} ${name}`);
      } else {
        console.log(`   ${colors.bold}In the store${colors.reset}`);
        for (const [key, projects] of [...versions].sort()) {
          const unique = [...new Set(projects)];
          console.log(`   ${colors.green}✓${colors.reset} ${key}  ${colors.dim}${unique.length} project${unique.length === 1 ? "" : "s"}${colors.reset}`);
          for (const p of unique.slice(0, 5)) console.log(`       ${colors.dim}${p}${colors.reset}`);
          if (unique.length > 5) console.log(`       ${colors.dim}+${unique.length - 5} more${colors.reset}`);
        }
      }
      if (remote?.latest) console.log(`\n   ${colors.bold}npm${colors.reset}  latest ${colors.cyan}${remote.latest}${colors.reset} ${colors.dim}(${remote.versions} versions)${colors.reset}`);
      else if (!options.offline) console.log(`\n   ${colors.dim}npm: not found or offline${colors.reset}`);
      return 0;
    }

    case "ls":
    case "list": {
      const used = usage();
      const entries = listEntries();
      const byPackage = new Map<string, { versions: Set<string>; entries: number; projects: Set<string> }>();
      for (const e of entries) {
        const row = byPackage.get(e.name) ?? { versions: new Set(), entries: 0, projects: new Set() };
        row.versions.add(e.version);
        row.entries++;
        for (const p of used.get(e.entry) ?? []) row.projects.add(p);
        byPackage.set(e.name, row);
      }
      const size = diskUsage(storeRoot());
      if (options.json) {
        console.log(JSON.stringify({ store: storeRoot(), bytes: size, packages: Object.fromEntries([...byPackage].map(([n, r]) => [n, { versions: [...r.versions], entries: r.entries, projects: r.projects.size }])) }, null, 2));
        return 0;
      }
      // Clones (macOS) and hard links (Linux) share blocks with Bun's cache: the disk holds less
      const sizeNote = process.platform === "darwin" ? " (before clone sharing)" : "";
      console.log(`   ${colors.bold}${storeRoot()}${colors.reset}  ${byPackage.size} packages, ${entries.length} entries, ${formatBytes(size)}${sizeNote}, ${new Set([...used.values()].flat()).size} projects\n`);
      for (const [name, row] of [...byPackage].sort((a, b) => a[0].localeCompare(b[0]))) {
        console.log(`   ${name} ${colors.dim}${[...row.versions].join(", ")}  · ${row.projects.size} project${row.projects.size === 1 ? "" : "s"}${colors.reset}`);
      }
      return 0;
    }

    case "why": {
      const name = args[0];
      if (!name || !existsSync(join(projectDir, "bun.lock"))) {
        logger.error(name ? "No bun.lock in this project" : "Usage: com pkg why <name>");
        return 1;
      }
      const chains = why(projectDir, name);
      if (chains.length === 0) {
        console.log(`   ${name} is not a dependency of this project`);
        return 0;
      }
      for (const chain of chains) console.log(`   ${chain.join(` ${colors.dim}→${colors.reset} `)}`);
      return 0;
    }

    case "gc": {
      const before = diskUsage(storeRoot());
      const result = gc(!!options.dryRun);
      const after = options.dryRun ? before : diskUsage(storeRoot());
      const verb = options.dryRun ? "Would remove" : "Removed";
      logger.success(`${verb} ${result.entries.length} unused entries${options.dryRun ? "" : ` (${formatBytes(Math.max(0, before - after))} freed)`}`);
      return 0;
    }

    case "link": {
      if (!needStore(projectDir)) return 1;
      await ensurePackages(projectDir, { quiet: true });
      const result = linkNodeModules(projectDir);
      if (result === "exists") {
        logger.error("node_modules already exists and is not the store link");
        return 1;
      }
      logger.success(`node_modules → ${farmDir(projectDir)} (for editors and tsc; the project still uses the store)`);
      return 0;
    }

    case "path": {
      const name = args[0];
      if (name) {
        const used = listEntries().filter((e) => e.name === name);
        for (const e of used) console.log(entryDir(e.entry));
      } else {
        console.log(storeRoot());
      }
      return 0;
    }

    default:
      logger.error(`Unknown action: com pkg ${action}`, "install | add | remove | search | ls | why | gc | link | path");
      return 1;
  }
}
