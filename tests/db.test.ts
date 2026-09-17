import { describe, expect, it } from "bun:test";
import {
  mapSqlTypeToTs,
  toPascalCase,
  formatDefaultValue,
  parseJsonComment,
  generateTypeScriptTypes,
  generateSqlDdl,
  updateAgentsMarkdown,
  type DatabaseSchema,
} from "../src/commands/db";

describe("Database Schema Generator", () => {
  it("parses stringified JSON comments into clean human-readable descriptions", () => {
    expect(parseJsonComment(null)).toBe(null);
    expect(parseJsonComment("")).toBe(null);
    expect(parseJsonComment("Mô tả bình thường")).toBe("Mô tả bình thường");
    expect(parseJsonComment('{"title": "Danh sách khách", "description": "Quản lý khách mời"}')).toBe("Danh sách khách - Quản lý khách mời");
    expect(parseJsonComment('{"label": "Tên khách", "name": "name"}')).toBe("Tên khách");
    expect(parseJsonComment('{"description": "Ghi chú nhiều dòng\nTiếp theo"}')).toBe("Ghi chú nhiều dòng Tiếp theo");
    expect(parseJsonComment('["Tag 1", "Tag 2"]')).toBe("Tag 1, Tag 2");
  });

  it("formats SQL column defaults properly including AST objects", () => {
    expect(formatDefaultValue(null)).toBe(null);
    expect(formatDefaultValue(undefined)).toBe(null);
    expect(formatDefaultValue("nextval('users_id_seq'::regclass)")).toBe("nextval('users_id_seq'::regclass)");
    expect(formatDefaultValue("CURRENT_TIMESTAMP")).toBe("CURRENT_TIMESTAMP");
    expect(formatDefaultValue({ type: "Identifier", name: "now" })).toBe("now()");
    expect(formatDefaultValue({ type: "Identifier", name: "CURRENT_TIMESTAMP" })).toBe("CURRENT_TIMESTAMP");
    expect(formatDefaultValue({ type: "Identifier", name: "gen_random_uuid" })).toBe("gen_random_uuid()");
    expect(formatDefaultValue({ type: "Literal", value: "active" })).toBe("'active'");
    expect(formatDefaultValue('{"type":"Identifier","name":"now"}')).toBe("now()");
  });

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
    expect(toPascalCase("program_guest_checklists")).toBe("ProgramGuestChecklists");
  });

  it("generates Kysely TypeScript definitions with comments, Generated<T>, and nullability", () => {
    const mockSchema: DatabaseSchema = {
      introspectedAt: "2026-09-17T10:00:00.000Z",
      tables: [
        {
          name: "program_guest_checklists",
          comment: "Quản lý checklist khách mời",
          columns: [
            {
              name: "id",
              dataType: "integer",
              udtName: "int4",
              isNullable: false,
              columnDefault: "nextval('program_guest_checklists_id_seq'::regclass)",
              comment: "ID tự tăng",
              isPrimaryKey: true,
            },
            {
              name: "created_at",
              dataType: "timestamp",
              udtName: "timestamp",
              isNullable: false,
              columnDefault: "now()",
              comment: "Thời gian tạo bản ghi",
              isPrimaryKey: false,
            },
            {
              name: "name",
              dataType: "text",
              udtName: "text",
              isNullable: true,
              columnDefault: null,
              comment: "Tên khách",
              isPrimaryKey: false,
            },
          ],
        },
      ],
    };

    const tsOutput = generateTypeScriptTypes(mockSchema);

    expect(tsOutput).toContain('import type { Generated } from "kysely";');
    expect(tsOutput).toContain('export interface Database {');
    expect(tsOutput).toContain('/** Quản lý checklist khách mời */');
    expect(tsOutput).toContain('"program_guest_checklists": ProgramGuestChecklistsTable;');
    expect(tsOutput).toContain('export interface ProgramGuestChecklistsTable {');
    expect(tsOutput).toContain('/** 📝 ID tự tăng | 🔑 Primary Key | Default: nextval(\'program_guest_checklists_id_seq\'::regclass) */');
    expect(tsOutput).toContain('id: Generated<number>;');
    expect(tsOutput).toContain('/** 📝 Thời gian tạo bản ghi | Default: now() */');
    expect(tsOutput).toContain('created_at: Generated<string>;');
    expect(tsOutput).toContain('/** 📝 Tên khách */');
    expect(tsOutput).toContain('name: string | null;');
  });

  it("generates SQL DDL schema with COMMENT ON TABLE and COLUMN", () => {
    const mockSchema: DatabaseSchema = {
      introspectedAt: "2026-09-17T10:00:00.000Z",
      tables: [
        {
          name: "posts",
          comment: "Bảng bài viết",
          columns: [
            {
              name: "id",
              dataType: "serial",
              udtName: "int4",
              isNullable: false,
              columnDefault: null,
              comment: "Mã bài viết",
              isPrimaryKey: true,
            },
            {
              name: "author_id",
              dataType: "integer",
              udtName: "int4",
              isNullable: false,
              columnDefault: null,
              comment: null,
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
    expect(sqlOutput).toContain('COMMENT ON TABLE "posts" IS \'Bảng bài viết\';');
    expect(sqlOutput).toContain('COMMENT ON COLUMN "posts"."id" IS \'Mã bài viết\';');
  });

  it("injects and updates schema in AGENTS.md including comments", () => {
    const mockSchema: DatabaseSchema = {
      introspectedAt: "2026-09-17T10:00:00.000Z",
      tables: [
        {
          name: "tasks",
          comment: "Bảng công việc",
          columns: [
            {
              name: "id",
              dataType: "integer",
              udtName: "int4",
              isNullable: false,
              columnDefault: "nextval()",
              comment: "ID công việc",
              isPrimaryKey: true,
            },
            {
              name: "title",
              dataType: "text",
              udtName: "text",
              isNullable: false,
              columnDefault: null,
              comment: "Tiêu đề công việc",
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
    expect(updated).toContain("> 📝 **Mô tả (Description):** Bảng công việc");
    expect(updated).toContain("Tiêu đề công việc");
    expect(updated).toContain("<!-- DATABASE_SCHEMA_END -->");
  });
});
