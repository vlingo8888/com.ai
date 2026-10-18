import { describe, it, expect, afterAll } from "bun:test";
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "fs";
import { tmpdir } from "os";
import { join } from "path";
import { runChecks, SUPPORTED_NEXT_CONFIG, unsupportedNextConfig } from "../src/commands/doctor";

const dir = mkdtempSync(join(tmpdir(), "com-doctor-"));
afterAll(() => rmSync(dir, { recursive: true, force: true }));

describe("com doctor", () => {
  it("knows the same next.config options as the engine", () => {
    const source = readFileSync(join(import.meta.dir, "../compiler-rs/src/next_config.rs"), "utf-8");
    const list = source.match(/const SUPPORTED: \[&str; \d+\] = \[([\s\S]*?)\];/)![1];
    const engineKeys = [...list.matchAll(/"([^"]+)"/g)].map((m) => m[1]);
    expect([...SUPPORTED_NEXT_CONFIG].sort()).toEqual(engineKeys.sort());
  });

  it("reports next.config options the engine ignores, including from a config function", async () => {
    writeFileSync(join(dir, "next.config.mjs"), "export default () => ({ basePath: '/x', distDir: 'build', i18n: {} });");
    expect(await unsupportedNextConfig(dir)).toEqual({ file: "next.config.mjs", unknown: ["distDir", "i18n"] });
  });

  it("fails without app/, warns about pages/ and React < 19", async () => {
    mkdirSync(join(dir, "pages"), { recursive: true });
    writeFileSync(join(dir, "package.json"), JSON.stringify({ dependencies: { react: "^18.2.0" } }));
    const checks = await runChecks(dir);
    const by = (name: string) => checks.find((c) => c.name === name)!;
    expect(by("App Router").status).toBe("fail");
    expect(by("Pages Router").status).toBe("warn");
    expect(by("React").status).toBe("warn");
    expect(by("next.config").status).toBe("warn");

    mkdirSync(join(dir, "app"));
    writeFileSync(join(dir, "package.json"), JSON.stringify({ dependencies: { react: "^19.1.0" } }));
    const fixed = await runChecks(dir);
    expect(fixed.find((c) => c.name === "App Router")!.status).toBe("ok");
    expect(fixed.find((c) => c.name === "React")!.status).toBe("ok");
  });
});
