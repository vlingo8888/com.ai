// Data cache client: `unstable_cache`, "use cache" functions and `fetch(..., { next })` store their
// results in the engine (`cache_store.rs`) through CALL frames, so the cache survives worker
// restarts, is bounded and persisted. Values are JSON with tags for Date, Map, Set, BigInt and
// undefined. Results containing React elements are not cached (rendered every time).
//
// Like Next.js, a stale entry is served while it is recomputed in the background, and every entry
// written while rendering a page is tagged with that page's path (`_N_T_/path`), so revalidatePath()
// purges it.

import { AsyncLocalStorage } from "node:async_hooks";
import { markDynamic, noteRevalidate, noteTags } from "./request.ts";

const g = globalThis as any;

export interface CacheScope {
  revalidate?: number;
  tags: string[];
}

const scopes = new AsyncLocalStorage<CacheScope>();

export function currentCacheScope(): CacheScope | undefined {
  return scopes.getStore();
}

export function markRevalidated() {
  g.__NATA_REVALIDATED__ = true;
}

const call = (cmd: string, input: unknown) => g.__NATA_CALL_ENGINE__(cmd, input) as Promise<{ meta: any; body: Uint8Array }>;

const REACT_ELEMENT = Symbol.for("react.transitional.element");
const LEGACY_ELEMENT = Symbol.for("react.element");

class Uncacheable extends Error {}

export function serialize(value: unknown): string {
  return JSON.stringify(value, function (this: any, key, v) {
    const raw = this[key];
    if (raw instanceof Date) return { $t: "Date", v: raw.toISOString() };
    if (raw instanceof Map) return { $t: "Map", v: [...raw] };
    if (raw instanceof Set) return { $t: "Set", v: [...raw] };
    if (typeof raw === "bigint") return { $t: "BigInt", v: raw.toString() };
    if (raw === undefined && key !== "") return { $t: "undefined" };
    if (raw && typeof raw === "object" && (raw.$$typeof === REACT_ELEMENT || raw.$$typeof === LEGACY_ELEMENT)) throw new Uncacheable();
    if (typeof raw === "function" || typeof raw === "symbol") throw new Uncacheable();
    return v;
  });
}

export function deserialize<T>(text: string): T {
  return JSON.parse(text, (_key, v) => {
    if (v && typeof v === "object" && typeof v.$t === "string") {
      switch (v.$t) {
        case "Date":
          return new Date(v.v);
        case "Map":
          return new Map(v.v);
        case "Set":
          return new Set(v.v);
        case "BigInt":
          return BigInt(v.v);
        case "undefined":
          return undefined;
      }
    }
    return v;
  });
}

/** Tag of the page being rendered: revalidatePath(path) purges what it cached */
function implicitTags(): string[] {
  const pathname = g.__NATA_LOCATION__?.pathname;
  return pathname ? [`_N_T_${pathname}`] : [];
}

const inflight = new Map<string, Promise<unknown>>();

async function compute<R>(key: string, run: () => Promise<R> | R, options: { revalidate?: number | false; tags: string[] }): Promise<R> {
  const scope: CacheScope = { revalidate: typeof options.revalidate === "number" ? options.revalidate : undefined, tags: [...options.tags] };
  const result = await scopes.run(scope, run);
  let text: string | null = null;
  try {
    text = serialize(result);
  } catch (error) {
    if (!(error instanceof Uncacheable)) throw error;
  }
  if (text !== null) {
    const revalidate = options.revalidate === false ? null : scope.revalidate ?? (options.revalidate === undefined ? null : options.revalidate);
    // The page using this data is no fresher than it
    noteRevalidate(revalidate);
    noteTags(scope.tags);
    await call("cache_set", { key, value: text, tags: [...new Set([...scope.tags, ...implicitTags()])], revalidate });
  }
  return result;
}

