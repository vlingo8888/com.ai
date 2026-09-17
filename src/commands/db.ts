import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { logger, colors } from "../core/logger";

export interface ColumnMeta {
  name: string;
  dataType: string;
  udtName: string;
  isNullable: boolean;
  columnDefault: string | null;
  comment: string | null;
  isPrimaryKey: boolean;
  foreignKey?: {
    foreignTable: string;
    foreignColumn: string;
  };
}

export interface TableMeta {
  name: string;
  comment: string | null;
  columns: ColumnMeta[];
}

export interface DatabaseSchema {
  tables: TableMeta[];
  introspectedAt: string;
}

/**
 * Preserves the original comment string (including stringified JSON metadata)
 * while ensuring it is cleanly formatted on a single line for SQL DDL and JSDoc.
 */
export function formatComment(raw: any): string | null {
  if (raw === null || raw === undefined) return null;

  if (typeof raw === "object") {
    try {
      return JSON.stringify(raw);
    } catch {
      return null;
    }
  }

  const str = String(raw).trim();
  if (!str || str.toLowerCase() === "null") return null;

  // Collapse newlines and multiple spaces for single-line SQL/JSDoc compatibility
  const clean = str.replace(/\r?\n|\r/g, " ").replace(/\s+/g, " ").trim();
  return clean || null;
}

export const parseJsonComment = formatComment;

/**
 * Robust formatter for SQL column defaults (handles strings, numbers, Bun.SQL parsed AST objects, stringified AST JSON, and unquoted strings)
 */
export function formatDefaultValue(
  val: any,
  udtName?: string,
  dataType?: string,
  columnName?: string
): string | null {
  if (val === null || val === undefined) return null;

  const colLower = (columnName || "").toLowerCase();
  const typeLower = (dataType || udtName || "").toLowerCase();

  // 1. If val is an object (AST node or parsed JSON)
  if (typeof val === "object") {
    if (Array.isArray(val)) {
      return `'[]'`;
    }

    // 1.1 Identifier node
    if (val.type === "Identifier" || val.name || val.identifier) {
      const name = String(val.name || val.identifier || "").trim();
      const lower = name.toLowerCase();
      if (lower === "now") return "now()";
      if (lower === "current_timestamp") return "CURRENT_TIMESTAMP";
      if (lower === "current_date") return "CURRENT_DATE";
      if (lower === "current_time") return "CURRENT_TIME";
      if (lower === "gen_random_uuid" || lower === "uuid_generate_v4") return `${name}()`;
      if (lower === "true" || lower === "false") return lower;
      return name;
    }

    // 1.2 CallExpression node
    if (val.type === "CallExpression" || val.callee) {
      const callee = typeof val.callee === "object" ? val.callee?.name || "fn" : String(val.callee);
      const argsList = Array.isArray(val.args || val.arguments)
        ? (val.args || val.arguments).map((a: any) => formatDefaultValue(a, udtName, dataType, columnName)).filter(Boolean).join(", ")
        : "";
      return `${callee}(${argsList})`;
    }

    // 1.3 Literal node
    if (val.type === "Literal" || val.value !== undefined) {
      if (typeof val.value === "string") return `'${val.value.replace(/'/g, "''")}'`;
      if (Array.isArray(val.value)) return `'[]'`;
      if (typeof val.value === "boolean" || typeof val.value === "number") return String(val.value);
      return `'${String(val.value)}'`;
    }

    // 1.4 ArrayExpression
    if (val.type === "ArrayExpression" || val.elements) {
      return `'[]'`;
    }

    if (val.raw) return String(val.raw).replace(/\r?\n|\r/g, " ").trim();
    if (val.sql) return String(val.sql).replace(/\r?\n|\r/g, " ").trim();

    // If it's a JSON type column
    if (typeLower.includes("json")) {
      return `'${JSON.stringify(val).replace(/'/g, "''")}'::jsonb`;
    }

    return null;
  }

  let str = String(val).trim();
  if (!str || str.toLowerCase() === "null") return null;

  // 2. If string is a partial or full AST JSON (like '{"type":"Identifier"' or '{"type":"Identifier","name":"now"}')
  if (
    str.includes('"type":"Identifier"') ||
    str.includes('{"type":"Identifier"') ||
    str === '{"type":"Identifier"' ||
    str.startsWith('{"type":') ||
    str.startsWith("{")
  ) {
    // 2.1 Check if it's a timestamp/created_at/updated_at column
    if (
      typeLower.includes("timestamp") ||
      typeLower.includes("date") ||
      typeLower.includes("time") ||
      colLower.includes("created_at") ||
      colLower.includes("updated_at")
    ) {
      return "now()";
    }

    // 2.2 Try JSON.parse
    try {
      const parsed = JSON.parse(str.replace(/[\n\r\t]/g, " "));
      const formatted = formatDefaultValue(parsed, udtName, dataType, columnName);
      if (formatted) return formatted;
    } catch {}

    // 2.3 Regex fallback for Identifier AST
    const idMatch =
      str.match(/"type"\s*:\s*"Identifier"\s*,\s*"name"\s*:\s*"([^"]+)"/i) ||
      str.match(/"name"\s*:\s*"([^"]+)"\s*,\s*"type"\s*:\s*"Identifier"/i);
    if (idMatch) {
      const name = idMatch[1].trim();
      const lower = name.toLowerCase();
      if (lower === "now") return "now()";
      if (lower === "current_timestamp") return "CURRENT_TIMESTAMP";
      if (lower === "current_date") return "CURRENT_DATE";
      if (lower === "current_time") return "CURRENT_TIME";
      if (lower === "gen_random_uuid" || lower === "uuid_generate_v4") return `${name}()`;
      if (lower === "true" || lower === "false") return lower;
      return name;
    }

    // 2.4 Regex fallback for CallExpression AST
    if (str.includes('"CallExpression"') || str.includes('"callee"')) {
      const calleeMatch = str.match(/"name"\s*:\s*"([^"]+)"/i);
      if (calleeMatch) {
        return `${calleeMatch[1]}()`;
      }
    }

    // 2.5 Regex fallback for Literal AST
    const litMatch = str.match(/"type"\s*:\s*"Literal"\s*,\s*"value"\s*:\s*([^,\}\]]+)/i);
    if (litMatch) {
      let v = litMatch[1].trim();
      if (v.startsWith('"') && v.endsWith('"')) {
        v = `'${v.slice(1, -1).replace(/'/g, "''")}'`;
      }
      return v;
    }

    // 2.6 Array fallback
    if (str.includes('"ArrayExpression"') || str.includes('"elements"')) {
      return `'[]'`;
    }

    // If it's a timestamp column fallback to now()
    if (colLower.includes("created_at") || colLower.includes("updated_at")) {
      return "now()";
    }

    // Avoid dumping raw JSON into SQL DEFAULT unless it's JSON type
    if (str.startsWith("{") && str.endsWith("}")) {
      if (typeLower.includes("json")) {
        return `'${str.replace(/\r?\n|\r/g, " ").replace(/'/g, "''")}'::jsonb`;
      }
      return null;
    }

    // If corrupted truncated JSON
    if (str.startsWith("{")) {
      return null;
    }
  }

  // 3. Handle array literals
  if (str === "[]" || str === "'[]'") {
    if (typeLower.includes("json")) return `'[]'::jsonb`;
    return `'[]'`;
  }

  // 4. Handle SQL functions and keywords
  const isFunctionCall =
    str.toLowerCase() === "now()" ||
    str.toLowerCase() === "current_timestamp" ||
    str.toLowerCase() === "current_date" ||
    str.toLowerCase() === "current_time" ||
    str.toLowerCase().startsWith("nextval(") ||
    str.toLowerCase().startsWith("gen_random_uuid(") ||
    str.toLowerCase().startsWith("uuid_generate_");

  if (isFunctionCall) {
    return str.replace(/\r?\n|\r/g, " ");
  }

  // 5. Handle Booleans
  if (str.toLowerCase() === "true" || str.toLowerCase() === "false") {
    return str.toLowerCase();
  }

  // 6. Handle Numbers
  if (
    /^-?\d+(\.\d+)?$/.test(str) &&
    (typeLower.includes("int") ||
      typeLower.includes("numeric") ||
      typeLower.includes("decimal") ||
      typeLower.includes("float") ||
      typeLower.includes("real"))
  ) {
    return str;
  }

  // 7. Handle String/Text types: ensure unquoted string literals (like #3b82f6, public, medium) are wrapped in single quotes
  if (
    typeLower.includes("text") ||
    typeLower.includes("varchar") ||
    typeLower.includes("char")
  ) {
    // If it already has single quotes '...', keep it
    if (str.startsWith("'") && str.endsWith("'")) {
      return str.replace(/\r?\n|\r/g, " ");
    }
    // If it has PostgreSQL typecast like 'active'::character varying or 'public'::text
    const castMatch = str.match(/^'([^']+)'::/);
    if (castMatch) {
      return `'${castMatch[1].replace(/'/g, "''")}'`;
    }
    // Wrap literal string in single quotes
    return `'${str.replace(/'/g, "''").replace(/\r?\n|\r/g, " ")}'`;
  }

  // 8. Clean standard string SQL expression (single line, no newlines)
  return str.replace(/\r?\n|\r/g, " ").replace(/\s+/g, " ");
}

