import { existsSync, mkdirSync, readFileSync, writeFileSync } from "fs";
import { join } from "path";

/**
 * Generates the .nata/core.ts source code with automatic in-memory mockDB
 * for tests and real Postgres / PGlite in production/dev.
 */
export function generateCoreRuntimeCode(): string {
  return `import {
  Kysely,
  PostgresAdapter,
  PostgresIntrospector,
  PostgresQueryCompiler,
  type Driver,
  type DatabaseConnection,
  type QueryResult as KyselyQueryResult,
  type CompiledQuery,
  sql,
} from "kysely";
import { existsSync, readFileSync } from "fs";
import { join } from "path";

export { sql };

export class HttpError extends Error {
  readonly statusCode: number;

  constructor(message: string, statusCode: number = 400) {
    super(message);
    this.name = "HttpError";
    this.statusCode = statusCode;
    Object.setPrototypeOf(this, HttpError.prototype);
  }

  static badRequest(message: string): HttpError { return new HttpError(message, 400); }
  static unauthorized(message = "Unauthorized"): HttpError { return new HttpError(message, 401); }
  static forbidden(message = "Forbidden"): HttpError { return new HttpError(message, 403); }
  static notFound(message: string): HttpError { return new HttpError(message, 404); }
  static conflict(message: string): HttpError { return new HttpError(message, 409); }
  static unprocessable(message: string): HttpError { return new HttpError(message, 422); }
  static internal(message = "Internal server error"): HttpError { return new HttpError(message, 500); }

  isClientError(): boolean { return this.statusCode >= 400 && this.statusCode < 500; }
  isServerError(): boolean { return this.statusCode >= 500; }
  getStatusCode(): number { return this.statusCode; }
}

class LoggingConnection implements DatabaseConnection {
  constructor(private readonly inner: DatabaseConnection) {}

  async executeQuery<R>(compiledQuery: CompiledQuery): Promise<KyselyQueryResult<R>> {
    const start = performance.now();
    const { sql: sqlStr, parameters } = compiledQuery;
    try {
      const res = await this.inner.executeQuery<R>(compiledQuery);
      const duration = Number((performance.now() - start).toFixed(2));
      const rows = res.rows || [];
      const rowCount = Array.isArray(rows) ? rows.length : (res.numAffectedRows !== undefined ? Number(res.numAffectedRows) : 0);
      
      const queryLog = {
        id: "sql_" + Math.random().toString(36).slice(2, 9),
        type: "sql",
        sql: sqlStr,
        parameters: parameters || [],
        response: rows,
        rowCount: rowCount,
        duration_ms: duration,
        status: "success",
        error: null,
        timestamp: new Date().toISOString()
      };
      const logStore = (globalThis as any).__NATA_QUERY_LOGS__ || ((globalThis as any).__NATA_QUERY_LOGS__ = []);
      logStore.push(queryLog);

      if (process.env.DEBUG_SQL === "true" || process.env.DEBUG_SQL === "1") {
        const paramStr = parameters && parameters.length > 0 ? \` [\${parameters.map(p => JSON.stringify(p)).join(", ")}]\` : "";
        console.log(\`\\x1b[36m⚡ [SQL]\\x1b[0m \\x1b[1m\${sqlStr}\\x1b[0m\\x1b[90m\${paramStr} (\${duration}ms)\\x1b[0m\`);
      }
      return res;
    } catch (err: any) {
      const duration = Number((performance.now() - start).toFixed(2));
      const paramStr = parameters && parameters.length > 0 ? \` [\${parameters.map(p => JSON.stringify(p)).join(", ")}]\` : "";
      
      const queryLog = {
        id: "sql_" + Math.random().toString(36).slice(2, 9),
        type: "sql",
        sql: sqlStr,
        parameters: parameters || [],
        response: null,
        rowCount: 0,
        duration_ms: duration,
        status: "error",
        error: err?.message || String(err),
        timestamp: new Date().toISOString()
      };
      const logStore = (globalThis as any).__NATA_QUERY_LOGS__ || ((globalThis as any).__NATA_QUERY_LOGS__ = []);
      logStore.push(queryLog);

      console.error(\`\\x1b[31m✖ [SQL Error]\\x1b[0m \\x1b[1m\${sqlStr}\\x1b[0m\\x1b[90m\${paramStr} (\${duration}ms)\\x1b[0m\`);
      console.error(\`\\x1b[31m  ↳ Error:\\x1b[0m \${err?.message || err}\`);
      throw err;
    }
  }

  async *streamQuery<R>(compiledQuery: CompiledQuery, chunkSize?: number): AsyncIterableIterator<KyselyQueryResult<R>> {
    yield* this.inner.streamQuery(compiledQuery, chunkSize);
  }
}

class LoggingDriver implements Driver {
  constructor(private readonly inner: Driver) {}

  async init(): Promise<void> { await this.inner.init(); }
  async acquireConnection(): Promise<DatabaseConnection> {
    const conn = await this.inner.acquireConnection();
    return new LoggingConnection(conn);
  }
  async releaseConnection(connection: DatabaseConnection): Promise<void> {
    const unwrapped = (connection as any).inner || connection;
    await this.inner.releaseConnection(unwrapped);
  }
  async beginTransaction(connection: DatabaseConnection, settings: any): Promise<void> {
    const unwrapped = (connection as any).inner || connection;
    await this.inner.beginTransaction(unwrapped, settings);
  }
  async commitTransaction(connection: DatabaseConnection): Promise<void> {
    const unwrapped = (connection as any).inner || connection;
    await this.inner.commitTransaction(unwrapped);
  }
  async rollbackTransaction(connection: DatabaseConnection): Promise<void> {
    const unwrapped = (connection as any).inner || connection;
    await this.inner.rollbackTransaction(unwrapped);
  }
  async destroy(): Promise<void> { await this.inner.destroy(); }
}

let dbInstance: Kysely<any> | null = null;
let rawPgInstance: any = null;

/**
 * Creates in-memory SQLite / PGlite database driver that runs SQL DDL from schema.sql
 */
function createInMemoryDbDriver(): Driver {
  let sqliteDb: any = null;
  try {
    const { Database: BunDatabase } = require("bun:sqlite");
    sqliteDb = new BunDatabase(":memory:");
  } catch {}

  // Automatically execute schema.sql in memory if available
  if (sqliteDb) {
    const currentDir = typeof import.meta !== "undefined" && import.meta.dir ? import.meta.dir : (typeof __dirname !== "undefined" ? __dirname : process.cwd());
    const candidatePaths = [
      process.env.COM_SCHEMA_PATH,
      join(currentDir, "..", "schema.sql"),
      join(currentDir, "schema.sql"),
      join(process.cwd(), "schema.sql"),
      join(process.cwd(), "data", "schema.sql"),
    ].filter(Boolean) as string[];

    for (const fullPath of candidatePaths) {
      if (existsSync(fullPath)) {
        try {
          let ddl = readFileSync(fullPath, "utf-8");
          // Sanitize Postgres-specific DDL for in-memory SQLite compatibility
          ddl = ddl
            .replace(/COMMENT ON (TABLE|COLUMN)[^;]+;/gi, "")
            .replace(/SERIAL PRIMARY KEY/gi, "INTEGER PRIMARY KEY AUTOINCREMENT")
            .replace(/BIGSERIAL PRIMARY KEY/gi, "INTEGER PRIMARY KEY AUTOINCREMENT")
            .replace(/SERIAL/gi, "INTEGER")
            .replace(/BIGSERIAL/gi, "INTEGER")
            .replace(/TIMESTAMP WITH(OUT)? TIME ZONE/gi, "TEXT")
            .replace(/TIMESTAMPTZ/gi, "TEXT")
            .replace(/TIMESTAMP/gi, "TEXT")
            .replace(/JSONB?/gi, "TEXT")
            .replace(/BOOLEAN/gi, "INTEGER")
            .replace(/now\\(\\)/gi, "CURRENT_TIMESTAMP")
            .replace(/gen_random_uuid\\(\\)/gi, "(hex(randomblob(16)))");
          sqliteDb.exec(ddl);
          break;
        } catch {}
      }
    }
  }

  class InMemoryConnection implements DatabaseConnection {
    async executeQuery<R>(compiledQuery: CompiledQuery): Promise<KyselyQueryResult<R>> {
      const { sql: sqlStr, parameters } = compiledQuery;
      if (!sqliteDb) return { rows: [] };

      try {
        let transformedSql = sqlStr;
        // Transform postgres positional params $1, $2 to sqlite ?
        transformedSql = transformedSql.replace(/\\$([0-9]+)/g, "?");
        
        // Handle RETURNING clause in SQLite
        const isInsert = /^\\s*insert/i.test(transformedSql);
        const isUpdate = /^\\s*update/i.test(transformedSql);
        const isDelete = /^\\s*delete/i.test(transformedSql);

        if (isInsert || isUpdate || isDelete) {
          const stmt = sqliteDb.prepare(transformedSql.replace(/RETURNING\\s+.*?$/i, ""));
          const res = stmt.run(...(parameters as any[]));
          
          if (/RETURNING/i.test(sqlStr)) {
            const tableRegex = new RegExp('(?:into|update|from)\\\\s+["\\\`]?([a-zA-Z0-9_]+)', 'i');
            const tableMatch = sqlStr.match(tableRegex);
            if (tableMatch && isInsert && res.lastInsertRowid) {
              const fetchStmt = sqliteDb.prepare("SELECT * FROM " + tableMatch[1] + " WHERE rowid = ?");
              const row = fetchStmt.get(res.lastInsertRowid);
              return { rows: row ? [row] as R[] : [], numAffectedRows: BigInt(res.changes || 1) };
            }
          }
          return { rows: [] as R[], numAffectedRows: BigInt(res.changes || 0) };
        }

        const stmt = sqliteDb.prepare(transformedSql);
        const rows = stmt.all(...(parameters as any[])) as R[];
        return { rows: rows || [] };
      } catch (err: any) {
        // Fallback or rethrow
        throw err;
      }
    }
    async *streamQuery<R>(): AsyncIterableIterator<KyselyQueryResult<R>> {
      throw new Error("streamQuery not supported on InMemoryConnection");
    }
  }

  return {
    async init() {},
    async acquireConnection() { return new InMemoryConnection(); },
    async releaseConnection() {},
    async beginTransaction() {},
    async commitTransaction() {},
    async rollbackTransaction() {},
    async destroy() {},
  };
}

function createDbInstance(): Kysely<any> {
  const isTestMode = process.env.NODE_ENV === "test" || process.env.COM_TEST === "true";
  const dbUrl = !isTestMode ? (process.env.DATABASE_URL || process.env.POSTGRES_URL) : null;

  if (dbUrl) {
    try {
      const { Pool } = require("pg");
      const { PostgresDialect } = require("kysely");
      const baseDialect = new PostgresDialect({
        pool: new Pool({
          connectionString: dbUrl,
          max: 5,
          idleTimeoutMillis: 1000,
          connectionTimeoutMillis: 5000,
        }),
      });
      return new Kysely<any>({
        dialect: {
          createDriver: () => new LoggingDriver(baseDialect.createDriver()),
          createQueryCompiler: () => baseDialect.createQueryCompiler(),
          createAdapter: () => baseDialect.createAdapter(),
          createIntrospector: (d) => baseDialect.createIntrospector(d),
        },
      });
    } catch {}
  }

  // PGlite or In-Memory MockDB engine
  try {
    let PGliteClass: any = null;
    try { PGliteClass = require("@electric-sql/pglite").PGlite; } catch {}
    if (!PGliteClass) {
      try { PGliteClass = require("@pglite/core").PGlite; } catch {}
    }

    if (PGliteClass) {
      const pgliteInstance = isTestMode ? new PGliteClass() : new PGliteClass(process.env.DB_PATH || "data/app.db");
      rawPgInstance = pgliteInstance;

      // Auto-load schema.sql in memory for tests
      const currentDir = typeof import.meta !== "undefined" && import.meta.dir ? import.meta.dir : (typeof __dirname !== "undefined" ? __dirname : process.cwd());
      const candidatePaths = [
        process.env.COM_SCHEMA_PATH,
        join(currentDir, "..", "schema.sql"),
        join(currentDir, "schema.sql"),
        join(process.cwd(), "schema.sql"),
        join(process.cwd(), "data", "schema.sql"),
      ].filter(Boolean) as string[];

      for (const sp of candidatePaths) {
        if (existsSync(sp)) {
          try {
            const ddl = readFileSync(sp, "utf-8");
            pgliteInstance.exec(ddl).catch(() => {});
            break;
          } catch {}
        }
      }

      class PGLiteConnection implements DatabaseConnection {
        async executeQuery<R>(compiledQuery: CompiledQuery): Promise<KyselyQueryResult<R>> {
          const { sql: sqlStr, parameters } = compiledQuery;
          const res: any = await pgliteInstance.query(sqlStr, parameters as unknown[]);
          const rows = (Array.isArray(res?.rows) ? res.rows : (Array.isArray(res) ? res : [])) as R[];
          return {
            rows,
            numAffectedRows: res?.affectedRows !== undefined ? BigInt(res.affectedRows) : undefined,
          };
        }
        async *streamQuery<R>(): AsyncIterableIterator<KyselyQueryResult<R>> {
          throw new Error("PGLite streamQuery not supported");
        }
      }

      class PGLiteDriver implements Driver {
        async init(): Promise<void> {}
        async acquireConnection(): Promise<DatabaseConnection> { return new PGLiteConnection(); }
        async releaseConnection(): Promise<void> {}
        async beginTransaction(connection: DatabaseConnection): Promise<void> {
          await connection.executeQuery({ sql: "BEGIN", parameters: [], query: {} as any });
        }
        async commitTransaction(connection: DatabaseConnection): Promise<void> {
          await connection.executeQuery({ sql: "COMMIT", parameters: [], query: {} as any });
        }
        async rollbackTransaction(connection: DatabaseConnection): Promise<void> {
          await connection.executeQuery({ sql: "ROLLBACK", parameters: [], query: {} as any });
        }
        async destroy(): Promise<void> {}
      }

      return new Kysely<any>({
        dialect: {
          createDriver: () => new LoggingDriver(new PGLiteDriver()),
          createQueryCompiler: () => new PostgresQueryCompiler(),
          createAdapter: () => new PostgresAdapter(),
          createIntrospector: (d) => new PostgresIntrospector(d),
        },
      });
    }
  } catch {}

  // Built-in Bun In-Memory SQLite Bridge
  return new Kysely<any>({
    dialect: {
      createDriver: () => new LoggingDriver(createInMemoryDbDriver()),
      createQueryCompiler: () => new PostgresQueryCompiler(),
      createAdapter: () => new PostgresAdapter(),
      createIntrospector: (d) => new PostgresIntrospector(d),
    },
  });
}

export const db = new Proxy({} as Kysely<any>, {
  get(target, prop, receiver) {
    if (!dbInstance) dbInstance = createDbInstance();
    const val = (dbInstance as any)[prop];
    return typeof val === "function" ? val.bind(dbInstance) : val;
  },
});

/**
 * Helper to reset/truncate tables during testing
 */
export async function truncateTables(tableNames: string[]) {
  if (rawPgInstance) {
    for (const tbl of tableNames) {
      await rawPgInstance.exec('TRUNCATE TABLE "' + tbl + '" CASCADE;');
    }
  } else if (dbInstance) {
    for (const tbl of tableNames) {
      await (dbInstance as any).deleteFrom(tbl).execute();
    }
  }
}

class PubSub {
  private readonly channels = new Map<string, Set<(event: string, data: any) => void>>();

  subscribe(channel: string, listener: (event: string, data: any) => void): () => void {
    if (!this.channels.has(channel)) this.channels.set(channel, new Set());
    const set = this.channels.get(channel)!;
    set.add(listener);
    return () => {
      set.delete(listener);
      if (set.size === 0) this.channels.delete(channel);
    };
  }

  async publish(channel: string, event: string, data: any): Promise<void> {
    const set = this.channels.get(channel);
    if (!set || set.size === 0) return;
    for (const listener of set) {
      try { listener(event, data); } catch (err) { console.error("[pubsub] Error:", err); }
    }
  }

  clear(channel: string): void { this.channels.delete(channel); }
}
export const pubsub = new PubSub();

export interface UploadOptions {
  path?: string;
  filename?: string;
  acl?: "public-read" | "private";
}

export async function upload(file: any, options: UploadOptions = {}) {
  const uploadPath = options.path || "uploads";
  const name = (file && typeof file === "object" && file.name) ? file.name : "file.bin";
  const ext = name.split(".").pop() || "bin";
  const key = uploadPath + "/" + Date.now() + "-" + Math.random().toString(36).slice(2) + "." + ext;
  return {
    url: "https://cdn.myworkbeast.com/" + key,
    key,
    name,
    size: (file && file.size) || 0,
    uploadedAt: new Date().toISOString(),
  };
}

export const connections = {
  get: () => ({ status: "active", database: "connected" }),
  ping: async () => true,
};

export function json(data: any, init: { status?: number; headers?: Record<string, string> } = {}) {
  return new Response(JSON.stringify(data), {
    status: init.status || 200,
    headers: { "Content-Type": "application/json", ...(init.headers || {}) },
  });
}

export async function cookies() {
  const { CookieStore } = require("./headers");
  return new CookieStore();
}

export async function auth() {
  return { id: "1", name: "Admin User", email: "admin@example.com", role: "admin" };
}

export default {
  db,
  sql,
  HttpError,
  pubsub,
  upload,
  connections,
  cookies,
  json,
  auth,
  truncateTables,
};
`;
}

