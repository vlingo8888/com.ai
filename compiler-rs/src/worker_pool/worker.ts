// Com.AI.VN persistent Bun worker.
//
// One long-lived process per project serves SSR renders, Server Actions and route handlers for the
// Rust engine. Requests run concurrently; everything request-scoped (cookies, Set-Cookie list, query
// log, route params, request URL, captured console output) lives in an AsyncLocalStorage store, exposed
// through the same `globalThis.__NATA_*` names the runtimes and `core` already use. Like Next.js, there
// is no `window`/`document` on the server.
//
// Transport: binary frames on stdin/stdout (stdout carries nothing else; stray output goes to stderr).
//   u32 LE length (of everything after it) | u8 kind | u32 LE id | u32 LE meta length | meta JSON | body
// Kinds: 1 request (engine -> worker), 2 result, 3 chunk (part of a streamed result, before it),
// 4 ready, 5 shutdown, 6 call (worker -> engine, e.g. load a module), 7 reply (to a call).
// Request meta: {"cmd": "ssr" | "rsc" | "action" | "route" | "rpc" | "eval", "input": {...}}; body = raw
// request body. Result meta: {"ok": bool, "output": ..., "code": n, "logs": {"stdout", "stderr"},
// "error"?}; body = raw response body (route handlers).
//
// Modules: `rsc:/abs/file.tsx?v=2` and `ssr:/abs/file.tsx` specifiers are loaded through the engine
// (see `module_server.rs`), compiled for the React Server Components layer (`react-server` condition)
// or the SSR layer. The two layers are separate module instances in this one process.

import { AsyncLocalStorage } from "node:async_hooks";
import { existsSync, mkdirSync, unlinkSync, writeFileSync } from "fs";
import { dirname, join } from "path";
import { inspect } from "util";
import { explainServerError } from "./explain_error.ts";
import { serialImport } from "./serial_import.ts";

(globalThis as any).__nata_import = serialImport;

const KIND_REQUEST = 1;
const KIND_RESULT = 2;
const KIND_CHUNK = 3;
const KIND_READY = 4;
const KIND_SHUTDOWN = 5;
const KIND_CALL = 6;
const KIND_REPLY = 7;

const nataDir = join(process.cwd(), ".nata");

interface Store {
  cmd: string;
  stdout: string[];
  stderr: string[];
  exitCode: number | null;
  globals: Record<string, unknown>;
}

const als = new AsyncLocalStorage<Store>();

// ---------- output: stdout is reserved for frames ----------

const realStdoutWrite = process.stdout.write.bind(process.stdout);
const realStderrWrite = process.stderr.write.bind(process.stderr);
const realConsoleError = console.error.bind(console);
const realExit = process.exit.bind(process);

// Runtimes loaded by the worker (route handlers, Server Actions) rewrite their errors with it too
(globalThis as any).__NATA_EXPLAIN_ERROR__ = explainServerError;

const format = (args: unknown[]) =>
  args.map(explainServerError).map((a) => (typeof a === "string" ? a : inspect(a, { colors: false, depth: 6 }))).join(" ") + "\n";
const toText = (chunk: unknown) =>
  typeof chunk === "string" ? chunk : chunk instanceof Uint8Array ? new TextDecoder().decode(chunk) : String(chunk);

function captureWrite(target: "stdout" | "stderr") {
  return ((chunk: unknown, ...rest: any[]) => {
    const store = als.getStore();
    const cb = rest.find((r) => typeof r === "function");
    if (store) {
      store[target].push(toText(chunk));
      if (cb) queueMicrotask(cb);
      return true;
    }
    // Outside a request: never touch stdout, it carries the protocol
    return realStderrWrite(chunk as any, ...rest);
  }) as typeof process.stdout.write;
}
process.stdout.write = captureWrite("stdout");
process.stderr.write = captureWrite("stderr");

for (const level of ["log", "info", "debug"] as const) {
  console[level] = (...args: unknown[]) => {
    const store = als.getStore();
    if (store) store.stdout.push(format(args));
    else realStderrWrite(format(args));
  };
}
for (const level of ["warn", "error", "trace"] as const) {
  console[level] = (...args: unknown[]) => {
    const store = als.getStore();
    if (store) store.stderr.push(format(args));
    else realStderrWrite(format(args));
  };
}

