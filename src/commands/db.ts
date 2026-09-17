import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { logger, colors } from "../core/logger";

export interface ColumnMeta {
  name: string;
  dataType: string;
  udtName: string;
  isNullable: boolean;
  columnDefault: string | null;
  isPrimaryKey: boolean;
  foreignKey?: {
    foreignTable: string;
    foreignColumn: string;
  };
}

export interface TableMeta {
  name: string;
  columns: ColumnMeta[];
}

export interface DatabaseSchema {
  tables: TableMeta[];
  introspectedAt: string;
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
  return str
    .replace(/[^a-zA-Z0-9]+(.)/g, (_, chr) => chr.toUpperCase())
    .replace(/^[a-z]/, (chr) => chr.toUpperCase())
    .replace(/[^a-zA-Z0-9]/g, "");
}

/**
 * Connects to PostgreSQL and extracts schema metadata using native Bun.SQL
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

      const colMeta: ColumnMeta = {
        name: col.column_name,
        dataType: col.data_type,
        udtName: col.udt_name,
        isNullable: col.is_nullable === "YES",
        columnDefault: col.column_default,
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
      tables.push({ name, columns });
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
    lines.push(`  ${JSON.stringify(table.name)}: ${interfaceName};`);
  }

  lines.push("}");
  lines.push("");

  for (const table of schema.tables) {
    const interfaceName = `${toPascalCase(table.name)}Table`;
    lines.push(`export interface ${interfaceName} {`);

    for (const col of table.columns) {
      const baseTsType = mapSqlTypeToTs(col.udtName, col.dataType);
      const hasDefaultOrGenerated =
        col.isPrimaryKey ||
        (col.columnDefault !== null && col.columnDefault !== undefined);

      let finalType: string;
      if (hasDefaultOrGenerated) {
        finalType = `Generated<${baseTsType}>`;
      } else if (col.isNullable) {
        finalType = `${baseTsType} | null`;
      } else {
        finalType = baseTsType;
      }

      // Add JSDoc comment if foreign key or primary key
      const commentParts: string[] = [];
      if (col.isPrimaryKey) commentParts.push("Primary Key");
      if (col.foreignKey) {
        commentParts.push(
          `References ${col.foreignKey.foreignTable}.${col.foreignKey.foreignColumn}`
        );
      }
      if (col.columnDefault) {
        commentParts.push(`Default: ${col.columnDefault}`);
      }

      if (commentParts.length > 0) {
        lines.push(`  /** ${commentParts.join(" | ")} */`);
      }
      lines.push(`  ${col.name}: ${finalType};`);
    }

    lines.push("}");
    lines.push("");
  }

  return lines.join("\n");
}

/**
 * Generates SQL DDL schema file (schema.sql)
 */
export function generateSqlDdl(schema: DatabaseSchema): string {
  const lines: string[] = [];

  lines.push("-- Auto-generated Database Schema for Com.AI.VN Project");
  lines.push(`-- Last synchronized: ${schema.introspectedAt}`);
  lines.push("");

  for (const table of schema.tables) {
    lines.push(`CREATE TABLE IF NOT EXISTS "${table.name}" (`);
    const colDefs: string[] = [];

    for (const col of table.columns) {
      let def = `  "${col.name}" ${col.dataType.toUpperCase()}`;
      if (!col.isNullable && !col.isPrimaryKey) {
        def += " NOT NULL";
      }
      if (col.columnDefault) {
        def += ` DEFAULT ${col.columnDefault}`;
      }
      if (col.isPrimaryKey) {
        def += " PRIMARY KEY";
      }
      if (col.foreignKey) {
        def += ` REFERENCES "${col.foreignKey.foreignTable}"("${col.foreignKey.foreignColumn}")`;
      }
      colDefs.push(def);
    }

    lines.push(colDefs.join(",\n"));
    lines.push(");");
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
    `> AI coding assistants can reference this live schema and use strongly-typed queries with \`import { db } from "core"\`.`,
    "",
  ];

  for (const table of schema.tables) {
    schemaDocLines.push(`### Table: \`${table.name}\``);
    schemaDocLines.push("| Column | Type | Nullable | Details |");
    schemaDocLines.push("| :--- | :--- | :---: | :--- |");

    for (const col of table.columns) {
      const tsType = mapSqlTypeToTs(col.udtName, col.dataType);
      const nullableBadge = col.isNullable ? "✅ Yes" : "❌ No";
      const details: string[] = [];

      if (col.isPrimaryKey) details.push("🔑 **Primary Key**");
      if (col.foreignKey) {
        details.push(
          `🔗 FK \`-> ${col.foreignKey.foreignTable}.${col.foreignKey.foreignColumn}\``
        );
      }
      if (col.columnDefault) {
        details.push(`Default: \`${col.columnDefault}\``);
      }

      schemaDocLines.push(
        `| \`${col.name}\` | \`${col.dataType}\` (\`${tsType}\`) | ${nullableBadge} | ${details.join(", ") || "-"} |`
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
        value: "types/db.d.ts (Kysely typed)",
        color: colors.cyan,
      },
      {
        label: "SQL Schema",
        value: "schema.sql (DDL)",
        color: colors.yellow,
      },
      {
        label: "AI Guidelines",
        value: "AGENTS.md (Schema injected)",
        color: colors.sky,
      },
    ]);

    logger.success(
      `Successfully synced database schema! AI coding assistants (Antigravity/Cursor/Copilot) now have full visibility into your database.`
    );
  } catch (err: any) {
    logger.error(`Database introspection failed: ${err.message || String(err)}`);
    process.exit(1);
  }
}
