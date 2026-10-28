// The shared store end to end, offline: packages come from a stand-in Bun cache, the lockfile is
// written by hand (so no `bun install --lockfile-only` runs). Projects live in os.tmpdir(), which on
// macOS sits behind the /var → /private/var link: relative store links must still resolve.

import { afterAll, beforeAll, describe, expect, it } from "bun:test";
import { existsSync, lstatSync, mkdirSync, mkdtempSync, readlinkSync, realpathSync, rmSync, writeFileSync } from "fs";
import { tmpdir } from "os";
import { join } from "path";
import { farmDir, gc, installProject, linksCurrent, listEntries, packagesDir, readProjectState, usesStore, why } from "./index";

const saved = { home: process.env.COM_HOME, cache: process.env.BUN_INSTALL_CACHE_DIR, store: process.env.COM_STORE };
let base: string;

function cachedPackage(name: string, version: string, manifest: Record<string, unknown>, files: Record<string, string>) {
  const dir = join(process.env.BUN_INSTALL_CACHE_DIR!, `${name}@${version}@@@1`);
  mkdirSync(dir, { recursive: true });
  writeFileSync(join(dir, "package.json"), JSON.stringify({ name, version, ...manifest }));
  for (const [file, code] of Object.entries(files)) writeFileSync(join(dir, file), code);
}

const LOCK = `{
  "lockfileVersion": 1,
  "workspaces": {
    "": {
      "dependencies": { "a": "^1.0.0", "c": "^1.0.0", },
    },
  },
  "packages": {
    "a": ["a@1.0.0", "", { "dependencies": { "b": "^1.0.0" } }, "sha512-a"],
    "b": ["b@1.0.0", "", {}, "sha512-b1"],
    "c": ["c@1.0.0", "", { "dependencies": { "b": "^2.0.0" } }, "sha512-c"],
    "c/b": ["b@2.0.0", "", {}, "sha512-b2"],
  }
}
`;

function project(name: string): string {
  const dir = join(base, name);
  mkdirSync(dir, { recursive: true });
  writeFileSync(join(dir, "package.json"), JSON.stringify({ name, dependencies: { a: "^1.0.0", c: "^1.0.0" } }));
  writeFileSync(join(dir, "bun.lock"), LOCK);
  return dir;
}

