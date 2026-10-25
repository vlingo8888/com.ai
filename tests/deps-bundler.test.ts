import { afterAll, describe, expect, it } from "bun:test";
import { existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, realpathSync, rmSync, statSync, writeFileSync } from "fs";
import { tmpdir } from "os";
import { join } from "path";
import { bundleDependency, isEsmModule, packageFile, rewriteBareImports } from "../compiler-rs/src/deps_bundler";

describe("deps bundler: rewriteBareImports", () => {
  it("rewrites single-line, minified and side-effect imports", () => {
    const code = [
      'import * as Q from "react";',
      'import{a as b}from"scheduler";',
      'import "some-polyfill";',
      'export * from "@scope/pkg/sub";',
      'export { x } from "other";',
    ].join("\n");
    expect(rewriteBareImports(code)).toBe(
      [
        'import * as Q from "/_nata/deps/react";',
        'import{a as b}from"/_nata/deps/scheduler";',
        'import "/_nata/deps/some-polyfill";',
        'export * from "/_nata/deps/@scope/pkg/sub";',
        'export { x } from "/_nata/deps/other";',
      ].join("\n"),
    );
  });

  it("rewrites import statements that span several lines (Bun.build output for long import lists)", () => {
    const code = 'import {\n  FocusScope as X,\n  Root as Y\n} from "@radix-ui/react-focus-scope";\nvar a = 1;';
    expect(rewriteBareImports(code)).toContain('} from "/_nata/deps/@radix-ui/react-focus-scope";');
  });

  it("rewrites dynamic imports of packages", () => {
    expect(rewriteBareImports('const m = await import("lazy-pkg");')).toBe('const m = await import("/_nata/deps/lazy-pkg");');
  });

  it("leaves relative, absolute, URL and node: specifiers alone", () => {
    const code = ['import a from "./local";', 'import b from "/_nata/deps/react";', 'import c from "https://cdn.example/x.js";', 'import d from "node:fs";'].join("\n");
    expect(rewriteBareImports(code)).toBe(code);
  });

  it("does not touch strings that only look like imports", () => {
    const code = `console.error('You should instead import it from "react-dom/client".');`;
    expect(rewriteBareImports(code)).toBe(code);
  });
});

describe("deps bundler: isEsmModule", () => {
  const dir = mkdtempSync(join(tmpdir(), "com-esm-"));
  afterAll(() => rmSync(dir, { recursive: true, force: true }));
  const file = (name: string, code: string) => {
    const path = join(dir, name);
    writeFileSync(path, code);
    return path;
  };

  it("treats a file that only re-exports another package as ESM (victory-vendor/d3-shape)", () => {
    expect(isEsmModule(file("reexport.js", '// vendored\nexport * from "d3-shape";\n'))).toBe(true);
    expect(isEsmModule(file("side-effect.js", 'import "./polyfill";\n'))).toBe(true);
    expect(isEsmModule(file("named.js", "export const a = 1;\n"))).toBe(true);
  });

  it("keeps CommonJS files CommonJS", () => {
    expect(isEsmModule(file("cjs.js", 'const d3 = require("d3-shape");\nmodule.exports = d3;\n'))).toBe(false);
    expect(isEsmModule(file("cjs-dynamic.js", 'module.exports = () => import("./lazy.js");\n'))).toBe(false);
    expect(isEsmModule(file("x.cjs", "export const a = 1;\n"))).toBe(false);
  });
});

describe("deps bundler: packageFile", () => {
  const dir = mkdtempSync(join(tmpdir(), "deps-pkgfile-"));
  afterAll(() => rmSync(dir, { recursive: true, force: true }));
  mkdirSync(join(dir, "node_modules/next-intl/dist/shared"), { recursive: true });
  mkdirSync(join(dir, "node_modules/@scope/pkg/lib"), { recursive: true });
  writeFileSync(join(dir, "node_modules/next-intl/dist/shared/Provider.js"), "export {};");
  writeFileSync(join(dir, "node_modules/@scope/pkg/lib/x.mjs"), "export {};");

  it("finds files inside a package that its exports do not list", () => {
    expect(packageFile("next-intl/dist/shared/Provider.js", dir)).toBe(join(dir, "node_modules/next-intl/dist/shared/Provider.js"));
    expect(packageFile("next-intl/dist/shared/Provider", dir)).toBe(join(dir, "node_modules/next-intl/dist/shared/Provider.js"));
    expect(packageFile("@scope/pkg/lib/x", dir)).toBe(join(dir, "node_modules/@scope/pkg/lib/x.mjs"));
  });

  it("ignores package roots, missing files and parent paths", () => {
    expect(packageFile("next-intl", dir)).toBeNull();
    expect(packageFile("@scope/pkg", dir)).toBeNull();
    expect(packageFile("next-intl/dist/missing", dir)).toBeNull();
    expect(packageFile("next-intl/../../etc/passwd", dir)).toBeNull();
  });
});

