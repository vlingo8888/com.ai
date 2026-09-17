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
 * Parses table & column comments that may be stored as stringified JSON objects
 * and extracts a clean, human-readable description string without newlines.
 */
export function parseJsonComment(raw: any): string | null {
  if (raw === null || raw === undefined) return null;

  if (typeof raw === "object") {
    return extractTextFromJson(raw);
  }

  let str = String(raw).trim();
  if (!str) return null;

  if ((str.startsWith("{") && str.endsWith("}")) || (str.startsWith("[") && str.endsWith("]"))) {
    try {
      const parsed = JSON.parse(str);
      const text = extractTextFromJson(parsed);
      if (text) return text;
    } catch {}

    try {
      const sanitized = str.replace(/[\n\r\t]/g, " ");
      const parsed = JSON.parse(sanitized);
      const text = extractTextFromJson(parsed);
      if (text) return text;
    } catch {}
  }

  // Remove any newline characters and collapse multiple spaces
  const clean = str.replace(/\r?\n|\r/g, " ").replace(/\s+/g, " ").trim();
  return clean || null;
}

function extractTextFromJson(obj: any): string | null {
  if (!obj || typeof obj !== "object") return null;

  if (Array.isArray(obj)) {
    const items = obj
      .map((item) => (typeof item === "string" ? item : extractTextFromJson(item)))
      .filter(Boolean);
    return items.length > 0 ? items.join(", ") : null;
  }

  // Common metadata keys used in fullstack frameworks:
  const parts: string[] = [];
  if (obj.title) parts.push(String(obj.title).trim());
  if (obj.label && obj.label !== obj.title) parts.push(String(obj.label).trim());
  if (
    obj.description &&
    obj.description !== obj.title &&
    obj.description !== obj.label
  ) {
    parts.push(String(obj.description).trim());
  }
  if (obj.comment && !parts.includes(obj.comment)) parts.push(String(obj.comment).trim());
  if (obj.summary && !parts.includes(obj.summary)) parts.push(String(obj.summary).trim());
  if (obj.note && !parts.includes(obj.note)) parts.push(String(obj.note).trim());
  if (obj.name && parts.length === 0 && !["Identifier", "Literal", "CallExpression"].includes(obj.name)) {
    parts.push(String(obj.name).trim());
  }

  if (parts.length > 0) {
    return parts.join(" - ").replace(/\r?\n|\r/g, " ").replace(/\s+/g, " ");
  }

  // Fallback: collect meaningful non-technical string values
  const stringVals = Object.entries(obj)
    .filter(([k, v]) => typeof v === "string" && v.trim() && !["type", "dataType", "udtName", "kind"].includes(k))
    .map(([_, v]) => String(v).trim());

  if (stringVals.length > 0) {
    return stringVals.join(" - ").replace(/\r?\n|\r/g, " ").replace(/\s+/g, " ");
  }

  return null;
}

/**
 * Robust formatter for SQL column defaults (handles strings, numbers, Bun.SQL parsed AST objects, and stringified AST JSON)
 */
export function formatDefaultValue(val: any, udtName?: string): string | null {
  if (val === null || val === undefined) return null;

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
        ? (val.args || val.arguments).map((a: any) => formatDefaultValue(a, udtName)).filter(Boolean).join(", ")
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
    if (udtName === "json" || udtName === "jsonb") {
      return `'${JSON.stringify(val).replace(/'/g, "''")}'::jsonb`;
    }

    return null;
  }

  let str = String(val).trim();
  if (!str) return null;

  // 2. If string is a stringified AST JSON
  if (str.includes('"type"') || str.startsWith("{") || str.startsWith("[")) {
    // 2.1 Try JSON.parse
    try {
      const parsed = JSON.parse(str.replace(/[\n\r\t]/g, " "));
      const formatted = formatDefaultValue(parsed, udtName);
      if (formatted) return formatted;
    } catch {}

    // 2.2 Regex fallback for Identifier AST
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

    // 2.3 Regex fallback for CallExpression AST
    if (str.includes('"CallExpression"') || str.includes('"callee"')) {
      const calleeMatch = str.match(/"name"\s*:\s*"([^"]+)"/i);
      if (calleeMatch) {
        return `${calleeMatch[1]}()`;
      }
    }

    // 2.4 Regex fallback for Literal AST
    const litMatch = str.match(/"type"\s*:\s*"Literal"\s*,\s*"value"\s*:\s*([^,\}\]]+)/i);
    if (litMatch) {
      let v = litMatch[1].trim();
      if (v.startsWith('"') && v.endsWith('"')) {
        v = `'${v.slice(1, -1).replace(/'/g, "''")}'`;
      }
      return v;
    }

    // 2.5 Array fallback
    if (str.includes('"ArrayExpression"') || str.includes('"elements"')) {
      return `'[]'`;
    }

    // If string still starts with { and ends with }, avoid dumping raw JSON into SQL DEFAULT unless it's JSON type
    if (str.startsWith("{") && str.endsWith("}")) {
      if (udtName === "json" || udtName === "jsonb") {
        return `'${str.replace(/\r?\n|\r/g, " ").replace(/'/g, "''")}'::jsonb`;
      }
      return null;
    }
  }

  // 3. Clean standard string SQL expression (single line, no newlines)
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
 * Connects to PostgreSQL and extracts schema metadata including table/column comments using native Bun.SQL
 */
