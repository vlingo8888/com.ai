use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct RpcPayload {
    pub module: String,
    pub action: Option<String>,
    pub args: Option<serde_json::Value>,
    #[serde(default)]
    pub cookies: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct SetCookieInfo {
    pub name: String,
    pub value: String,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default, rename = "maxAge")]
    pub max_age: Option<i64>,
    #[serde(default)]
    pub expires: Option<String>,
    #[serde(default)]
    pub domain: Option<String>,
    #[serde(default)]
    pub secure: Option<bool>,
    #[serde(default, rename = "sameSite")]
    pub same_site: Option<String>,
    #[serde(default)]
    pub deleted: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RpcExecutionOutput {
    pub data: serde_json::Value,
    #[serde(default)]
    pub set_cookies: Vec<SetCookieInfo>,
    #[serde(default)]
    pub queries: Vec<serde_json::Value>,
}

#[derive(Debug, Serialize)]
pub struct RpcResponse {
    pub success: bool,
    pub data: Option<serde_json::Value>,
    pub error: Option<String>,
}

pub struct RpcExecutor;

impl RpcExecutor {
    /// Ensures that the project has the canonical NATA runtime files (.nata/core.ts, node_modules/core, etc.)
    pub fn ensure_runtime_env<P: AsRef<Path>>(project_dir: P) {
        let root = project_dir.as_ref();
        // Called on every render/action/route request: write the files once per project per process
        static PREPARED: std::sync::OnceLock<std::sync::Mutex<std::collections::HashSet<std::path::PathBuf>>> =
            std::sync::OnceLock::new();
        let prepared = PREPARED.get_or_init(Default::default);
        if !prepared.lock().map(|mut set| set.insert(root.to_path_buf())).unwrap_or(true) {
            return;
        }
        let nata_dir = root.join(".nata");
        let _ = std::fs::create_dir_all(&nata_dir);
        let _ = std::fs::create_dir_all(root.join("data"));

        // 1. Write .nata/core.ts
        let core_ts = r#"import {
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
        const paramStr = parameters && parameters.length > 0 ? ` [${parameters.map(p => JSON.stringify(p)).join(", ")}]` : "";
        console.log(`\x1b[36m⚡ [SQL]\x1b[0m \x1b[1m${sqlStr}\x1b[0m\x1b[90m${paramStr} (${duration}ms)\x1b[0m`);
        
        let resPreview = "";
        if (Array.isArray(rows)) {
          if (rows.length === 0) {
            resPreview = "[] (0 rows)";
          } else if (rows.length <= 5) {
            resPreview = JSON.stringify(rows);
          } else {
            resPreview = `${JSON.stringify(rows.slice(0, 3))} ... (+${rows.length - 3} more rows, total: ${rows.length})`;
          }
        } else if (res.numAffectedRows !== undefined) {
          resPreview = `affectedRows: ${res.numAffectedRows}`;
        } else {
          resPreview = JSON.stringify(rows);
        }
        console.log(`\x1b[90m↳ [Result: ${rowCount} row(s)]\x1b[0m ${resPreview}`);
      }
      return res;
    } catch (err: any) {
      const duration = Number((performance.now() - start).toFixed(2));
      const paramStr = parameters && parameters.length > 0 ? ` [${parameters.map(p => JSON.stringify(p)).join(", ")}]` : "";
      
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

      if (process.env.COM_DEV_QUERY_LOG !== "1") {
        console.error(`\x1b[31m✖ [SQL Error]\x1b[0m \x1b[1m${sqlStr}\x1b[0m\x1b[90m${paramStr} (${duration}ms)\x1b[0m`);
        console.error(`\x1b[31m  ↳ Error:\x1b[0m ${err?.message || err}`);
      }
      if (err?.code === "ECONNREFUSED" || err?.name === "AggregateError" || (err?.message && String(err.message).includes("ECONNREFUSED"))) {
        console.error(`\x1b[33m  💡 [Tip]\x1b[0m Server PostgreSQL offline. Chạy 'com dev' để chuyển sang PGlite hoặc comment DATABASE_URL trong .env.\x1b[0m`);
      }
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

function createDbInstance(): Kysely<any> {
  const dbUrl = process.env.DATABASE_URL || process.env.POSTGRES_URL;
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

  try {
    const fs = require("fs");
    const path = require("path");
    const dbPath = process.env.DB_PATH || "data/app.db";
    const resolvedDbPath = path.resolve(process.cwd(), dbPath);
    const parentDir = path.dirname(resolvedDbPath);
    if (!fs.existsSync(parentDir)) {
      try { fs.mkdirSync(parentDir, { recursive: true }); } catch {}
    }

    const { PGlite } = require("@electric-sql/pglite");
    // Created on the first query: loading PGlite's WASM costs tens of MB, and many requests never query
    let pgliteInstance: any = null;
    const getPglite = () => (pgliteInstance ??= new PGlite(dbPath));

    let schemaInitPromise: Promise<void> | null = null;
    const ensureSchema = () => (schemaInitPromise ??= initSchemaIfNeeded());
    async function initSchemaIfNeeded() {
      const pgliteInstance = getPglite();
      const candidatePaths = [
        process.env.COM_SCHEMA_PATH,
        path.join(process.cwd(), "schema.sql"),
        path.join(process.cwd(), "data", "schema.sql"),
        path.join(process.cwd(), "migrations", "00_base_schema.sql"),
        path.join(process.cwd(), "core", "database", "00_base_schema.sql"),
      ].filter(Boolean);

      for (const sp of candidatePaths) {
        if (fs.existsSync(sp)) {
          try {
            const ddl = fs.readFileSync(sp, "utf-8");
            await pgliteInstance.exec(ddl);
            break;
          } catch (e: any) {
            // Ignore table already exists or minor DDL warnings
          }
        }
      }

      // Also auto-run any additional migration files in migrations/*.sql
      const migrationsDir = path.join(process.cwd(), "migrations");
      if (fs.existsSync(migrationsDir)) {
        try {
          const files = fs.readdirSync(migrationsDir).filter((f: string) => f.endsWith(".sql") && f !== "00_base_schema.sql").sort();
          for (const file of files) {
            try {
              const ddl = fs.readFileSync(path.join(migrationsDir, file), "utf-8");
              await pgliteInstance.exec(ddl);
            } catch (_) {}
          }
        } catch (_) {}
      }
    }
    class PGLiteConnection implements DatabaseConnection {
      async executeQuery<R>(compiledQuery: CompiledQuery): Promise<KyselyQueryResult<R>> {
        await ensureSchema();
        const { sql: sqlStr, parameters } = compiledQuery;
        const res: any = await getPglite().query(sqlStr, parameters as unknown[]);
        const rows = (Array.isArray(res?.rows) ? res.rows : (Array.isArray(res) ? res : [])) as R[];
        return {
          rows,
          numAffectedRows: res?.rowCount !== undefined ? BigInt(res.rowCount) : (res?.affectedRows !== undefined ? BigInt(res.affectedRows) : undefined),
        };
      }
      async *streamQuery<R>(): AsyncIterableIterator<KyselyQueryResult<R>> {
        throw new Error("PGLite streamQuery not supported");
      }
    }

    // PGlite is a single connection. Requests run concurrently in the dev worker, so hand it out like a
    // pool of size 1: a transaction holds it until commit/rollback and other requests wait their turn.
    // Re-entrant per request (`__NATA_REQUEST__`, set by the worker): a query from the request already
    // holding the connection (e.g. `db` used inside its own transaction) never waits on itself.
    let connectionQueue: Promise<void> = Promise.resolve();
    let holder: unknown = undefined;
    let depth = 0;
    let releaseHolder: (() => void) | null = null;

    class PGLiteDriver implements Driver {
      async init(): Promise<void> {}
      async acquireConnection(): Promise<DatabaseConnection> {
        const owner = (globalThis as any).__NATA_REQUEST__;
        // Outside the worker there is no owner and no concurrency: behave like an unlocked connection
        if (depth > 0 && holder === owner) {
          depth++;
          return new PGLiteConnection();
        }
        let release!: () => void;
        const turn = new Promise<void>((resolve) => (release = resolve));
        const previous = connectionQueue;
        connectionQueue = previous.then(() => turn);
        await previous;
        holder = owner;
        depth = 1;
        releaseHolder = release;
        return new PGLiteConnection();
      }
      async releaseConnection(): Promise<void> {
        depth = Math.max(0, depth - 1);
        if (depth === 0 && releaseHolder) {
          const release = releaseHolder;
          releaseHolder = null;
          holder = undefined;
          release();
        }
      }
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
  } catch (err: any) {
    console.warn("\x1b[33m[NATA Core] Database driver initialization warning (PG/PGlite not available or DATABASE_URL not set). Falling back to mock driver.\x1b[0m", err?.message || err);
    class MockDriver implements Driver {
      async init(): Promise<void> {}
      async acquireConnection(): Promise<DatabaseConnection> {
        return {
          executeQuery: async (compiledQuery) => {
            console.warn(`\x1b[33m[NATA Core MockDB]\x1b[0m Executing query on mock driver (returns empty rows):`, compiledQuery.sql);
            return { rows: [] };
          },
          streamQuery: async function* () { yield { rows: [] }; },
        };
      }
      async releaseConnection(): Promise<void> {}
      async beginTransaction(): Promise<void> {}
      async commitTransaction(): Promise<void> {}
      async rollbackTransaction(): Promise<void> {}
      async destroy(): Promise<void> {}
    }

    return new Kysely<any>({
      dialect: {
        createDriver: () => new LoggingDriver(new MockDriver()),
        createQueryCompiler: () => new PostgresQueryCompiler(),
        createAdapter: () => new PostgresAdapter(),
        createIntrospector: (d) => new PostgresIntrospector(d),
      },
    });
  }
}

let functionModule: any = null;
export const db = new Proxy({} as Kysely<any>, {
  get(target, prop, receiver) {
    if (prop === "fn") {
      if (!functionModule) {
        try {
          const { createFunctionModule } = require("kysely");
          functionModule = createFunctionModule();
        } catch {}
      }
      return functionModule;
    }
    if (!dbInstance) dbInstance = createDbInstance();
    const val = (dbInstance as any)[prop];
    return typeof val === "function" ? val.bind(dbInstance) : val;
  },
});

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
      try { listener(event, data); } catch (err) { console.error(`[pubsub] Error:`, err); }
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
  const key = `${uploadPath}/${Date.now()}-${Math.random().toString(36).slice(2)}.${ext}`;

  if (process.env.AWS_S3_BUCKET && process.env.AWS_ACCESS_KEY_ID) {
    try {
      const { S3Client, PutObjectCommand } = await import("@aws-sdk/client-s3");
      const client = new S3Client({
        region: process.env.AWS_REGION || "ap-southeast-1",
        credentials: {
          accessKeyId: process.env.AWS_ACCESS_KEY_ID!,
          secretAccessKey: process.env.AWS_SECRET_ACCESS_KEY!,
        },
      });
      const buffer = file instanceof Blob ? Buffer.from(await file.arrayBuffer()) : Buffer.from(file);
      await client.send(
        new PutObjectCommand({
          Bucket: process.env.AWS_S3_BUCKET!,
          Key: key,
          Body: buffer,
          ContentType: (file && file.type) || "application/octet-stream",
          ACL: options.acl || "public-read",
        })
      );
      const url = `https://${process.env.AWS_S3_BUCKET}.s3.${process.env.AWS_REGION || "ap-southeast-1"}.amazonaws.com/${key}`;
      return { url, key, size: buffer.length };
    } catch (e) {}
  }

  return {
    url: typeof file === "string" ? file : `/_nata/uploads/${key}`,
    key,
    size: (file && file.size) || 1024,
  };
}

export const connections = {
  async executeCapability(connectionId: number, capabilityKey: string, args: Record<string, any>) {
    const baseUrl = process.env.NEXT_PUBLIC_APP_URL || "";
    if (baseUrl) {
      try {
        const response = await fetch(`${baseUrl}/api/connections/${connectionId}/execute`, {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ capability: capabilityKey, args }),
        });
        if (response.ok) return await response.json();
      } catch {}
    }
    return { success: true, connectionId, capabilityKey, data: args };
  },
  async listConnections(providerId?: string | number) {
    const baseUrl = process.env.NEXT_PUBLIC_APP_URL || "";
    if (baseUrl) {
      try {
        const response = await fetch(`${baseUrl}/api/connections?provider_id=${providerId || ""}`);
        if (response.ok) return await response.json();
      } catch {}
    }
    return [];
  },
  async execute(...args: any[]) { return { success: true, data: {} }; },
  async get(...args: any[]) { return {}; },
};

