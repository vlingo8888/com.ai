import { describe, expect, it } from "bun:test";
import {
  mapSqlTypeToTs,
  toPascalCase,
  generateTypeScriptTypes,
  generateSqlDdl,
  updateAgentsMarkdown,
  type DatabaseSchema,
} from "../src/commands/db";

describe("Database Schema Generator", () => {
  it("maps SQL types to TypeScript types correctly", () => {
    expect(mapSqlTypeToTs("int4", "integer")).toBe("number");
    expect(mapSqlTypeToTs("int8", "bigint")).toBe("number");
    expect(mapSqlTypeToTs("varchar", "character varying")).toBe("string");
    expect(mapSqlTypeToTs("text", "text")).toBe("string");
    expect(mapSqlTypeToTs("bool", "boolean")).toBe("boolean");
    expect(mapSqlTypeToTs("timestamp", "timestamp without time zone")).toBe("string");
    expect(mapSqlTypeToTs("jsonb", "jsonb")).toBe("any");
    expect(mapSqlTypeToTs("_text", "ARRAY")).toBe("string[]");
    expect(mapSqlTypeToTs("_int4", "ARRAY")).toBe("number[]");
  });

  it("converts snake_case table names to PascalCase", () => {
    expect(toPascalCase("users")).toBe("Users");
    expect(toPascalCase("sos_requests")).toBe("SosRequests");
    expect(toPascalCase("user_role_permissions")).toBe("UserRolePermissions");
  });

  it("generates Kysely TypeScript definitions with Generated<T> and nullability", () => {
    const mockSchema: DatabaseSchema = {
      introspectedAt: "2026-09-17T10:00:00.000Z",
      tables: [
        {
          name: "users",
          columns: [
            {
              name: "id",
              dataType: "integer",
              udtName: "int4",
              isNullable: false,
              columnDefault: "nextval('users_id_seq'::regclass)",
              isPrimaryKey: true,
            },
            {
              name: "email",
              dataType: "varchar",
              udtName: "varchar",
              isNullable: false,
              columnDefault: null,
              isPrimaryKey: false,
            },
            {
              name: "bio",
              dataType: "text",
              udtName: "text",
              isNullable: true,
              columnDefault: null,
              isPrimaryKey: false,
            },
          ],
        },
      ],
    };

    const tsOutput = generateTypeScriptTypes(mockSchema);

    expect(tsOutput).toContain('import type { Generated } from "kysely";');
    expect(tsOutput).toContain('export interface Database {');
    expect(tsOutput).toContain('"users": UsersTable;');
    expect(tsOutput).toContain('export interface UsersTable {');
    expect(tsOutput).toContain('id: Generated<number>;');
    expect(tsOutput).toContain('email: string;');
    expect(tsOutput).toContain('bio: string | null;');
  });

  it("generates SQL DDL schema", () => {
    const mockSchema: DatabaseSchema = {
      introspectedAt: "2026-09-17T10:00:00.000Z",
      tables: [
        {
          name: "posts",
          columns: [
            {
              name: "id",
              dataType: "serial",
              udtName: "int4",
              isNullable: false,
              columnDefault: null,
              isPrimaryKey: true,
            },
            {
              name: "author_id",
              dataType: "integer",
              udtName: "int4",
              isNullable: false,
              columnDefault: null,
              isPrimaryKey: false,
              foreignKey: {
                foreignTable: "users",
                foreignColumn: "id",
              },
            },
          ],
        },
      ],
    };

    const sqlOutput = generateSqlDdl(mockSchema);
    expect(sqlOutput).toContain('CREATE TABLE IF NOT EXISTS "posts" (');
    expect(sqlOutput).toContain('"id" SERIAL PRIMARY KEY');
    expect(sqlOutput).toContain('REFERENCES "users"("id")');
  });

  it("injects and updates schema in AGENTS.md", () => {
    const mockSchema: DatabaseSchema = {
      introspectedAt: "2026-09-17T10:00:00.000Z",
      tables: [
        {
          name: "tasks",
          columns: [
            {
              name: "id",
              dataType: "integer",
              udtName: "int4",
              isNullable: false,
              columnDefault: "nextval()",
              isPrimaryKey: true,
            },
            {
              name: "title",
              dataType: "text",
              udtName: "text",
              isNullable: false,
              columnDefault: null,
              isPrimaryKey: false,
            },
          ],
        },
      ],
    };

    const initialMd = "# Project Guide\n\n## 1. Structure\n\n## 2. Core\n\n## 4. Backend\n";
    const updated = updateAgentsMarkdown(initialMd, mockSchema);

    expect(updated).toContain("<!-- DATABASE_SCHEMA_START -->");
    expect(updated).toContain("### Table: `tasks`");
    expect(updated).toContain("`title`");
    expect(updated).toContain("<!-- DATABASE_SCHEMA_END -->");

    // Test updating an existing schema block
    const reUpdated = updateAgentsMarkdown(updated, mockSchema);
    expect(reUpdated.match(/<!-- DATABASE_SCHEMA_START -->/g)?.length).toBe(1);
  });
});