/**
 * Maps PostgreSQL data types to TypeScript types for Kysely interfaces
 */
export function mapSqlTypeToTs(udtName: string, dataType: string): string {
  const type = (udtName || dataType || "").toLowerCase();

  // Numbers
  if (
    [
      "int2",
      "int4",
      "int8",
      "integer",
      "smallint",
      "bigint",
      "serial",
      "bigserial",
      "numeric",
      "decimal",
      "real",
      "float4",
      "float8",
      "double precision",
    ].includes(type)
  ) {
    return "number";
  }

  // Booleans
  if (["bool", "boolean"].includes(type)) {
    return "boolean";
  }

  // Strings / Dates / UUID
  if (
    [
      "varchar",
      "text",
      "char",
      "bpchar",
      "uuid",
      "date",
      "time",
      "timetz",
      "timestamp",
      "timestamptz",
      "interval",
      "citext",
    ].includes(type)
  ) {
    return "string";
  }

  // JSON
  if (["json", "jsonb"].includes(type)) {
    return "any";
  }

  // Binary
  if (["bytea"].includes(type)) {
    return "Buffer | Uint8Array";
  }

  // Array types (e.g. _text, _int4)
  if (type.startsWith("_")) {
    const inner = mapSqlTypeToTs(type.slice(1), dataType);
    return `${inner}[]`;
  }

  return "any";
}

/**
 * Converts snake_case or kebab-case table names to PascalCase for TypeScript interface names
 */
export function toPascalCase(str: string): string {
  let res = str
    .replace(/[^a-zA-Z0-9]+(.)/g, (_, chr) => chr.toUpperCase())
    .replace(/^[a-z]/, (chr) => chr.toUpperCase())
    .replace(/[^a-zA-Z0-9]/g, "");
  if (!res || /^[0-9]/.test(res)) {
    res = `Table${res}`;
  }
  return res;
}

/**
 * Formats column data type into clean SQL type for DDL
 */
export function formatSqlType(dataType: string, udtName: string): string {
  const dt = (dataType || "").toUpperCase();
  const udt = (udtName || "").toLowerCase();

  if (dt === "ARRAY" && udt.startsWith("_")) {
    return `${udt.slice(1).toUpperCase()}[]`;
  }
  if (dt === "USER-DEFINED" && udtName) {
    return udtName;
  }
  return dt || udt.toUpperCase() || "TEXT";
}

