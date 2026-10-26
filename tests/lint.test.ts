import { describe, it, expect, afterAll } from "bun:test";
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from "fs";
import { tmpdir } from "os";
import { join } from "path";
import { oxlintArgs, tscCommand, TYPESCRIPT_VERSION } from "../src/commands/lint";

const dir = mkdtempSync(join(tmpdir(), "com-lint-"));
afterAll(() => rmSync(dir, { recursive: true, force: true }));

describe("com lint", () => {
  it("enables React and Next.js rules and skips build output when the project has no oxlint config", () => {
    const args = oxlintArgs(dir, {});
    expect(args).toContain("--nextjs-plugin");
    expect(args).toContain("--react-plugin");
    expect(args).toContain(".nata/**");
    expect(args.at(-1)).toBe(".");
  });

  it("leaves rule selection to the project's .oxlintrc.json", () => {
    const own = mkdtempSync(join(dir, "own-"));
    writeFileSync(join(own, ".oxlintrc.json"), "{}");
    const args = oxlintArgs(own, { fix: true, json: true, paths: ["app"] });
    expect(args).not.toContain("--nextjs-plugin");
    expect(args).toContain("--fix");
    expect(args.join(" ")).toContain("--format json");
    expect(args.at(-1)).toBe("app");
  });

  it("type checks with the project's TypeScript when installed, else the pinned 5.x", () => {
    expect(tscCommand(dir)).toEqual([process.execPath, "x", `typescript@${TYPESCRIPT_VERSION}`]);
    mkdirSync(join(dir, "node_modules/.bin"), { recursive: true });
    writeFileSync(join(dir, "node_modules/.bin/tsc"), "");
    expect(tscCommand(dir)).toEqual([join(dir, "node_modules/.bin/tsc")]);
  });
});
