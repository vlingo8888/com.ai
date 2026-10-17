// Com.AI.VN dependency pre-bundler, run in a one-off Bun process per package version (results are
// cached in .nata/deps/), so Bun.build's memory never stays in the long-lived worker.
//
//   bun -e 'import { bundleDependency } from ".nata/deps_bundler.ts"; ...'

import { mkdirSync, readFileSync, unlinkSync, writeFileSync } from "fs";
import { join } from "path";

const nataDir = join(process.cwd(), ".nata");

const IDENTIFIER = /^[A-Za-z_$][\w$]*$/;
// Import/export statements emitted by Bun.build, possibly spanning lines (`import {\n a,\n b\n} from "x"`)
// or minified (`import{a}from"x"`). Anchored to line starts and limited to import-clause characters, so
// string literals that merely contain `from "x"` (e.g. React's error messages) are left alone.
const BARE_STATIC_IMPORT = /^((?:import|export)\s*(?:[\w\s{},*$]*?\bfrom\s*)?)(["'])(?![a-z]+:)([^"'./][^"']*)\2/gm;
// Lazy chunks: `import("x")`
const BARE_DYNAMIC_IMPORT = /\bimport\(\s*(["'])(?![a-z]+:)([^"'./][^"']*)\1\s*\)/g;

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
          build.onResolve({ filter: /^[^./]/ }, (args) => (args.importer === probe ? undefined : { path: args.path, external: true }));
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

function isEsmModule(file: string): boolean {
  if (file.endsWith(".mjs")) return true;
  if (file.endsWith(".cjs")) return false;
  try {
    const loader = /\.[mc]?tsx?$/.test(file) ? "tsx" : "jsx";
    return new Bun.Transpiler({ loader }).scan(readFileSync(file, "utf-8")).exports.length > 0;
  } catch {
    return false;
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
 */
export async function bundleDependency(input: { spec: string; outFile: string; nodeEnv?: string }) {
  const { spec, outFile } = input;
  const nodeEnv = input.nodeEnv || "development";

  const entryDir = join(nataDir, "deps", ".entries");
  mkdirSync(entryDir, { recursive: true });
  const base = join(entryDir, spec.replace(/[^\w.-]/g, "_"));
  const probe = base + ".probe.js";
  const wrapper = base + ".js";

  const resolved = await resolveForBrowser(spec, probe);
  const esm = resolved !== null && isEsmModule(resolved);

  let names: string[] = [];
  if (!esm) {
    try {
      const mod = await import(Bun.resolveSync(spec, process.cwd()));
      names = Object.keys(mod).filter((n) => n !== "default" && IDENTIFIER.test(n));
    } catch {
      // Browser-only packages may throw on the server; `export *` in the wrapper still applies
    }
    const literal = JSON.stringify(spec);
    writeFileSync(
      wrapper,
      `import * as __m from ${literal};\nexport * from ${literal};\n` +
        `export default (__m.default !== undefined ? __m.default : __m);\n` +
        names.map((n) => `export const ${n} = __m.${n};`).join("\n") + "\n",
    );
  }
  const entry = esm ? resolved! : wrapper;

  try {
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
            build.onResolve({ filter: /^[^./]/ }, (args) => {
              if (args.path.startsWith("node:")) return undefined;
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
    const code = rewriteBareImports(await result.outputs[0].text());
    mkdirSync(join(outFile, ".."), { recursive: true });
    writeFileSync(outFile, code);
    return { bytes: code.length, format: esm ? "esm" : "cjs", exports: names.length };
  } finally {
    for (const file of [probe, wrapper]) {
      try { unlinkSync(file); } catch {}
    }
  }
}