export class CookieStore {
  private store: Map<string, any>;
  constructor() {
    this.store = (globalThis as any).__NATA_COOKIES__ || ((globalThis as any).__NATA_COOKIES__ = new Map<string, any>());
  }
  get(name: string) {
    const val = this.store.get(name);
    return val !== undefined ? { name, value: String(val) } : undefined;
  }
  getAll(name?: string) {
    const res: Array<{ name: string; value: string }> = [];
    for (const [k, v] of this.store.entries()) {
      if (!name || k === name) {
        res.push({ name: k, value: String(v) });
      }
    }
    return res;
  }
  set(nameOrOptions: any, value?: string, options?: any) {
    let name = nameOrOptions;
    let val = value;
    let opts = options || {};
    if (typeof nameOrOptions === "object" && nameOrOptions !== null) {
      name = nameOrOptions.name;
      val = nameOrOptions.value;
      opts = nameOrOptions;
    }
    if (name) {
      this.store.set(name, String(val ?? ""));
      const setCookies = (globalThis as any).__NATA_SET_COOKIES__ || ((globalThis as any).__NATA_SET_COOKIES__ = []);
      setCookies.push({
        name,
        value: String(val ?? ""),
        path: opts.path || "/",
        maxAge: opts.maxAge,
        expires: opts.expires instanceof Date ? opts.expires.toUTCString() : opts.expires,
        domain: opts.domain,
        secure: opts.secure,
        sameSite: opts.sameSite || "Lax",
        deleted: false,
      });
    }
  }
  delete(nameOrOptions: any) {
    const name = typeof nameOrOptions === "object" ? nameOrOptions?.name : nameOrOptions;
    if (name) {
      this.store.delete(name);
      const setCookies = (globalThis as any).__NATA_SET_COOKIES__ || ((globalThis as any).__NATA_SET_COOKIES__ = []);
      setCookies.push({
        name,
        value: "",
        path: "/",
        maxAge: 0,
        expires: "Thu, 01 Jan 1970 00:00:00 GMT",
        deleted: true,
      });
    }
  }
  has(name: string) {
    return this.store.has(name);
  }
  clear() {
    for (const k of Array.from(this.store.keys())) {
      this.delete(k);
    }
  }
  get size() {
    return this.store.size;
  }
  toString() {
    return Array.from(this.store.entries()).map(([k, v]) => `${encodeURIComponent(k)}=${encodeURIComponent(String(v))}`).join("; ");
  }
}

