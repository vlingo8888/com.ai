#!/usr/bin/env bun
/**
 * Builds the engine for the browser: `binaries/wasm/com-compiler.wasm` (wasm32-wasip1 reactor) and
 * its host `binaries/wasm/host.js` (compiler-rs/wasm/host.ts, bundled).
 *
 * Usage:
 *   bun run build:wasm            # build, then smoke-test es5 / tailwind / routes with node:wasi
 *   bun run build:wasm --dev      # also start `com dev` in the wasm engine (real Bun workers)
 *   bun run build:wasm --no-test
 *
 * The native engine (`build:rust`, `build:binaries`) is unaffected: the same sources, with
 * `src/platform` choosing tokio (native) or the host (wasm) at compile time.
 * Toolchain: `rustup target add wasm32-wasip1`; `wasm-opt` (binaryen) shrinks the output when present.
 */

import { copyFileSync, mkdirSync, statSync } from "node:fs";
import { join, resolve } from "node:path";
import { tmpdir } from "node:os";

const ROOT = resolve(import.meta.dir, "..");
const WASM_CRATE = join(ROOT, "compiler-rs/wasm");
const OUT_DIR = join(ROOT, "binaries/wasm");
const BUILT = join(ROOT, "compiler-rs/target/wasm32-wasip1/release/com_compiler_wasm.wasm");
const args = process.argv.slice(2);

function run(cmd: string[], cwd = ROOT) {
  const result = Bun.spawnSync(cmd, { cwd, stdout: "inherit", stderr: "inherit" });
  if (result.exitCode !== 0) {
    console.error(`✖ ${cmd.join(" ")} failed (${result.exitCode})`);
    process.exit(1);
  }
}

const mb = (path: string) => `${(statSync(path).size / 1024 / 1024).toFixed(1)} MB`;

const targets = Bun.spawnSync(["rustup", "target", "list", "--installed"]).stdout.toString();
if (!targets.includes("wasm32-wasip1")) run(["rustup", "target", "add", "wasm32-wasip1"]);

run(["cargo", "build", "--release"], WASM_CRATE);
mkdirSync(OUT_DIR, { recursive: true });
const out = join(OUT_DIR, "com-compiler.wasm");
if (Bun.which("wasm-opt")) {
  run(["wasm-opt", "-Os", "--enable-bulk-memory", "--enable-nontrapping-float-to-int", "--enable-sign-ext", BUILT, "-o", out]);
} else {
  copyFileSync(BUILT, out);
}
run(["bun", "build", join(WASM_CRATE, "host.ts"), "--target", "browser", "--format", "esm", "--outfile", join(OUT_DIR, "host.js")]);
console.log(`✓ ${out} (${mb(out)})`);

if (!args.includes("--no-test")) {
  const smoke = join(tmpdir(), "com-wasm-smoke.mjs");
  run(["bun", "build", join(WASM_CRATE, "smoke.ts"), "--target", "node", "--outfile", smoke]);
  run(["node", smoke, out, ...args.filter((a) => a === "--dev")]);
}
