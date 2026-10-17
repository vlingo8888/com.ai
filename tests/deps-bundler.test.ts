import { describe, expect, it } from "bun:test";
import { rewriteBareImports } from "../compiler-rs/src/deps_bundler";

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
