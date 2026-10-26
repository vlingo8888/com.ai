#!/usr/bin/env bun
/**
 * Builds installers of the com CLI that need neither Node, npm nor Bun on the machine: each one ships
 * the standalone `com` (the CLI compiled with `bun build --compile`, Bun embedded; it also runs the
 * engine's worker and bundler as Bun, src/core/self.ts) and the Rust engine next to it.
 *
 *   release/com-macos.dmg            macOS (Apple Silicon + Intel), a .pkg into /usr/local/com
 *   release/com-macos.pkg
 *   release/com-macos.tar.gz         macOS, installed by release/install.sh (curl … | bash)
 *   release/com-windows-setup.exe    Windows x64, per user, adds com to the PATH (also run by install.ps1)
 *   release/com-linux-x64.tar.gz     Linux, installed by release/install.sh
 *   release/com-linux-arm64.tar.gz
 *   release/SHA256SUMS, release/version.txt
 *
 * Usage:
 *   bun scripts/build-installers.ts [macos] [windows] [linux-x64] [linux-arm64]   # default: all
 *     --build        rebuild the engines first (scripts/build-binaries.ts), one target at a time
 *     --upload       upload to the S3 bucket, under com-cli/v<version>/ and com-cli/latest/
 *     --env <file>   read AWS_ENDPOINT, AWS_REGION, AWS_ACCESS_KEY_ID, AWS_SECRET_ACCESS_KEY and
 *                    AWS_S3_BUCKET_NAME from this file instead of the environment
 *
 * Runs on macOS (pkgbuild, hdiutil, lipo, codesign); the Windows installer needs NSIS (brew install makensis).
 * The installers are not signed: macOS asks to allow the .pkg in System Settings > Privacy &
 * Security, Windows SmartScreen shows "More info > Run anyway".
 */

import { chmodSync, cpSync, existsSync, mkdirSync, readFileSync, readdirSync, rmSync, statSync, symlinkSync, writeFileSync } from "node:fs";
import { basename, dirname, join, resolve } from "node:path";
import { S3Client } from "bun";
import { TARGETS } from "./build-binaries";

const ROOT = resolve(import.meta.dir, "..");
const INSTALLERS = join(ROOT, "scripts/installers");
const OUT = join(ROOT, "release");
const CACHE = join(OUT, ".cache");
const PKG = JSON.parse(readFileSync(join(ROOT, "package.json"), "utf8"));
const VERSION: string = PKG.version;
const BUN_VERSION: string = PKG.dependencies.bun;
const DOWNLOAD_PREFIX = "com-cli";

type Target = "darwin-arm64" | "darwin-x64" | "linux-x64" | "linux-arm64" | "win32-x64";
const BUN_ASSET: Record<Target, string> = {
  "darwin-arm64": "bun-darwin-aarch64",
  "darwin-x64": "bun-darwin-x64",
  "linux-x64": "bun-linux-x64",
  "linux-arm64": "bun-linux-aarch64",
  "win32-x64": "bun-windows-x64",
};
const BUN_COMPILE_TARGET: Record<Target, string> = {
  "darwin-arm64": "bun-darwin-arm64",
  "darwin-x64": "bun-darwin-x64",
  "linux-x64": "bun-linux-x64",
  "linux-arm64": "bun-linux-arm64",
  "win32-x64": "bun-windows-x64",
};
const PLATFORMS = {
  macos: ["darwin-arm64", "darwin-x64"],
  windows: ["win32-x64"],
  "linux-x64": ["linux-x64"],
  "linux-arm64": ["linux-arm64"],
} satisfies Record<string, Target[]>;
type Platform = keyof typeof PLATFORMS;

const exe = (name: string, target: Target) => (target === "win32-x64" ? `${name}.exe` : name);

function run(cmd: string[], cwd = ROOT) {
  console.log(`\x1b[90m$ ${cmd.join(" ")}\x1b[0m`);
  const proc = Bun.spawnSync(cmd, { cwd, stdout: "inherit", stderr: "inherit" });
  if (proc.exitCode !== 0) throw new Error(`${cmd[0]} failed (exit ${proc.exitCode})`);
}

function output(cmd: string[], cwd = ROOT): string {
  const proc = Bun.spawnSync(cmd, { cwd, stdout: "pipe", stderr: "pipe" });
  return proc.exitCode === 0 ? proc.stdout.toString().trim() : "";
}

