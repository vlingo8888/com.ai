import { afterAll, beforeEach, describe, expect, it } from "bun:test";
import { existsSync, linkSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "fs";
import { tmpdir } from "os";
import { join } from "path";
import { cacheCommand, diskUsage, projectDepsDir, sharedDepsDir } from "./cache";

describe("com cache", () => {
  const root = mkdtempSync(join(tmpdir(), "com-cache-"));
  const project = join(root, "app");
  const previousHome = process.env.COM_HOME;
  process.env.COM_HOME = join(root, "home");
  afterAll(() => {
    if (previousHome === undefined) delete process.env.COM_HOME;
    else process.env.COM_HOME = previousHome;
    rmSync(root, { recursive: true, force: true });
  });

  const sharedFile = () => join(sharedDepsDir(), "v9/react@19.2.0/index.js");
  const projectFile = () => join(projectDepsDir(project), "v9/react@19.2.0/index.js");
  beforeEach(() => {
    rmSync(join(root, "home"), { recursive: true, force: true });
    rmSync(project, { recursive: true, force: true });
    mkdirSync(join(sharedFile(), ".."), { recursive: true });
    mkdirSync(join(projectFile(), ".."), { recursive: true });
    writeFileSync(sharedFile(), "export default 1;\n".repeat(100));
    linkSync(sharedFile(), projectFile());
  });

  it("lives under COM_HOME", () => {
    expect(sharedDepsDir()).toBe(join(root, "home", "cache", "deps"));
  });

  it("counts a project's hard link to a shared bundle once", () => {
    const size = "export default 1;\n".length * 100;
    expect(diskUsage(sharedDepsDir())).toBe(size);
    const seen = new Set<string>();
    expect(diskUsage(sharedDepsDir(), seen) + diskUsage(projectDepsDir(project), seen)).toBe(size);
  });

  it("clean --global keeps the project's copy readable (hard link)", async () => {
    await cacheCommand({ action: "clean", global: true, dir: project });
    expect(existsSync(sharedDepsDir())).toBe(false);
    expect(readFileSync(projectFile(), "utf-8")).toContain("export default 1;");
  });

  it("clean --project leaves the shared cache for other projects", async () => {
    await cacheCommand({ action: "clean", project: true, dir: project });
    expect(existsSync(projectDepsDir(project))).toBe(false);
    expect(existsSync(sharedFile())).toBe(true);
  });

  it("clean removes both by default", async () => {
    await cacheCommand({ action: "clean", dir: project });
    expect(existsSync(projectDepsDir(project))).toBe(false);
    expect(existsSync(sharedDepsDir())).toBe(false);
  });
});
