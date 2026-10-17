#!/usr/bin/env bun
/**
 * Cross-compiles the Rust engine (com-compiler) for every supported platform
 * from a single macOS machine and stages the results in `binaries/<target>/`.
 *
 * Usage:
 *   bun run build:binaries                  # all targets
 *   bun run build:binaries win32-x64        # only some targets
 *
 * Toolchain:
 *   - macOS targets: plain `cargo build`
 *   - Linux targets: `cargo zigbuild` (brew install zig && cargo install cargo-zigbuild)
 *   - Windows target: `cargo xwin` (cargo install cargo-xwin)
 *
 * Afterwards `bun run release` packages these binaries and publishes everything.
 */

import { copyFileSync, existsSync, mkdirSync } from "node:fs";
import { join, resolve } from "node:path";
import { SUPPORTED_COMPILER_TARGETS, exeName } from "../src/core/binaries";

const PROJECT_ROOT = resolve(import.meta.dir, "..");
const COMPILER_DIR = join(PROJECT_ROOT, "compiler-rs");
export const BINARIES_DIR = join(PROJECT_ROOT, "binaries");

type Target = (typeof SUPPORTED_COMPILER_TARGETS)[number];

const TARGETS: Record<Target, { rust: string; tool: "cargo" | "zigbuild" | "xwin"; zigSuffix?: string }> = {
  "darwin-arm64": { rust: "aarch64-apple-darwin", tool: "cargo" },
  "darwin-x64": { rust: "x86_64-apple-darwin", tool: "cargo" },
  // Pin glibc 2.28 so the binary runs on older distros (Debian 10, RHEL 8, Ubuntu 18.10+)
  "linux-x64": { rust: "x86_64-unknown-linux-gnu", tool: "zigbuild", zigSuffix: ".2.28" },
  "linux-arm64": { rust: "aarch64-unknown-linux-gnu", tool: "zigbuild", zigSuffix: ".2.28" },
  "win32-x64": { rust: "x86_64-pc-windows-msvc", tool: "xwin" },
};

function run(cmd: string[], cwd = COMPILER_DIR): boolean {
  console.log(`\x1b[90m$ ${cmd.join(" ")}\x1b[0m`);
  const proc = Bun.spawnSync(cmd, { cwd, stdout: "inherit", stderr: "inherit" });
  return proc.exitCode === 0;
}

function requireTool(bin: string, hint: string) {
  if (!Bun.which(bin)) {
    console.error(`\x1b[31m✗ Missing '${bin}'. Install with: ${hint}\x1b[0m`);
    process.exit(1);
  }
}

function buildTarget(target: Target): boolean {
  const { rust, tool, zigSuffix = "" } = TARGETS[target];
  console.log(`\n\x1b[36m▶ Building ${target} (${rust})\x1b[0m`);

  if (!run(["rustup", "target", "add", rust])) return false;

  const common = ["--release", "--bin", "com-compiler"];
  const cmd =
    tool === "cargo"
      ? ["cargo", "build", "--target", rust, ...common]
      : tool === "zigbuild"
        ? ["cargo", "zigbuild", "--target", `${rust}${zigSuffix}`, ...common]
        : ["cargo", "xwin", "build", "--target", rust, ...common];
  if (!run(cmd)) return false;

  const binName = exeName("com-compiler", target.split("-")[0] as NodeJS.Platform);
  const built = join(COMPILER_DIR, "target", rust, "release", binName);
  if (!existsSync(built)) {
    console.error(`\x1b[31m✗ Expected ${built} after build\x1b[0m`);
    return false;
  }
  mkdirSync(join(BINARIES_DIR, target), { recursive: true });
  copyFileSync(built, join(BINARIES_DIR, target, binName));
  console.log(`\x1b[32m✓ ${target} → binaries/${target}/${binName}\x1b[0m`);
  return true;
}

if (import.meta.main) {
  if (process.platform !== "darwin") {
    console.error("This script cross-compiles from macOS. On other systems use the GitHub Actions release workflow.");
    process.exit(1);
  }

  const requested = process.argv.slice(2) as Target[];
  const unknown = requested.filter((t) => !(t in TARGETS));
  if (unknown.length) {
    console.error(`Unknown target(s): ${unknown.join(", ")}. Valid: ${SUPPORTED_COMPILER_TARGETS.join(", ")}`);
    process.exit(1);
  }
  const targets = requested.length ? requested : [...SUPPORTED_COMPILER_TARGETS];

  requireTool("rustup", "https://rustup.rs");
  if (targets.some((t) => TARGETS[t].tool === "zigbuild")) {
    requireTool("zig", "brew install zig");
    requireTool("cargo-zigbuild", "cargo install cargo-zigbuild");
  }
  if (targets.some((t) => TARGETS[t].tool === "xwin")) {
    requireTool("cargo-xwin", "cargo install cargo-xwin");
  }

  const failed = targets.filter((t) => !buildTarget(t));
  console.log("");
  if (failed.length) {
    console.error(`\x1b[31m✗ Failed: ${failed.join(", ")}\x1b[0m`);
    process.exit(1);
  }
  console.log(`\x1b[32m✓ Built ${targets.length} target(s) into binaries/. Next: bun run release\x1b[0m`);
}