export const cookies = async () => new CookieStore();

export function json(data: any, init?: ResponseInit | number): Response {
  const status = typeof init === "number" ? init : init?.status || 200;
  const headers = typeof init === "object" ? init.headers : undefined;
  return new Response(JSON.stringify(data), {
    status,
    headers: { "Content-Type": "application/json", ...headers },
  });
}

export async function auth(token?: string) {
  if (token) {
    try {
      const jwt = await import("jsonwebtoken");
      const decoded: any = jwt.decode(token);
      if (decoded && typeof decoded === "object") {
        return { id: decoded.userId || decoded.id || "1", ...decoded };
      }
    } catch {}
  }
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
};
"#;
        let _ = std::fs::write(nata_dir.join("core.ts"), core_ts);

        // 2-3. next/headers, next/server, next/navigation, next/cache: compiler-rs/src/next/
        crate::next_shims::write(root);


        // next-intl/server: compiler-rs/src/next/intl-server.ts
        let _ = std::fs::write(nata_dir.join("next-intl-server.ts"), include_str!("next/intl-server.ts"));

        // 4. Ensure node_modules/core has canonical NATA package files (overriding any broken ancient core package)
        let nm_core = root.join("node_modules").join("core");
        let _ = std::fs::create_dir_all(&nm_core);
        let _ = std::fs::write(
            nm_core.join("package.json"),
            r#"{"name":"core","version":"1.0.0","main":"../../.nata/core.ts","module":"../../.nata/core.ts","types":"../../.nata/core.ts"}"#,
        );
        let _ = std::fs::write(
            nm_core.join("index.js"),
            r#"module.exports = require("../../.nata/core.ts");"#,
        );

        // 5. Ensure node_modules/next/headers, server, navigation, and font mock files
        let nm_next = root.join("node_modules").join("next");
        let _ = std::fs::create_dir_all(&nm_next);
        let _ = std::fs::write(
            nm_next.join("package.json"),
            r#"{"name":"next","version":"14.2.5","main":"index.js","types":"index.d.ts"}"#,
        );
        let _ = std::fs::write(
            nm_next.join("index.js"),
            r#"module.exports = {};"#,
        );
        let _ = std::fs::write(
            nm_next.join("index.d.ts"),
            r#"export interface Metadata {
  title?: string | { default?: string; template?: string; absolute?: string };
  description?: string;
  applicationName?: string;
  authors?: Array<{ name: string; url?: string }> | { name: string; url?: string };
  generator?: string;
  keywords?: string | string[];
  referrer?: string;
  themeColor?: string | Array<{ media?: string; color: string }>;
  colorScheme?: 'dark' | 'light' | 'dark light';
  creator?: string;
  publisher?: string;
  robots?: string | { index?: boolean; follow?: boolean; nocache?: boolean; noarchive?: boolean; googleBot?: any };
  alternates?: { canonical?: string; languages?: Record<string, string> };
  icons?: string | { icon?: string | string[]; apple?: string | string[]; shortcut?: string };
  openGraph?: {
    title?: string;
    description?: string;
    url?: string;
    siteName?: string;
    images?: Array<{ url: string; width?: number; height?: number; alt?: string }> | string;
    locale?: string;
    type?: string;
  };
  twitter?: {
    card?: string;
    title?: string;
    description?: string;
    site?: string;
    creator?: string;
    images?: string | string[];
  };
  manifest?: string;
  other?: Record<string, string | number | (string | number)[]>;
  [key: string]: any;
}

