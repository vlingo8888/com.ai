// next/cache: revalidation and cached functions. The data cache lives in the engine (shared by worker
// restarts, bounded, persisted to disk); see `cache_store.rs`.

import { cacheCall, cacheInvalidate, markRevalidated, currentCacheScope } from "./cache-runtime.ts";

/** Purges cached data for `path` (and, in a Server Action, re-renders the page in the same response) */
export function revalidatePath(path: string, type?: "page" | "layout") {
  markRevalidated();
  return cacheInvalidate({ paths: [type === "layout" ? `${path}#layout` : path] });
}

export function revalidateTag(tag: string, _profile?: string | { expire?: number }) {
  markRevalidated();
  return cacheInvalidate({ tags: [tag] });
}

/** Next.js 16: expire a tag and read fresh data in the same request */
export function updateTag(tag: string) {
  return revalidateTag(tag);
}

/** Re-renders the current page after a Server Action (Next.js 16) */
export function refresh() {
  markRevalidated();
}

type CachedOptions = { revalidate?: number | false; tags?: string[] };

/** Caches the result of `fn` by `keyParts` + arguments (JSON-serializable values) */
export function unstable_cache<A extends unknown[], R>(fn: (...args: A) => Promise<R> | R, keyParts: string[] = [], options: CachedOptions = {}) {
  const id = `unstable_cache:${keyParts.join(",")}:${fn.toString().length}:${fn.name}`;
  return (...args: A) => cacheCall<R>(id, args, () => fn(...args), { revalidate: options.revalidate, tags: options.tags ?? [] });
}

/** "use cache": how long a cached result stays fresh */
export function cacheLife(profile: string | { stale?: number; revalidate?: number; expire?: number }) {
  const scope = currentCacheScope();
  if (!scope) return;
  const seconds = typeof profile === "object" ? profile.revalidate ?? profile.expire : CACHE_PROFILES[profile];
  if (typeof seconds === "number") scope.revalidate = Math.min(scope.revalidate ?? Infinity, seconds);
}

/** "use cache": tags the cached result so revalidateTag() purges it */
export function cacheTag(...tags: string[]) {
  currentCacheScope()?.tags.push(...tags);
}

/** Next.js profiles (`revalidate` seconds) */
export const CACHE_PROFILES: Record<string, number> = {
  default: 900,
  seconds: 1,
  minutes: 60,
  hours: 3600,
  days: 86400,
  weeks: 604800,
  max: 2592000,
};

export function unstable_noStore() {}
export const noStore = unstable_noStore;

export default { revalidatePath, revalidateTag, updateTag, refresh, unstable_cache, cacheLife, cacheTag, unstable_noStore };
