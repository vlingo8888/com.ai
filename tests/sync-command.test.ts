import { describe, expect, it } from "bun:test";
import { scanProjectFiles, syncCommand } from "../src/commands/sync";
import { existsSync, mkdirSync, rmSync, writeFileSync, readFileSync } from "fs";
import { join } from "path";

describe("Com Sync Command", () => {
  it("scans project files recursively and ignores node_modules and hidden dot directories", () => {
    const files = scanProjectFiles(process.cwd());
    expect(files.length).toBeGreaterThan(0);

    const filePaths = files.map((f) => f.path);
    expect(filePaths.some((p) => p.includes("bin/cli.ts"))).toBe(true);
    expect(filePaths.some((p) => p.includes("src/commands/sync.ts"))).toBe(true);
    expect(filePaths.some((p) => p.includes("node_modules"))).toBe(false);
    expect(filePaths.some((p) => p.includes(".git/"))).toBe(false);
  });

  it("synchronizes AGENTS.md, package manifests, and returns structured sync result", async () => {
    const testDir = join(process.cwd(), ".test-sync-sandbox");
    if (existsSync(testDir)) {
      rmSync(testDir, { recursive: true, force: true });
    }
    mkdirSync(testDir, { recursive: true });

    // Create mock source files with imports
    const appDir = join(testDir, "app");
    mkdirSync(appDir, { recursive: true });
    writeFileSync(
      join(appDir, "page.tsx"),
      `import React from "react";\nimport { motion } from "framer-motion";\nimport { Calendar } from "lucide-react";\n\nexport default function Page() { return <div />; }\n`
    );

    const syncResult = await syncCommand({
      dir: testDir,
      json: true,
    });

    expect(syncResult).toBeDefined();
    expect(syncResult.agentsPath).toBe(join(testDir, "AGENTS.md"));
    expect(existsSync(syncResult.agentsPath)).toBe(true);
    expect(existsSync(join(testDir, ".agents", "rules", "architecture.md"))).toBe(true);
    expect(existsSync(join(testDir, ".agent", "rules", "architecture.md"))).toBe(true);
    expect(existsSync(join(testDir, ".antigravityrules"))).toBe(true);
    expect(existsSync(join(testDir, "CLAUDE.md"))).toBe(true);
    expect(existsSync(join(testDir, ".cursorrules"))).toBe(true);
    expect(existsSync(join(testDir, ".github", "copilot-instructions.md"))).toBe(true);

    const agentsContent = readFileSync(syncResult.agentsPath, "utf-8");
    expect(agentsContent).toContain("# Clean Architecture & AI Coding Guidelines");
    expect(agentsContent).toContain("Unit, Integration & Component Testing Standards");

    expect(existsSync(syncResult.packageJsonPath)).toBe(true);
    const pkg = JSON.parse(readFileSync(syncResult.packageJsonPath, "utf-8"));
    expect(pkg.dependencies["framer-motion"]).toBeDefined();
    expect(pkg.dependencies["lucide-react"]).toBeDefined();

    expect(existsSync(syncResult.tsconfigPath)).toBe(true);

    // Clean up sandbox
    rmSync(testDir, { recursive: true, force: true });
  });
});
