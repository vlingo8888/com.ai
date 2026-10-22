import { describe, it, expect, beforeEach, afterEach } from "bun:test";
import { existsSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from "fs";
import { tmpdir } from "os";
import { join } from "path";
import { isComxBuild, resolveProjectDir } from "../src/commands/dev";
import { ensureRuntimeEnvironment } from "../src/core/runtime";

describe("com dev on a comx build (build/nextjs)", () => {
  let workspace: string;
  let build: string;

  beforeEach(() => {
    workspace = mkdtempSync(join(tmpdir(), "comx-ws-"));
    build = join(workspace, "build/nextjs");
    mkdirSync(join(build, "app"), { recursive: true });
    mkdirSync(join(build, "core/rpc"), { recursive: true });
    mkdirSync(join(build, "config"), { recursive: true });
    writeFileSync(join(build, "core/index.ts"), "export const db = {};\n");
    writeFileSync(join(build, "core/rpc/proxy-dispatcher.ts"), "export {};\n");
    writeFileSync(join(build, "config/plugins.json"), "[]");
  });

  afterEach(() => rmSync(workspace, { recursive: true, force: true }));

  it("runs build/nextjs when started from the comx workspace", () => {
    expect(resolveProjectDir(workspace)).toBe(build);
    expect(isComxBuild(build)).toBe(true);
    expect(isComxBuild(workspace)).toBe(false);
  });

  it("keeps the current folder when it is an app, and --dir when given", () => {
    mkdirSync(join(workspace, "app"));
    expect(resolveProjectDir(workspace)).toBe(workspace);
    expect(resolveProjectDir(workspace, "other")).toBe(join(workspace, "other"));
  });

  it("does not shadow the project's own core with node_modules/core", () => {
    ensureRuntimeEnvironment(build);
    expect(existsSync(join(build, ".nata/core.ts"))).toBe(true);
    expect(existsSync(join(build, "node_modules/core"))).toBe(false);
  });
});