describe("shared package store", () => {
  beforeAll(() => {
    base = mkdtempSync(join(tmpdir(), "com-store-"));
    // The store on the real path, projects through the link (on macOS): as ~/.com and a project in /var
    process.env.COM_HOME = join(realpathSync(base), "home");
    process.env.BUN_INSTALL_CACHE_DIR = join(base, "bun-cache");
    delete process.env.COM_STORE;
    cachedPackage("a", "1.0.0", { main: "index.js", exports: { ".": "./index.js", "./sub": "./sub.js" } }, {
      "index.js": 'module.exports = require("b") + 1;',
      "sub.js": "module.exports = 'sub';",
    });
    cachedPackage("b", "1.0.0", { main: "index.js" }, { "index.js": "module.exports = 41;" });
    cachedPackage("b", "2.0.0", { main: "index.js" }, { "index.js": "module.exports = 2;" });
    cachedPackage("c", "1.0.0", { main: "index.js", scripts: { postinstall: "node build.js" } }, { "index.js": 'module.exports = require("b");' });
  });

  afterAll(() => {
    rmSync(base, { recursive: true, force: true });
    for (const [key, value] of [["COM_HOME", saved.home], ["BUN_INSTALL_CACHE_DIR", saved.cache], ["COM_STORE", saved.store]] as const) {
      if (value === undefined) delete process.env[key];
      else process.env[key] = value;
    }
  });

  it("links a project's packages from the store, each with its own dependency versions", async () => {
    const dir = project("one");
    expect(usesStore(dir)).toBe(true);
    const result = await installProject(dir, "bun-is-not-needed");
    expect(result.upToDate).toBe(false);
    expect(result.created).toBe(4);
    expect(result.downloaded).toBe(0);
    expect(result.skippedScripts).toEqual(["c"]);
    // node_modules is one link to the store links (for what Bun loads itself), no files of its own
    expect(lstatSync(join(dir, "node_modules")).isSymbolicLink()).toBe(true);
    expect(realpathSync(join(dir, "node_modules"))).toBe(realpathSync(farmDir(dir)));
    expect(usesStore(dir)).toBe(true);
    expect(packagesDir(dir)).toBe(farmDir(dir));

    const farm = farmDir(dir);
    // a → b@1, c → its nested b@2: the hoisted tree's answers, through links that resolve
    expect(require(join(farm, "a"))).toBe(42);
    expect(require(join(farm, "c"))).toBe(2);
    expect(require(join(farm, "b"))).toBe(41);
    expect(readlinkSync(join(farm, "a")).startsWith("/")).toBe(false);
    // Bun resolves from .nata/modules with `exports`
    expect(Bun.resolveSync("a/sub", join(dir, ".nata", "modules"))).toContain(join("store", "v1"));
  });

  it("shares every entry with a project on the same lockfile, and skips work when nothing changed", async () => {
    const before = listEntries().length;
    const dir = project("two");
    const result = await installProject(dir, "bun-is-not-needed");
    expect(result.created).toBe(0);
    expect(listEntries().length).toBe(before);
    expect(readProjectState(dir)!.entries).toEqual(readProjectState(join(base, "one"))!.entries);
    const again = await installProject(dir, "bun-is-not-needed");
    expect(again.upToDate).toBe(true);
  });

  it("trusts the links while package.json and bun.lock are untouched, and checks again once they change", async () => {
    const dir = project("stamp");
    await installProject(dir, "bun-is-not-needed");
    expect(linksCurrent(dir)).toBe(true);
    // An edit to package.json (even one that changes nothing) means the lockfile must be compared again
    writeFileSync(join(dir, "package.json"), JSON.stringify({ name: "stamp", dependencies: { c: "^1.0.0", a: "^1.0.0" } }));
    expect(linksCurrent(dir)).toBe(false);
    expect((await installProject(dir, "bun-is-not-needed")).upToDate).toBe(true);
    expect(linksCurrent(dir)).toBe(true);
    // A removed link (a package deleted from the store) is noticed without the lockfile
    rmSync(join(farmDir(dir), "a"), { recursive: true, force: true });
    expect(linksCurrent(dir)).toBe(false);
    expect((await installProject(dir, "bun-is-not-needed")).upToDate).toBe(false);
    expect(require(join(farmDir(dir), "a"))).toBe(42);
    rmSync(dir, { recursive: true, force: true });
  });

  it("removes the temporary folders of installs whose process is gone, keeping running ones'", async () => {
    const tmp = join(process.env.COM_HOME!, "store", "v1", ".tmp");
    // A pid no process has (above every system's pid limit) and this process's own
    const abandoned = join(tmp, "prefetch-99999999-abc123");
    const running = join(tmp, `prefetch-${process.pid}-def456`);
    mkdirSync(join(abandoned, "node_modules"), { recursive: true });
    mkdirSync(running, { recursive: true });
    const dir = project("three");
    rmSync(join(dir, ".nata"), { recursive: true, force: true });
    await installProject(dir, "bun-is-not-needed");
    expect(existsSync(abandoned)).toBe(false);
    expect(existsSync(running)).toBe(true);
    rmSync(running, { recursive: true, force: true });
    // The gc test below counts the projects that use the entries
    rmSync(dir, { recursive: true, force: true });
  });

  it("explains why a package is installed", () => {
    const chains = why(join(base, "one"), "b").map((c) => c.join(">"));
    expect(chains.sort()).toEqual(["a>b", "c>b"]);
  });

  it("collects entries no project uses any more", async () => {
    expect(gc(true).entries).toEqual([]);
    rmSync(join(base, "one"), { recursive: true, force: true });
    expect(gc(true).entries).toEqual([]);
    rmSync(join(base, "two"), { recursive: true, force: true });
    expect(gc(true).entries.length).toBe(4);
    gc();
    expect(listEntries()).toEqual([]);
  });

  it("links an npm alias under its alias name, next to the real package (no EEXIST)", async () => {
    // @swc/helpers@0.4.37 depends on "@swc/legacy-helpers": "npm:@swc/helpers@0.4.14", hoisted next to @swc/helpers@0.5
    cachedPackage("d", "1.0.0", { main: "index.js", dependencies: { "old-b": "npm:b@1.0.0" } }, { "index.js": 'module.exports = require("old-b");' });
    const dir = join(base, "alias");
    mkdirSync(dir, { recursive: true });
    writeFileSync(join(dir, "package.json"), JSON.stringify({ name: "alias", dependencies: { b: "^2.0.0", d: "^1.0.0" } }));
    writeFileSync(
      join(dir, "bun.lock"),
      `{
  "lockfileVersion": 1,
  "workspaces": { "": { "dependencies": { "b": "^2.0.0", "d": "^1.0.0", }, }, },
  "packages": {
    "b": ["b@2.0.0", "", {}, "sha512-b2"],
    "d": ["d@1.0.0", "", { "dependencies": { "old-b": "npm:b@1.0.0" } }, "sha512-d"],
    "old-b": ["b@1.0.0", "", {}, "sha512-b1"],
  }
}
`
    );
    await installProject(dir, "bun-is-not-needed");
    const farm = farmDir(dir);
    expect(require(join(farm, "b"))).toBe(2);
    expect(require(join(farm, "old-b"))).toBe(41);
    expect(require(join(farm, "d"))).toBe(41);
    expect(readProjectState(dir)!.links).toEqual(["b", "d", "old-b"]);
  });

  it("leaves projects with their own node_modules alone", () => {
    const dir = project("classic");
    mkdirSync(join(dir, "node_modules"));
    expect(usesStore(dir)).toBe(false);
    expect(packagesDir(dir)).toBe(join(dir, "node_modules"));
    process.env.COM_STORE = "0";
    rmSync(join(dir, "node_modules"), { recursive: true });
    expect(usesStore(dir)).toBe(false);
    delete process.env.COM_STORE;
  });
});
