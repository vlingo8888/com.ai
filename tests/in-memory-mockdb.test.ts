import { describe, expect, it, beforeEach } from "bun:test";
import { ensureRuntimeEnvironment } from "../src/core/runtime";
import { existsSync, mkdirSync, rmSync, writeFileSync } from "fs";
import { join } from "path";

describe("Com Test - Zero-Mock In-Memory Database Engine", () => {
  const sandboxDir = join(process.cwd(), ".test-mockdb-sandbox");

  beforeEach(() => {
    if (existsSync(sandboxDir)) {
      rmSync(sandboxDir, { recursive: true, force: true });
    }
    mkdirSync(sandboxDir, { recursive: true });
  });

  it("provisions .nata/core.ts and node_modules/core in project directory", () => {
    ensureRuntimeEnvironment(sandboxDir);

    expect(existsSync(join(sandboxDir, ".nata", "core.ts"))).toBe(true);
    expect(existsSync(join(sandboxDir, ".nata", "headers.ts"))).toBe(true);
    expect(existsSync(join(sandboxDir, "node_modules", "core", "package.json"))).toBe(true);
  });

  it("auto-executes schema.sql and runs real Kysely queries (insert/select/delete) in memory", async () => {
    // 1. Write mock schema.sql
    const mockSchema = `
      CREATE TABLE IF NOT EXISTS "articles" (
        "id" SERIAL PRIMARY KEY,
        "title" TEXT NOT NULL,
        "content" TEXT,
        "status" TEXT DEFAULT 'draft',
        "created_at" TIMESTAMP DEFAULT now()
      );
    `;
    writeFileSync(join(sandboxDir, "schema.sql"), mockSchema);

    // 2. Provision runtime
    ensureRuntimeEnvironment(sandboxDir);

    // 3. Import generated core runtime in test mode
    process.env.NODE_ENV = "test";
    process.env.COM_TEST = "true";

    const coreModule = await import(join(sandboxDir, ".nata", "core.ts"));
    const { db, truncateTables } = coreModule;

    // 4. Test real database insert into in-memory table
    const inserted = await db
      .insertInto("articles")
      .values({
        title: "Test In-Memory Article",
        content: "Testing PGlite/SQLite mock database...",
        status: "published",
      })
      .returningAll()
      .executeTakeFirstOrThrow();

    expect(inserted).toBeDefined();
    expect(inserted.id).toBeDefined();
    expect(inserted.title).toBe("Test In-Memory Article");

    // 5. Test real database select
    const list = await db
      .selectFrom("articles")
      .selectAll()
      .where("status", "=", "published")
      .execute();

    expect(list).toHaveLength(1);
    expect(list[0].title).toBe("Test In-Memory Article");

    // 6. Test truncateTables cleanup helper
    await truncateTables(["articles"]);

    const afterTruncate = await db.selectFrom("articles").selectAll().execute();
    expect(afterTruncate).toHaveLength(0);

    // Clean up
    rmSync(sandboxDir, { recursive: true, force: true });
  });
});
