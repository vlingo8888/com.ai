#!/usr/bin/env bun
/**
 * Releases everything in one command, from a macOS machine:
 *
 *   1. bumps the version in package.json (and the engine packages it pins)
 *   2. runs the tests: bun test, cargo test
 *   3. builds the engines and the installers (scripts/build-installers.ts --build)
 *   4. uploads the installers: curl -fsSL https://com.ai.vn/install | bash gets this version
 *   5. publishes the engine packages and the CLI to npm (scripts/publish-npm.ts)
 *   6. commits package.json and tags v<version> (not pushed)
 *
 * Usage:
 *   bun run release:all patch --env ~/.com-release.env     # or minor, major, 1.2.3
 *     --dry-run          everything but writing package.json, uploading, publishing and git
 *     --no-build         reuse the engines in binaries/ (bun run build:binaries)
 *     --no-test          skip step 2
 *     --skip-installers  skip steps 3 and 4
 *     --skip-npm         skip step 5
 *     --otp <code>       npm one-time password
 *     --allow-dirty      release with uncommitted changes (they are not part of the commit)
 *
 * The --env file holds AWS_ENDPOINT, AWS_REGION, AWS_ACCESS_KEY_ID, AWS_SECRET_ACCESS_KEY and
 * AWS_S3_BUCKET_NAME (scripts/build-installers.ts reads it); without it, the environment.
 */

import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";

const ROOT = resolve(import.meta.dir, "..");
const PKG_PATH = join(ROOT, "package.json");

export interface ReleaseOptions {
  bump: string;
  dryRun: boolean;
  build: boolean;
  test: boolean;
  installers: boolean;
  npm: boolean;
  allowDirty: boolean;
  envFile?: string;
  otp?: string;
}

export function parseArgs(argv: string[]): ReleaseOptions {
  const options: ReleaseOptions = {
    bump: "",
    dryRun: false,
    build: true,
    test: true,
    installers: true,
    npm: true,
    allowDirty: false,
  };
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    if (arg === "--dry-run") options.dryRun = true;
    else if (arg === "--no-build") options.build = false;
    else if (arg === "--no-test" || arg === "--skip-tests") options.test = false;
    else if (arg === "--skip-installers") options.installers = false;
    else if (arg === "--skip-npm") options.npm = false;
    else if (arg === "--allow-dirty") options.allowDirty = true;
    else if (arg === "--env" && argv[i + 1]) options.envFile = resolve(argv[++i]);
    else if (arg === "--otp" && argv[i + 1]) options.otp = argv[++i];
    else if (arg.startsWith("--otp=")) options.otp = arg.slice("--otp=".length);
    else if (arg.startsWith("--")) throw new Error(`Unknown option: ${arg}`);
    else options.bump = arg;
  }
  return options;
}

/** The version a bump (`patch`, `minor`, `major` or `x.y.z`) makes of `current` */
export function nextVersion(current: string, bump: string): string {
  if (/^\d+\.\d+\.\d+(-[\w.]+)?$/.test(bump)) return bump;
  const [major = 0, minor = 0, patch = 0] = current.split("-")[0].split(".").map(Number);
  if (bump === "patch") return `${major}.${minor}.${patch + 1}`;
  if (bump === "minor") return `${major}.${minor + 1}.0`;
  if (bump === "major") return `${major + 1}.0.0`;
  throw new Error(`Expected patch, minor, major or a version like 1.2.3, got "${bump || "nothing"}"`);
}

/** package.json at `version`, the prebuilt engine packages pinned to it as publish-npm.ts does */
export function withVersion(pkg: any, version: string): any {
  const optionalDependencies = { ...pkg.optionalDependencies };
  for (const dep of Object.keys(optionalDependencies)) {
    if (dep.startsWith("@com.ai.vn/compiler-")) optionalDependencies[dep] = version;
  }
  return { ...pkg, version, optionalDependencies };
}

function step(n: number, title: string) {
  console.log(`\n\x1b[1m\x1b[36m[${n}/6] ${title}\x1b[0m`);
}

function run(cmd: string[], cwd = ROOT) {
  console.log(`\x1b[90m$ ${cmd.join(" ")}\x1b[0m`);
  const proc = Bun.spawnSync(cmd, { cwd, stdout: "inherit", stderr: "inherit", stdin: "inherit" });
  if (proc.exitCode !== 0) throw new Error(`${cmd.slice(0, 3).join(" ")} failed (exit ${proc.exitCode})`);
}