/** Returns the cached result of `run` for `id` + `args`, computing (and storing) it when missing */
export async function cacheCall<R>(id: string, args: unknown[], run: () => Promise<R> | R, options: { revalidate?: number | false; tags: string[] }): Promise<R> {
  let argsKey: string;
  try {
    argsKey = serialize(args);
  } catch {
    return run();
  }
  const key = `${id}|${argsKey}`;
  noteRevalidate(options.revalidate);
  noteTags(options.tags);
  const { meta } = await call("cache_get", { key });
  if (meta.hit) {
    // Stale: serve it now, refresh in the background (stale-while-revalidate)
    if (meta.stale && !inflight.has(key)) {
      const refresh = compute(key, run, options).catch(() => {}).finally(() => inflight.delete(key));
      inflight.set(key, refresh);
    }
    return deserialize<R>(meta.value);
  }
  const pending = inflight.get(key);
  if (pending) return pending as Promise<R>;
  const fresh = compute(key, run, options).finally(() => inflight.delete(key));
  inflight.set(key, fresh);
  return fresh;
}

export function cacheInvalidate(input: { tags?: string[]; paths?: string[] }) {
  const tags = [...(input.tags ?? []), ...(input.paths ?? []).map((p) => (p.endsWith("#layout") ? `_N_T_${p.slice(0, -7)}*` : `_N_T_${p}`))];
  return call("cache_invalidate", { tags }).then(() => undefined);
}

/** "use cache" function wrapper emitted by the compiler: `__nata_use_cache(fn, "file#name")` */
export function useCache<A extends unknown[], R>(fn: (...args: A) => R, id: string) {
  const cached = function (this: unknown, ...args: A) {
    return cacheCall(id, args, () => fn.apply(this, args), { revalidate: undefined, tags: [] });
  };
  Object.defineProperty(cached, "name", { value: fn.name });
  return cached;
}

// ---------- fetch(url, { cache, next: { revalidate, tags } }) ----------

const nativeFetch: typeof fetch = g.__NATA_NATIVE_FETCH__ || (g.__NATA_NATIVE_FETCH__ = g.fetch.bind(g));

export function installFetchCache() {
  if (g.__NATA_FETCH_CACHE__) return;
  g.__NATA_FETCH_CACHE__ = true;
  g.fetch = async function cachedFetch(input: RequestInfo | URL, init: RequestInit & { next?: { revalidate?: number | false; tags?: string[] } } = {}) {
    const method = (init.method || (input instanceof Request ? input.method : "GET")).toUpperCase();
    // Route segment config of the page being rendered: `revalidate` is the default for its fetches,
    // `fetchCache` / `dynamic` force or forbid caching
    const segment = g.__NATA_SEGMENT_CONFIG__ || {};
    const revalidate = init.next?.revalidate ?? (typeof segment.revalidate === "number" ? segment.revalidate : undefined);
    const tags = init.next?.tags ?? [];
    if (segment.fetchCache === "force-no-store" || segment.fetchCache === "only-no-store" || segment.dynamic === "force-dynamic") init = { ...init, cache: "no-store" };
    else if (segment.fetchCache === "force-cache" || segment.dynamic === "force-static") init = { ...init, cache: init.cache ?? "force-cache" };
    // Next.js 15: not cached unless asked (`cache: "force-cache"`, `next.revalidate` > 0, tags)
    const wanted = init.cache === "force-cache" || (typeof revalidate === "number" && revalidate > 0) || (tags.length > 0 && init.cache !== "no-store");
    if (!wanted || method !== "GET" || revalidate === 0 || init.cache === "no-store") {
      // Like Next.js: an uncached fetch runs at build time unless it opts out explicitly
      if (method === "GET" && (revalidate === 0 || init.cache === "no-store")) markDynamic(`fetch ${input instanceof Request ? input.url : String(input)} with no-store`);
      const { next: _next, ...rest } = init;
      return nativeFetch(input as any, rest);
    }
    const url = input instanceof Request ? input.url : String(input);
    const headers = [...new Headers(init.headers || (input instanceof Request ? input.headers : undefined))].sort();
    const entry = await cacheCall(
      `fetch:${url}`,
      [headers],
      async () => {
        const { next: _next, ...rest } = init;
        const res = await nativeFetch(input as any, rest);
        const body = Buffer.from(await res.arrayBuffer()).toString("base64");
        return { status: res.status, statusText: res.statusText, headers: [...res.headers], body };
      },
      { revalidate: revalidate === undefined ? false : revalidate, tags },
    );
    return new Response(entry.status === 204 || entry.status === 304 ? null : Buffer.from(entry.body, "base64"), {
      status: entry.status,
      statusText: entry.statusText,
      headers: entry.headers,
    });
  } as typeof fetch;
}
