#!/usr/bin/env bun
/**
 * NPM Release & Publish Script for @com.ai.vn/cli
 * 
 * Usage:
 *   bun run release                 # Interactive bump & publish
 *   bun run release patch           # Bump patch (1.0.1 -> 1.0.2) & publish
 *   bun run release minor           # Bump minor (1.0.1 -> 1.1.0) & publish
 *   bun run release major           # Bump major (1.0.1 -> 2.0.0) & publish
 *   bun run release 1.0.5           # Set specific version & publish
 *   bun run release --dry-run       # Test packaging without uploading
 *   bun run release --no-test       # Skip running test suite
 */

import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { resolve, join, delimiter } from "node:path";
import * as readline from "node:readline";
import { buildPlatformPackages } from "./build-platform-packages";

// ANSI Styling
const colors = {
  reset: "\x1b[0m",
  bold: "\x1b[1m",
  dim: "\x1b[2m",
  red: "\x1b[31m",
  green: "\x1b[32m",
  yellow: "\x1b[33m",
  blue: "\x1b[34m",
  magenta: "\x1b[35m",
  cyan: "\x1b[36m",
  white: "\x1b[37m",
  gray: "\x1b[90m",
};

const PKG_PATH = resolve(import.meta.dir, "../package.json");
const PROJECT_ROOT = resolve(import.meta.dir, "..");
const BINARIES_DIR = join(PROJECT_ROOT, "binaries");
const DIST_NPM_DIR = join(PROJECT_ROOT, "dist-npm");

// Ensure common tool paths (Node/npm/bun) are in PATH
const extendedPath = [
  "/usr/local/bin",
  "/opt/homebrew/bin",
  join(process.env.HOME || "", ".bun/bin"),
  join(process.env.HOME || "", ".cargo/bin"),
  process.env.PATH || "",
].join(delimiter);

const env = {
  ...process.env,
  PATH: extendedPath,
};

function readPackageJson() {
  const content = readFileSync(PKG_PATH, "utf-8");
  return JSON.parse(content);
}

function writePackageJson(data: any) {
  writeFileSync(PKG_PATH, JSON.stringify(data, null, 2) + "\n", "utf-8");
}

function findNpmExecutable(): string {
  // Check PATH first
  const whichNpm = Bun.which("npm");
  if (whichNpm) return whichNpm;

  const candidates = [
    "/usr/local/bin/npm",
    "/opt/homebrew/bin/npm",
    join(process.env.HOME || "", ".nvm/current/bin/npm"),
  ];

  for (const p of candidates) {
    if (existsSync(p)) return p;
  }
  return "npm";
}

function parseSemver(version: string): { major: number; minor: number; patch: number; prerelease?: string } {
  const [core, prerelease] = version.split("-");
  const parts = core.split(".").map(Number);
  return {
    major: parts[0] || 0,
    minor: parts[1] || 0,
    patch: parts[2] || 0,
    prerelease,
  };
}

function bumpVersion(current: string, type: "patch" | "minor" | "major"): string {
  const parsed = parseSemver(current);
  switch (type) {
    case "patch":
      return `${parsed.major}.${parsed.minor}.${parsed.patch + 1}`;
    case "minor":
      return `${parsed.major}.${parsed.minor + 1}.0`;
    case "major":
      return `${parsed.major + 1}.0.0`;
  }
}

async function promptQuestion(query: string): Promise<string> {
  const rl = readline.createInterface({
    input: process.stdin,
    output: process.stdout,
  });

  return new Promise((resolve) => {
    rl.question(query, (ans) => {
      rl.close();
      resolve(ans.trim());
    });
  });
}

async function runCommand(args: string[], cwd: string = PROJECT_ROOT, silent: boolean = false): Promise<{ code: number; stdout: string; stderr: string }> {
  const proc = Bun.spawn(args, {
    cwd,
    env,
    stdout: "pipe",
    stderr: "pipe",
  });

  const stdout = await new Response(proc.stdout).text();
  const stderr = await new Response(proc.stderr).text();
  const code = await proc.exited;

  if (!silent && code !== 0 && stderr) {
    console.error(colors.red + stderr.trim() + colors.reset);
  }

  return { code, stdout, stderr };
}

