import { describe, it, expect, beforeEach, afterEach } from "bun:test";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "fs";
import { tmpdir } from "os";
import { join } from "path";
import { parseLockfile } from "../store/lockfile";
import { configWantsStandalone, createStandalone, installPath, productionPaths, shipsProjectFile } from "./standalone";

const LOCK = `{
  "lockfileVersion": 2,
  "workspaces": {
    "": {
      "name": "app",
      "dependencies": { "@pglite/core": "^1.0.0", "react-dom": "^19.0.0", "react": "^19.0.0" },
      "devDependencies": { "@types/bun": "latest", "kysely": "^0.29.0" },
    },
  },
  "packages": {
    "@pglite/core": ["@pglite/core@1.0.0", "", { "dependencies": { "mitata": "^1.0.34" }, "peerDependencies": { "typescript": "^5" } }, "sha512-a"],
    "@types/bun": ["@types/bun@1.4.4", "", { "dependencies": { "bun-types": "1.4.4" } }, "sha512-b"],
    "bun-types": ["bun-types@1.4.4", "", {}, "sha512-c"],
    "kysely": ["kysely@0.29.7", "", {}, "sha512-d"],
    "mitata": ["mitata@1.0.34", "", {}, "sha512-e"],
    "react": ["react@19.3.1", "", {}, "sha512-f"],
    "react-dom": ["react-dom@19.3.1", "", { "dependencies": { "scheduler": "^0.28.0" }, "peerDependencies": { "react": "^19.3.1" } }, "sha512-g"],
    "scheduler": ["scheduler@0.28.0", "", {}, "sha512-h"],
    "typescript": ["typescript@5.9.3", "", {}, "sha512-i"],
    "react-dom/scheduler": ["scheduler@0.27.0", "", {}, "sha512-j"],
  }
}`;

describe("standalone: productionPaths", () => {
  const paths = productionPaths(parseLockfile(LOCK));

  it("keeps dependencies and what they need, nested versions included", () => {
    expect(paths).toContain("@pglite/core");
    expect(paths).toContain("mitata");
    expect(paths).toContain("react-dom");
    expect(paths).toContain("react-dom/scheduler");
  });

  it("drops devDependencies and peers nothing else needs", () => {
    expect(paths).not.toContain("@types/bun");
    expect(paths).not.toContain("bun-types");
    expect(paths).not.toContain("typescript");
    // react-dom resolves the nested scheduler, not the hoisted one
    expect(paths).not.toContain("scheduler");
  });

  it("keeps the framework's runtime packages even as devDependencies", () => {
    expect(paths).toContain("kysely");
    expect(paths).toContain("react");
  });
});

describe("standalone: layout helpers", () => {
  it("maps lockfile paths to the hoisted node_modules tree", () => {
    expect(installPath("react")).toBe(join("node_modules", "react"));
    expect(installPath("next/@swc/helpers")).toBe(join("node_modules", "next", "node_modules", "@swc/helpers"));
  });

  it("ships sources and production env files, not tests, data or local env", () => {
    expect(shipsProjectFile("app/page.tsx")).toBe(true);
    expect(shipsProjectFile("modules/news/index.ts")).toBe(true);
    expect(shipsProjectFile(".env")).toBe(true);
    expect(shipsProjectFile(".env.production")).toBe(true);
    expect(shipsProjectFile(".env.local")).toBe(false);
    expect(shipsProjectFile("modules/news/news.test.ts")).toBe(false);
    expect(shipsProjectFile("data/app.db")).toBe(false);
    expect(shipsProjectFile("node_modules/react/index.js")).toBe(false);
    expect(shipsProjectFile(".git/HEAD")).toBe(false);
    expect(shipsProjectFile("app/.DS_Store")).toBe(false);
  });
});

describe("standalone: createStandalone", () => {
  let dir: string;
  const write = (rel: string, text = "x") => {
    mkdirSync(join(dir, rel, ".."), { recursive: true });
    writeFileSync(join(dir, rel), text);
  };

  beforeEach(() => {
    dir = mkdtempSync(join(tmpdir(), "com-standalone-"));
    write("package.json", JSON.stringify({ dependencies: { react: "^19", "react-dom": "^19", "react-server-dom-webpack": "^19" } }));
    write("bun.lock", LOCK);
    write("app/page.tsx", "export default () => null");
    write("app/page.test.tsx", "test");
    write("data/app.db", "local data");
    write(".env", "A=1");
    write(".env.local", "SECRET=1");
    write(".nata/build/manifest.json", "{}");
    write(".nata/build/static/a.js", "js");
    write(".nata/core.ts", "runtime");
    for (const name of ["@pglite/core", "mitata", "react", "react-dom", "scheduler", "@types/bun", "typescript", "kysely"]) {
      write(`node_modules/${name}/package.json`, JSON.stringify({ name, version: "1.0.0" }));
    }
    write("node_modules/react/package.json", JSON.stringify({ name: "react", version: "19.3.1" }));
    write("node_modules/react-dom/node_modules/scheduler/package.json", JSON.stringify({ name: "scheduler", version: "0.27.0" }));
  });

  afterEach(() => rmSync(dir, { recursive: true, force: true }));

  it("copies sources, the build and only the production packages", () => {
    const result = createStandalone(dir);
    const out = join(dir, ".nata/standalone");
    expect(result.outDir).toBe(out);
    expect(existsSync(join(out, "app/page.tsx"))).toBe(true);
    expect(existsSync(join(out, "app/page.test.tsx"))).toBe(false);
    expect(existsSync(join(out, "data"))).toBe(false);
    expect(existsSync(join(out, ".env"))).toBe(true);
    expect(existsSync(join(out, ".env.local"))).toBe(false);
    expect(existsSync(join(out, ".nata/build/static/a.js"))).toBe(true);
    expect(existsSync(join(out, ".nata/core.ts"))).toBe(false);
    expect(existsSync(join(out, "node_modules/react-dom/package.json"))).toBe(true);
    expect(readFileSync(join(out, "node_modules/react-dom/node_modules/scheduler/package.json"), "utf-8")).toContain("0.27.0");
    expect(existsSync(join(out, "node_modules/@types/bun"))).toBe(false);
    expect(existsSync(join(out, "node_modules/typescript"))).toBe(false);
    // The copy of the standalone folder never ends up inside itself on the next build
    createStandalone(dir);
    expect(existsSync(join(out, ".nata/standalone"))).toBe(false);
  });

  it("refuses an output folder that would delete something else", () => {
    write("elsewhere/keep.txt", "keep");
    expect(() => createStandalone(dir, { outDir: join(dir, "elsewhere") })).toThrow("not empty");
    expect(() => createStandalone(dir, { outDir: dir })).toThrow("contains the project");
    expect(existsSync(join(dir, "elsewhere/keep.txt"))).toBe(true);
  });

  it("needs a build", () => {
    rmSync(join(dir, ".nata/build"), { recursive: true });
    expect(() => createStandalone(dir)).toThrow("com build");
  });
});

describe("standalone: next.config output", () => {
  it("detects output: 'standalone'", () => {
    const dir = mkdtempSync(join(tmpdir(), "com-standalone-cfg-"));
    try {
      expect(configWantsStandalone(dir)).toBe(false);
      writeFileSync(join(dir, "next.config.ts"), `export default { output: "standalone" };`);
      expect(configWantsStandalone(dir)).toBe(true);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });
});