/** The engine of `target` from binaries/ (scripts/build-binaries.ts), warning when it predates its sources */
function engine(target: Target): string {
  const file = join(ROOT, "binaries", target, exe("com-compiler", target));
  if (!existsSync(file)) throw new Error(`No engine for ${target}: run with --build, or bun scripts/build-binaries.ts ${target}`);
  const changed = Number(output(["git", "log", "-1", "--format=%ct", "--", "compiler-rs"])) * 1000;
  if (changed && statSync(file).mtimeMs < changed) {
    console.warn(`\x1b[33m⚠ ${file} is older than the last commit to compiler-rs: run with --build to ship the current engine\x1b[0m`);
  }
  return file;
}

/** The Bun binary of `target`, downloaded once from the Bun GitHub release the CLI depends on */
function bunBinary(target: Target): string {
  const asset = BUN_ASSET[target];
  const dir = join(CACHE, `bun-v${BUN_VERSION}`);
  const file = join(dir, asset, exe("bun", target));
  if (existsSync(file)) return file;
  const url = `https://github.com/oven-sh/bun/releases/download/bun-v${BUN_VERSION}/${asset}.zip`;
  console.log(`Downloading ${url}`);
  mkdirSync(dir, { recursive: true });
  const zip = join(dir, `${asset}.zip`);
  run(["curl", "-fL", "--retry", "3", "--progress-bar", url, "-o", zip]);
  run(["unzip", "-o", "-q", zip, "-d", dir]);
  rmSync(zip);
  if (!existsSync(file)) throw new Error(`${url} has no ${asset}/${exe("bun", target)}`);
  return file;
}

/** The host's Bun of the version the CLI depends on: `--compile` embeds the Bun that compiles */
function hostBun(): string {
  return bunBinary(process.arch === "arm64" ? "darwin-arm64" : "darwin-x64");
}

/** The standalone `com` of `target`: the CLI and its dependencies compiled into the Bun of `target` */
function compileCli(target: Target): string {
  const out = join(OUT, "stage/cli", target, exe("com", target));
  rmSync(dirname(out), { recursive: true, force: true });
  mkdirSync(dirname(out), { recursive: true });
  run([hostBun(), "build", "--compile", "--minify", `--target=${BUN_COMPILE_TARGET[target]}-v${BUN_VERSION}`, join(ROOT, "bin/cli.ts"), "--outfile", out]);
  return out;
}

/** `<dir>/com`: the folder every installer puts on the machine */
function assemble(dir: string, target: Target, cli: string, compiler: string): string {
  const home = join(dir, "com");
  rmSync(dir, { recursive: true, force: true });
  mkdirSync(home, { recursive: true });
  cpSync(cli, join(home, exe("com", target)));
  cpSync(compiler, join(home, exe("com-compiler", target)));
  for (const name of ["cai", "aivn"]) {
    // Aliases: a link on unix, a two-line script on Windows (a copy would weigh 100MB more)
    if (target === "win32-x64") writeFileSync(join(home, `${name}.cmd`), `@echo off\r\n"%~dp0com.exe" %*\r\n`);
    else symlinkSync("com", join(home, name));
  }
  if (target !== "win32-x64") {
    chmodSync(join(home, "com"), 0o755);
    chmodSync(join(home, "com-compiler"), 0o755);
  }
  return home;
}

