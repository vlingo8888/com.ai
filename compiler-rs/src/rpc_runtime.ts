/**
 * Server Action (RPC) executor shared by the persistent worker and the one-off fallback runner.
 * Request state (cookies, Set-Cookie list, query log) lives in `globalThis.__NATA_*`, which the worker
 * scopes per request with AsyncLocalStorage.
 */

// One import at a time for modules a request has not loaded yet (serial_import.ts)
const importModule = (specifier: string): Promise<any> => ((globalThis as any).__nata_import ?? ((s: string) => import(s)))(specifier);

export interface RpcInput {
  /** Absolute path of the module that exports the action */
  tsPath: string;
  /** `rsc:` specifier when the engine serves server modules (edits apply without a restart) */
  moduleSpecifier?: string | null;
  action: string;
  args: unknown;
  cookiesStr: string;
  /** Module path as written by the caller, used in error messages, e.g. `modules/items` */
  clean: string;
  /** Module name used to locate `lifecycle.ts`, e.g. `items` */
  cleanMod: string;
}

export interface RpcResult {
  payload: Record<string, unknown>;
  /** 0 = success, 1 = the action threw */
  code: number;
}

function parseCookies(header: string): Map<string, string> {
  const cookieMap = new Map<string, string>();
  if (typeof header === "string" && header.trim()) {
    for (const pair of header.split(";")) {
      const idx = pair.indexOf("=");
      if (idx !== -1) {
        const k = decodeURIComponent(pair.slice(0, idx).trim());
        const v = decodeURIComponent(pair.slice(idx + 1).trim());
        if (k) cookieMap.set(k, v);
      }
    }
  }
  return cookieMap;
}

// lifecycle.ts `onInstalled(db)` runs once per process for each lifecycle file
const lifecycleDone = new Set<string>();
// Modules whose lifecycle candidates were already looked up: the outcome cannot change while this
// worker lives (adding a lifecycle file restarts it), so later calls skip the four `existsSync`
const lifecycleChecked = new Set<string>();

async function runLifecycle(input: RpcInput) {
  const key = `${input.tsPath}\0${input.cleanMod}`;
  if (lifecycleChecked.has(key)) return;
  lifecycleChecked.add(key);
  const fs = await import("fs");
  const path = await import("path");
  const modDir = path.dirname(input.tsPath);
  const candidates = [
    path.join(modDir, "lifecycle.ts"),
    path.join(modDir, "..", "lifecycle.ts"),
    path.join(process.cwd(), "modules", input.cleanMod, "lifecycle.ts"),
    path.join(process.cwd(), "plugins", input.cleanMod, "lifecycle.ts"),
  ];
  for (const lcPath of candidates) {
    if (!fs.existsSync(lcPath)) continue;
    if (lifecycleDone.has(lcPath)) return;
    lifecycleDone.add(lcPath);
    try {
      const lcMod: any = await importModule(lcPath);
      const lc = lcMod?.lifecycle || lcMod?.default || lcMod;
      if (lc && typeof lc.onInstalled === "function") {
        const { db } = await import("core");
        await lc.onInstalled(db);
      }
    } catch (lcErr) {
      console.warn("[Lifecycle Warning]", lcErr);
    }
    return;
  }
}

async function invoke(input: RpcInput): Promise<unknown> {
  const mod: any = await importModule(input.moduleSpecifier || input.tsPath);
  const rawArgs = input.args;
  const args = Array.isArray(rawArgs) ? rawArgs : rawArgs !== null && rawArgs !== undefined ? [rawArgs] : [];
  const actionName = input.action;

  let target = actionName === "default" ? mod.default || mod : mod[actionName];
  if (!target && mod.default && typeof mod.default === "object" && mod.default[actionName]) {
    target = mod.default[actionName];
  }
  if (!target && typeof mod === "function") target = mod;
  if (!target && typeof mod.default === "function") target = mod.default;
  if (!target) {
    throw new Error("Action '" + actionName + "' not found in module '" + input.clean + "'");
  }

  let res: unknown;
  if (typeof target === "function") {
    if (target.prototype && target.prototype.constructor === target && typeof target.prototype.execute === "function") {
      res = await new target().execute(...args);
    } else {
      res = await target(...args);
    }
  } else if (target && typeof target.execute === "function") {
    res = await target.execute(...args);
  } else {
    res = target;
  }

  // Unwrap Response instances (e.g. from core.json() or new Response())
  if (res instanceof Response) {
    const contentType = res.headers.get("content-type") || "";
    const text = await res.text();
    if (contentType.includes("application/json")) {
      try { return JSON.parse(text); } catch { return text; }
    }
    try { return JSON.parse(text); } catch { return text; }
  }
  return res;
}

export async function runRpc(input: RpcInput): Promise<RpcResult> {
  const g = globalThis as any;
  g.__NATA_COOKIES__ = parseCookies(input.cookiesStr);
  g.__NATA_SET_COOKIES__ = [];
  g.__NATA_QUERY_LOGS__ = [];

  const queries = () => (Array.isArray(g.__NATA_QUERY_LOGS__) ? g.__NATA_QUERY_LOGS__ : []);
  try {
    await runLifecycle(input);
    const res = await invoke(input);
    const setCookies = Array.isArray(g.__NATA_SET_COOKIES__) ? g.__NATA_SET_COOKIES__ : [];
    return {
      payload: { __nata_rpc_data__: res ?? null, __nata_set_cookies__: setCookies, __nata_queries__: queries() },
      code: 0,
    };
  } catch (err: any) {
    g.__NATA_EXPLAIN_ERROR__?.(err);
    return {
      payload: { error: err?.message || String(err), stack: err?.stack || undefined, __nata_queries__: queries() },
      code: 1,
    };
  }
}
