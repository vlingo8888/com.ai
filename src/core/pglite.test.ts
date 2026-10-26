import { describe, it, expect } from "bun:test";
import { join } from "path";
import { loadPGliteClass, packageEntry, requireFromProject } from "./pglite";

const REPO = join(import.meta.dir, "../..");

describe("PGlite loader: packageEntry", () => {
  it("follows exports, its conditions, then main", () => {
    expect(packageEntry({ exports: { ".": "./dist/index.js" }, main: "x.js" })).toBe("./dist/index.js");
    expect(packageEntry({ exports: { ".": { import: "./esm.js", require: "./cjs.js" } } })).toBe("./cjs.js");
    expect(packageEntry({ exports: { ".": { node: "./n.js", default: { import: "./d.mjs" } } } })).toBe("./d.mjs");
    expect(packageEntry({ exports: "./only.js" })).toBe("./only.js");
    expect(packageEntry({ main: "lib/main.js" })).toBe("lib/main.js");
    expect(packageEntry({})).toBe("index.js");
  });
});

describe("PGlite loader: loadPGliteClass", () => {
  it("loads the project's @pglite/core by the path of its entry", () => {
    const pkg = requireFromProject(REPO, "@pglite/core");
    expect(typeof (pkg.PGLite || pkg.PGLiteNative)).toBe("function");
  });

  it("returns a constructor that runs queries", async () => {
    const PGlite = loadPGliteClass(REPO);
    expect(PGlite).not.toBeNull();
    const db = new PGlite();
    expect(await db.query("SELECT 1 AS x")).toEqual([{ x: 1 }]);
    await db.close?.();
  });

  it("throws for a package the project does not have", () => {
    expect(() => requireFromProject(REPO, "not-a-real-package-xyz")).toThrow();
  });
});