/**
 * Connects to PostgreSQL and extracts schema metadata including table/column comments using native Bun.SQL
 */
export async function introspectPostgres(dbUrl: string): Promise<DatabaseSchema> {
  // @ts-ignore - Bun.SQL is built-in in Bun 1.2+
  const sql = new Bun.SQL(dbUrl);

  try {
    // 1. Fetch tables (querying comment column from information_schema if available)
    let tableRows: { table_name: string; comment?: string | null }[] = [];
    try {
      tableRows = await sql`
        SELECT table_name, comment 
        FROM information_schema.tables 
        WHERE table_schema = 'public' 
          AND table_type = 'BASE TABLE'
        ORDER BY table_name;
      `;
    } catch {
      tableRows = await sql`
        SELECT table_name 
        FROM information_schema.tables 
        WHERE table_schema = 'public' 
          AND table_type = 'BASE TABLE'
        ORDER BY table_name;
      `;
    }

    // 2. Fetch columns (querying comment column from information_schema if available)
    let columnRows: {
      table_name: string;
      column_name: string;
      data_type: string;
      udt_name: string;
      is_nullable: string;
      column_default: any;
      comment?: string | null;
    }[] = [];
    try {
      columnRows = await sql`
        SELECT 
          table_name,
          column_name,
          data_type,
          udt_name,
          is_nullable,
          column_default,
          comment
        FROM information_schema.columns 
        WHERE table_schema = 'public' 
        ORDER BY table_name, ordinal_position;
      `;
    } catch {
      columnRows = await sql`
        SELECT 
          table_name,
          column_name,
          data_type,
          udt_name,
          is_nullable,
          column_default
        FROM information_schema.columns 
        WHERE table_schema = 'public' 
        ORDER BY table_name, ordinal_position;
      `;
    }

    // 3. Fetch primary keys
    const pkRows = await sql`
      SELECT
        tc.table_name, 
        kcu.column_name
      FROM information_schema.table_constraints tc
      JOIN information_schema.key_column_usage kcu
        ON tc.constraint_name = kcu.constraint_name
        AND tc.table_schema = kcu.table_schema
      WHERE tc.constraint_type = 'PRIMARY KEY'
        AND tc.table_schema = 'public';
    `;

    // 4. Fetch foreign keys
    const fkRows = await sql`
      SELECT
        kcu.table_name,
        kcu.column_name,
        ccu.table_name AS foreign_table_name,
        ccu.column_name AS foreign_column_name
      FROM information_schema.table_constraints AS tc
      JOIN information_schema.key_column_usage AS kcu
        ON tc.constraint_name = kcu.constraint_name
        AND tc.table_schema = kcu.table_schema
      JOIN information_schema.constraint_column_usage AS ccu
        ON ccu.constraint_name = tc.constraint_name
        AND ccu.table_schema = tc.table_schema
      WHERE tc.constraint_type = 'FOREIGN KEY'
        AND tc.table_schema = 'public';
    `;

    // 5. Fetch Table Comments from information_schema and pg_catalog
    let tableCommentsMap = new Map<string, string>();
    for (const row of tableRows) {
      if (row.comment) {
        tableCommentsMap.set(row.table_name, String(row.comment).trim());
      }
    }
    try {
      const tableCommentRows = await sql`
        SELECT 
          c.relname AS table_name,
          COALESCE(d.description, pg_catalog.obj_description(c.oid, 'pg_class')) AS table_comment
        FROM pg_catalog.pg_class c
        JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
        LEFT JOIN pg_catalog.pg_description d ON d.objoid = c.oid AND d.objsubid = 0
        WHERE n.nspname = 'public' 
          AND c.relkind IN ('r', 'p');
      `;
      for (const row of tableCommentRows) {
        if (row.table_comment && !tableCommentsMap.has(row.table_name)) {
          tableCommentsMap.set(row.table_name, String(row.table_comment).trim());
        }
      }
    } catch {}

    // 6. Fetch Column Comments from information_schema and pg_catalog
    let columnCommentsMap = new Map<string, string>();
    for (const col of columnRows) {
      if (col.comment) {
        columnCommentsMap.set(`${col.table_name}.${col.column_name}`, String(col.comment).trim());
      }
    }
    try {
      const colCommentRows = await sql`
        SELECT 
          c.relname AS table_name,
          a.attname AS column_name,
          COALESCE(d.description, pg_catalog.col_description(c.oid, a.attnum)) AS column_comment
        FROM pg_catalog.pg_class c
        JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
        JOIN pg_catalog.pg_attribute a ON a.attrelid = c.oid
        LEFT JOIN pg_catalog.pg_description d ON d.objoid = c.oid AND d.objsubid = a.attnum
        WHERE n.nspname = 'public'
          AND c.relkind IN ('r', 'p')
          AND a.attnum > 0
          AND NOT a.attisdropped;
      `;
      for (const row of colCommentRows) {
        const key = `${row.table_name}.${row.column_name}`;
        if (row.column_comment && !columnCommentsMap.has(key)) {
          columnCommentsMap.set(key, String(row.column_comment).trim());
        }
      }
    } catch {}

    const primaryKeyMap = new Set<string>();
    for (const row of pkRows) {
      primaryKeyMap.add(`${row.table_name}.${row.column_name}`);
    }

    const fkMap = new Map<string, { foreignTable: string; foreignColumn: string }>();
    for (const row of fkRows) {
      fkMap.set(`${row.table_name}.${row.column_name}`, {
        foreignTable: row.foreign_table_name,
        foreignColumn: row.foreign_column_name,
      });
    }

    const tablesMap = new Map<string, ColumnMeta[]>();
    for (const t of tableRows) {
      tablesMap.set(t.table_name, []);
    }

    for (const col of columnRows) {
      const key = `${col.table_name}.${col.column_name}`;
      const isPk = primaryKeyMap.has(key);
      const fk = fkMap.get(key);
      const rawComment = columnCommentsMap.get(key) || col.comment || null;
      const comment = parseJsonComment(rawComment);
      const formattedDefault = formatDefaultValue(
        col.column_default,
        col.udt_name,
        col.data_type,
        col.column_name
      );

      const colMeta: ColumnMeta = {
        name: col.column_name,
        dataType: col.data_type,
        udtName: col.udt_name,
        isNullable: col.is_nullable === "YES",
        columnDefault: formattedDefault,
        comment,
        isPrimaryKey: isPk,
        foreignKey: fk,
      };

      if (!tablesMap.has(col.table_name)) {
        tablesMap.set(col.table_name, []);
      }
      tablesMap.get(col.table_name)!.push(colMeta);
    }

    const tables: TableMeta[] = [];
    for (const [name, columns] of tablesMap.entries()) {
      const rawTableComment = tableCommentsMap.get(name) || null;
      const tableComment = parseJsonComment(rawTableComment);
      tables.push({ name, comment: tableComment, columns });
    }

    return {
      tables,
      introspectedAt: new Date().toISOString(),
    };
  } finally {
    try {
      await sql.close();
    } catch {}
  }
}