async function main() {
  console.log(`\n${colors.bold}${colors.cyan}====================================================${colors.reset}`);
  console.log(`${colors.bold}${colors.cyan}  🚀 @com.ai.vn/cli - NPM Publish & Release Engine  ${colors.reset}`);
  console.log(`${colors.bold}${colors.cyan}====================================================${colors.reset}\n`);

  const pkg = readPackageJson();
  const currentVersion = pkg.version || "1.0.0";
  console.log(`${colors.gray}Package:${colors.reset}    ${colors.bold}${pkg.name}${colors.reset}`);
  console.log(`${colors.gray}Current:${colors.reset}    ${colors.yellow}v${currentVersion}${colors.reset}`);

  // CLI Arguments parsing
  const rawArgs = process.argv.slice(2);
  const isDryRun = rawArgs.includes("--dry-run");
  const skipTests = rawArgs.includes("--no-test") || rawArgs.includes("--skip-tests");
  const bumpArg = rawArgs.find((a) => !a.startsWith("--"));

  let otpCode: string | undefined;
  for (let i = 0; i < rawArgs.length; i++) {
    if (rawArgs[i].startsWith("--otp=")) {
      otpCode = rawArgs[i].split("=")[1];
    } else if (rawArgs[i] === "--otp" && rawArgs[i + 1]) {
      otpCode = rawArgs[++i];
    }
  }

  const npmBin = findNpmExecutable();

  // 1. Preflight: Check npm auth
  console.log(`\n${colors.blue}▶ Checking npm authentication...${colors.reset}`);
  const whoamiRes = await runCommand([npmBin, "whoami"], PROJECT_ROOT, true);
  if (whoamiRes.code !== 0) {
    console.log(`\n${colors.yellow}⚠️  You are not currently logged in to npm!${colors.reset}`);
    console.log(`${colors.gray}Please authenticate with:${colors.reset} ${colors.bold}npm login${colors.reset}\n`);
    
    if (!isDryRun) {
      const proceed = await promptQuestion(`${colors.yellow}Attempt to proceed anyway? [y/N]: ${colors.reset}`);
      if (proceed.toLowerCase() !== "y") {
        console.log(`${colors.red}Aborted. Run 'npm login' first.${colors.reset}\n`);
        process.exit(1);
      }
    }
  } else {
    console.log(`${colors.green}✓ Logged in as:${colors.reset} ${colors.bold}${whoamiRes.stdout.trim()}${colors.reset}`);
  }

  // 2. Preflight: Run tests
  if (!skipTests) {
    console.log(`\n${colors.blue}▶ Running test suite with Bun (${process.execPath})...${colors.reset}`);
    const testProc = Bun.spawn([process.execPath, "test"], {
      cwd: PROJECT_ROOT,
      env,
      stdout: "inherit",
      stderr: "inherit",
    });
    const testExit = await testProc.exited;
    if (testExit !== 0) {
      console.error(`\n${colors.red}❌ Test suite failed with exit code ${testExit}! Aborting publish.${colors.reset}\n`);
      process.exit(1);
    }
    console.log(`${colors.green}✓ All tests passed!${colors.reset}`);
  } else {
    console.log(`${colors.gray}▶ Skipped test suite (--no-test)${colors.reset}`);
  }

  // 3. Determine next version
  let nextVersion = currentVersion;
  if (bumpArg) {
    if (bumpArg === "patch" || bumpArg === "minor" || bumpArg === "major") {
      nextVersion = bumpVersion(currentVersion, bumpArg);
    } else if (/^\d+\.\d+\.\d+/.test(bumpArg)) {
      nextVersion = bumpArg;
    } else {
      console.error(`${colors.red}Invalid version or bump type: "${bumpArg}"${colors.reset}`);
      process.exit(1);
    }
  } else if (!isDryRun) {
    const patchOpt = bumpVersion(currentVersion, "patch");
    const minorOpt = bumpVersion(currentVersion, "minor");
    const majorOpt = bumpVersion(currentVersion, "major");

    console.log(`\n${colors.bold}Choose a release version bump:${colors.reset}`);
    console.log(`  1) patch: ${colors.green}${patchOpt}${colors.reset} (recommended)`);
    console.log(`  2) minor: ${colors.yellow}${minorOpt}${colors.reset}`);
    console.log(`  3) major: ${colors.red}${majorOpt}${colors.reset}`);
    console.log(`  4) keep current: ${colors.gray}${currentVersion}${colors.reset}`);
    console.log(`  5) custom version\n`);

    const answer = await promptQuestion(`${colors.cyan}Select [1-5] (default: 1): ${colors.reset}`);
    if (answer === "2") nextVersion = minorOpt;
    else if (answer === "3") nextVersion = majorOpt;
    else if (answer === "4") nextVersion = currentVersion;
    else if (answer === "5") {
      const customVer = await promptQuestion(`${colors.cyan}Enter custom version: ${colors.reset}`);
      if (!customVer) {
        console.log(`${colors.red}Version cannot be empty.${colors.reset}`);
        process.exit(1);
      }
      nextVersion = customVer;
    } else {
      nextVersion = patchOpt;
    }
  }

  console.log(`\n${colors.green}▶ Target Version:${colors.reset} ${colors.bold}${colors.yellow}v${nextVersion}${colors.reset}`);

  // 4. Update package.json version if changed
  if (nextVersion !== currentVersion && !isDryRun) {
    pkg.version = nextVersion;
    // Keep the prebuilt engine packages locked to the CLI version
    for (const dep of Object.keys(pkg.optionalDependencies || {})) {
      if (dep.startsWith("@com.ai.vn/compiler-")) pkg.optionalDependencies[dep] = nextVersion;
    }
    writePackageJson(pkg);
    console.log(`${colors.green}✓ Updated package.json version to v${nextVersion}${colors.reset}`);
  }

  // 5. Publish the prebuilt engine packages from `binaries/` (filled by `bun run build:binaries`).
  // Without them, installs fall back to compiling Rust locally, which fails on most Windows machines.
  if (existsSync(BINARIES_DIR)) {
    console.log(`\n${colors.blue}▶ Packaging prebuilt engines from binaries/ ...${colors.reset}`);
    const pkgDirs = buildPlatformPackages(BINARIES_DIR, DIST_NPM_DIR, nextVersion);
    for (const dir of pkgDirs) {
      const { name } = JSON.parse(readFileSync(join(dir, "package.json"), "utf-8"));
      const exists = await runCommand([npmBin, "view", `${name}@${nextVersion}`, "version"], PROJECT_ROOT, true);
      if (exists.code === 0) {
        console.log(`${colors.gray}↷ ${name}@${nextVersion} already published${colors.reset}`);
        continue;
      }
      const args = [npmBin, "publish", dir, "--access", "public"];
      if (isDryRun) args.push("--dry-run");
      if (otpCode) args.push(`--otp=${otpCode.trim()}`);
      console.log(`${colors.bold}${colors.cyan}[COMMAND] $ ${args.join(" ")}${colors.reset}`);
      const proc = Bun.spawn(args, { cwd: PROJECT_ROOT, env, stdout: "inherit", stderr: "inherit", stdin: "inherit" });
      const exit = await proc.exited;
      if (exit !== 0) {
        console.error(`\n${colors.red}❌ Failed to publish ${name}. Aborting before publishing the CLI.${colors.reset}\n`);
        process.exit(exit);
      }
      console.log(`${colors.green}✓ Published ${name}@${nextVersion}${colors.reset}`);
    }
  } else {
    console.log(`\n${colors.yellow}ℹ No binaries/ folder. Run 'bun run build:binaries' first to ship prebuilt engines.${colors.reset}`);
  }

  if (!isDryRun) {
    const compilerPkgs = Object.keys(pkg.optionalDependencies || {}).filter((d) => d.startsWith("@com.ai.vn/compiler-"));
    const missing: string[] = [];
    for (const dep of compilerPkgs) {
      const res = await runCommand([npmBin, "view", `${dep}@${nextVersion}`, "version"], PROJECT_ROOT, true);
      if (res.code !== 0) missing.push(dep);
    }
    if (missing.length > 0) {
      console.log(`\n${colors.yellow}⚠️  Prebuilt engine packages not found on npm for v${nextVersion}:${colors.reset}`);
      for (const dep of missing) console.log(`   ${colors.gray}- ${dep}${colors.reset}`);
      console.log(`${colors.gray}Preferred: push tag v${nextVersion} and let GitHub Actions publish everything.${colors.reset}`);
      const proceed = await promptQuestion(`${colors.yellow}Publish the CLI anyway? [y/N]: ${colors.reset}`);
      if (proceed.toLowerCase() !== "y") {
        console.log(`${colors.red}Aborted.${colors.reset}\n`);
        process.exit(1);
      }
    }
  }

  // 6. Publish to npm (Direct native execution, standard interactive npm)
  console.log(`\n${colors.blue}▶ Running npm publish...${colors.reset}`);
  const publishArgs = [npmBin, "publish", "--access", "public"];
  if (isDryRun) {
    publishArgs.push("--dry-run");
    console.log(`${colors.yellow}ℹ Running in DRY-RUN mode. No packages will be actually uploaded.${colors.reset}`);
  }
  if (otpCode) {
    publishArgs.push(`--otp=${otpCode.trim()}`);
  }

  console.log(`${colors.bold}${colors.cyan}[COMMAND] $ ${publishArgs.join(" ")}${colors.reset}\n`);

  const publishProc = Bun.spawn(publishArgs, {
    cwd: PROJECT_ROOT,
    env,
    stdout: "inherit",
    stderr: "inherit",
    stdin: "inherit",
  });

  const publishExit = await publishProc.exited;

  if (publishExit !== 0) {
    console.error(`\n${colors.red}❌ npm publish exited with code ${publishExit}.${colors.reset}\n`);
    process.exit(publishExit);
  }

  // 7. Git commit & Tag (if git repo and not dry run)
  if (!isDryRun && nextVersion !== currentVersion) {
    const gitStatus = await runCommand(["git", "status", "--porcelain"], PROJECT_ROOT, true);
    if (gitStatus.code === 0) {
      const tagChoice = await promptQuestion(`\n${colors.cyan}Create git tag v${nextVersion} and commit package.json? [Y/n]: ${colors.reset}`);
      if (tagChoice.toLowerCase() !== "n") {
        console.log(`${colors.gray}[GIT] Staging and committing release v${nextVersion}...${colors.reset}`);
        await runCommand(["git", "add", "package.json"], PROJECT_ROOT);
        await runCommand(["git", "commit", "-m", `release: v${nextVersion}`], PROJECT_ROOT);
        await runCommand(["git", "tag", `v${nextVersion}`], PROJECT_ROOT);
        console.log(`${colors.green}✓ Git commit and tag v${nextVersion} created!${colors.reset}`);
        console.log(`${colors.gray}  To push tags to remote: git push && git push --tags${colors.reset}`);
      }
    }
  }

  // 8. Live Registry Validation Query (if not dry run)
  if (!isDryRun) {
    console.log(`\n${colors.blue}▶ Validating package on npm registry...${colors.reset}`);
    console.log(`${colors.gray}[EXEC] $ ${npmBin} view ${pkg.name}@${nextVersion} version${colors.reset}`);
    const checkRes = await runCommand([npmBin, "view", `${pkg.name}@${nextVersion}`, "version"], PROJECT_ROOT, true);
    if (checkRes.code === 0) {
      console.log(`${colors.green}✓ Registry Confirmation: ${pkg.name}@${checkRes.stdout.trim()} is LIVE!${colors.reset}`);
    } else {
      console.log(`${colors.gray}ℹ Registry propagation might take 10-30 seconds to appear on all CDN edge nodes.${colors.reset}`);
    }
  }

  // 9. Success Summary Card & Verification Links
  console.log(`\n${colors.bold}${colors.green}╔═══════════════════════════════════════════════════════════════════════╗${colors.reset}`);
  console.log(`${colors.bold}${colors.green}║  🎉  SUCCESSFULLY PUBLISHED TO NPM!                                  ║${colors.reset}`);
  console.log(`${colors.bold}${colors.green}╠═══════════════════════════════════════════════════════════════════════╣${colors.reset}`);
  console.log(`${colors.bold}${colors.green}║${colors.reset}  Package:         ${colors.white}${pkg.name}${colors.reset}`);
  console.log(`${colors.bold}${colors.green}║${colors.reset}  Live Version:    ${colors.yellow}v${nextVersion}${colors.reset}`);
  console.log(`${colors.bold}${colors.green}║${colors.reset}  NPM Package:     ${colors.cyan}https://www.npmjs.com/package/${pkg.name}${colors.reset}`);
  console.log(`${colors.bold}${colors.green}║${colors.reset}  Registry JSON:   ${colors.cyan}https://registry.npmjs.org/${pkg.name}${colors.reset}`);
  console.log(`${colors.bold}${colors.green}║${colors.reset}  Tarball Link:    ${colors.cyan}https://registry.npmjs.org/${pkg.name}/-/${pkg.name.split("/")[1] || "cli"}-${nextVersion}.tgz${colors.reset}`);
  console.log(`${colors.bold}${colors.green}╠═══════════════════════════════════════════════════════════════════════╣${colors.reset}`);
  console.log(`${colors.bold}${colors.green}║  🔍  VALIDATION COMMANDS (Run to verify on your machine):            ║${colors.reset}`);
  console.log(`${colors.bold}${colors.green}║${colors.reset}  1) View on npm:  ${colors.white}npm view ${pkg.name}${colors.reset}`);
  console.log(`${colors.bold}${colors.green}║${colors.reset}  2) Test with Bun: ${colors.white}bunx ${pkg.name} --version${colors.reset}`);
  console.log(`${colors.bold}${colors.green}║${colors.reset}  3) Test with Npx: ${colors.white}npx ${pkg.name} --version${colors.reset}`);
  console.log(`${colors.bold}${colors.green}║${colors.reset}  4) Global Install: ${colors.white}npm i -g ${pkg.name}${colors.reset}  or  ${colors.white}bun add -g ${pkg.name}${colors.reset}`);
  console.log(`${colors.bold}${colors.green}╚═══════════════════════════════════════════════════════════════════════╝${colors.reset}\n`);
}

main().catch((err) => {
  console.error(`\n${colors.red}Unhandled error:${colors.reset}`, err);
  process.exit(1);
});