// Code calling process.exit inside a request ends that request, not the worker
process.exit = ((code?: number) => {
  const store = als.getStore();
  if (store) {
    store.exitCode = typeof code === "number" ? code : 0;
    return undefined as never;
  }
  return realExit(code);
}) as typeof process.exit;

// ---------- request-scoped globals ----------

const REQUEST_GLOBALS = ["__NATA_COOKIES__", "__NATA_SET_COOKIES__", "__NATA_QUERY_LOGS__", "__NATA_PARAMS__", "__NATA_LOCATION__", "__NATA_REVALIDATED__", "__NATA_HEADERS__", "__NATA_AFTER__", "__NATA_SEGMENT_CONFIG__", "__NATA_INTL_LOCALE__", "__NATA_RENDER__"] as const;
const freshGlobals = (): Record<string, unknown> => ({
  __NATA_COOKIES__: new Map<string, string>(),
  __NATA_SET_COOKIES__: [],
  __NATA_QUERY_LOGS__: [],
  __NATA_PARAMS__: {},
  __NATA_LOCATION__: { pathname: "/", search: "", href: "http://localhost:3000/" },
  __NATA_REVALIDATED__: false,
  __NATA_HEADERS__: {},
  __NATA_AFTER__: [],
  __NATA_SEGMENT_CONFIG__: {},
  __NATA_INTL_LOCALE__: null,
  __NATA_RENDER__: null,
});
const processGlobals = freshGlobals();

for (const key of REQUEST_GLOBALS) {
  Object.defineProperty(globalThis, key, {
    configurable: true,
    get() {
      const store = als.getStore();
      return store ? store.globals[key] : processGlobals[key];
    },
    set(value) {
      const store = als.getStore();
      if (store) store.globals[key] = value;
      else processGlobals[key] = value;
    },
  });
}

// Identity of the request being served (the store object itself), e.g. for re-entrant DB locking
Object.defineProperty(globalThis, "__NATA_REQUEST__", {
  configurable: true,
  get: () => als.getStore(),
});

(globalThis as any).__NATA_WORKER_MODE__ = true;

// ---------- CSS Modules on the server ----------
// Class names must match what the engine's Lightning CSS emits for the browser: `<local>_<fnv1a(path)>`
// (see `css_module_suffix` in stylesheet.rs), so server-rendered markup hydrates with the right classes.

function fnv1a(text: string): string {
  let hash = 0x811c9dc5;
  for (const byte of new TextEncoder().encode(text)) {
    hash = Math.imul(hash ^ byte, 0x01000193) >>> 0;
  }
  return hash.toString(16).padStart(8, "0");
}

function cssModuleExports(source: string, suffix: string): Record<string, string> {
  const css = source.replace(/\/\*[\s\S]*?\*\//g, "").replace(/url\([^)]*\)/g, "").replace(/"[^"]*"|'[^']*'/g, '""');
  const exports: Record<string, string> = {};
  for (const [, local] of css.matchAll(/\.(-?[_a-zA-Z][\w-]*)/g)) {
    exports[local] ??= `${local}_${suffix}`;
  }
  // `composes: a b;` inside `.x { }` adds the composed classes after x's own name
  for (const [, selector, body] of css.matchAll(/\.(-?[_a-zA-Z][\w-]*)\s*\{([^}]*)\}/g)) {
    const composes = body.match(/composes\s*:\s*([^;]+?)(?:\s+from\s+[^;]+)?;/);
    if (composes) {
      const names = composes[1].trim().split(/\s+/).map((n) => `${n}_${suffix}`);
      exports[selector] = [exports[selector], ...names].join(" ");
    }
  }
  return exports;
}

