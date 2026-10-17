#!/usr/bin/env node
"use strict";

/**
 * Cross-platform launcher for @com.ai.vn/cli.
 *
 * npm wraps `bin` entries in .cmd shims on Windows, and the CLI itself is
 * TypeScript that needs the Bun runtime. This Node entry point locates a Bun
 * binary (bundled via the `bun` npm dependency, or installed on PATH) and
 * forwards all arguments to bin/cli.ts.
 */

const { spawn } = require("child_process");
const { existsSync, statSync } = require("fs");
const { join, dirname, delimiter } = require("path");

const isWindows = process.platform === "win32";
const bunExe = isWindows ? "bun.exe" : "bun";

// The `bun` npm package ships a tiny placeholder that its postinstall replaces
// with the real binary; anything this small is not a usable runtime.
const MIN_BUN_SIZE = 1024 * 1024;

function isUsableBinary(file) {
  try {
    return existsSync(file) && statSync(file).size > MIN_BUN_SIZE;
  } catch {
    return false;
  }
}

function resolvePackageDir(name) {
  try {
    return dirname(require.resolve(`${name}/package.json`, { paths: [__dirname] }));
  } catch {
    return null;
  }
}

function findOnPath(binName) {
  const dirs = (process.env.PATH || "").split(delimiter).filter(Boolean);
  for (const dir of dirs) {
    const candidate = join(dir, binName);
    if (existsSync(candidate)) return candidate;
  }
  return null;
}

function resolveBun() {
  if (process.env.COM_BUN_PATH && existsSync(process.env.COM_BUN_PATH)) {
    return process.env.COM_BUN_PATH;
  }

  // Already running under Bun (e.g. `bunx @com.ai.vn/cli`)
  if (process.versions && process.versions.bun) {
    return process.execPath;
  }

  const bunPkgDir = resolvePackageDir("bun");
  if (bunPkgDir) {
    // The `bun` npm package names its binary bun.exe on every platform
    const bundled = join(bunPkgDir, "bin", "bun.exe");
    if (isUsableBinary(bundled)) return bundled;
  }

  // Postinstall skipped (--ignore-scripts, pnpm): use the native package directly
  const arch = process.arch === "arm64" ? "aarch64" : process.arch;
  const nativePkgs = [`@oven/bun-${process.platform}-${arch}`, `@oven/bun-${process.platform}-${arch}-baseline`];
  for (const name of nativePkgs) {
    const dir = resolvePackageDir(name);
    if (!dir) continue;
    const candidate = join(dir, "bin", bunExe);
    if (isUsableBinary(candidate)) return candidate;
  }

  return findOnPath(bunExe);
}

function main() {
  const bunPath = resolveBun();
  if (!bunPath) {
    console.error(
      "[com] Could not find the Bun runtime.\n" +
        "      Reinstall the CLI (npm i -g @com.ai.vn/cli) or install Bun from https://bun.sh\n" +
        "      You can also point COM_BUN_PATH to an existing bun executable."
    );
    process.exit(1);
  }

  const cliEntry = join(__dirname, "cli.ts");
  const child = spawn(bunPath, [cliEntry, ...process.argv.slice(2)], {
    stdio: "inherit",
    windowsHide: false,
  });

  const forward = (signal) => {
    if (!child.killed) child.kill(signal);
  };
  process.on("SIGINT", forward);
  process.on("SIGTERM", forward);

  child.on("error", (err) => {
    console.error(`[com] Failed to start Bun at ${bunPath}: ${err.message}`);
    process.exit(1);
  });
  child.on("exit", (code, signal) => {
    if (signal) process.kill(process.pid, signal);
    else process.exit(code == null ? 1 : code);
  });
}

if (require.main === module) {
  main();
}

module.exports = { resolveBun, isUsableBinary };