export async function introspectPostgres(dbUrl: string): Promise<DatabaseSchema> {
  // @ts-ignore - Bun.SQL is built-in in Bun 1.2+
  const sql = new Bun.SQL(dbUrl);

  try {
    // 1. Fetch tables
    const tableRows = await sql`
      SELECT table_name 
      FROM information_schema.tables 
      WHERE table_schema = 'public' 
        AND table_type = 'BASE TABLE'
      ORDER BY table_name;
    `;

    // 2. Fetch columns
    const columnRows = await sql`
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

    // 5. Fetch Table Comments from PostgreSQL catalog
    let tableCommentsMap = new Map<string, string>();
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
        if (row.table_comment) {
          tableCommentsMap.set(row.table_name, String(row.table_comment).trim());
        }
      }
    } catch {}

    // 6. Fetch Column Comments from PostgreSQL catalog
    let columnCommentsMap = new Map<string, string>();
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
        if (row.column_comment) {
          columnCommentsMap.set(`${row.table_name}.${row.column_name}`, String(row.column_comment).trim());
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
      const rawComment = columnCommentsMap.get(key) || null;
      const comment = parseJsonComment(rawComment);
      const formattedDefault = formatDefaultValue(col.column_default, col.udt_name);

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
      let def = `  "${col.name}" ${col.dataType.toUpperCase()}`;
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
export function updateAgentsMarkdown(
  existingContent: string,
  schema: DatabaseSchema
): string {
  const sectionHeader = "## 3. Database Schema Overview (Live Synced)";
  const startTag = "<!-- DATABASE_SCHEMA_START -->";
  const endTag = "<!-- DATABASE_SCHEMA_END -->";

  const schemaDocLines: string[] = [
    startTag,
    sectionHeader,
    "",
    `> **Last Synced:** \`${schema.introspectedAt}\` | **Total Tables:** \`${schema.tables.length}\``,
    `> AI coding assistants should reference \`types/db.d.ts\` and the schema table below for exact table structures, column types, and business descriptions when writing database queries with \`import { db } from "core"\`.`,
    "",
  ];

  for (const table of schema.tables) {
    schemaDocLines.push(`### Table: \`${table.name}\``);
    if (table.comment) {
      schemaDocLines.push(`> 📝 **Mô tả (Description):** ${table.comment}`);
      schemaDocLines.push("");
    }
    schemaDocLines.push("| Cột (Column) | Kiểu (Type) | Nullable | Khóa & Mặc định (Keys & Defaults) | Ghi chú (Description) |");
    schemaDocLines.push("| :--- | :--- | :---: | :--- | :--- |");

    for (const col of table.columns) {
      const tsType = mapSqlTypeToTs(col.udtName, col.dataType);
      const nullableBadge = col.isNullable ? "✅ Có" : "❌ Không";
      const details: string[] = [];

      if (col.isPrimaryKey) details.push("🔑 **Khóa chính (PK)**");
      if (col.foreignKey) {
        details.push(
          `🔗 FK \`-> ${col.foreignKey.foreignTable}.${col.foreignKey.foreignColumn}\``
        );
      }
      if (col.columnDefault) {
        details.push(`Default: \`${col.columnDefault}\``);
      }

      const commentText = col.comment ? col.comment : "-";

      schemaDocLines.push(
        `| \`${col.name}\` | \`${col.dataType}\` (\`${tsType}\`) | ${nullableBadge} | ${details.join(", ") || "-"} | ${commentText} |`
      );
    }
    schemaDocLines.push("");
  }

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
  let agentsContent = "";
  if (existsSync(agentsGuidePath)) {
    agentsContent = readFileSync(agentsGuidePath, "utf-8");
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
        value: "AGENTS.md (Schema & Descriptions injected)",
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