export type ResolvingMetadata = Promise<Metadata>;

export namespace MetadataRoute {
  export type Robots = {
    rules: {
      userAgent?: string | string[];
      allow?: string | string[];
      disallow?: string | string[];
      crawlDelay?: number;
    } | Array<{
      userAgent?: string | string[];
      allow?: string | string[];
      disallow?: string | string[];
      crawlDelay?: number;
    }>;
    sitemap?: string | string[];
    host?: string;
  };

  export type Sitemap = Array<{
    url: string;
    lastModified?: string | Date;
    changeFrequency?: 'always' | 'hourly' | 'daily' | 'weekly' | 'monthly' | 'yearly' | 'never';
    priority?: number;
  }>;

  export type Manifest = {
    name?: string;
    short_name?: string;
    description?: string;
    start_url?: string;
    display?: 'fullscreen' | 'standalone' | 'minimal-ui' | 'browser';
    background_color?: string;
    theme_color?: string;
    icons?: Array<{
      src: string;
      sizes?: string;
      type?: string;
      purpose?: string;
    }>;
    [key: string]: any;
  };
}
"#,
        );


        // Server-renderable counterparts of the browser shims in `/_nata/shims/*`, so SSR markup matches hydration
        let nm_next_link = r#"const React = require('react');

function hrefString(href) {
  if (typeof href === 'string') return href;
  if (!href || typeof href !== 'object') return '#';
  const query = href.query && typeof href.query === 'object' ? new URLSearchParams(href.query).toString() : '';
  return (href.pathname || '') + (query ? '?' + query : '') + (href.hash ? (href.hash.startsWith('#') ? href.hash : '#' + href.hash) : '');
}