/**
 * Generates TypeScript type definitions for Kysely (types/db.d.ts)
 */
export function generateTypeScriptTypes(schema: DatabaseSchema): string {
  const lines: string[] = [];

  lines.push("/**");
  lines.push(" * Auto-generated Database Types for Kysely & Com.AI.VN Framework");
  lines.push(` * Last synchronized: ${schema.introspectedAt}`);
  lines.push(" * Do not edit this file directly. Run `com db pull` to regenerate.");
  lines.push(" */");
  lines.push("");
  lines.push('import type { Generated } from "kysely";');
  lines.push("");
  lines.push("export interface Database {");

  for (const table of schema.tables) {
    const interfaceName = `${toPascalCase(table.name)}Table`;
    if (table.comment) {
      lines.push(`  /** ${table.comment.replace(/\*\//g, "* /")} */`);
    }
    lines.push(`  ${JSON.stringify(table.name)}: ${interfaceName};`);
  }

  lines.push("}");
  lines.push("");

  for (const table of schema.tables) {
    const interfaceName = `${toPascalCase(table.name)}Table`;
    if (table.comment) {
      lines.push("/**");
      lines.push(` * 📋 ${table.comment.replace(/\*\//g, "* /")}`);
      lines.push(" */");
    }
    lines.push(`export interface ${interfaceName} {`);

    for (const col of table.columns) {
      const baseTsType = mapSqlTypeToTs(col.udtName, col.dataType);
      const hasDefaultOrGenerated =
        col.isPrimaryKey ||
        (col.columnDefault !== null && col.columnDefault !== undefined);

      let finalType: string;
      if (hasDefaultOrGenerated) {
        if (col.isNullable && !col.isPrimaryKey) {
          finalType = `Generated<${baseTsType} | null>`;
        } else {
          finalType = `Generated<${baseTsType}>`;
        }
      } else if (col.isNullable) {
        finalType = `${baseTsType} | null`;
      } else {
        finalType = baseTsType;
      }

      // Build JSDoc comment with comment description, foreign keys, and defaults
      const commentParts: string[] = [];
      if (col.comment) {
        commentParts.push(`📝 ${col.comment.replace(/\*\//g, "* /")}`);
      }
      if (col.isPrimaryKey) {
        commentParts.push("🔑 Primary Key");
      }
      if (col.foreignKey) {
        commentParts.push(
          `🔗 FK -> ${col.foreignKey.foreignTable}.${col.foreignKey.foreignColumn}`
        );
      }
      if (col.columnDefault) {
        commentParts.push(`Default: ${col.columnDefault.replace(/\*\//g, "* /")}`);
      }

      if (commentParts.length > 0) {
        lines.push(`  /** ${commentParts.join(" | ")} */`);
      }

      const isValidIdent = /^[a-zA-Z_$][a-zA-Z0-9_$]*$/.test(col.name);
      const propKey = isValidIdent ? col.name : JSON.stringify(col.name);
      lines.push(`  ${propKey}: ${finalType};`);
    }

    lines.push("}");
    lines.push("");
  }

  return lines.join("\n");
}

/**
 * Generates SQL DDL schema file (schema.sql) with table & column comments
 */