function output(cmd: string[]): string {
  const proc = Bun.spawnSync(cmd, { cwd: ROOT, stdout: "pipe", stderr: "pipe" });
  return proc.exitCode === 0 ? proc.stdout.toString().trim() : "";
}

async function release(options: ReleaseOptions) {
  const pkg = JSON.parse(readFileSync(PKG_PATH, "utf8"));
  const version = nextVersion(pkg.version, options.bump);
  const tag = `v${version}`;
  const bun = process.execPath;

  // Preflight: everything that would fail half way, before anything leaves this machine
  if (process.platform !== "darwin" && options.installers) throw new Error("The installers are built on macOS (pkgbuild, hdiutil, lipo)");
  if (!options.allowDirty && output(["git", "status", "--porcelain"])) {
    throw new Error("Uncommitted changes: commit them first (or --allow-dirty)");
  }
  if (output(["git", "tag", "--list", tag])) throw new Error(`Tag ${tag} already exists`);
  if (options.installers && !options.dryRun) {
    if (options.envFile && !existsSync(options.envFile)) throw new Error(`No such file: ${options.envFile}`);
    if (!options.envFile && !process.env.AWS_ACCESS_KEY_ID) throw new Error("Uploading needs --env <file> (or AWS_* in the environment)");
  }

  console.log(`\x1b[1m${pkg.name}\x1b[0m v${pkg.version} → \x1b[1m\x1b[33m${tag}\x1b[0m${options.dryRun ? "  \x1b[33m(dry run)\x1b[0m" : ""}`);

  step(1, `Version ${version}`);
  if (options.dryRun) console.log("\x1b[90mdry run: package.json unchanged\x1b[0m");
  else writeFileSync(PKG_PATH, JSON.stringify(withVersion(pkg, version), null, 2) + "\n");

  try {
    step(2, "Tests");
    if (options.test) {
      run([bun, "test"]);
      run(["cargo", "test", "--release"], join(ROOT, "compiler-rs"));
    } else console.log("\x1b[90mskipped (--no-test)\x1b[0m");

    step(3, "Engines and installers");
    step(4, "Upload to com.ai.vn");
    if (options.installers) {
      const args = [bun, join(ROOT, "scripts/build-installers.ts")];
      if (options.build) args.push("--build");
      if (!options.dryRun) args.push("--upload", ...(options.envFile ? ["--env", options.envFile] : []));
      run(args);
      if (options.dryRun) console.log("\x1b[90mdry run: built into release/, not uploaded\x1b[0m");
    } else console.log("\x1b[90mskipped (--skip-installers)\x1b[0m");
  } catch (error) {
    // Nothing has left the machine yet: leave package.json as it was
    if (!options.dryRun) writeFileSync(PKG_PATH, JSON.stringify(pkg, null, 2) + "\n");
    throw error;
  }

  step(5, "npm");
  if (options.npm) {
    // package.json already holds the version: publish-npm.ts neither bumps it nor tags
    const args = [bun, join(ROOT, "scripts/publish-npm.ts"), version, "--no-test"];
    if (options.dryRun) args.push("--dry-run");
    if (options.otp) args.push("--otp", options.otp);
    run(args);
  } else console.log("\x1b[90mskipped (--skip-npm)\x1b[0m");

  step(6, `Commit and tag ${tag}`);
  if (options.dryRun) {
    console.log("\x1b[90mdry run: no commit, no tag\x1b[0m");
  } else {
    run(["git", "add", "package.json"]);
    run(["git", "commit", "-m", `release: ${tag}`]);
    run(["git", "tag", tag]);
  }

  console.log(`\n\x1b[32m✓ ${options.dryRun ? "Dry run of" : "Released"} ${tag}\x1b[0m`);
  if (!options.dryRun) {
    console.log("  git push && git push --tags");
    if (options.installers) console.log("  curl -fsSL https://com.ai.vn/install | bash");
    if (options.npm) console.log(`  npm i -g ${pkg.name}@${version}`);
  }
}

if (import.meta.main) {
  try {
    await release(parseArgs(process.argv.slice(2)));
  } catch (error) {
    console.error(`\n\x1b[31m✗ ${(error as Error).message}\x1b[0m`);
    process.exit(1);
  }
}