function withBasePath(url) {
  const base = globalThis.__NATA_BASE_PATH__ || '';
  return base && url.startsWith('/') && !url.startsWith('//') && url !== base && !url.startsWith(base + '/') ? base + url : url;
}

function Link({ href, children, onClick, onMouseEnter, target, replace, scroll, prefetch, shallow, passHref, legacyBehavior, locale, ...props }) {
  return React.createElement('a', { ...props, href: withBasePath(hrefString(href)), target }, children);
}

module.exports = Link;
module.exports.default = Link;
"#;
        let _ = std::fs::write(nm_next.join("link.js"), nm_next_link);


        let nm_next_head = r#"function Head() {
  return null;
}

module.exports = Head;
module.exports.default = Head;
"#;
        let _ = std::fs::write(nm_next.join("head.js"), nm_next_head);

        let nm_next_router = r#"exports.useRouter = function() {
  const loc = globalThis.__NATA_LOCATION__ || { pathname: '/', search: '' };
  const noop = function() {};
  return {
    push: noop,
    replace: noop,
    back: noop,
    forward: noop,
    reload: noop,
    prefetch: function() { return Promise.resolve(); },
    pathname: loc.pathname || '/',
    asPath: (loc.pathname || '/') + (loc.search || ''),
    query: { ...(globalThis.__NATA_PARAMS__ || {}) },
    events: { on: noop, off: noop, emit: noop }
  };
};
exports.default = { useRouter: exports.useRouter };
"#;
        let _ = std::fs::write(nm_next.join("router.js"), nm_next_router);


        let nm_next_font_google = nm_next.join("font").join("google");
        let _ = std::fs::create_dir_all(&nm_next_font_google);
        let nm_font_code = r#"const createFont = (name) => (opts = {}) => ({
  className: 'font-sans',
  variable: opts.variable || '--font-sans',
  style: { fontFamily: 'sans-serif' }
});

module.exports = new Proxy({}, {
  get: (target, prop) => {
    if (prop === '__esModule') return true;
    if (prop === 'default') return createFont('default');
    return createFont(String(prop));
  }
});
"#;
        let _ = std::fs::write(nm_next_font_google.join("index.js"), nm_font_code);

        let nm_next_font_local = nm_next.join("font").join("local");
        let _ = std::fs::create_dir_all(&nm_next_font_local);
        let _ = std::fs::write(nm_next_font_local.join("index.js"), nm_font_code);

        // 6. Ensure node_modules/zmp-sdk and zmp-sdk/apis shims exist
        let nm_zmp = root.join("node_modules").join("zmp-sdk");
        let _ = std::fs::create_dir_all(&nm_zmp);
        let _ = std::fs::create_dir_all(nm_zmp.join("apis"));
        let zmp_shim_code = r#"const mockUser = { id: "123456789", name: "Zalo Tester", avatar: "https://h5.zdn.vn/static/images/avatar.png" };