// Rolldown runs when COM_TEST_ROLLDOWN is the path of its module (installed by the engine in ~/.com/rolldown)
const BUNDLERS: Array<[string, { rolldown?: string }]> = [["Bun.build", {}]];
if (process.env.COM_TEST_ROLLDOWN) BUNDLERS.push(["Rolldown", { rolldown: process.env.COM_TEST_ROLLDOWN }]);

describe.each(BUNDLERS)("deps bundler: bundleDependency (%s)", (_name, bundler) => {
  const dir = realpathSync(mkdtempSync(join(tmpdir(), "deps-imports-")));
  afterAll(() => rmSync(dir, { recursive: true, force: true }));
  // vfile's shape: `#minpath` picks a browser file through package.json `imports`
  mkdirSync(join(dir, "node_modules/vf/lib"), { recursive: true });
  writeFileSync(
    join(dir, "node_modules/vf/package.json"),
    JSON.stringify({ name: "vf", type: "module", exports: "./index.js", imports: { "#minpath": { node: "./lib/minpath.js", default: "./lib/minpath.browser.js" } } }),
  );
  writeFileSync(join(dir, "node_modules/vf/index.js"), 'export { minpath } from "#minpath";\n');
  writeFileSync(join(dir, "node_modules/vf/lib/minpath.js"), 'export const minpath = "node";\n');
  writeFileSync(join(dir, "node_modules/vf/lib/minpath.browser.js"), 'export const minpath = "browser";\n');

  it("bundles the package's own #subpath imports instead of fetching /_nata/deps/#name", async () => {
    const outFile = join(dir, "out/vf.js");
    await bundleDependency({ spec: "vf", outFile, resolveDir: dir, ...bundler });
    const code = readFileSync(outFile, "utf-8");
    expect(code).not.toContain("/_nata/deps/#");
    expect(code).toContain('"browser"');
  });

  // use-intl's shape: `.` re-exports react.js, which `./react` also exposes (it owns a React context)
  mkdirSync(join(dir, "node_modules/ui/dist"), { recursive: true });
  writeFileSync(
    join(dir, "node_modules/ui/package.json"),
    JSON.stringify({ name: "ui", type: "module", exports: { ".": { development: "./dist/index.js", default: "./dist/index.prod.js" }, "./react": { development: "./dist/react.js", default: "./dist/react.prod.js" } } }),
  );
  writeFileSync(join(dir, "node_modules/ui/dist/index.js"), 'export { Ctx } from "./react.js";\nexport { helper } from "./helper.js";\n');
  writeFileSync(join(dir, "node_modules/ui/dist/react.js"), 'export const Ctx = { id: Math.random() };\n');
  writeFileSync(join(dir, "node_modules/ui/dist/helper.js"), 'export const helper = 1;\n');

  it("imports a file the package exports as another specifier from that specifier (one shared module)", async () => {
    const outFile = join(dir, "out/ui.js");
    await bundleDependency({ spec: "ui", outFile, resolveDir: dir, ...bundler });
    const code = readFileSync(outFile, "utf-8");
    expect(code).toContain('from "/_nata/deps/ui/react"');
    expect(code).not.toContain("Math.random");
    // Files no other specifier exposes are still bundled
    expect(code).not.toContain("helper.js\"");

    const reactFile = join(dir, "out/ui-react.js");
    await bundleDependency({ spec: "ui/react", outFile: reactFile, resolveDir: dir, ...bundler });
    expect(readFileSync(reactFile, "utf-8")).toContain("Math.random");
  });

  // konva's shape: wildcard `./lib/*` exports, the root reaches lib/_Core.js, a binding (react-konva)
  // imports `kv/lib/Core.js`, which reaches the same lib/_Core.js
  mkdirSync(join(dir, "node_modules/kv/lib"), { recursive: true });
  writeFileSync(
    join(dir, "node_modules/kv/package.json"),
    JSON.stringify({ name: "kv", type: "module", exports: { ".": "./lib/index.js", "./lib/*": "./lib/*.js", "./lib/*.js": "./lib/*.js" } }),
  );
  writeFileSync(join(dir, "node_modules/kv/lib/index.js"), 'import { Kv } from "./_Full.js";\nexport default Kv;\n');
  writeFileSync(join(dir, "node_modules/kv/lib/_Full.js"), 'import { Kv } from "./_Core.js";\nKv.Rect = 1;\nexport { Kv };\n');
  writeFileSync(join(dir, "node_modules/kv/lib/_Core.js"), "export const Kv = { id: Math.random() };\n");
  writeFileSync(join(dir, "node_modules/kv/lib/Core.js"), 'import { Kv } from "./_Core.js";\nexport default Kv;\n');
  mkdirSync(join(dir, "node_modules/react-kv"), { recursive: true });
  writeFileSync(join(dir, "node_modules/react-kv/package.json"), JSON.stringify({ name: "react-kv", type: "module", exports: "./index.js", peerDependencies: { kv: "*" } }));
  writeFileSync(join(dir, "node_modules/react-kv/index.js"), 'import "kv";\nimport Kv from "kv/lib/Core.js";\nexport { Kv };\n');

  it("shares the files of a wildcard export that a dependent imports by subpath (one Konva object)", async () => {
    const rootFile = join(dir, "out/kv.js");
    await bundleDependency({ spec: "kv", outFile: rootFile, resolveDir: dir, ...bundler });
    const root = readFileSync(rootFile, "utf-8");
    expect(root).toContain("/_nata/deps/kv/lib/_Full");
    expect(root).not.toContain("Math.random");

    const coreFile = join(dir, "out/kv-core.js");
    await bundleDependency({ spec: "kv/lib/Core.js", outFile: coreFile, resolveDir: dir, ...bundler });
    const core = readFileSync(coreFile, "utf-8");
    expect(core).toContain("/_nata/deps/kv/lib/_Core");
    expect(core).not.toContain("Math.random");
  });

  it("links the machine-wide copy of a bundle into every project instead of bundling again", async () => {
    const sharedFile = join(dir, "shared/ui@1.0.0/index.js");
    const first = join(dir, "projectA/.nata/deps/ui.js");
    await bundleDependency({ spec: "vf", outFile: first, sharedFile, resolveDir: dir, ...bundler });
    expect(existsSync(sharedFile)).toBe(true);
    expect(statSync(first).ino).toBe(statSync(sharedFile).ino);

    const second = join(dir, "projectB/.nata/deps/ui.js");
    const result = await bundleDependency({ spec: "vf", outFile: second, sharedFile, resolveDir: dir, ...bundler });
    expect(result.shared).toBe(true);
    expect(statSync(second).ino).toBe(statSync(sharedFile).ino);
  });

  it("keeps bundles whose siblings depend on the project apart in the shared cache (kv with and without react-kv)", async () => {
    const sharedFile = join(dir, "shared/kv@1.0.0/index.js");
    await bundleDependency({ spec: "kv", outFile: join(dir, "projectA/kv.js"), sharedFile, resolveDir: dir, ...bundler });
    const withBinding = readdirSync(join(dir, "shared/kv@1.0.0"));
    expect(withBinding).toHaveLength(1);
    expect(withBinding[0]).toMatch(/^index~[0-9a-f]{12}\.js$/);

    rmSync(join(dir, "node_modules/react-kv"), { recursive: true });
    await bundleDependency({ spec: "kv", outFile: join(dir, "projectB/kv.js"), sharedFile, resolveDir: dir, ...bundler });
    expect(readdirSync(join(dir, "shared/kv@1.0.0"))).toHaveLength(2);
    expect(readFileSync(join(dir, "projectB/kv.js"), "utf-8")).toContain("Math.random");
  });

  // lucide-react's shape: no `exports`, "sideEffects": false, icons built by a shared factory that reads
  // a React context. The compiler imports icons by file (swc_compiler/barrel.rs): each is its own bundle
  mkdirSync(join(dir, "node_modules/glyphs/dist/icons"), { recursive: true });
  writeFileSync(join(dir, "node_modules/glyphs/package.json"), JSON.stringify({ name: "glyphs", module: "./dist/index.mjs", sideEffects: false }));
  writeFileSync(join(dir, "node_modules/glyphs/dist/index.mjs"), "export { default as Star } from './icons/star.mjs';\nexport { Ctx } from './context.mjs';\n");
  writeFileSync(join(dir, "node_modules/glyphs/dist/context.mjs"), "export const Ctx = { id: Math.random() };\n");
  writeFileSync(join(dir, "node_modules/glyphs/dist/factory.mjs"), "import { Ctx } from './context.mjs';\nexport default (name) => ({ name, Ctx });\n");
  writeFileSync(join(dir, "node_modules/glyphs/dist/icons/star.mjs"), "import make from '../factory.mjs';\nexport default make('star');\n");

  it("keeps the files of a side-effect-free package without exports separate modules when one is imported by path", async () => {
    const outFile = join(dir, "out/glyphs-star.js");
    await bundleDependency({ spec: "glyphs/dist/icons/star.mjs", outFile, resolveDir: dir, ...bundler });
    const star = readFileSync(outFile, "utf-8");
    expect(star).toContain('from "/_nata/deps/glyphs/dist/factory.mjs"');
    expect(star).not.toContain("Math.random");

    const factory = join(dir, "out/glyphs-factory.js");
    await bundleDependency({ spec: "glyphs/dist/factory.mjs", outFile: factory, resolveDir: dir, ...bundler });
    expect(readFileSync(factory, "utf-8")).toContain('from "/_nata/deps/glyphs/dist/context.mjs"');

    // The package entry is still one bundle
    const entry = join(dir, "out/glyphs.js");
    await bundleDependency({ spec: "glyphs", outFile: entry, resolveDir: dir, ...bundler });
    expect(readFileSync(entry, "utf-8")).toContain("Math.random");
  });
});
