import { existsSync } from "fs";
import { join, resolve } from "path";
import { logger } from "../core/logger";
import { packagesDir, usesStore } from "../store";

// Pinned so `com lint` behaves the same on every machine; fetched once by bunx (zero-install).
// TypeScript stays on 5.x: 7.x rejects `target: "es5"` / `moduleResolution: "node"`, still common in Next projects
export const OXLINT_VERSION = "1.88.0";
export const TYPESCRIPT_VERSION = "5.9.3";

const IGNORED = [".nata", ".next", "dist", "node_modules", "out"];

export interface LintOptions {
  dir?: string;
  paths?: string[];
  fix?: boolean;
  json?: boolean;
  types?: boolean;
}

/** oxlint arguments: the project's own config if it has one, else React + Next.js rules on top of the defaults */
export function oxlintArgs(projectDir: string, options: LintOptions): string[] {
  const args: string[] = [];
  const hasConfig = [".oxlintrc.json", ".oxlintrc.jsonc", "oxlint.config.ts"].some((f) => existsSync(join(projectDir, f)));
  if (!hasConfig) args.push("--react-plugin", "--nextjs-plugin", "--import-plugin", "--jsx-a11y-plugin");
  for (const dir of IGNORED) args.push("--ignore-pattern", `${dir}/**`);
  if (options.fix) args.push("--fix");
  if (options.json) args.push("--format", "json");
  args.push(...(options.paths?.length ? options.paths : ["."]));
  return args;
}

/** The project's TypeScript if installed, else the pinned one */
export function tscCommand(projectDir: string): string[] {
  const local = join(projectDir, "node_modules/.bin/tsc");
  if (existsSync(local)) return [local];
  // Store packages have no `.bin`: run the package's own script
  const fromStore = join(packagesDir(projectDir), "typescript/bin/tsc");
  if (usesStore(projectDir) && existsSync(fromStore)) return [process.execPath, fromStore];
  return ["bunx", `typescript@${TYPESCRIPT_VERSION}`];
}

async function run(cmd: string[], cwd: string): Promise<number> {
  const proc = Bun.spawn(cmd, { cwd, stdout: "inherit", stderr: "inherit", stdin: "inherit" });
  return await proc.exited;
}

export async function typecheck(projectDir: string): Promise<number> {
  if (!existsSync(join(projectDir, "tsconfig.json"))) {
    logger.warn("No tsconfig.json: skipping type check");
    return 0;
  }
  return run([...tscCommand(projectDir), "--noEmit", "-p", "tsconfig.json"], projectDir);
}

export async function lintCommand(options: LintOptions = {}) {
  const projectDir = resolve(process.cwd(), options.dir || ".");
  let code = await run(["bunx", `oxlint@${OXLINT_VERSION}`, ...oxlintArgs(projectDir, options)], projectDir);
  if (options.types) {
    if (!options.json) logger.info("Type checking (tsc --noEmit)...");
    code = (await typecheck(projectDir)) || code;
  }
  if (code !== 0) process.exit(code);
}