exports.getUserInfo = async () => ({ userInfo: mockUser });
exports.getPhoneNumber = async () => ({ number: "0912345678" });
exports.getSetting = async () => ({ authSetting: { "scope.userInfo": true, "scope.userLocation": true, "scope.userPhonenumber": true } });
exports.authorize = async () => ({ success: true });
exports.getLocation = async () => ({ latitude: 10.7769, longitude: 106.7009 });
exports.openChat = async () => ({ success: true });
exports.followOA = async () => ({ success: true });
exports.createOrder = async () => ({ orderId: "order_123", success: true });
exports.closeApp = async () => ({ success: true });
exports.showToast = async () => ({ success: true });
exports.getStorage = async () => ({ data: null });
exports.setStorage = async () => ({ success: true });
exports.removeStorage = async () => ({ success: true });
exports.clearStorage = async () => ({ success: true });
exports.getSystemInfo = async () => ({ platform: "browser", system: "Web", windowWidth: 390, windowHeight: 844 });
exports.default = {
  getUserInfo: exports.getUserInfo,
  getPhoneNumber: exports.getPhoneNumber,
  getSetting: exports.getSetting,
  authorize: exports.authorize,
  getLocation: exports.getLocation,
  openChat: exports.openChat,
  followOA: exports.followOA,
  createOrder: exports.createOrder,
  closeApp: exports.closeApp,
  showToast: exports.showToast,
  getStorage: exports.getStorage,
  setStorage: exports.setStorage,
  removeStorage: exports.removeStorage,
  clearStorage: exports.clearStorage,
  getSystemInfo: exports.getSystemInfo,
};
"#;
        let _ = std::fs::write(nm_zmp.join("index.js"), zmp_shim_code);
        let _ = std::fs::write(nm_zmp.join("apis").join("index.js"), zmp_shim_code);

        // 7. Ensure tsconfig.json has paths configured
        let tsconfig_path = root.join("tsconfig.json");
        if tsconfig_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&tsconfig_path) {
                if let Ok(mut json_val) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(obj) = json_val.as_object_mut() {
                        let compiler_options = obj
                            .entry("compilerOptions")
                            .or_insert_with(|| serde_json::json!({}));
                        if let Some(opts) = compiler_options.as_object_mut() {
                            opts.entry("baseUrl").or_insert_with(|| serde_json::json!("."));
                            let paths = opts
                                .entry("paths")
                                .or_insert_with(|| serde_json::json!({}));
                            if let Some(paths_obj) = paths.as_object_mut() {
                                paths_obj.insert("core".to_string(), serde_json::json!(["./.nata/core.ts"]));
                                paths_obj.insert("next/headers".to_string(), serde_json::json!(["./.nata/headers.ts"]));
                                paths_obj.insert("next/server".to_string(), serde_json::json!(["./.nata/server.ts"]));
                                paths_obj.insert("next-intl/server".to_string(), serde_json::json!(["./.nata/next-intl-server.ts"]));
                                if !paths_obj.contains_key("@/*") {
                                    paths_obj.insert("@/*".to_string(), serde_json::json!(["./*", "./src/*"]));
                                }
                                if !paths_obj.contains_key("~/*") {
                                    paths_obj.insert("~/*".to_string(), serde_json::json!(["./*", "./src/*"]));
                                }
                            }
                        }
                        if let Ok(updated_str) = serde_json::to_string_pretty(&json_val) {
                            if content.trim() != updated_str.trim() {
                                let _ = std::fs::write(&tsconfig_path, updated_str);
                            }
                        }
                    }
                }
            }
        } else {
            let standard_tsconfig = serde_json::json!({
                "compilerOptions": {
                    "target": "ESNext",
                    "module": "ESNext",
                    "moduleResolution": "bundler",
                    "baseUrl": ".",
                    "paths": {
                        "core": ["./.nata/core.ts"],
                        "next/headers": ["./.nata/headers.ts"],
                        "next/server": ["./.nata/server.ts"],
                        "next-intl/server": ["./.nata/next-intl-server.ts"],
                        "@/*": ["./*", "./src/*"],
                        "~/*": ["./*", "./src/*"]
                    }
                }
            });
            let _ = std::fs::write(&tsconfig_path, serde_json::to_string_pretty(&standard_tsconfig).unwrap());
        }

        let nm_next_intl = root.join("node_modules").join("next-intl");
        if nm_next_intl.exists() {
            let _ = std::fs::write(
                nm_next_intl.join("server.js"),
                r#"module.exports = require("../../.nata/next-intl-server.ts");"#,
            );
        }

        // 6. Ensure global.css <-> globals.css compatibility for Next.js starter templates
        crate::css_compiler::CssCompiler::sync_css_aliases(root);
    }

    /// Dispatches a Server Action / Backend Module execution request using Bun runtime, returning full output and set cookies
    pub async fn execute_full<P: AsRef<Path>>(
        project_dir: P,
        payload: RpcPayload,
    ) -> Result<RpcExecutionOutput, String> {
        let root_canon = dunce_canonicalize(project_dir.as_ref());
        let root = &root_canon;
        
        // Ensure runtime environment (.nata/core.ts, node_modules/core shim, tsconfig paths)
        Self::ensure_runtime_env(root);

        let ts_path = match Self::resolve_module_path(root, &payload.module) {
            Ok(p) => p,
            Err(e) => return Err(e),
        };

        let args_json = payload.args.unwrap_or(serde_json::json!([]));
        let action = payload.action.unwrap_or_else(|| "default".to_string());
        let clean = payload.module.trim_start_matches("./").trim_start_matches("@/").trim_start_matches("~/").trim_start_matches('/');

        let input = serde_json::json!({
            "tsPath": clean_path(&ts_path),
            "moduleSpecifier": crate::module_graph::server_module_specifier(root, &clean_path(&ts_path)),
            "action": action,
            "args": args_json,
            "cookiesStr": payload.cookies.unwrap_or_default(),
            "clean": clean,
            "cleanMod": clean.trim_start_matches("modules/").trim_start_matches("plugins/"),
        });

        let start_time = std::time::Instant::now();

        // Persistent Bun worker; a one-off process (same runtime, printing between delimiters) otherwise
        let (parsed, user_stdout, stdout, stderr, success) =
            match crate::worker_pool::call(root, "rpc", input.clone(), Vec::new()).await {
                Ok(reply) => {
                    let parsed = match reply.error() {
                        Some(err) => serde_json::json!({ "error": err }),
                        None => reply.output().clone(),
                    };
                    let success = reply.code() == 0 && reply.error().is_none();
                    (parsed, reply.stdout().to_string(), reply.stdout().to_string(), reply.stderr().to_string(), success)
                }
                Err(crate::worker_pool::WorkerError::Failed(e)) => {
                    (serde_json::json!({ "error": e }), String::new(), String::new(), String::new(), false)
                }
                Err(crate::worker_pool::WorkerError::Unavailable(_)) => {
                    let runner_script = format!(
                        r#"import {{ runRpc }} from {runtime};
const {{ payload, code }} = await runRpc({input});
process.stdout.write("__NATA_RPC_OUT_START__" + JSON.stringify(payload) + "__NATA_RPC_OUT_END__\n", () => process.exit(code));
// Open database handles would otherwise keep the process alive
setTimeout(() => process.exit(code), 10);
"#,
                        runtime = serde_json::to_string(&clean_path(root.join(".nata/rpc_runtime.ts"))).unwrap_or_default(),
                        input = input,
                    );
                    let output = crate::worker_pool::spawn_once(root, &runner_script).await.map_err(|msg| {
                        // Spawn failures never reach the per-call RPC log block, so report them here
                        crate::term::error("Server Action runtime failed to start", Some(&msg), None);
                        msg
                    })?;
                    let stdout = output.stdout;
                    // Separate user console logs from the delimited RPC response
                    let (user_stdout, json_str) = match (stdout.find("__NATA_RPC_OUT_START__"), stdout.find("__NATA_RPC_OUT_END__")) {
                        (Some(start), Some(end)) => (
                            format!("{}{}", &stdout[..start], &stdout[end + "__NATA_RPC_OUT_END__".len()..]),
                            stdout[start + "__NATA_RPC_OUT_START__".len()..end].to_string(),
                        ),
                        _ => (String::new(), stdout.trim().to_string()),
                    };
                    let parsed = serde_json::from_str(&json_str).unwrap_or(serde_json::Value::String(json_str));
                    (parsed, user_stdout, stdout, output.stderr, output.success)
                }
            };

        let elapsed = start_time.elapsed();
        let display_module = format_module_path(clean);
        // Query logs are attached to both success and error payloads.
        let queries: Vec<serde_json::Value> = parsed
            .get("__nata_queries__")
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default();
        let log_cfg = crate::dev_log::LogConfig::from_env();
        let duration_ms = elapsed.as_secs_f64() * 1000.0;

        if !success {
            let err_msg = match parsed.get("error").and_then(|m| m.as_str()) {
                Some(msg) => msg.to_string(),
                None if !stderr.trim().is_empty() => stderr.trim().to_string(),
                None if !stdout.trim().is_empty() => stdout.trim().to_string(),
                None => "Unknown execution failure in Bun".to_string(),
            };
            print!(
                "{}",
                crate::dev_log::render_rpc(
                    &crate::dev_log::RpcLogEntry {
                        module: &display_module,
                        action: &action,
                        duration_ms,
                        args: &args_json,
                        queries: &queries,
                        stdout: &user_stdout,
                        stderr: &stderr,
                        outcome: crate::dev_log::RpcOutcome::Err(&err_msg),
                    },
                    &log_cfg,
                )
            );
            return Err(err_msg);
        }

        let (data, set_cookies) = match parsed.as_object() {
            Some(obj) if obj.contains_key("__nata_rpc_data__") => (
                obj.get("__nata_rpc_data__").cloned().unwrap_or(serde_json::Value::Null),
                obj.get("__nata_set_cookies__")
                    .and_then(|v| serde_json::from_value::<Vec<SetCookieInfo>>(v.clone()).ok())
                    .unwrap_or_default(),
            ),
            _ => (parsed.clone(), vec![]),
        };

        print!(
            "{}",
            crate::dev_log::render_rpc(
                &crate::dev_log::RpcLogEntry {
                    module: &display_module,
                    action: &action,
                    duration_ms,
                    args: &args_json,
                    queries: &queries,
                    stdout: &user_stdout,
                    stderr: &stderr,
                    outcome: crate::dev_log::RpcOutcome::Ok(&data),
                },
                &log_cfg,
            )
        );

        Ok(RpcExecutionOutput { data, set_cookies, queries })
    }

    /// Dispatches a Server Action / Backend Module execution request using Bun runtime (data only)
    pub async fn execute<P: AsRef<Path>>(
        project_dir: P,
        payload: RpcPayload,
    ) -> Result<serde_json::Value, String> {
        let output = Self::execute_full(project_dir, payload).await?;
        Ok(output.data)
    }

    /// Resolves module specifier to an existing file path in the project
    pub fn resolve_module_path<P: AsRef<Path>>(
        project_dir: P,
        module_spec: &str,
    ) -> Result<std::path::PathBuf, String> {
        let root = project_dir.as_ref();
        let clean = module_spec
            .trim_start_matches("./")
            .trim_start_matches("@/")
            .trim_start_matches("~/")
            .trim_start_matches('/');

        let mut candidates = Vec::new();

        let prefixes = [
            "",
            "modules",
            "src",
            "src/modules",
            "actions",
            "src/actions",
            "app",
            "src/app",
        ];

        let clean_no_modules = clean
            .trim_start_matches("modules/")
            .trim_start_matches("src/")
            .trim_start_matches("actions/")
            .trim_start_matches("app/");

        for prefix in prefixes {
            let base_list = if prefix.is_empty() {
                vec![root.join(clean)]
            } else {
                vec![
                    root.join(prefix).join(clean),
                    root.join(prefix).join(clean_no_modules),
                ]
            };

            for base in base_list {
                let base_str = base.to_string_lossy().to_string();
                candidates.push(base.clone());
                candidates.push(std::path::PathBuf::from(format!("{}.ts", base_str)));
                candidates.push(std::path::PathBuf::from(format!("{}.tsx", base_str)));
                candidates.push(std::path::PathBuf::from(format!("{}.js", base_str)));
                candidates.push(std::path::PathBuf::from(format!("{}.mjs", base_str)));
                candidates.push(base.join("index.ts"));
                candidates.push(base.join("index.tsx"));
                candidates.push(base.join("index.js"));
                candidates.push(base.join("index.mjs"));
            }
        }

        match candidates.into_iter().find(|p| p.exists() && p.is_file()) {
            Some(p) => Ok(p),
            None => Err(format!("Backend module not found at path: {:?}", clean)),
        }
    }
}

