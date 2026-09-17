import { describe, expect, it } from "bun:test";
import { buildBunTestArgs, testCommand } from "../src/commands/test";

describe("Com Test Command - Argument Builder", () => {
  it("generates default bun test arguments when no options are provided", () => {
    const args = buildBunTestArgs();
    expect(args).toEqual(["test"]);
  });

  it("appends target pattern when specified", () => {
    const args = buildBunTestArgs("modules/news");
    expect(args).toEqual(["test", "modules/news"]);
  });

  it("trims whitespace from pattern", () => {
    const args = buildBunTestArgs("   features/payroll   ");
    expect(args).toEqual(["test", "features/payroll"]);
  });

  it("adds --watch flag when watch option is enabled", () => {
    const args = buildBunTestArgs(undefined, { watch: true });
    expect(args).toEqual(["test", "--watch"]);
  });

  it("adds --coverage flag when coverage option is enabled", () => {
    const args = buildBunTestArgs(undefined, { coverage: true });
    expect(args).toEqual(["test", "--coverage"]);
  });

  it("adds --bail flag when bail option is enabled", () => {
    const args = buildBunTestArgs(undefined, { bail: true });
    expect(args).toEqual(["test", "--bail"]);
  });

  it("adds --test-name-pattern with regex/string when filter option is enabled", () => {
    const args = buildBunTestArgs(undefined, { filter: "Database Schema" });
    expect(args).toEqual(["test", "--test-name-pattern", "Database Schema"]);
  });

  it("adds --timeout with millisecond value", () => {
    const args = buildBunTestArgs(undefined, { timeout: 10000 });
    expect(args).toEqual(["test", "--timeout", "10000"]);
  });

  it("adds snapshot, only, todo, json, and env flags correctly", () => {
    const args = buildBunTestArgs("app/api", {
      updateSnapshots: true,
      only: true,
      todo: true,
      json: true,
      env: ".env.test",
    });

    expect(args).toContain("app/api");
    expect(args).toContain("--update-snapshots");
    expect(args).toContain("--only");
    expect(args).toContain("--todo");
    expect(args).toContain("--json");
    expect(args).toContain("--env-file");
    expect(args).toContain(".env.test");
  });

  it("builds combined arguments in correct order with pattern and multiple flags", () => {
    const args = buildBunTestArgs("db.test.ts", {
      watch: true,
      bail: true,
      filter: "Kysely",
      timeout: 5000,
    });

    expect(args).toEqual([
      "test",
      "db.test.ts",
      "--test-name-pattern",
      "Kysely",
      "--watch",
      "--bail",
      "--timeout",
      "5000",
    ]);
  });
});

describe("Com Test Command - CLI Execution", () => {
  it("successfully executes a fast targeted test run with zero exit code", async () => {
    // Run scanner test specifically via testCommand in JSON/quiet mode
    const exitCode = await testCommand(["tests/scanner.test.ts"], {
      json: true,
      bail: true,
    });

    expect(exitCode).toBe(0);
  });
});
