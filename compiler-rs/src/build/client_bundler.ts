// Browser bundle of a production build (`com build`, see build/client.rs). Every module is read from
// the build's internal server by its development URL (`/_bundle/app/x.tsx`, `/_nata/deps/react`), so
// the bundle has exactly the code `com dev` serves, compiled for production. Bun.build then splits
// shared code into chunks, tree-shakes and minifies; file names carry a content hash.
//
//   bun client_bundler.ts <config.json>
//
// Prints `__NATA_BUNDLE__{json}` on its last line: entry → bundle URL, static imports, the files
// written and the modules read (the engine compresses the files and decides from the modules whether
// the next build can reuse this bundle).

import { readFileSync, rmSync } from "node:fs";
import { relative } from "node:path";

interface Config {
  base: string;
  entries: string[];
  outdir: string;
  publicPath: string;
  importMap: Record<string, string>;
}

const config: Config = JSON.parse(readFileSync(process.argv[2], "utf-8"));
const errors: string[] = [];
const inputs = new Set<string>();

/** `/a/b.js` + `./c.js` → `/a/c.js` (query kept) */
function resolveUrl(spec: string, importer: string) {
  const url = new URL(spec, "http://engine" + importer);
  return url.pathname + url.search;
}

const LOADERS: Record<string, "js" | "css" | "json"> = { "text/css": "css", "application/json": "json" };

const engine: import("bun").BunPlugin = {
  name: "nata-engine",
  setup(build) {
    // Every module comes from the engine; Bun's `namespace` filter does not apply to the importer, so
    // one resolver handles all specifiers
    build.onResolve({ filter: /.*/ }, (args) => {
      const spec = args.path;
      if (/^(https?:)?\/\/[^/]/.test(spec)) return { path: spec, external: true };
      if (spec.startsWith("/")) return { path: spec, namespace: "nata" };
      if (spec.startsWith("./") || spec.startsWith("../")) return { path: resolveUrl(spec, args.importer), namespace: "nata" };
      // Bare specifiers left in served code (shims written against the import map)
      return { path: config.importMap[spec] ?? `/_nata/deps/${spec}`, namespace: "nata" };
    });
    build.onLoad({ filter: /.*/, namespace: "nata" }, async (args) => {
      inputs.add(args.path);
      const res = await fetch(config.base + args.path);
      const text = await res.text();
      if (!res.ok) throw new Error(`${args.path}: HTTP ${res.status}\n${text.slice(0, 2000)}`);
      // Modules the engine could not compile throw a build error in the browser: fail the build instead
      if (text.includes("__NATA_BUILD_ERROR__")) throw new Error(`${args.path} does not compile:\n${text.slice(0, 2000)}`);
      const type = (res.headers.get("content-type") ?? "").split(";")[0].trim();
      return { contents: text, loader: LOADERS[type] ?? "js" };
    });
  },
};

rmSync(config.outdir, { recursive: true, force: true });
const result = await Bun.build({
  entrypoints: config.entries,
  root: process.cwd(),
  outdir: config.outdir,
  target: "browser",
  format: "esm",
  splitting: true,
  minify: true,
  sourcemap: "none",
  naming: { entry: "[name]-[hash].[ext]", chunk: "chunks/[hash].[ext]", asset: "assets/[name]-[hash].[ext]" },
  define: { "process.env.NODE_ENV": '"production"' },
  metafile: true,
  throw: false,
  plugins: [engine],
}).catch((error: any) => {
  errors.push(String(error?.stack || error));
  return null;
});

const entries: Record<string, string> = {};
const imports: Record<string, string[]> = {};
const files: string[] = [];

if (result && !result.success) {
  for (const log of result.logs) errors.push(String(log.message ?? log));
}

if (result?.success) {
  const toUrl = (file: string) => config.publicPath + file.replace(/^\.\//, "").replaceAll("\\", "/");
  for (const [file, meta] of Object.entries((result as any).metafile.outputs as Record<string, any>)) {
    const url = toUrl(file);
    if (meta.entryPoint) entries[String(meta.entryPoint).replace(/^nata:/, "")] = url;
    // Import paths in the metafile are relative to `outdir`, like output keys
    imports[url] = (meta.imports ?? []).filter((i: any) => i.kind === "import-statement" && !i.external).map((i: any) => toUrl(i.path));
  }
  for (const output of result.outputs) files.push(relative(config.outdir, output.path).replaceAll("\\", "/"));
}

console.log("__NATA_BUNDLE__" + JSON.stringify({ entries, imports, files, inputs: [...inputs], errors }));