pub fn format_module_path(clean: &str) -> String {
    let without_ext = clean
        .trim_end_matches(".ts")
        .trim_end_matches(".tsx")
        .trim_end_matches(".js")
        .trim_end_matches(".mjs");
    if without_ext.starts_with("modules/") {
        format!("@/{}", without_ext)
    } else if without_ext.starts_with("@/") {
        without_ext.to_string()
    } else {
        format!("@/modules/{}", without_ext)
    }
}

pub fn find_bun_bin() -> std::path::PathBuf {
    if let Some(userprofile) = std::env::var_os("USERPROFILE") {
        let p = std::path::PathBuf::from(userprofile).join(".bun/bin/bun.exe");
        if p.exists() {
            return p;
        }
        let p_no_ext = std::path::PathBuf::from(&p).with_extension("");
        if p_no_ext.exists() {
            return p_no_ext;
        }
    }
    if let Some(home) = std::env::var_os("HOME") {
        let p = std::path::PathBuf::from(home).join(".bun/bin/bun");
        if p.exists() {
            return p;
        }
    }
    for candidate in &[
        "/usr/local/bin/bun",
        "/opt/homebrew/bin/bun",
        "/usr/bin/bun",
    ] {
        let p = std::path::PathBuf::from(candidate);
        if p.exists() {
            return p;
        }
    }
    std::path::PathBuf::from("bun")
}

pub fn clean_path<P: AsRef<Path>>(p: P) -> String {
    let mut s = p.as_ref().to_string_lossy().to_string();
    if s.starts_with(r"\\?\UNC\") {
        s = format!(r"\\{}", &s[8..]);
    } else if s.starts_with(r"\\?\") {
        s = s[4..].to_string();
    } else if s.starts_with("//?/UNC/") {
        s = format!(r"//{}", &s[8..]);
    } else if s.starts_with("//?/") {
        s = s[4..].to_string();
    }
    s.replace('\\', "/")
}

pub fn dunce_canonicalize<P: AsRef<Path>>(p: P) -> std::path::PathBuf {
    match p.as_ref().canonicalize() {
        Ok(canon) => {
            let s = canon.to_string_lossy();
            if s.starts_with(r"\\?\UNC\") {
                std::path::PathBuf::from(format!(r"\\{}", &s[8..]))
            } else if s.starts_with(r"\\?\") {
                std::path::PathBuf::from(&s[4..])
            } else {
                canon
            }
        }
        Err(_) => p.as_ref().to_path_buf(),
    }
}