Bun.plugin({
  name: "nata-css-modules",
  setup(build) {
    build.onLoad({ filter: /\.module\.css$/ }, async (args) => {
      const { readFileSync } = await import("fs");
      const { relative, sep } = await import("path");
      const rel = relative(process.cwd(), args.path).split(sep).join("/");
      const exports = cssModuleExports(readFileSync(args.path, "utf-8"), fnv1a(rel));
      return { contents: `export default ${JSON.stringify(exports)};`, loader: "js" };
    });
  },
});

// ---------- next/font on the server ----------
// `import { Inter } from "next/font/google"` needs a named export for every font the project uses, which
// a fixed shim cannot have. When Bun loads the shim, generate it from the fonts imported anywhere in the
// project (scanned once per worker; code changes restart the worker), built like the browser transform
// (`ImportResolver::transform_next_fonts`: className from the local binding name) so hydration agrees.

const NEXT_FONT_IMPORT = /\bimport\s+(?:type\s+)?([^;'"]*?)\s+from\s*['"]next\/font\/(?:google|local)['"]/gs;

function fontFactory(local: string): string {
  const css = local.toLowerCase().replace(/_/g, "-");
  const family = local.replace(/_/g, " ");
  return `(opts = {}) => ({ className: "font-${css}", variable: (opts && opts.variable) ? opts.variable : "--font-${css}", style: { fontFamily: "${family}, -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif" } })`;
}

/** imported name -> local binding name, plus the local name of a default import, for one source file */
export function fontImports(source: string): { named: Map<string, string>; defaults: string[] } {
  const named = new Map<string, string>();
  const defaults: string[] = [];
  for (const [, clause] of source.matchAll(NEXT_FONT_IMPORT)) {
    const braces = clause.match(/\{([\s\S]*)\}/);
    for (const part of braces ? braces[1].split(",") : []) {
      const m = part.trim().replace(/^type\s+/, "").match(/^([\w$]+)(?:\s+as\s+([\w$]+))?$/);
      if (m && !named.has(m[1])) named.set(m[1], m[2] ?? m[1]);
    }
    const defaultName = clause.replace(/\{[\s\S]*\}/, "").replace(/,/g, " ").trim().split(/\s+/)[0];
    if (defaultName && /^[\w$]+$/.test(defaultName)) defaults.push(defaultName);
  }
  return { named, defaults };
}

let nextFontShim: string | null = null;

async function buildNextFontShim(): Promise<string> {
  const { readdirSync, readFileSync, statSync } = await import("fs");
  const named = new Map<string, string>();
  let defaultName: string | null = null;
  const walk = (dir: string) => {
    let entries: string[] = [];
    try { entries = readdirSync(dir); } catch { return; }
    for (const name of entries) {
      if (name === "node_modules" || name.startsWith(".")) continue;
      const path = join(dir, name);
      let stat;
      try { stat = statSync(path); } catch { continue; }
      if (stat.isDirectory()) walk(path);
      else if (/\.(tsx|ts|jsx|js|mjs)$/.test(name)) {
        const source = readFileSync(path, "utf-8");
        if (!source.includes("next/font/")) continue;
        const found = fontImports(source);
        for (const [imported, local] of found.named) if (!named.has(imported)) named.set(imported, local);
        defaultName ??= found.defaults[0] ?? null;
      }
    }
  };
  walk(process.cwd());
  const lines = [...named].map(([imported, local]) => `export const ${imported} = ${fontFactory(local)};`);
  lines.push(`export default ${fontFactory(defaultName ?? "Local_Font")};`);
  return lines.join("\n") + "\n";
}

Bun.plugin({
  name: "nata-next-font",
  setup(build) {
    build.onLoad({ filter: /[\\/]node_modules[\\/]next[\\/]font[\\/](?:google|local)[\\/]index\.js$/ }, async () => {
      nextFontShim ??= await buildNextFontShim();
      return { contents: nextFontShim, loader: "js" };
    });
  },
});

// ---------- next/* in packages Bun loads natively ----------
// With the real `next` installed (the app also runs with `next dev`), node_modules/next is left alone, so
// packages loaded natively (next-intl in the SSR layer) would get Next.js internals that need its app
// router: their files load as the framework's modules instead, as the engine's resolver aliases do
// (next_shims.rs SERVER_MODULES). Same for next-intl/server (its non react-server build), imported by the
// project's i18n/request.ts that the next-intl/server shim imports natively. Bun's runtime `onResolve`
// does not see bare imports of native files, so this replaces what the package files load (synchronously,
// so `require()` works too).

/** module -> [file in .nata, has a default export] */
const NEXT_SERVER_MODULES: Record<string, [string, boolean]> = {
  headers: ["next/headers.ts", true], server: ["next/server.ts", true], navigation: ["next/navigation.ts", true],
  cache: ["next/cache.ts", true], og: ["next/og.ts", false], dynamic: ["next/dynamic.js", true],
  script: ["next/script.js", true], image: ["next/image.js", true], link: ["next/link.js", true],
  router: ["next/router.js", true], head: ["next/head.js", true],
};

function reexport(file: string, hasDefault: boolean) {
  const target = JSON.stringify(join(process.cwd(), ".nata", file));
  return { contents: `export * from ${target};\n` + (hasDefault ? `export { default } from ${target};\n` : ""), loader: "js" as const };
}

// The project's packages: node_modules, or the shared store's links (COM_PACKAGES_DIR, store.rs)
if (existsSync(join(existsSync(join(process.cwd(), "node_modules")) ? join(process.cwd(), "node_modules") : process.env.COM_PACKAGES_DIR || join(process.cwd(), "node_modules"), "next/dist"))) {
  Bun.plugin({
    name: "nata-next-modules",
    setup(build) {
      build.onLoad({ filter: /[\\/]node_modules[\\/]next[\\/](headers|server|navigation|cache|og|dynamic|script|image|link|router|head)\.js$/ }, (args) => {
        const name = args.path.replace(/^.*[\\/]/, "").replace(/\.js$/, "");
        return reexport(...NEXT_SERVER_MODULES[name]);
      });
      build.onLoad({ filter: /[\\/]node_modules[\\/]next-intl[\\/]dist[\\/][^\\/]+[\\/][^\\/]+[\\/]server\.react-client\.js$/ }, () =>
        reexport("next-intl-server.ts", true),
      );
    },
  });
}

// ---------- module layers (rsc: / ssr:), loaded through the engine ----------

const engineCalls = new Map<number, (reply: { meta: any; body: Uint8Array }) => void>();
let nextCallId = 1;

/** Asks the engine (CALL frame) and waits for its REPLY */
export function callEngine(cmd: string, input: unknown): Promise<{ meta: any; body: Uint8Array }> {
  const id = nextCallId++;
  return new Promise((resolve) => {
    engineCalls.set(id, resolve);
    sendFrame(KIND_CALL, id, { cmd, input });
  });
}
(globalThis as any).__NATA_CALL_ENGINE__ = callEngine;

function buildError(meta: any): Error {
  const err: any = new Error(meta?.error?.message ?? meta?.message ?? "module failed to load");
  if (meta?.error) err.__nataBuildError = meta.error;
  return err;
}

// CommonJS files of a server layer (React, Flight, most packages): the engine sends the whole `require`
// graph, resolved with the layer's conditions; evaluated here, one instance per file and layer
const cjsModules = new Map<string, { exports: any; loaded: boolean }>();
const cjsSources = new Map<string, any>();
const esmForRequire = new Map<string, any>();

function layerRequire(layer: string, file: string): any {
  const key = layer + ":" + file;
  const cached = cjsModules.get(key);
  if (cached) return cached.exports;
  const entry = cjsSources.get(key);
  if (!entry) throw new Error(`[${layer}] module not loaded: ${file}`);
  if (entry.json) {
    const mod = { exports: JSON.parse(entry.code), loaded: true };
    entry.code = null;
    cjsModules.set(key, mod);
    return mod.exports;
  }
  const mod = { exports: {} as any, loaded: false };
  cjsModules.set(key, mod);
  const dirname = file.slice(0, file.lastIndexOf("/"));
  const localRequire = (spec: string) => {
    const target = entry.requires?.[spec];
    if (!target) return nodeRequire(spec, dirname);
    if (target.builtin) return nodeRequire(target.builtin, dirname);
    if (target.file) return layerRequire(layer, target.file);
    if (target.esm) return esmForRequire.get(target.esm);
    throw new Error(`Cannot find module '${spec}' from '${file}' (${layer} layer)`);
  };
  const fn = new Function("module", "exports", "require", "__filename", "__dirname", entry.code + "\n//# sourceURL=" + layer + ":" + file);
  // The source text is not needed once compiled (React's development builds are megabytes)
  entry.code = null;
  fn(mod, mod.exports, localRequire, file, dirname);
  mod.loaded = true;
  return mod.exports;
}

function nodeRequire(spec: string, dirname: string) {
  const { createRequire } = require("node:module");
  return createRequire(dirname + "/")(spec);
}

function cjsNamespace(exports: any) {
  const ns: Record<string, unknown> = { default: exports };
  if (exports && (typeof exports === "object" || typeof exports === "function")) {
    for (const key of Object.keys(exports)) if (key !== "default") ns[key] = exports[key];
    if (exports.__esModule && "default" in exports) ns.default = exports.default;
  }
  return ns;
}

for (const layer of ["rsc", "ssr"] as const) {
  Bun.plugin({
    name: `nata-${layer}`,
    setup(build) {
      build.onResolve({ filter: /.*/, namespace: layer }, (args) => ({ path: args.path, namespace: layer }));
      build.onLoad({ filter: /.*/, namespace: layer }, async (args) => {
        const { meta, body } = await callEngine("load_module", { path: args.path, layer });
        if (meta.kind === "esm") return { contents: decoder.decode(body), loader: "js" };
        if (meta.kind === "cjs") {
          for (const [file, entry] of Object.entries<any>(meta.files)) {
            // Sources come one after the other in the body; files this layer already has are not decoded
            if (!cjsSources.has(layer + ":" + file)) {
              entry.code = decoder.decode(body.subarray(entry.offset, entry.offset + entry.length));
              cjsSources.set(layer + ":" + file, entry);
            }
            for (const target of Object.values<any>(entry.requires ?? {})) {
              if (target.esm && !esmForRequire.has(target.esm)) esmForRequire.set(target.esm, await import(target.esm));
            }
          }
          return { exports: cjsNamespace(layerRequire(layer, meta.entry)), loader: "object" };
        }
        throw buildError(meta);
      });
    },
  });
}

// ---------- commands ----------

interface Handled {
  output?: unknown;
  code?: number;
  body?: Uint8Array;
  /** Streamed response: `head` goes in the first chunk, then the body chunks, then the result */
  head?: unknown;
  stream?: ReadableStream<Uint8Array>;
}

const DELIMITED = /__NATA_[A-Z]+_OUT_START__[\s\S]*?__NATA_[A-Z]+_OUT_END__\n?/g;
let evalCounter = 0;

async function runCommand(cmd: string, input: any, body: Uint8Array): Promise<Handled> {
  switch (cmd) {
    case "ssr": {
      const { runSsr } = await import(join(nataDir, "ssr_runtime.ts"));
      return { output: await runSsr(input) };
    }
    case "rsc": {
      const { renderRoute } = await import(join(nataDir, "rsc_render.ts"));
      return await renderRoute(input);
    }
    case "static_params": {
      const rsc = await import(input.runtime);
      return { output: await rsc.staticParams(input.specifiers) };
    }
    case "instrumentation": {
      // instrumentation.ts: register() once per worker, before any request; onRequestError for later
      const mod = await import(input.specifier);
      if (typeof mod.register === "function") await mod.register();
      (globalThis as any).__NATA_ON_REQUEST_ERROR__ = mod.onRequestError;
      return { output: { registered: typeof mod.register === "function", onRequestError: typeof mod.onRequestError === "function" } };
    }
    case "next_config": {
      const { loadNextConfig } = await import(join(nataDir, "next", "config-runtime.ts"));
      return { output: await loadNextConfig(input) };
    }
    case "fetch_bytes": {
      // Remote images for the optimizer (the engine has no HTTP client)
      const res = await fetch(input.url, { redirect: "follow", headers: input.headers ?? {} });
      return { output: { ok: res.ok, status: res.status, contentType: res.headers.get("content-type") }, body: new Uint8Array(await res.arrayBuffer()) };
    }
    case "proxy": {
      // External rewrites (next.config): forward the request, return the response as is
      const res = await fetch(input.url, { method: input.method, headers: input.headers, body: ["GET", "HEAD"].includes(input.method) ? undefined : body, redirect: "manual" });
      const headers: [string, string][] = [];
      res.headers.forEach((value: string, key: string) => {
        if (!["content-encoding", "content-length", "transfer-encoding", "connection"].includes(key)) headers.push([key, value]);
      });
      return { output: { status: res.status, headers }, body: new Uint8Array(await res.arrayBuffer()) };
    }
    case "middleware_config": {
      const { middlewareConfig } = await import(join(nataDir, "next", "middleware-runtime.ts"));
      return { output: await middlewareConfig(input) };
    }
    case "middleware": {
      const { runMiddleware } = await import(join(nataDir, "next", "middleware-runtime.ts"));
      return await runMiddleware(input);
    }
    case "action": {
      const { runAction } = await import(join(nataDir, "rsc_render.ts"));
      return await runAction(input, body);
    }
    case "route": {
      const { handleRoute } = await import(join(nataDir, "route_handler_runtime.ts"));
      const result = await handleRoute({ ...input, bodyBytes: body });
      if ("head" in result) return result;
      return { output: result.output, body: result.body };
    }
    case "rpc": {
      const { runRpc } = await import(join(nataDir, "rpc_runtime.ts"));
      const result = await runRpc(input);
      return { output: result.payload, code: result.code };
    }
    case "css":
      return { output: await buildStylesheet(input) };
    case "eval": {
      // Arbitrary runner script (tests, one-off tooling); the only command that touches the disk
      const dir = join(nataDir, "worker-scripts");
      mkdirSync(dir, { recursive: true });
      const file = join(dir, `eval-${process.pid}-${++evalCounter}.ts`);
      writeFileSync(file, String(input.script ?? ""));
      try {
        await import(file);
        await new Promise((resolve) => setTimeout(resolve, 0));
        const scriptCode = (globalThis as any).__NATA_EXIT_CODE__;
        return { code: typeof scriptCode === "number" ? scriptCode : 0 };
      } finally {
        try { unlinkSync(file); } catch {}
      }
    }
    default:
      throw new Error(`Unknown worker command: ${cmd}`);
  }
}

// ---------- Tailwind CSS (served inline in the page and at /_nata/styles.css) ----------

let stylesheetCache: { input: string; dependencies: string[]; stamp: string; css: string } | null = null;

async function loadTailwind() {
  try {
    // A project on the shared store resolves its packages from `.nata/modules` (store.rs)
    const from = process.env.COM_PACKAGES_DIR ? dirname(process.env.COM_PACKAGES_DIR) : process.cwd();
    const node = await import(Bun.resolveSync("@tailwindcss/node", from));
    const oxide = await import(Bun.resolveSync("@tailwindcss/oxide", from));
    return { compile: node.compile, optimize: node.optimize, Scanner: oxide.Scanner };
  } catch {
    // The engine installs the packages and retries
    throw new Error("TAILWIND_MISSING");
  }
}

/**
 * Compiles the project stylesheet the way `@tailwindcss/cli` does: compile the CSS, scan the project
 * with Oxide for class candidates, build. Cached until the CSS or one of its imports changes (code
 * changes restart the worker, which drops the cache).
 */
async function buildStylesheet(input: { entry: string | null; css: string }) {
  const { statSync } = await import("fs");
  const { dirname } = await import("path");
  const cwd = process.cwd();
  // mtimes of the CSS files the entry imports: editing any of them invalidates the cache
  const stamp = (files: string[]) =>
    files.map((f) => { try { return f + ":" + statSync(f).mtimeMs; } catch { return f; } }).join("|");

  if (stylesheetCache && stylesheetCache.input === input.css && stylesheetCache.stamp === stamp(stylesheetCache.dependencies)) {
    return { css: stylesheetCache.css, cached: true };
  }

  const tw = await loadTailwind();
  const dependencies = new Set<string>();
  const from = input.entry || join(cwd, "app", "globals.css");
  const compiler = await tw.compile(input.css, { base: input.entry ? dirname(input.entry) : cwd, from, onDependency: (p: string) => dependencies.add(p) });
  const sources =
    compiler.root === "none" ? [] : compiler.root === null ? [{ base: cwd, pattern: "**/*", negated: false }] : [{ ...compiler.root, negated: false }];
  // Never scan generated output (dependency bundles in .nata/ would add thousands of bogus candidates)
  const ignored = [".nata/**", "node_modules/**", "data/**"].map((pattern) => ({ base: cwd, pattern, negated: true }));
  const scanner = new tw.Scanner({ sources: [...sources, ...compiler.sources, ...ignored] });
  const built = compiler.build(scanner.scan());
  const css = tw.optimize(built, { file: from, minify: process.env.NODE_ENV === "production" }).code;

  const depList = [...dependencies];
  stylesheetCache = { input: input.css, dependencies: depList, stamp: stamp(depList), css };
  return { css, cached: false };
}

async function handleRequest(id: number, meta: any, body: Uint8Array) {
  const store: Store = { cmd: meta.cmd, stdout: [], stderr: [], exitCode: null, globals: freshGlobals() };
  let result: Record<string, unknown>;
  let responseBody: Uint8Array = new Uint8Array();
  try {
    const handled = await als.run(store, async () => {
      const handled = await runCommand(meta.cmd, meta.input ?? {}, body);
      if (handled.head !== undefined) {
        sendFrame(KIND_CHUNK, id, { head: handled.head });
        if (handled.stream) for await (const chunk of handled.stream as any) sendFrame(KIND_CHUNK, id, {}, chunk);
        warnRepeatedQueries(store);
      }
      return handled;
    });
    responseBody = handled.body ?? responseBody;
    const code = store.exitCode ?? handled.code ?? 0;
    result = { ok: true, output: handled.output ?? null, code };
    // Streamed pages: whether the render used dynamic APIs, its revalidate time and cache tags
    // (prerendering at build time and the page cache of `com start`, see next/request.ts)
    if (handled.head !== undefined && store.globals.__NATA_RENDER__) result.render = store.globals.__NATA_RENDER__;
  } catch (err: any) {
    explainServerError(err);
    store.stderr.push(String(err?.stack || err?.message || err) + "\n");
    result = { ok: false, code: 1, error: String(err?.message || err) };
  }
  result.logs = { stdout: store.stdout.join("").replace(DELIMITED, ""), stderr: store.stderr.join("") };
  sendFrame(KIND_RESULT, id, result, responseBody);
  await runAfterCallbacks(store);
  scheduleIdleGc();
}

/** `after()` callbacks: run in the request's context once its response has been sent */
async function runAfterCallbacks(store: Store) {
  const queue = store.globals.__NATA_AFTER__ as Array<() => unknown>;
  if (!Array.isArray(queue) || queue.length === 0) return;
  await als.run(store, async () => {
    while (queue.length) {
      const task = queue.shift()!;
      try {
        await task();
      } catch (error: any) {
        realStderrWrite(`⨯ Error in after(): ${error?.stack || error}\n`);
      }
    }
  });
  const logs = store.stdout.join("") + store.stderr.join("");
  if (logs.trim()) realStderrWrite(logs);
}

/**
 * Dev hint: the same SQL statement run more than 5 times while rendering one page is usually an N+1
 * query (a query per list item); suggest batching it or wrapping the reader in React.cache()
 */
function warnRepeatedQueries(store: Store) {
  const logs = store.globals.__NATA_QUERY_LOGS__ as Array<{ sql?: string }> | undefined;
  if (!Array.isArray(logs) || logs.length <= 5) return;
  const counts = new Map<string, number>();
  for (const log of logs) if (log && typeof log.sql === "string") counts.set(log.sql, (counts.get(log.sql) ?? 0) + 1);
  for (const [sql, count] of counts) {
    if (count > 5) {
      store.stderr.push(
        `⚠ The same query ran ${count} times while rendering this page (N+1?). Load the rows in one query, or wrap the data loader in React.cache():\n    ${sql.replace(/\s+/g, " ").slice(0, 200)}\n`,
      );
    }
  }
}

// A full collection once requests stop for a moment: rendering leaves garbage (Flight and HTML
// buffers, compiled module sources) that the incremental collector keeps around for a long time.
// Only when the heap grew since the last one, and at most every GC_MIN_INTERVAL: under steady traffic a
// stop-the-world collection after every burst cost CPU for little memory
const GC_MIN_INTERVAL = 10_000;
const GC_MIN_GROWTH = 8 * 1024 * 1024;
let idleGcTimer: ReturnType<typeof setTimeout> | null = null;
let lastGcAt = 0;
let heapAfterGc = 0;
function scheduleIdleGc() {
  if (idleGcTimer) clearTimeout(idleGcTimer);
  const wait = Math.max(1000, lastGcAt + GC_MIN_INTERVAL - Date.now());
  idleGcTimer = setTimeout(() => {
    idleGcTimer = null;
    if (process.env.COM_WORKER_GC !== "full" && process.memoryUsage().heapUsed - heapAfterGc < GC_MIN_GROWTH) return;
    Bun.gc(true);
    lastGcAt = Date.now();
    heapAfterGc = process.memoryUsage().heapUsed;
  }, wait);
  (idleGcTimer as any).unref?.();
}

// ---------- framing ----------

const encoder = new TextEncoder();
const decoder = new TextDecoder();

function sendFrame(kind: number, id: number, meta: unknown, body: Uint8Array = new Uint8Array()) {
  const metaBytes = encoder.encode(JSON.stringify(meta));
  const frame = new Uint8Array(4 + 1 + 4 + 4 + metaBytes.length + body.length);
  const view = new DataView(frame.buffer);
  view.setUint32(0, frame.length - 4, true);
  view.setUint8(4, kind);
  view.setUint32(5, id, true);
  view.setUint32(9, metaBytes.length, true);
  frame.set(metaBytes, 13);
  frame.set(body, 13 + metaBytes.length);
  realStdoutWrite(frame);
}

process.on("uncaughtException", (err) => {
  explainServerError(err);
  const store = als.getStore();
  if (store) store.stderr.push(String((err as any)?.stack || err) + "\n");
  else realConsoleError(err);
});
process.on("unhandledRejection", (err) => {
  explainServerError(err);
  const store = als.getStore();
  if (store) store.stderr.push(String((err as any)?.stack || err) + "\n");
  else realConsoleError(err);
});

sendFrame(KIND_READY, 0, { pid: process.pid });

let buffer = new Uint8Array(0);
const reader = Bun.stdin.stream().getReader();
while (true) {
  const { value, done } = await reader.read();
  if (done) break;
  const merged = new Uint8Array(buffer.length + value.length);
  merged.set(buffer);
  merged.set(value, buffer.length);
  buffer = merged;

  while (buffer.length >= 4) {
    const view = new DataView(buffer.buffer, buffer.byteOffset, buffer.byteLength);
    const length = view.getUint32(0, true);
    if (buffer.length < 4 + length) break;
    const kind = view.getUint8(4);
    const id = view.getUint32(5, true);
    const metaLength = view.getUint32(9, true);
    const meta = JSON.parse(decoder.decode(buffer.subarray(13, 13 + metaLength)));
    const body = buffer.slice(13 + metaLength, 4 + length);
    buffer = buffer.slice(4 + length);

    if (kind === KIND_SHUTDOWN) realExit(0);
    if (kind === KIND_REQUEST) handleRequest(id, meta, body);
    if (kind === KIND_REPLY) {
      const resolve = engineCalls.get(id);
      engineCalls.delete(id);
      resolve?.({ meta, body });
    }
  }
}

// stdin closed: the engine exited, so do not linger (an open database would keep the event loop alive)
realExit(0);
