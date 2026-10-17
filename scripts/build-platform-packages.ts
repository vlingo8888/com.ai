#!/usr/bin/env bun
/**
 * Assembles the per-platform npm packages that ship the prebuilt Rust engine:
 *   @com.ai.vn/compiler-<platform>-<arch>
 *
 * Usage:
 *   bun scripts/build-platform-packages.ts <binaries-dir> [out-dir]
 *
 * <binaries-dir> must contain one folder per target named like the target
 * (e.g. `win32-x64/com-compiler.exe`, `darwin-arm64/com-compiler`).
 * Targets without a binary are skipped with a warning.
 */

import { chmodSync, copyFileSync, existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { SUPPORTED_COMPILER_TARGETS, exeName } from "../src/core/binaries";

const PROJECT_ROOT = resolve(import.meta.dir, "..");

export function buildPlatformPackages(binariesDir: string, outDir: string, version: string): string[] {
  rmSync(outDir, { recursive: true, force: true });
  const built: string[] = [];

  for (const target of SUPPORTED_COMPILER_TARGETS) {
    const [platform, arch] = target.split("-") as [NodeJS.Platform, string];
    const binName = exeName("com-compiler", platform);
    const source = join(binariesDir, target, binName);
    if (!existsSync(source)) {
      console.warn(`⚠ Skipping ${target}: ${source} not found`);
      continue;
    }

    const pkgDir = join(outDir, `compiler-${target}`);
    mkdirSync(join(pkgDir, "bin"), { recursive: true });
    const dest = join(pkgDir, "bin", binName);
    copyFileSync(source, dest);
    if (platform !== "win32") chmodSync(dest, 0o755);

    const pkg = {
      name: `@com.ai.vn/compiler-${target}`,
      version,
      description: `Prebuilt Com.AI.VN Rust compiler engine for ${platform} ${arch}`,
      license: "MIT",
      os: [platform],
      cpu: [arch],
      files: ["bin"],
      publishConfig: { access: "public" },
    };
    writeFileSync(join(pkgDir, "package.json"), JSON.stringify(pkg, null, 2) + "\n");
    writeFileSync(
      join(pkgDir, "README.md"),
      `# ${pkg.name}\n\nPlatform binary for [@com.ai.vn/cli](https://www.npmjs.com/package/@com.ai.vn/cli). Do not install directly.\n`
    );
    built.push(pkgDir);
  }

  return built;
}

if (import.meta.main) {
  const [binariesDir, outDir = join(PROJECT_ROOT, "dist-npm")] = process.argv.slice(2);
  if (!binariesDir) {
    console.error("Usage: bun scripts/build-platform-packages.ts <binaries-dir> [out-dir]");
    process.exit(1);
  }
  const { version } = JSON.parse(readFileSync(join(PROJECT_ROOT, "package.json"), "utf-8"));
  const built = buildPlatformPackages(resolve(binariesDir), resolve(outDir), version);
  console.log(`✓ Built ${built.length} platform package(s) for v${version}`);
  for (const dir of built) console.log(`  ${dir}`);
  if (built.length === 0) process.exit(1);
}
