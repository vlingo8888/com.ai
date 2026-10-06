import { describe, it, expect, afterAll } from "bun:test";
import { existsSync, rmSync, readFileSync } from "fs";
import { join } from "path";
import { createProject, getStarterProjectFiles } from "../src/commands/create";
import { introspectSchema } from "../src/commands/db";

const TEST_PROJECT_DIR = join(import.meta.dir, "temp-test-project");

describe("Command: com create (Next.js Clean Architecture with PGlite)", () => {
  afterAll(() => {
    // Cleanup generated temp directory
    try {
      if (existsSync(TEST_PROJECT_DIR)) {
        rmSync(TEST_PROJECT_DIR, { recursive: true, force: true });
      }
    } catch {}
  });

  it("generates correct template files list with sanitized project name", () => {
    const files = getStarterProjectFiles("My Awesome App!");
    const paths = files.map((f) => f.path);

    expect(paths).toContain("package.json");
    expect(paths).toContain("tsconfig.json");
    expect(paths).toContain(".env");
    expect(paths).toContain("schema.sql");
    expect(paths).toContain("types/db.d.ts");
    expect(paths).toContain("app/globals.css");
    expect(paths).toContain("app/layout.tsx");
    expect(paths).toContain("app/page.tsx");
    expect(paths).toContain("app/api/health/route.ts");
    expect(paths).toContain("modules/items/use-cases/get-items.use-case.ts");
    expect(paths).toContain("features/items/ui/ItemDashboard.tsx");

    const pkg = files.find((f) => f.path === "package.json");
    const parsedPkg = JSON.parse(pkg!.content);
    expect(parsedPkg.name).toBe("my-awesome-app");
    expect(parsedPkg.dependencies["@pglite/core"]).toBeDefined();
    expect(parsedPkg.dependencies["kysely"]).toBeDefined();
  });

  it("scaffolds project on disk and bootstraps local PGlite database", async () => {
    if (existsSync(TEST_PROJECT_DIR)) {
      rmSync(TEST_PROJECT_DIR, { recursive: true, force: true });
    }

    await createProject("demo-pglite-project", {
      dir: TEST_PROJECT_DIR,
      force: true,
      install: false,
    });

    // 1. Verify filesystem structure
    expect(existsSync(join(TEST_PROJECT_DIR, "package.json"))).toBe(true);
    expect(existsSync(join(TEST_PROJECT_DIR, "tsconfig.json"))).toBe(true);
    expect(existsSync(join(TEST_PROJECT_DIR, ".env"))).toBe(true);
    expect(existsSync(join(TEST_PROJECT_DIR, "schema.sql"))).toBe(true);
    expect(existsSync(join(TEST_PROJECT_DIR, "types", "db.d.ts"))).toBe(true);
    expect(existsSync(join(TEST_PROJECT_DIR, "AGENTS.md"))).toBe(true);
    expect(existsSync(join(TEST_PROJECT_DIR, ".antigravityrules"))).toBe(true);
    expect(existsSync(join(TEST_PROJECT_DIR, ".nata", "core.ts"))).toBe(true);
    expect(existsSync(join(TEST_PROJECT_DIR, "app", "layout.tsx"))).toBe(true);
    expect(existsSync(join(TEST_PROJECT_DIR, "app", "page.tsx"))).toBe(true);
    expect(existsSync(join(TEST_PROJECT_DIR, "app", "globals.css"))).toBe(true);
    expect(existsSync(join(TEST_PROJECT_DIR, "modules", "items", "index.ts"))).toBe(true);
    expect(existsSync(join(TEST_PROJECT_DIR, "features", "items", "index.ts"))).toBe(true);

    // 2. Verify PGlite local database file was created
    expect(existsSync(join(TEST_PROJECT_DIR, "data", "app.db"))).toBe(true);

    // 3. Verify .env specifies DB_PATH="data/app.db"
    const envContent = readFileSync(join(TEST_PROJECT_DIR, ".env"), "utf-8");
    expect(envContent).toContain('DB_PATH="data/app.db"');

    // 4. Verify PGlite schema introspection
    const { schema, engine } = await introspectSchema(TEST_PROJECT_DIR);
    expect(engine).toBe("pglite");
    expect(schema.tables.some((t) => t.name === "items")).toBe(true);

    const itemsTable = schema.tables.find((t) => t.name === "items");
    expect(itemsTable).toBeDefined();
    const colNames = itemsTable!.columns.map((c) => c.name);
    expect(colNames).toContain("id");
    expect(colNames).toContain("title");
    expect(colNames).toContain("description");
    expect(colNames).toContain("status");
    expect(colNames).toContain("created_at");
  });

  it("executes bun test inside the generated project successfully with in-memory PGlite", async () => {
    // Run unit tests inside the newly generated project
    const proc = Bun.spawn([process.execPath, "test"], {
      cwd: TEST_PROJECT_DIR,
      stdout: "pipe",
      stderr: "pipe",
    });

    const exitCode = await proc.exited;
    const stdout = await new Response(proc.stdout).text();
    const stderr = await new Response(proc.stderr).text();
    const output = stdout + "\n" + stderr;
    if (exitCode !== 0) {
      console.error("Test failures in generated project:", output);
    }

    expect(exitCode).toBe(0);
    expect(output).toContain("pass");
  });
});