/**
 * Ensures that the project has the canonical runtime environment:
 * - `.nata/core.ts`
 * - `.nata/headers.ts`
 * - `node_modules/core/package.json`, `index.js`, `index.d.ts`
 * - `node_modules/next/headers`, `navigation`
 */
export function ensureRuntimeEnvironment(projectDir: string) {
  const nataDir = join(projectDir, ".nata");
  if (!existsSync(nataDir)) {
    mkdirSync(nataDir, { recursive: true });
  }

  // 1. Write .nata/core.ts
  const coreTsPath = join(nataDir, "core.ts");
  writeFileSync(coreTsPath, generateCoreRuntimeCode());

  // 2. Write .nata/headers.ts
  const headersTs = `export class CookieStore {
  private store: Map<string, any> = (globalThis as any).__NATA_COOKIES__ || ((globalThis as any).__NATA_COOKIES__ = new Map<string, any>());
  get(name: string) {
    const val = this.store.get(name);
    return val !== undefined ? { name, value: String(val) } : undefined;
  }
  getAll(name?: string) {
    const res: Array<{ name: string; value: string }> = [];
    for (const [k, v] of this.store.entries()) {
      if (!name || k === name) res.push({ name: k, value: String(v) });
    }
    return res;
  }
  set(name: string, value: string) { this.store.set(name, value); }
  delete(name: string) { this.store.delete(name); }
}

export async function cookies() { return new CookieStore(); }
export async function headers() { return new Headers(); }
export async function draftMode() { return { isEnabled: false, enable() {}, disable() {} }; }
`;
  writeFileSync(join(nataDir, "headers.ts"), headersTs);

  // 3. Write node_modules/core
  const nmCore = join(projectDir, "node_modules", "core");
  if (!existsSync(nmCore)) {
    mkdirSync(nmCore, { recursive: true });
  }
  writeFileSync(
    join(nmCore, "package.json"),
    JSON.stringify({
      name: "core",
      version: "1.0.0",
      main: "../../.nata/core.ts",
      module: "../../.nata/core.ts",
      types: "../../.nata/core.ts",
    }, null, 2)
  );
  writeFileSync(
    join(nmCore, "index.js"),
    `module.exports = require("../../.nata/core.ts");\n`
  );
}
