// The request being served, as the worker scopes it per request (AsyncLocalStorage, see worker.ts):
// headers, cookies, the Set-Cookie list, `after()` callbacks. Read by next/headers, next/server,
// next/cache and next/navigation on the server.

const g = globalThis as any;

export interface SetCookie {
  name: string;
  value: string;
  path?: string;
  maxAge?: number;
  expires?: string;
  domain?: string;
  secure?: boolean;
  httpOnly?: boolean;
  sameSite?: string;
  deleted?: boolean;
}

/** Request headers (lower-case names) */
export function requestHeaders(): Record<string, string> {
  return g.__NATA_HEADERS__ || {};
}

export function requestCookies(): Map<string, string> {
  return g.__NATA_COOKIES__ || (g.__NATA_COOKIES__ = new Map<string, string>());
}

export function setCookies(): SetCookie[] {
  return g.__NATA_SET_COOKIES__ || (g.__NATA_SET_COOKIES__ = []);
}

/**
 * What a page render depends on, reported to the engine with the result: a dynamic API used (the page
 * cannot be prerendered or cached whole), the shortest `revalidate` of its data and its cache tags
 */
export interface RenderInfo {
  dynamic: string | null;
  revalidate: number | null;
  tags: string[];
  forceStatic?: boolean;
}

export function renderInfo(): RenderInfo {
  return g.__NATA_RENDER__ || (g.__NATA_RENDER__ = { dynamic: null, revalidate: null, tags: [] });
}

/** A request-time API was used (`cookies()`, `headers()`, `searchParams`, `fetch` with no-store...) */
export function markDynamic(reason: string) {
  const info = renderInfo();
  if (!info.dynamic) info.dynamic = reason;
}

/** Data the page used revalidates after `seconds`: so does the page */
export function noteRevalidate(seconds: number | false | null | undefined) {
  if (typeof seconds !== "number") return;
  if (seconds <= 0) return markDynamic("revalidate = 0");
  const info = renderInfo();
  info.revalidate = info.revalidate === null ? seconds : Math.min(info.revalidate, seconds);
}

export function noteTags(tags: string[]) {
  const info = renderInfo();
  for (const tag of tags) if (!info.tags.includes(tag)) info.tags.push(tag);
}

/** `searchParams`, as pages receive it: reading it makes the render dynamic */
export function dynamicProps<T extends object>(value: T, reason: string): Promise<T> & T {
  const target = Object.assign(Promise.resolve(value), value) as Promise<T> & T;
  return new Proxy(target, {
    get(t, prop, receiver) {
      if (typeof prop === "string" && prop !== "$$typeof" && prop !== "toJSON") markDynamic(reason);
      const v = Reflect.get(t, prop, receiver);
      return typeof v === "function" && (prop === "then" || prop === "catch" || prop === "finally") ? v.bind(t) : v;
    },
    has(t, prop) {
      markDynamic(reason);
      return Reflect.has(t, prop);
    },
    ownKeys(t) {
      markDynamic(reason);
      return Reflect.ownKeys(t);
    },
  });
}

/** Callbacks registered with `after()`, run once the response has been sent */
export function afterQueue(): Array<() => unknown> {
  return g.__NATA_AFTER__ || (g.__NATA_AFTER__ = []);
}

/** Parses a `Cookie` header */
export function parseCookieHeader(header: string | null | undefined): Map<string, string> {
  const map = new Map<string, string>();
  for (const pair of (header || "").split(";")) {
    const idx = pair.indexOf("=");
    if (idx === -1) continue;
    try {
      const key = decodeURIComponent(pair.slice(0, idx).trim());
      if (key) map.set(key, decodeURIComponent(pair.slice(idx + 1).trim()));
    } catch {}
  }
  return map;
}

/** `Set-Cookie` header value of a cookie */
export function serializeCookie(c: SetCookie): string {
  let out = `${encodeURIComponent(c.name)}=${encodeURIComponent(c.value ?? "")}; Path=${c.path || "/"}`;
  if (c.maxAge !== undefined && c.maxAge !== null) out += `; Max-Age=${c.maxAge}`;
  if (c.expires) out += `; Expires=${c.expires}`;
  if (c.domain) out += `; Domain=${c.domain}`;
  if (c.secure) out += "; Secure";
  if (c.httpOnly) out += "; HttpOnly";
  out += `; SameSite=${c.sameSite ? String(c.sameSite)[0].toUpperCase() + String(c.sameSite).slice(1).toLowerCase() : "Lax"}`;
  return out;
}

/** A value usable both as a Promise (Next.js 15 `await cookies()`) and synchronously (older code) */
export function syncPromise<T extends object>(value: T): Promise<T> & T {
  const promise = Promise.resolve(value) as Promise<T> & T;
  return new Proxy(promise, {
    get(target, prop) {
      if (prop === "then" || prop === "catch" || prop === "finally") return (target as any)[prop].bind(target);
      const v = (value as any)[prop];
      return typeof v === "function" ? v.bind(value) : v;
    },
    has: (_t, prop) => prop in value,
  }) as Promise<T> & T;
}

/**
 * instrumentation.ts `onRequestError(error, request, context)`: called for errors while rendering,
 * in route handlers, Server Actions and middleware
 */
export async function reportRequestError(error: unknown, context: { routePath?: string; routeType: "render" | "route" | "action" | "middleware" }) {
  const hook = g.__NATA_ON_REQUEST_ERROR__;
  if (typeof hook !== "function") return;
  const location = g.__NATA_LOCATION__ || { pathname: "/", search: "" };
  try {
    await hook(error, { path: location.pathname + (location.search || ""), method: g.__NATA_METHOD__ || "GET", headers: requestHeaders() }, {
      routerKind: "App Router",
      routePath: context.routePath ?? location.pathname,
      routeType: context.routeType,
      renderSource: context.routeType === "render" ? "react-server-components" : undefined,
      revalidateReason: undefined,
      renderType: "dynamic",
    });
  } catch (hookError) {
    console.error("Error in instrumentation onRequestError:", hookError);
  }
}
