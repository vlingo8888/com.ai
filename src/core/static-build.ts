// Static publish, built on this machine: the same build as the web's "Xuất bản tĩnh" and the COM
// app's Publish button. The publisher serves its builder (builder.mjs); here it runs in Bun on
// native engines instead of esbuild-wasm: Rolldown (Rust) bundles and tree-shakes the app with its
// npm packages, the engine's tailwind-rs compiles the CSS. The server only stores the files.
import { existsSync, mkdirSync, readFileSync, renameSync, writeFileSync } from "fs";
import { homedir } from "os";
import { join } from "path";
import { gunzipSync } from "zlib";
import { findEngineBinary } from "../commands/dev";
import { createRolldownEsbuild } from "./rolldown-esbuild";

const CACHE_DIR = join(homedir(), ".nata", "cache", "static-publish");

export type StaticPublishStage = "prepare" | "compiler" | "packages" | "bundle" | "css" | "package" | "upload" | "done";

/** Percent reached when a stage starts, the same steps as the web's progress modal. */
export const STAGE_PERCENT: Record<StaticPublishStage, number> = {
  prepare: 0,
  compiler: 10,
  packages: 25,
  bundle: 30,
  css: 60,
  package: 72,
  upload: 75,
  done: 100,
};

export interface StaticPrepared {
  unchanged?: boolean;
  builderUrl: string;
  runtimeHash: string;
  [key: string]: unknown;
}

/** Downloads `url` once into the cache (files there never change: their names carry a version or hash). */
async function cached(url: string, file: string): Promise<string> {
  const path = join(CACHE_DIR, file);
  if (existsSync(path)) return path;
  const res = await fetch(url);
  if (!res.ok) throw new Error(`Cannot download ${url} (${res.status})`);
  mkdirSync(join(path, ".."), { recursive: true });
  const tmp = `${path}.${process.pid}.tmp`;
  writeFileSync(tmp, new Uint8Array(await res.arrayBuffer()));
  renameSync(tmp, path);
  return path;
}

const ROLLDOWN_VERSION = "1.2.13";

/** Rolldown, installed once per machine where the engine keeps it too (`COM_BUNDLER=rolldown`). */
async function rolldownModule(): Promise<string> {
  const dir = join(homedir(), ".com", "rolldown", ROLLDOWN_VERSION);
  const module = join(dir, "node_modules", "rolldown", "dist", "index.mjs");
  if (existsSync(module)) return module;
  mkdirSync(dir, { recursive: true });
  writeFileSync(
    join(dir, "package.json"),
    JSON.stringify({ name: "com-framework-rolldown", private: true, dependencies: { rolldown: ROLLDOWN_VERSION } })
  );
  const proc = Bun.spawn([process.execPath, "install"], { cwd: dir, stdout: "ignore", stderr: "pipe" });
  const stderr = await new Response(proc.stderr).text();
  if ((await proc.exited) !== 0 || !existsSync(module)) {
    throw new Error(`Could not install Rolldown ${ROLLDOWN_VERSION}: ${stderr.trim().split("\n").pop()}`);
  }
  return module;
}

/** The stylesheet compiled by the engine's native Tailwind v4 (`com-compiler tailwind`). */
async function compileTailwind(input: { css: string; files: Array<{ path: string; content: string }> }): Promise<string> {
  const engine = findEngineBinary();
  if (!engine) throw new Error("the com engine (com-compiler) is missing");
  const proc = Bun.spawn([engine, "tailwind"], { stdin: "pipe", stdout: "pipe", stderr: "pipe" });
  proc.stdin.write(JSON.stringify({ ...input, minify: true }));
  proc.stdin.end();
  const [stdout, stderr, code] = await Promise.all([new Response(proc.stdout).text(), new Response(proc.stderr).text(), proc.exited]);
  if (code !== 0) throw new Error(stderr.trim() || `com-compiler tailwind exited with ${code}`);
  const out = JSON.parse(stdout);
  if (out.error) throw new Error(out.error);
  return out.css;
}

/** npm tarballs on disk (the browser keeps them in IndexedDB). */
export const tarballCache = {
  file: (key: string) => join(CACHE_DIR, "npm", `${key.replace(/\//g, "+")}.tgz`),
  async get(key: string): Promise<Uint8Array | null> {
    try {
      return new Uint8Array(readFileSync(this.file(key)));
    } catch {
      return null;
    }
  },
  put(key: string, bytes: Uint8Array) {
    try {
      const path = this.file(key);
      mkdirSync(join(path, ".."), { recursive: true });
      writeFileSync(path, bytes);
    } catch {
      // Disk full or read-only: download again next time
    }
  },
};

/** The builder gunzips tarballs with DecompressionStream, which older Bun versions lack. */
export function ensureDecompressionStream() {
  if (typeof (globalThis as any).DecompressionStream !== "undefined") return;
  (globalThis as any).DecompressionStream = class {
    readonly readable: ReadableStream<Uint8Array>;
    readonly writable: WritableStream<Uint8Array>;
    constructor(format: string) {
      if (format !== "gzip") throw new TypeError(`Unsupported format: ${format}`);
      const chunks: Uint8Array[] = [];
      let push!: (data: Uint8Array) => void;
      let fail!: (error: unknown) => void;
      this.readable = new ReadableStream({
        start(controller) {
          push = (data) => {
            controller.enqueue(data);
            controller.close();
          };
          fail = (error) => controller.error(error);
        },
      });
      this.writable = new WritableStream({
        write: (chunk) => void chunks.push(chunk),
        close: () => {
          try {
            push(new Uint8Array(gunzipSync(Buffer.concat(chunks))));
          } catch (error) {
            fail(error);
          }
        },
      });
    }
  };
}

/**
 * Builds the prepared app here. Resolves with the payload to POST back to the publisher:
 * `{ ok: true, … }`, or `{ ok: false, errors }` for build errors.
 */
export async function buildStaticApp(prepared: StaticPrepared, onStage: (stage: StaticPublishStage) => void): Promise<any> {
  ensureDecompressionStream();
  const builderPath = await cached(prepared.builderUrl, `builder/${prepared.builderUrl.split("/").pop()}`);
  const { buildApp } = await import(builderPath);
  onStage("compiler");
  const esbuild = await createRolldownEsbuild(await rolldownModule());
  return buildApp(prepared, { esbuild, compileTailwind, tarballCache, onProgress: onStage });
}