export function generateSqlDdl(schema: DatabaseSchema): string {
  const lines: string[] = [];

  lines.push("-- ============================================================================");
  lines.push("-- Auto-generated Database Schema for Com.AI.VN Project");
  lines.push(`-- Last synchronized: ${schema.introspectedAt}`);
  lines.push(`-- Total Tables: ${schema.tables.length}`);
  lines.push("-- ============================================================================");
  lines.push("");

  for (const table of schema.tables) {
    lines.push("-- ----------------------------------------------------------------------------");
    lines.push(`-- Table: ${table.name}`);
    if (table.comment) {
      lines.push(`-- Description: ${table.comment}`);
    }
    lines.push("-- ----------------------------------------------------------------------------");
    lines.push(`CREATE TABLE IF NOT EXISTS "${table.name}" (`);

    const colDefs: string[] = [];
    const totalCols = table.columns.length;

    for (let i = 0; i < totalCols; i++) {
      const col = table.columns[i];
      const sqlType = formatSqlType(col.dataType, col.udtName);
      let def = `  "${col.name}" ${sqlType}`;
      if (col.columnDefault) {
        def += ` DEFAULT ${col.columnDefault}`;
      }
      if (!col.isNullable && !col.isPrimaryKey) {
        def += " NOT NULL";
      }
      if (col.isPrimaryKey) {
        def += " PRIMARY KEY";
      }
      if (col.foreignKey) {
        def += ` REFERENCES "${col.foreignKey.foreignTable}"("${col.foreignKey.foreignColumn}")`;
      }

      const isLast = i === totalCols - 1;
      const comma = isLast ? "" : ",";

      // Inline comment
      const commentBadges: string[] = [];
      if (col.isPrimaryKey) commentBadges.push("🔑 PK");
      if (col.foreignKey) commentBadges.push(`🔗 FK -> ${col.foreignKey.foreignTable}.${col.foreignKey.foreignColumn}`);
      if (col.comment) commentBadges.push(col.comment);

      const inlineComment = commentBadges.length > 0 ? ` -- ${commentBadges.join(" | ")}` : "";
      colDefs.push(`${def}${comma}${inlineComment}`);
    }

    lines.push(colDefs.join("\n"));
    lines.push(");");
    lines.push("");

    // Add standard PostgreSQL COMMENT ON TABLE
    if (table.comment) {
      const escapedComment = table.comment.replace(/'/g, "''");
      lines.push(`COMMENT ON TABLE "${table.name}" IS '${escapedComment}';`);
    }

    // Add standard PostgreSQL COMMENT ON COLUMN
    for (const col of table.columns) {
      if (col.comment) {
        const escapedColComment = col.comment.replace(/'/g, "''");
        lines.push(
          `COMMENT ON COLUMN "${table.name}"."${col.name}" IS '${escapedColComment}';`
        );
      }
    }

    lines.push("");
  }

  return lines.join("\n");
}

/**
 * Injects or updates the Database Schema Overview section in AGENTS.md
 */
/**
 * Extracts a clean, human-readable text from either a stringified JSON comment or plain text
 */
export function extractHumanComment(raw: string | null): string | null {
  if (!raw) return null;
  const str = raw.trim();
  if (str.startsWith("{") && str.endsWith("}")) {
    try {
      const parsed = JSON.parse(str);
      const parts: string[] = [];
      if (parsed.label) parts.push(String(parsed.label).trim());
      if (parsed.description && parsed.description !== parsed.label) parts.push(String(parsed.description).trim());
      if (parsed.comment && !parts.includes(parsed.comment)) parts.push(String(parsed.comment).trim());
      if (parts.length > 0) return parts.join(" - ");
    } catch {}
  }
  return str;
}

/**
 * Injects or updates the compact Database Schema Overview section in AGENTS.md
 */
export function updateAgentsMarkdown(
  existingContent: string,
  schema: DatabaseSchema
): string {
  const sectionHeader = "## 3. Database Schema & Migration Guide (Live Synced)";
  const startTag = "<!-- DATABASE_SCHEMA_START -->";
  const endTag = "<!-- DATABASE_SCHEMA_END -->";

  const schemaDocLines: string[] = [
    startTag,
    sectionHeader,
    "",
    `> **Last Synced:** \`${schema.introspectedAt}\` | **Total Tables:** \`${schema.tables.length}\``,
    `> When querying database in Server Actions or backend logic, always use \`import { db } from "core"\` and reference \`types/db.d.ts\` for full TypeScript auto-completion.`,
    "",
    "### ⚡ On-Demand Database Tools for AI Coding Assistants",
    "To save context tokens and explore tables on-demand, run these CLI commands:",
    "- **List all tables & descriptions**: `com db list` (or `com db list --json`)",
    "- **Inspect a specific table**: `com db describe <table_name>` (e.g. `com db describe wellness_assessments`)",
    "- **Search tables/columns by keyword**: `com db search <keyword>` (e.g. `com db search \"khảo sát\"`)",
    "- **Execute database migration**: `com migrate \"<SQL_STATEMENT>\"` (or `com migrate ./file.sql`) — automatically runs `com db pull` upon completion.",
    "",
    "### 📝 Metadata Comment Standards for Migrations",
    "When creating or altering tables, always attach stringified JSON metadata comments:",
    "- **Table Comment**:",
    "  ```sql",
    "  COMMENT ON TABLE \"table_name\" IS '{\"label\": \"Tên hiển thị\", \"description\": \"Mô tả nghiệp vụ chi tiết\"}';",
    "  ```",
    "- **Column Comment**:",
    "  ```sql",
    "  COMMENT ON COLUMN \"table_name\".\"col_name\" IS '{\"label\": \"Tên cột\", \"type\": \"short_text|long_text|number|decimal|boolean|datetime|select|json|foreign_key\", \"required\": true, \"description\": \"Mô tả ý nghĩa\", \"enums\": [{\"label\": \"Nhãn\", \"value\": \"gia_tri\"}]}';",
    "  ```",
    "",
    "### 📊 Compact Database Tables Index",
    "| Bảng (Table) | Mô tả nghiệp vụ (Description) | Cột (Cols) | Khóa ngoại chính (Foreign Keys) |",
    "| :--- | :--- | :---: | :--- |",
  ];

  for (const table of schema.tables) {
    const desc = extractHumanComment(table.comment) || "-";
    const fkSummary =
      table.columns
        .filter((c) => c.foreignKey)
        .map((c) => `\`${c.name}\` -> \`${c.foreignKey!.foreignTable}\``)
        .join(", ") || "-";

    schemaDocLines.push(
      `| \`${table.name}\` | ${desc} | ${table.columns.length} | ${fkSummary} |`
    );
  }

  schemaDocLines.push("");
  schemaDocLines.push(
    "> 💡 *Tip: Run `com db describe <table_name>` to inspect full column definitions, constraints, and default values.*"
  );
  schemaDocLines.push(endTag);
  const newSectionContent = schemaDocLines.join("\n");

  if (existingContent.includes(startTag) && existingContent.includes(endTag)) {
    const startIndex = existingContent.indexOf(startTag);
    const endIndex = existingContent.indexOf(endTag) + endTag.length;
    return (
      existingContent.slice(0, startIndex) +
      newSectionContent +
      existingContent.slice(endIndex)
    );
  }

  // If tags don't exist yet, insert after section 2 or at the end
  if (existingContent.includes("## 2.")) {
    const parts = existingContent.split(/(?=## 2\.)/);
    if (parts.length >= 2) {
      // Find the next heading after ## 2.
      const afterSection2 = parts[1];
      const nextHeadingMatch = afterSection2.search(/\n## [0-9]+\./);
      if (nextHeadingMatch !== -1) {
        const insertionPoint = parts[0].length + nextHeadingMatch;
        return (
          existingContent.slice(0, insertionPoint) +
          "\n\n" +
          newSectionContent +
          "\n" +
          existingContent.slice(insertionPoint)
        );
      }
    }
  }

  return existingContent + "\n\n" + newSectionContent + "\n";
}

/**
 * Finds database URL from environment or .env file
 */
export function findDatabaseUrl(projectDir: string, customEnv?: string): string | null {
  if (process.env.DATABASE_URL) return process.env.DATABASE_URL;
  if (process.env.POSTGRES_URL) return process.env.POSTGRES_URL;

  const envFiles = customEnv ? [customEnv] : [".env", ".env.local", ".env.development"];
  for (const file of envFiles) {
    const envPath = join(projectDir, file);
    if (existsSync(envPath)) {
      try {
        const content = readFileSync(envPath, "utf-8");
        for (const line of content.split("\n")) {
          const trimmed = line.trim();
          if (trimmed.startsWith("#") || !trimmed.includes("=")) continue;
          const [key, ...vals] = trimmed.split("=");
          const val = vals.join("=").trim().replace(/^["']|["']$/g, "");
          if ((key.trim() === "DATABASE_URL" || key.trim() === "POSTGRES_URL") && val) {
            return val;
          }
        }
      } catch {}
    }
  }

  return null;
}

/**
 * Executes the full schema pull and generation workflow
 */
export async function introspectAndGenerateSchema(
  projectDir: string,
  dbUrl: string,
  options: { silent?: boolean } = {}
): Promise<{ tableCount: number; typesPath: string; schemaPath: string }> {
  const schema = await introspectPostgres(dbUrl);

  // 1. Write types/db.d.ts
  const typesDir = join(projectDir, "types");
  const typesPath = join(typesDir, "db.d.ts");
  const typesContent = generateTypeScriptTypes(schema);
  await Bun.write(typesPath, typesContent);

  // 2. Write schema.sql
  const schemaPath = join(projectDir, "schema.sql");
  const sqlContent = generateSqlDdl(schema);
  await Bun.write(schemaPath, sqlContent);

  // 3. Update AGENTS.md
  const agentsGuidePath = join(projectDir, "AGENTS.md");
  let agentsContent = existsSync(agentsGuidePath) ? readFileSync(agentsGuidePath, "utf-8") : "";
  if (!agentsContent || !agentsContent.includes("Clean Architecture & AI Coding Guidelines") || !agentsContent.includes("Testing Standards")) {
    const { generateAgentsGuide } = await import("../templates/ai_guidelines");
    const { getLocalViewConfig } = await import("../core/config");
    const localView = getLocalViewConfig(projectDir);
    const projectName = localView?.name || projectDir.split("/").filter(Boolean).pop() || "com-project";
    const viewId = localView?.viewId || 1;
    agentsContent = generateAgentsGuide(projectName, viewId);
  }
  const updatedAgents = updateAgentsMarkdown(agentsContent, schema);
  await Bun.write(agentsGuidePath, updatedAgents);

  return {
    tableCount: schema.tables.length,
    typesPath,
    schemaPath,
  };
}

/**
 * CLI Command: `com db pull` / `com db sync`
 */
export async function dbPullCommand(options: {
  dir?: string;
  env?: string;
  dbUrl?: string;
} = {}): Promise<void> {
  const projectDir = options.dir || process.cwd();
  const dbUrl = options.dbUrl || findDatabaseUrl(projectDir, options.env);

  logger.hero();
  logger.section("DATABASE SCHEMA INTROSPECTION & SYNC");

  if (!dbUrl) {
    logger.error(
      "No DATABASE_URL or POSTGRES_URL found!",
      "Please configure DATABASE_URL in your .env file or provide --url option."
    );
    process.exit(1);
  }

  const maskedUrl = dbUrl.replace(/:([^:@]+)@/, ":****@");
  logger.info(`Connecting to database: ${colors.sky}${maskedUrl}${colors.reset}`);

  try {
    const result = await introspectAndGenerateSchema(projectDir, dbUrl);

    logger.card("DATABASE SYNC COMPLETE", [
      {
        label: "Tables Found",
        value: `${result.tableCount} tables`,
        color: colors.bold + colors.emerald,
      },
      {
        label: "TypeScript Types",
        value: "types/db.d.ts (Kysely typed + Comments)",
        color: colors.cyan,
      },
      {
        label: "SQL Schema",
        value: "schema.sql (DDL + Table/Column Comments)",
        color: colors.yellow,
      },
      {
        label: "AI Guidelines",
        value: "AGENTS.md (Compact Schema & Tools injected)",
        color: colors.sky,
      },
    ]);

    logger.success(
      `Successfully synced database schema! AI coding assistants (Antigravity/Cursor/Copilot) now have full visibility and business comments for your database.`
    );
  } catch (err: any) {
    logger.error(`Database introspection failed: ${err.message || String(err)}`);
    process.exit(1);
  }
}

/**
 * CLI Command: `com db list` / `com db tables`
 */
export async function dbListCommand(options: {
  dir?: string;
  env?: string;
  dbUrl?: string;
  json?: boolean;
} = {}): Promise<void> {
  const projectDir = options.dir || process.cwd();
  const dbUrl = options.dbUrl || findDatabaseUrl(projectDir, options.env);

  if (!dbUrl) {
    logger.error(
      "No DATABASE_URL found!",
      "Please configure DATABASE_URL in your .env file or provide --url option."
    );
    process.exit(1);
  }

  const schema = await introspectPostgres(dbUrl);

  if (options.json) {
    const list = schema.tables.map((t) => ({
      name: t.name,
      description: extractHumanComment(t.comment),
      columnsCount: t.columns.length,
      primaryKey: t.columns.find((c) => c.isPrimaryKey)?.name || null,
      foreignKeys: t.columns
        .filter((c) => c.foreignKey)
        .map(
          (c) =>
            `${c.name} -> ${c.foreignKey!.foreignTable}.${c.foreignKey!.foreignColumn}`
        ),
      rawComment: t.comment,
    }));
    console.log(JSON.stringify(list, null, 2));
    return;
  }

  logger.hero();
  logger.section(`DATABASE TABLES (${schema.tables.length} tables total)`);

  for (const t of schema.tables) {
    const desc =
      extractHumanComment(t.comment) ||
      `${colors.darkGray}(No description)${colors.reset}`;
    const fks = t.columns.filter((c) => c.foreignKey);
    const fkInfo =
      fks.length > 0 ? ` ${colors.darkGray}[${fks.length} FKs]${colors.reset}` : "";
    console.log(
      `  ${colors.bold}${colors.green}• ${t.name}${colors.reset} ${colors.darkGray}(${t.columns.length} cols)${colors.reset}${fkInfo}`
    );
    console.log(`    ${colors.gray}${desc}${colors.reset}`);
  }
  console.log(
    `\n  ${colors.darkGray}Tip: Run ${colors.cyan}com db describe <table_name>${colors.darkGray} to inspect full column details.${colors.reset}\n`
  );
}

/**
 * CLI Command: `com db describe <table_name>` / `com db show <table_name>`
 */
export async function dbDescribeCommand(
  tableName: string | undefined,
  options: {
    dir?: string;
    env?: string;
    dbUrl?: string;
    json?: boolean;
  } = {}
): Promise<void> {
  const projectDir = options.dir || process.cwd();
  const dbUrl = options.dbUrl || findDatabaseUrl(projectDir, options.env);

  if (!tableName || !tableName.trim()) {
    logger.error(
      "Missing required <table_name> argument!",
      "Usage: com db describe <table_name>\n    Example: com db describe wellness_assessments"
    );
    process.exit(1);
  }

  if (!dbUrl) {
    logger.error(
      "No DATABASE_URL found!",
      "Please configure DATABASE_URL in your .env file or provide --url option."
    );
    process.exit(1);
  }

  const schema = await introspectPostgres(dbUrl);
  const target = schema.tables.find(
    (t) => t.name.toLowerCase() === tableName.trim().toLowerCase()
  );

  if (!target) {
    const similar = schema.tables
      .filter((t) =>
        t.name.toLowerCase().includes(tableName.trim().toLowerCase())
      )
      .map((t) => t.name);
    logger.error(
      `Table "${tableName}" not found in database!`,
      similar.length > 0
        ? `Did you mean one of these?\n    ${similar.join("\n    ")}`
        : "Run `com db list` to view all available tables."
    );
    process.exit(1);
  }

  if (options.json) {
    console.log(JSON.stringify(target, null, 2));
    return;
  }

  logger.hero();
  logger.section(`TABLE: ${target.name}`);

  const desc = extractHumanComment(target.comment);
  if (desc) {
    console.log(
      `  ${colors.bold}📝 Description:${colors.reset} ${colors.white}${desc}${colors.reset}\n`
    );
  }

  console.log(
    `  ${colors.bold}${colors.white}COLUMNS (${target.columns.length}):${colors.reset}`
  );
  for (const col of target.columns) {
    const badges: string[] = [];
    if (col.isPrimaryKey) badges.push(`${colors.yellow}[PK]${colors.reset}`);
    if (col.foreignKey) {
      badges.push(
        `${colors.sky}[FK -> ${col.foreignKey.foreignTable}.${col.foreignKey.foreignColumn}]${colors.reset}`
      );
    }
    if (!col.isNullable && !col.isPrimaryKey) {
      badges.push(`${colors.red}[NOT NULL]${colors.reset}`);
    }
    if (col.columnDefault) {
      badges.push(
        `${colors.darkGray}[default: ${col.columnDefault}]${colors.reset}`
      );
    }

    const colDesc = extractHumanComment(col.comment);
    const descText = colDesc ? ` - ${colors.gray}${colDesc}${colors.reset}` : "";
    const badgeText = badges.length > 0 ? ` ${badges.join(" ")}` : "";

    console.log(
      `  • ${colors.bold}${colors.green}${col.name}${colors.reset} ${colors.cyan}(${col.dataType})${colors.reset}${badgeText}${descText}`
    );
  }
  console.log("");
}

/**
 * CLI Command: `com db search <query>` / `com db find <query>`
 */
export async function dbSearchCommand(
  query: string | undefined,
  options: {
    dir?: string;
    env?: string;
    dbUrl?: string;
    json?: boolean;
  } = {}
): Promise<void> {
  const projectDir = options.dir || process.cwd();
  const dbUrl = options.dbUrl || findDatabaseUrl(projectDir, options.env);

  if (!query || !query.trim()) {
    logger.error(
      "Missing search query!",
      "Usage: com db search <keyword>\n    Example: com db search \"khảo sát\""
    );
    process.exit(1);
  }

  if (!dbUrl) {
    logger.error(
      "No DATABASE_URL found!",
      "Please configure DATABASE_URL in your .env file or provide --url option."
    );
    process.exit(1);
  }

  const q = query.trim().toLowerCase();
  const schema = await introspectPostgres(dbUrl);

  const matchedTables: {
    table: TableMeta;
    matchedColumns: ColumnMeta[];
    matchType: "name" | "comment" | "column";
  }[] = [];

  for (const t of schema.tables) {
    const tableNameMatch = t.name.toLowerCase().includes(q);
    const tableCommentMatch = (t.comment || "").toLowerCase().includes(q);
    const matchedCols = t.columns.filter(
      (c) =>
        c.name.toLowerCase().includes(q) ||
        (c.comment || "").toLowerCase().includes(q)
    );

    if (tableNameMatch || tableCommentMatch || matchedCols.length > 0) {
      matchedTables.push({
        table: t,
        matchedColumns: matchedCols,
        matchType: tableNameMatch
          ? "name"
          : tableCommentMatch
          ? "comment"
          : "column",
      });
    }
  }

  if (options.json) {
    console.log(JSON.stringify(matchedTables, null, 2));
    return;
  }

  logger.hero();
  logger.section(`DATABASE SEARCH: "${query}" (${matchedTables.length} matches)`);

  if (matchedTables.length === 0) {
    logger.info(
      `No tables or columns matching "${query}". Run \`com db list\` to view all tables.`
    );
    return;
  }

  for (const item of matchedTables) {
    const desc = extractHumanComment(item.table.comment) || "(No description)";
    console.log(
      `\n  ${colors.bold}${colors.green}📋 Table: ${item.table.name}${colors.reset}`
    );
    console.log(`     ${colors.gray}Description: ${desc}${colors.reset}`);

    if (item.matchedColumns.length > 0) {
      console.log(
        `     ${colors.cyan}Matching Columns (${item.matchedColumns.length}):${colors.reset}`
      );
      for (const col of item.matchedColumns) {
        const colDesc = extractHumanComment(col.comment) || "-";
        console.log(
          `       • ${colors.bold}${col.name}${colors.reset} (${col.dataType}): ${colors.gray}${colDesc}${colors.reset}`
        );
      }
    }
  }
  console.log(
    `\n  ${colors.darkGray}Tip: Run ${colors.cyan}com db describe <table_name>${colors.darkGray} for full details.${colors.reset}\n`
  );
}

/**
 * CLI Command: `com migrate <SQL|file.sql>` or `com db migrate <SQL|file.sql>`
 * Executes custom SQL migrations and automatically triggers `com db pull` to refresh types and schemas.
 */
export async function dbMigrateCommand(
  sqlOrFile: string | undefined,
  options: {
    dir?: string;
    env?: string;
    dbUrl?: string;
  } = {}
): Promise<void> {
  const projectDir = options.dir || process.cwd();
  const dbUrl = options.dbUrl || findDatabaseUrl(projectDir, options.env);

  logger.hero();
  logger.section("DATABASE MIGRATION & AUTO-SYNC");

  if (!sqlOrFile || !sqlOrFile.trim()) {
    logger.error(
      "Missing SQL statement or migration file path!",
      `Usage:
    com migrate "<SQL_STATEMENT>"
    com migrate ./migrations/001_create_table.sql
    com db migrate "<SQL_STATEMENT>"`
    );
    process.exit(1);
  }

  if (!dbUrl) {
    logger.error(
      "No DATABASE_URL or POSTGRES_URL found!",
      "Please configure DATABASE_URL in your .env file or provide --url option."
    );
    process.exit(1);
  }

  let sqlContent = sqlOrFile.trim();
  let isFilePath = false;
  const potentialPath = join(projectDir, sqlContent);

  if (existsSync(sqlContent)) {
    isFilePath = true;
    sqlContent = readFileSync(sqlContent, "utf-8");
  } else if (existsSync(potentialPath)) {
    isFilePath = true;
    sqlContent = readFileSync(potentialPath, "utf-8");
  }

  if (!sqlContent.trim()) {
    logger.error("Migration SQL content is empty!");
    process.exit(1);
  }

  const maskedUrl = dbUrl.replace(/:([^:@]+)@/, ":****@");
  logger.info(`Target database: ${colors.sky}${maskedUrl}${colors.reset}`);
  if (isFilePath) {
    logger.info(`Executing migration file: ${colors.yellow}${sqlOrFile}${colors.reset}`);
  } else {
    logger.info(`Executing SQL statement(s)...`);
  }

  // @ts-ignore - Bun.SQL is built-in
  const sql = new Bun.SQL(dbUrl);

  try {
    // Execute SQL migration
    await sql.unsafe(sqlContent);
    logger.success("Database migration executed successfully!");
  } catch (err: any) {
    logger.error(
      `Migration failed: ${err.message || String(err)}`,
      "Please verify your SQL syntax, table/column names, and constraints."
    );
    process.exit(1);
  } finally {
    try {
      await sql.close();
    } catch {}
  }

  // Automatically trigger db pull to refresh types/db.d.ts, schema.sql, and AGENTS.md
  logger.info(`Auto-synchronizing database schema & types...`);
  try {
    const result = await introspectAndGenerateSchema(projectDir, dbUrl);
    logger.card("MIGRATION & SYNC COMPLETE", [
      {
        label: "Migration Status",
        value: "Applied successfully",
        color: colors.bold + colors.emerald,
      },
      {
        label: "Total Tables",
        value: `${result.tableCount} tables`,
        color: colors.sky,
      },
      {
        label: "TypeScript Types",
        value: "types/db.d.ts (updated)",
        color: colors.cyan,
      },
      {
        label: "SQL Schema",
        value: "schema.sql (updated)",
        color: colors.yellow,
      },
      {
        label: "AI Guidelines",
        value: "AGENTS.md (updated)",
        color: colors.emerald,
      },
    ]);
  } catch (err: any) {
    logger.warn(`Migration succeeded, but auto db pull failed: ${err.message || String(err)}`);
  }
}
