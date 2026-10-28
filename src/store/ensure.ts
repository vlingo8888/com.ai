/**
 * Makes the project's packages available before the engine (or the test runner) starts: from the
 * store when the project has no `node_modules`, else with `bun install` as before.
 */

import { existsSync, readFileSync } from "fs";
import { join } from "path";
import { logger, colors } from "../core/logger";
import { resolveWorkerBun } from "../core/binaries";
import { bunEnv } from "../core/self";
import { installProject, linksCurrent, lockfileCurrent, readProjectState, StoreUnsupportedError, usesStore } from "./index";

/** The Bun that resolves versions (`--lockfile-only`) and fills its cache */
export function storeBun(): string {
  return resolveWorkerBun() ?? process.execPath;
}

/** Dependencies declared in package.json that are not present in node_modules */
export function missingDependencies(projectDir: string): string[] {
  const pkgPath = join(projectDir, "package.json");
  if (!existsSync(pkgPath)) return [];
  try {
    const pkg = JSON.parse(readFileSync(pkgPath, "utf-8"));
    const names = Object.keys({ ...pkg.dependencies, ...pkg.devDependencies });
    return names.filter((name) => !existsSync(join(projectDir, "node_modules", name, "package.json")));
  } catch {
    return [];
  }
}

async function bunInstall(projectDir: string, quiet: boolean) {
  const missing = missingDependencies(projectDir);
  if (missing.length === 0) return;
  const spinner = quiet ? null : logger.spinner(`Installing ${missing.length} missing ${missing.length === 1 ? "dependency" : "dependencies"} (bun install)...`);
  const proc = Bun.spawn([process.execPath, "install"], { cwd: projectDir, stdout: "ignore", stderr: "pipe", env: bunEnv(process.execPath, { ...process.env, COM_STORE: "0" }) });
  const stderr = await new Response(proc.stderr).text();
  if ((await proc.exited) === 0) {
    spinner?.stop(true, "Dependencies installed");
  } else {
    spinner?.stop(false, "bun install failed; missing packages will be loaded from esm.sh");
    const detail = stderr.trim().split("\n").slice(-5).join("\n");
    if (detail && !quiet) console.log(`   ${colors.dim}${detail}${colors.reset}`);
  }
}

export interface EnsureOptions {
  quiet?: boolean;
}

/** Installs what the project needs; never throws (the engine falls back to esm.sh for what is missing) */
export async function ensurePackages(projectDir: string, options: EnsureOptions = {}): Promise<"store" | "node_modules"> {
  if (!existsSync(join(projectDir, "package.json"))) return usesStore(projectDir) ? "store" : "node_modules";
  if (!usesStore(projectDir)) {
    await bunInstall(projectDir, !!options.quiet);
    return "node_modules";
  }
  // package.json and bun.lock untouched since the last install: a few `stat`s, nothing to do
  if (linksCurrent(projectDir)) return "store";
  // Nothing to say when the lockfile still matches: only the links are checked
  const likelyCurrent = lockfileCurrent(projectDir) && readProjectState(projectDir) !== null;
  const spinner = options.quiet || likelyCurrent ? null : logger.spinner("Linking packages from the shared store...");
  try {
    const result = await installProject(projectDir, storeBun(), (step) => spinner?.update(step));
    if (!result.upToDate) {
      const detail = `${result.packages} packages, ${result.created} new in the store${result.downloaded ? `, ${result.downloaded} downloaded` : ""}`;
      if (spinner) spinner.stop(true, `Packages ready (${detail}) in ${Math.round(result.ms)}ms`);
      else if (!options.quiet) logger.success(`Packages ready (${detail}) in ${Math.round(result.ms)}ms`);
    } else {
      spinner?.stop(true, "Packages ready");
    }
    if (result.skippedScripts.length && !options.quiet) {
      logger.warn(`Install scripts are not run for store packages: ${result.skippedScripts.join(", ")}`);
    }
    return "store";
  } catch (err: any) {
    if (err instanceof StoreUnsupportedError) {
      spinner?.stop(false, `Not using the shared store: ${err.message}`);
      await bunInstall(projectDir, !!options.quiet);
      return "node_modules";
    }
    spinner?.stop(false, "Could not prepare packages from the store");
    if (!options.quiet) console.log(`   ${colors.dim}${String(err?.message ?? err).split("\n").slice(0, 6).join("\n   ")}${colors.reset}`);
    return "store";
  }
}
