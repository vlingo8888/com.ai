// Com.AI.VN dependency pre-bundler, run in a one-off Bun process per package version (results are
// cached in ~/.com/cache/deps/ and linked into .nata/deps/), so Bun.build's memory never stays in the long-lived worker.
//
//   bun -e 'import { bundleDependency } from ".nata/deps_bundler.ts"; ...'

import { copyFileSync, existsSync, linkSync, mkdirSync, readdirSync, readFileSync, realpathSync, renameSync, statSync, unlinkSync, writeFileSync } from "fs";
import { basename, dirname, join, relative, resolve, sep } from "path";

const nataDir = join(process.cwd(), ".nata");

const IDENTIFIER = /^[A-Za-z_$][\w$]*$/;
// Import/export statements emitted by Bun.build, possibly spanning lines (`import {\n a,\n b\n} from "x"`)
// or minified (`import{a}from"x"`). Anchored to line starts and limited to import-clause characters, so
// string literals that merely contain `from "x"` (e.g. React's error messages) are left alone.
const BARE_STATIC_IMPORT = /^((?:import|export)\s*(?:[\w\s{},*$]*?\bfrom\s*)?)(["'])(?![a-z]+:)([^"'./][^"']*)\2/gm;
// Lazy chunks: `import("x")`
const BARE_DYNAMIC_IMPORT = /\bimport\(\s*(["'])(?![a-z]+:)([^"'./][^"']*)\1\s*\)/g;
// The same statements importing a relative path (`./react.js`)
const RELATIVE_STATIC_IMPORT = /^((?:import|export)\s*(?:[\w\s{},*$]*?\bfrom\s*)?)(["'])(\.\.?\/[^"']*)\2/gm;
const RELATIVE_DYNAMIC_IMPORT = /\bimport\(\s*(["'])(\.\.?\/[^"']*)\1\s*\)/g;

/**
 * CommonJS view of a dependency bundle for `require()` callers. A CJS package's bundle default-exports
 * its original `module.exports` (every named export is also a property of it): return that object, so
 * callers share and may mutate it exactly as in Node. Otherwise (an ESM package) return the namespace.
 * The conditional also keeps Bun from collapsing the shim back into a constant `import * as` binding.
 */
function requireShim(url: string): string {
  return [
    // A static import (not require) so Bun emits a real ESM import instead of a runtime `__require`
    `import * as m from ${JSON.stringify(url)};`,
    `const d = m.default;`,
    `const isCjs = d !== null && (typeof d === "object" || typeof d === "function") && Object.keys(m).every((k) => k === "default" || k in d);`,
    `module.exports = isCjs ? d : m;`,
  ].join("\n");
}

const importerSources = new Map<string, string>();

/** Whether `importer` loads `specifier` with a CommonJS `require("specifier")` call */
function requiresSpecifier(importer: string, specifier: string): boolean {
  if (!importer) return false;
  let source = importerSources.get(importer);
  if (source === undefined) {
    try {
      source = readFileSync(importer, "utf-8");
    } catch {
      source = "";
    }
    importerSources.set(importer, source);
  }
  return source.includes(`require("${specifier}")`) || source.includes(`require('${specifier}')`);
}

/** Points every bare specifier left external by Bun.build at the dev server's dependency endpoint */
export function rewriteBareImports(code: string): string {
  return code
    .replace(BARE_STATIC_IMPORT, (_m, prefix, quote, bare) => `${prefix}${quote}/_nata/deps/${bare}${quote}`)
    .replace(BARE_DYNAMIC_IMPORT, (_m, quote, bare) => `import(${quote}/_nata/deps/${bare}${quote})`);
}

/** Points relative imports left external (`./react.js`) at the package specifier exposing that file */
export function rewriteSiblingImports(code: string, siblings: Map<string, string>): string {
  if (siblings.size === 0) return code;
  const target = (rel: string) => siblings.get(rel);
  return code
    .replace(RELATIVE_STATIC_IMPORT, (m, prefix, quote, rel) => (target(rel) ? `${prefix}${quote}/_nata/deps/${target(rel)}${quote}` : m))
    .replace(RELATIVE_DYNAMIC_IMPORT, (m, quote, rel) => (target(rel) ? `import(${quote}/_nata/deps/${target(rel)}${quote})` : m));
}

/**
 * Bundles one package specifier for the browser. CommonJS packages get explicit named exports
 * (read from the module itself), and every other bare import stays external, rewritten to
 * `/_nata/deps/<specifier>` so shared packages such as React are loaded exactly once.
 */
/**
 * The file Bun.build picks for `spec` with browser conditions (package `exports`, `browser`, `module`
 * fields), found by letting Bun resolve it from a probe entry and recording the first file it loads.
 */
async function resolveForBrowser(spec: string, probe: string): Promise<string | null> {
  writeFileSync(probe, `export * from ${JSON.stringify(spec)};\n`);
  let resolved: string | null = null;
  await Bun.build({
    entrypoints: [probe],
    target: "browser",
    format: "esm",
    plugins: [
      {
        name: "nata-probe",
        setup(build) {
          build.onResolve({ filter: /^[^./]/ }, (args) => (args.importer === probe || args.path.startsWith("#") ? undefined : { path: args.path, external: true }));
          build.onLoad({ filter: /.*/ }, (args) => {
            if (!resolved && args.path !== probe) resolved = args.path;
            return undefined;
          });
        },
      },
    ],
  }).catch(() => null);
  return resolved;
}

/**
 * Where the packages of `resolveDir` are: its node_modules, or for a project on the shared store
 * (no node_modules) the links `COM_PACKAGES_DIR` names (store.rs)
 */
/** The project whose sources import the package: `resolveDir`, or the project of `<project>/.nata/modules` */
function projectOf(resolveDir: string): string {
  return basename(resolveDir) === "modules" && basename(dirname(resolveDir)) === ".nata" ? dirname(dirname(resolveDir)) : resolveDir;
}

export function modulesDir(resolveDir: string): string {
  const own = join(resolveDir, "node_modules");
  const store = process.env.COM_PACKAGES_DIR;
  return store && !existsSync(own) && resolve(resolveDir) === resolve(process.cwd()) ? store : own;
}

/** `<package>/<path to a file>` read from node_modules, whatever the package's `exports` say */
export function packageFile(spec: string, resolveDir: string): string | null {
  const [first, second] = spec.split("/");
  const depth = first.startsWith("@") ? 2 : 1;
  if (spec.split("/").length <= depth || spec.split("/").includes("..") || (depth === 2 && !second)) return null;
  const base = join(modulesDir(resolveDir), spec);
  for (const file of [base, base + ".js", base + ".mjs", base + ".cjs", join(base, "index.js")]) {
    try {
      if (statSync(file).isFile()) return file;
    } catch {}
  }
  return null;
}

/**
 * Whether `file` is an ES module. A file that only re-exports (`export * from "d3-shape"`, as
 * victory-vendor does) lists no named exports, so `import` statements count too: wrapping such a
 * file as CommonJS loses the re-exported namespace (`ReferenceError: Ae is not defined`).
 */
export function isEsmModule(file: string): boolean {
  if (file.endsWith(".mjs")) return true;
  if (file.endsWith(".cjs")) return false;
  try {
    const loader = /\.[mc]?tsx?$/.test(file) ? "tsx" : "jsx";
    const { exports, imports } = new Bun.Transpiler({ loader }).scan(readFileSync(file, "utf-8"));
    return exports.length > 0 || imports.some((i) => i.kind === "import-statement");
  } catch {
    return false;
  }
}

/**
 * Named exports of a CommonJS file read from its source (`exports.x = ...`), following
 * `module.exports = require("./...")` re-exports: for browser-only packages that cannot be imported on
 * the server to list their exports (e.g. react-server-dom-webpack/client.browser)
 */
function cjsExportNames(file: string, nodeEnv: string, seen = new Set<string>()): string[] {
  if (seen.has(file)) return [];
  seen.add(file);
  let source = "";
  try {
    source = readFileSync(file, "utf-8");
  } catch {
    return [];
  }
  const names = new Set<string>();
  for (const [, name] of source.matchAll(/\bexports\.([A-Za-z_$][\w$]*)\s*=/g)) names.add(name);
  const reexports = [...source.matchAll(/module\.exports\s*=\s*require\(\s*["'](\.[^"']+)["']\s*\)/g)].map((m) => m[1]);
  const preferred = reexports.filter((r) => (nodeEnv === "production" ? !/development/.test(r) : !/production/.test(r)));
  for (const rel of preferred.length ? preferred : reexports) {
    try {
      const target = Bun.resolveSync(rel, file.slice(0, file.lastIndexOf("/")));
      for (const name of cjsExportNames(target, nodeEnv, seen)) names.add(name);
    } catch {}
  }
  return [...names].filter((n) => n !== "default" && n !== "__esModule");
}

/**
 * Files a package exposes as its own specifiers (`use-intl/react` → `.../dist/esm/development/react.js`),
 * read from package.json `exports` with the browser conditions. Each bundle keeps the others external,
 * so a module shared by two entries (use-intl's React context, imported by `use-intl` and `use-intl/react`)
 * exists once in the browser instead of once per bundle.
 */
export function packageEntries(spec: string, resolveDir: string, nodeEnv: string): Map<string, string> {
  const entries = new Map<string, string>();
  const parts = spec.split("/");
  const name = parts.slice(0, spec.startsWith("@") ? 2 : 1).join("/");
  let pkgDir: string;
  let exports: unknown;
  try {
    pkgDir = realpathSync(join(modulesDir(resolveDir), name));
    exports = JSON.parse(readFileSync(join(pkgDir, "package.json"), "utf-8")).exports;
  } catch {
    return entries;
  }
  if (!exports || typeof exports !== "object" || Array.isArray(exports)) return entries;
  const conditions = new Set(["browser", "import", "module", nodeEnv === "production" ? "production" : "development", "default"]);
  const pick = (target: unknown): string | null => {
    if (typeof target === "string") return target;
    if (Array.isArray(target)) return target.map(pick).find((t) => t !== null) ?? null;
    if (target && typeof target === "object") {
      for (const [key, value] of Object.entries(target)) {
        if (conditions.has(key)) {
          const picked = pick(value);
          if (picked !== null) return picked;
        }
      }
    }
    return null;
  };
  // `exports: { "import": ..., "default": ... }` is the package root alone
  const map = Object.keys(exports).every((k) => k.startsWith(".")) ? (exports as Record<string, unknown>) : { ".": exports };
  const wildcards: [string, string][] = [];
  for (const [subpath, target] of Object.entries(map)) {
    if (subpath === "./package.json") continue;
    const file = pick(target);
    if (!file || !/\.[mc]?js$/.test(file)) continue;
    if (subpath.includes("*")) {
      if (subpath.split("*").length === 2 && file.split("*").length === 2) wildcards.push([subpath, file]);
      continue;
    }
    try {
      entries.set(realpathSync(join(pkgDir, file)), subpath === "." ? name : name + subpath.slice(1));
    } catch {}
  }
  if (wildcards.length > 0) addWildcardEntries(entries, name, pkgDir, resolveDir, wildcards);
  return entries;
}

/** Above this many files a used wildcard (`lucide-react/icons/*`) shares only the files imported by name */
const WILDCARD_EXPAND_LIMIT = 500;
const SOURCE_FILE = /\.(?:[mc]?[jt]sx?)$/;
const SKIPPED_DIRS = new Set(["node_modules", ".nata", ".git", ".next", "dist", "build", "out", "coverage"]);

function sourceFiles(dir: string, out: string[], limit: number, skip: Set<string>) {
  let items: import("fs").Dirent[];
  try {
    items = readdirSync(dir, { withFileTypes: true });
  } catch {
    return;
  }
  for (const item of items) {
    if (out.length >= limit) return;
    const path = join(dir, item.name);
    if (item.isDirectory()) {
      if (!skip.has(item.name) && !item.name.startsWith(".")) sourceFiles(path, out, limit, skip);
    } else if (SOURCE_FILE.test(item.name) && !item.name.endsWith(".d.ts")) {
      out.push(path);
    }
  }
}

/**
 * Subpath specifiers of `name` (`konva/lib/Core.js`) imported by the project's sources and by the
 * installed packages that depend on `name` (react-konva)
 */
function importedSubpaths(name: string, resolveDir: string): Set<string> {
  const files: string[] = [];
  sourceFiles(projectOf(resolveDir), files, 20_000, SKIPPED_DIRS);
  const packagesDir = modulesDir(resolveDir);
  let packages: string[] = [];
  try {
    packages = readdirSync(packagesDir).flatMap((p) => (p.startsWith("@") ? readdirSync(join(packagesDir, p)).map((s) => `${p}/${s}`) : [p]));
  } catch {}
  for (const pkg of packages) {
    try {
      const json = JSON.parse(readFileSync(join(packagesDir, pkg, "package.json"), "utf-8"));
      const deps = { ...json.dependencies, ...json.peerDependencies, ...json.optionalDependencies };
      if (pkg !== name && name in deps) sourceFiles(realpathSync(join(packagesDir, pkg)), files, files.length + 5_000, new Set(["node_modules"]));
    } catch {}
  }
  const escaped = name.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const pattern = new RegExp(`["'](${escaped}/[^"'\\s]+)["']`, "g");
  const found = new Set<string>();
  for (const file of files) {
    let code: string;
    try {
      code = readFileSync(file, "utf-8");
    } catch {
      continue;
    }
    if (!code.includes(name + "/")) continue;
    for (const match of code.matchAll(pattern)) found.add(match[1]);
  }
  return found;
}

/**
 * Wildcard `exports` (`"./lib/*": "./lib/*.js"`) that something imports by subpath: konva's own entry
 * pulls in `./_CoreInternals.js` while react-konva imports `konva/lib/Core.js`, so unless every file of the
 * pattern is one shared module, the browser gets two Konva objects and two sets of Node classes. A
 * wildcard nothing imports adds nothing, a large one only the files imported by name.
 */
function addWildcardEntries(entries: Map<string, string>, name: string, pkgDir: string, resolveDir: string, wildcards: [string, string][]) {
  const used = [...importedSubpaths(name, resolveDir)];
  for (const [subpath, target] of wildcards) {
    const [subPrefix, subSuffix] = subpath.slice(1).split("*");
    const [fileMatchPrefix, fileSuffix] = target.split("*");
    const matched = used.filter((spec) => {
      const rest = spec.slice(name.length);
      return rest.startsWith(subPrefix) && rest.endsWith(subSuffix) && rest.length >= subPrefix.length + subSuffix.length;
    });
    if (matched.length === 0) continue;
    const specFor = (file: string) => {
      const rel = "./" + relative(pkgDir, file).split(sep).join("/");
      if (!rel.startsWith(fileMatchPrefix) || !rel.endsWith(fileSuffix)) return null;
      return name + subPrefix + rel.slice(fileMatchPrefix.length, rel.length - fileSuffix.length) + subSuffix;
    };
    const files: string[] = [];
    sourceFiles(join(pkgDir, dirname(fileMatchPrefix + "x")), files, WILDCARD_EXPAND_LIMIT + 1, new Set(["node_modules"]));
    const all = files.length <= WILDCARD_EXPAND_LIMIT;
    for (const file of files) {
      const spec = specFor(file);
      if (!spec || (!all && !matched.includes(spec))) continue;
      const real = realpathSync(file);
      // One specifier per file: the spelling dependents use (`konva/lib/Core.js`, not `konva/lib/Core`)
      if (!entries.has(real) || matched.includes(spec)) entries.set(real, spec);
    }
  }
}

/**
 * Bundles one package specifier for the browser. Every other bare import stays external, rewritten to
 * `/_nata/deps/<specifier>`, so shared packages such as React are loaded exactly once.
 *
 * - ESM packages are bundled from their own entry file, which keeps their exports exactly as written
 *   (including `export * from "<other package>"`, which Bun mishandles inside a wrapper module).
 * - CommonJS packages get a wrapper entry with explicit named exports (read from the module itself)
 *   and a default export of `module.exports`.
 *
 * `rolldown` is the path of Rolldown's module (installed by the engine, see deps.rs); without it the
 * package is bundled with Bun.build.
 */
export async function bundleDependency(input: {
  spec: string;
  outFile: string;
  sharedFile?: string;
  nodeEnv?: string;
  resolveDir?: string;
  rolldown?: string;
}) {
  const { spec, outFile } = input;
  const nodeEnv = input.nodeEnv || "development";
  // The framework's React (see framework_react.rs) is resolved from its own install directory
  const resolveDir = input.resolveDir || process.cwd();

  const entryDir = input.resolveDir ? join(input.resolveDir, ".entries") : join(nataDir, "deps", ".entries");
  mkdirSync(entryDir, { recursive: true });
  const base = join(entryDir, spec.replace(/[^\w.-]/g, "_"));
  const probe = base + ".probe.js";
  const wrapper = base + ".js";

  try {
    // A client component inside a package is referenced by its file (`next-intl/dist/.../X.js`), which the
    // package's `exports` may not expose: that file is the entry then
    const direct = packageFile(spec, resolveDir);
    const resolved = (await resolveForBrowser(spec, probe)) ?? direct;
    const esm = resolved !== null && isEsmModule(resolved);
    const target = resolved !== null && resolved === direct ? direct : spec;
    const siblings = esm ? packageEntries(spec, resolveDir, nodeEnv) : new Map<string, string>();

    // Another project already built this exact bundle: link to it
    const shared = input.sharedFile ? sharedBundleFile(input.sharedFile, siblings) : null;
    if (shared && existsSync(shared)) {
      linkBundle(shared, outFile);
      return { bytes: statSync(shared).size, format: esm ? "esm" : "cjs", shared: true };
    }

    let names: string[] = [];
    if (!esm) {
      try {
        const mod = await import(target === spec ? Bun.resolveSync(spec, resolveDir) : target);
        names = Object.keys(mod).filter((n) => n !== "default" && IDENTIFIER.test(n));
      } catch {
        // Browser-only packages may throw on the server; read their exports from the source instead
      }
      if (names.length === 0 && resolved) names = cjsExportNames(resolved, nodeEnv).filter((n) => IDENTIFIER.test(n));
      const literal = JSON.stringify(target);
      writeFileSync(
        wrapper,
        `import * as __m from ${literal};\nexport * from ${literal};\n` +
          `export default (__m.default !== undefined ? __m.default : __m);\n` +
          names.map((n) => `export const ${n} = __m.${n};`).join("\n") + "\n",
      );
    }
    const entry = esm ? resolved! : wrapper;
    const entryFile = esm ? realpathSync(resolved!) : "";

    const build = { spec, entry, esm, wrapper, nodeEnv, siblings, entryFile };
    const code = input.rolldown ? await buildWithRolldown(input.rolldown, build) : await buildWithBun(build);
    if (shared) {
      // Written under a temporary name and renamed, so a concurrent reader never sees half a file
      mkdirSync(dirname(shared), { recursive: true });
      const temp = `${shared}.${process.pid}.${Date.now()}.tmp`;
      writeFileSync(temp, code);
      renameSync(temp, shared);
      linkBundle(shared, outFile);
    } else {
      mkdirSync(join(outFile, ".."), { recursive: true });
      writeFileSync(outFile, code);
    }
    return { bytes: code.length, format: esm ? "esm" : "cjs", exports: names.length, bundler: input.rolldown ? "rolldown" : "bun" };
  } finally {
    for (const file of [probe, wrapper]) {
      try { unlinkSync(file); } catch {}
    }
  }
}

/**
 * The machine-wide file of a bundle. A bundle that imports files of its package as other specifiers
 * (siblings) depends on the project when those come from wildcard exports (konva's `./lib/*` is shared
 * only where react-konva is installed), so the sibling set is part of its name
 */
export function sharedBundleFile(sharedFile: string, siblings: Map<string, string>): string {
  if (siblings.size === 0) return sharedFile;
  const specs = [...siblings.values()].sort().join("\n");
  const fingerprint = new Bun.CryptoHasher("sha256").update(specs).digest("hex").slice(0, 12);
  return sharedFile.replace(/\.js$/, `~${fingerprint}.js`);
}

/** The project's copy of a shared bundle: a hard link (no extra disk space), or a copy across volumes */
function linkBundle(shared: string, outFile: string) {
  mkdirSync(dirname(outFile), { recursive: true });
  try { unlinkSync(outFile); } catch {}
  try {
    linkSync(shared, outFile);
  } catch {
    copyFileSync(shared, outFile);
  }
}

interface BuildInput {
  spec: string;
  entry: string;
  esm: boolean;
  wrapper: string;
  nodeEnv: string;
  /** Files of the package exposed as other specifiers (see packageEntries) */
  siblings: Map<string, string>;
  entryFile: string;
}

/**
 * Rolldown keeps the id a plugin gives an external import, so bare imports and sibling files point at
 * `/_nata/deps/` directly, with no rewrite of the output afterwards.
 */
async function buildWithRolldown(modulePath: string, b: BuildInput): Promise<string> {
  const { rolldown } = await import(modulePath);
  const bundle = await rolldown({
    input: b.entry,
    platform: "browser",
    // An import nothing resolves is an error (Rolldown would leave it external), other warnings are noise
    onLog(level: string, log: { code?: string; message: string }) {
      if (log.code === "UNRESOLVED_IMPORT") throw new Error(log.message);
    },
    resolve: { conditionNames: ["browser", "import", "module", b.nodeEnv === "production" ? "production" : "development", "default"] },
    transform: { define: { "process.env.NODE_ENV": JSON.stringify(b.nodeEnv) } },
    plugins: [
      {
        name: "nata-external-deps",
        // Filters run in Rust: only these ids cross into JavaScript (lucide-react has ~4,700 imports)
        resolveId: {
          filter: { id: /^(?:[^./\0]|\/_nata\/deps\/)/ },
          handler(id: string, importer: string | undefined, opts: { kind: string }) {
            // Already pointing at the dev server (emitted by the require shim below)
            if (id.startsWith("/_nata/deps/")) return { id, external: "absolute" };
            if (!importer || id.startsWith("node:")) return null;
            // `#name` is the package's own subpath import (package.json `imports`): part of the package
            if (id.startsWith("#")) return null;
            // The CommonJS wrapper bundles the package itself; everything else is another dependency
            if (!b.esm && importer === b.wrapper) return null;
            // `require("react")` must stay a plain variable (CommonJS code may reassign it), so requires
            // go through a CommonJS shim that Rolldown wraps in `__commonJSMin`
            if (opts.kind === "require-call") return { id: "\0nata-require:" + id };
            return { id: "/_nata/deps/" + id, external: "absolute" };
          },
        },
        load: {
          filter: { id: /^\0nata-require:/ },
          handler(id: string) {
            return requireShim("/_nata/deps/" + id.slice("\0nata-require:".length));
          },
        },
      },
      ...siblingPlugin(b),
    ],
  });
  try {
    const { output } = await bundle.generate({ format: "esm", codeSplitting: false, minify: b.nodeEnv === "production" });
    return output[0].code;
  } finally {
    await bundle.close();
  }
}

/**
 * Relative imports of a file the package also exports as another specifier, for Rolldown. Only ids
 * ending in one of those files' names reach JavaScript, and they are resolved synchronously.
 */
function siblingPlugin(b: BuildInput) {
  const files = [...b.siblings.keys()].filter((file) => file !== b.entryFile && b.siblings.get(file) !== b.spec);
  if (files.length === 0) return [];
  const escape = (text: string) => text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  // `./react.js` or `./react` for `.../react.js`
  const names = [...new Set(files.map((file) => basename(file).replace(/\.[mc]?js$/, "")))].map(escape);
  const filter = new RegExp(`^\\.\\.?/(?:.*/)?(?:${names.join("|")})(?:\\.[mc]?js)?$`);
  return [
    {
      name: "nata-sibling-entries",
      resolveId: {
        filter: { id: filter },
        handler(id: string, importer: string | undefined, opts: { kind: string }) {
          if (!importer || opts.kind === "require-call") return null;
          const base = join(dirname(importer), id);
          for (const candidate of [base, base + ".js", base + ".mjs", base + ".cjs"]) {
            let file: string;
            try {
              if (!statSync(candidate).isFile()) continue;
              file = realpathSync(candidate);
            } catch {
              continue;
            }
            const sibling = files.includes(file) ? b.siblings.get(file) : undefined;
            return sibling ? { id: "/_nata/deps/" + sibling, external: "absolute" } : null;
          }
          return null;
        },
      },
    },
  ];
}

async function buildWithBun(b: BuildInput): Promise<string> {
  const { spec, entry, esm, wrapper, nodeEnv, siblings, entryFile } = b;
  const siblingImports = new Map<string, string>();
  const result = await Bun.build({
    entrypoints: [entry],
    target: "browser",
    format: "esm",
    // Whitespace is kept so emitted import statements start at line beginnings (see BARE_STATIC_IMPORT)
    minify: { syntax: true, identifiers: true, whitespace: false },
    define: { "process.env.NODE_ENV": JSON.stringify(nodeEnv) },
    plugins: [
      {
        name: "nata-external-deps",
        setup(build) {
          // Already pointing at the dev server (emitted by the require shim below)
          build.onResolve({ filter: /^\/_nata\/deps\// }, (args) => ({ path: args.path, external: true }));
          // A relative import of a file the package also exports as another specifier. Bun keeps an
          // external's original text, so the specifier is rewritten after the build (see
          // rewriteSiblingImports); one that would mean two different files stays bundled
          build.onResolve({ filter: /^\.\.?\// }, (args) => {
            if (siblings.size === 0 || args.kind === "require-call") return undefined;
            let file: string;
            try {
              file = realpathSync(Bun.resolveSync(args.path, dirname(args.importer)));
            } catch {
              return undefined;
            }
            const sibling = siblings.get(file);
            if (!sibling || sibling === spec || file === entryFile) return undefined;
            const taken = siblingImports.get(args.path);
            if (taken !== undefined && taken !== sibling) return undefined;
            siblingImports.set(args.path, sibling);
            return { path: args.path, external: true };
          });
          build.onResolve({ filter: /^[^./]/ }, (args) => {
            if (args.path.startsWith("node:")) return undefined;
            // `#name` is the package's own subpath import (package.json `imports`): part of the package
            if (args.path.startsWith("#")) return undefined;
            // The CommonJS wrapper bundles the package itself; everything else is another dependency
            if (!esm && args.importer === wrapper) return undefined;
            // `require("react")` must stay a plain variable: Bun turns a require of an external into
            // `import * as X` (a constant binding), and CommonJS code may reassign it (React's dev
            // jsx-runtime does `React = {...}`). Route requires through a CommonJS shim instead: Bun
            // wraps it in `__commonJS`, so the caller gets the exports in an ordinary variable.
            // Bun reports these as "import-statement", so look at the importer's source
            if (args.kind === "require-call" || requiresSpecifier(args.importer, args.path)) {
              return { path: args.path, namespace: "nata-require" };
            }
            return { path: args.path, external: true };
          });
          build.onLoad({ filter: /.*/, namespace: "nata-require" }, (args) => ({
            contents: requireShim("/_nata/deps/" + args.path),
            loader: "js",
          }));
        },
      },
    ],
  });
  if (!result.success) {
    throw new Error(result.logs.map((l) => String(l.message || l)).join("\n"));
  }
  const code = rewriteBareImports(rewriteSiblingImports(await result.outputs[0].text(), siblingImports));
  // Production builds: whitespace goes too, once imports are rewritten (the rewrite needs lines)
  return nodeEnv === "production" ? new Bun.Transpiler({ loader: "js", minifyWhitespace: true }).transformSync(code) : code;
}

/**
 * Long-lived bundler process (see deps.rs): one JSON request per stdin line, `{ id, input }`, answered
 * by one line on stdout, `{ id, ok, result | error }`. Requests run concurrently, and Bun (or Rolldown,
 * whose native module takes ~30ms to load) starts once instead of once per package. The process exits
 * after `idleMs` without work, so the bundlers' memory is not kept for the whole `com dev` session.
 */
export async function serveBundler(options: { idleMs?: number } = {}) {
  const idleMs = options.idleMs ?? 30_000;
  // stdout carries replies only
  const write = (line: string) => process.stdout.write(line + "\n");
  console.log = console.info = console.debug = (...args: unknown[]) => console.error(...args);
  let running = 0;
  let idleTimer: ReturnType<typeof setTimeout> | undefined;
  const armIdle = () => {
    clearTimeout(idleTimer);
    if (running === 0) idleTimer = setTimeout(() => process.exit(0), idleMs);
  };
  armIdle();
  let buffered = "";
  for await (const chunk of process.stdin) {
    buffered += chunk.toString();
    let newline: number;
    while ((newline = buffered.indexOf("\n")) !== -1) {
      const line = buffered.slice(0, newline).trim();
      buffered = buffered.slice(newline + 1);
      if (!line) continue;
      let request: { id: number; input: Parameters<typeof bundleDependency>[0] };
      try {
        request = JSON.parse(line);
      } catch {
        continue;
      }
      running++;
      clearTimeout(idleTimer);
      bundleDependency(request.input)
        .then((result) => write(JSON.stringify({ id: request.id, ok: true, result })))
        .catch((error) => write(JSON.stringify({ id: request.id, ok: false, error: String(error?.stack || error) })))
        .finally(() => {
          running--;
          armIdle();
        });
    }
  }
  // The engine closed stdin: finish what is running, then exit
  const wait = () => (running === 0 ? process.exit(0) : setTimeout(wait, 50));
  wait();
}
