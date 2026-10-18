import { afterAll, describe, expect, it } from "bun:test";
import { mkdtempSync, rmSync, writeFileSync } from "fs";
import { tmpdir } from "os";
import { join } from "path";
import { isEsmModule, rewriteBareImports } from "../compiler-rs/src/deps_bundler";

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