async function buildMacos(): Promise<string[]> {
  const universal = join(OUT, "stage/macos-bin");
  mkdirSync(universal, { recursive: true });
  const cli = join(universal, "com");
  const compiler = join(universal, "com-compiler");
  run(["lipo", "-create", compileCli("darwin-arm64"), compileCli("darwin-x64"), "-output", cli]);
  // lipo drops the signature each compiled slice had: Apple Silicon kills unsigned executables
  run(["codesign", "--force", "--sign", "-", cli]);
  run(["lipo", "-create", engine("darwin-arm64"), engine("darwin-x64"), "-output", compiler]);

  const payload = join(OUT, "stage/macos");
  assemble(payload, "darwin-arm64", cli, compiler);
  run(["xattr", "-cr", payload]);
  const pkg = join(OUT, "com-macos.pkg");
  run([
    "pkgbuild",
    "--root", payload,
    "--install-location", "/usr/local",
    "--scripts", join(INSTALLERS, "macos"),
    "--identifier", "vn.com.ai.cli",
    "--version", VERSION,
    pkg,
  ]);

  const volume = join(OUT, "stage/dmg");
  rmSync(volume, { recursive: true, force: true });
  mkdirSync(volume, { recursive: true });
  cpSync(pkg, join(volume, `Install com ${VERSION}.pkg`));
  writeFileSync(
    join(volume, "README.txt"),
    `com ${VERSION}\n\n` +
      `1. Open "Install com ${VERSION}.pkg" and follow the steps.\n` +
      `   If macOS says it cannot verify the developer: System Settings > Privacy & Security > Open Anyway.\n` +
      `2. Open a new Terminal and run: com create my-app\n\n` +
      `com is installed in /usr/local/com, with com, cai and aivn in /usr/local/bin.\n` +
      `Uninstall: sudo rm -rf /usr/local/com /usr/local/bin/com /usr/local/bin/cai /usr/local/bin/aivn\n`,
  );
  const dmg = join(OUT, "com-macos.dmg");
  // hdiutil's own estimate of the volume size falls short of the .pkg: "No space left on device"
  const sizeMb = Math.ceil(statSync(pkg).size / 1048576) + 32;
  run(["hdiutil", "create", "-volname", `com ${VERSION}`, "-srcfolder", volume, "-size", `${sizeMb}m`, "-ov", "-format", "UDZO", dmg]);
  // For install.sh: files curl downloads carry no quarantine flag, so macOS runs them unsigned
  const archive = join(OUT, "com-macos.tar.gz");
  run(["tar", "--no-xattrs", "-czf", archive, "-C", payload, "com"]);
  return [dmg, pkg, archive];
}

async function buildWindows(): Promise<string[]> {
  if (!Bun.which("makensis")) throw new Error("The Windows installer needs NSIS: brew install makensis");
  const home = assemble(join(OUT, "stage/windows"), "win32-x64", compileCli("win32-x64"), engine("win32-x64"));
  const setup = join(OUT, "com-windows-setup.exe");
  run(["makensis", "-V2", `-DVERSION=${VERSION}`, `-DSRC=${home}`, `-DOUT=${setup}`, join(INSTALLERS, "com.nsi")]);
  return [setup];
}

async function buildLinux(target: "linux-x64" | "linux-arm64"): Promise<string[]> {
  const dir = join(OUT, `stage/${target}`);
  assemble(dir, target, compileCli(target), engine(target));
  const archive = join(OUT, `com-${target}.tar.gz`);
  run(["tar", "--no-xattrs", "-czf", archive, "-C", dir, "com"]);
  return [archive];
}

/** KEY=value lines of an env file */
function readEnvFile(file: string): Record<string, string> {
  const env: Record<string, string> = {};
  for (const line of readFileSync(file, "utf8").split(/\r?\n/)) {
    const match = line.match(/^\s*(?:export\s+)?([A-Za-z_][A-Za-z0-9_]*)\s*=\s*(.*?)\s*$/);
    if (match) env[match[1]] = match[2].replace(/^(['"])(.*)\1$/, "$2");
  }
  return env;
}

const CONTENT_TYPES: Record<string, string> = {
  ".dmg": "application/x-apple-diskimage",
  ".pkg": "application/octet-stream",
  ".exe": "application/vnd.microsoft.portable-executable",
  ".gz": "application/gzip",
  ".sh": "text/x-shellscript; charset=utf-8",
  ".ps1": "text/plain; charset=utf-8",
};

async function upload(files: string[], envFile: string | undefined) {
  const env = envFile ? readEnvFile(envFile) : (process.env as Record<string, string>);
  const missing = ["AWS_ENDPOINT", "AWS_ACCESS_KEY_ID", "AWS_SECRET_ACCESS_KEY", "AWS_S3_BUCKET_NAME"].filter((k) => !env[k]);
  if (missing.length) throw new Error(`Missing ${missing.join(", ")} ${envFile ? `in ${envFile}` : "in the environment"}`);
  const s3 = new S3Client({
    endpoint: env.AWS_ENDPOINT,
    region: env.AWS_REGION,
    bucket: env.AWS_S3_BUCKET_NAME,
    accessKeyId: env.AWS_ACCESS_KEY_ID,
    secretAccessKey: env.AWS_SECRET_ACCESS_KEY,
  });
  const base = `${env.AWS_ENDPOINT.replace(/\/$/, "")}/${env.AWS_S3_BUCKET_NAME}/${DOWNLOAD_PREFIX}`;
  // The versioned copy first: `latest/` never points at files that are not uploaded yet
  for (const prefix of [`v${VERSION}`, "latest"]) {
    for (const file of files) {
      const name = basename(file);
      const type = CONTENT_TYPES[name.slice(name.lastIndexOf("."))] ?? "text/plain; charset=utf-8";
      const key = `${DOWNLOAD_PREFIX}/${prefix}/${name}`;
      process.stdout.write(`↑ ${key} (${(statSync(file).size / 1048576).toFixed(1)}MB)\n`);
      await s3.write(key, Bun.file(file), { type, acl: "public-read" });
    }
  }
  for (const script of ["install.sh", "install.ps1"]) {
    const type = CONTENT_TYPES[script.slice(script.lastIndexOf("."))];
    await s3.write(`${DOWNLOAD_PREFIX}/${script}`, Bun.file(join(OUT, script)), { type, acl: "public-read" });
  }
  console.log(`\n\x1b[32m✓ Uploaded\x1b[0m`);
  for (const file of files.filter((f) => /\.(dmg|exe|gz)$/.test(f))) console.log(`  ${base}/latest/${basename(file)}`);
  console.log(`  Linux, macOS: curl -fsSL https://com.ai.vn/install | bash`);
  console.log(`  Windows: powershell -c "irm com.ai.vn/install.ps1 | iex"`);
}

if (import.meta.main) {
  const args = process.argv.slice(2);
  const envAt = args.indexOf("--env");
  const envFile = envAt >= 0 ? resolve(args[envAt + 1] ?? "") : undefined;
  const requested = args.filter((a, i) => !a.startsWith("--") && !(envAt >= 0 && i === envAt + 1)) as Platform[];
  const unknown = requested.filter((p) => !(p in PLATFORMS));
  if (unknown.length) {
    console.error(`Unknown platform(s): ${unknown.join(", ")}. Valid: ${Object.keys(PLATFORMS).join(", ")}`);
    process.exit(1);
  }
  const platforms = requested.length ? requested : (Object.keys(PLATFORMS) as Platform[]);
  if (process.platform !== "darwin") {
    console.error("This script builds the installers on macOS (pkgbuild, hdiutil, lipo).");
    process.exit(1);
  }

  try {
    if (args.includes("--build")) {
      // One target at a time, its build folder (several GB with LTO) deleted once the engine is copied
      for (const target of platforms.flatMap((p) => PLATFORMS[p])) {
        run([process.execPath, join(ROOT, "scripts/build-binaries.ts"), target]);
        rmSync(join(ROOT, "compiler-rs/target", TARGETS[target].rust), { recursive: true, force: true });
      }
    }
    mkdirSync(OUT, { recursive: true });
    for (const name of readdirSync(OUT)) {
      if (name.startsWith("com-") || name === "SHA256SUMS" || name === "version.txt") rmSync(join(OUT, name), { force: true });
    }
    // The layout before the standalone `com` (lib/ + runtime/bun)
    rmSync(join(OUT, "stage/lib"), { recursive: true, force: true });
    const files: string[] = [];
    for (const platform of platforms) {
      console.log(`\n\x1b[36m▶ ${platform}\x1b[0m`);
      if (platform === "macos") files.push(...(await buildMacos()));
      else if (platform === "windows") files.push(...(await buildWindows()));
      else files.push(...(await buildLinux(platform)));
    }

    for (const script of ["install.sh", "install.ps1"]) cpSync(join(INSTALLERS, script), join(OUT, script));
    writeFileSync(join(OUT, "version.txt"), `${VERSION}\n`);
    const sums = files.map((f) => `${new Bun.CryptoHasher("sha256").update(readFileSync(f)).digest("hex")}  ${basename(f)}`);
    writeFileSync(join(OUT, "SHA256SUMS"), sums.join("\n") + "\n");
    files.push(join(OUT, "SHA256SUMS"), join(OUT, "version.txt"));

    console.log("");
    for (const file of files) console.log(`\x1b[32m✓\x1b[0m ${file.replace(ROOT + "/", "")}  ${(statSync(file).size / 1048576).toFixed(1)}MB`);
    if (args.includes("--upload")) await upload(files, envFile);
  } catch (error) {
    console.error(`\x1b[31m✗ ${(error as Error).message}\x1b[0m`);
    process.exit(1);
  }
}
