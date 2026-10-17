// next/headers on the server: cookies(), headers(), draftMode() for the current request. Next.js 15
// returns Promises; the values are also usable synchronously, as older code expects.

import { parseCookieHeader, requestCookies, requestHeaders, serializeCookie, setCookies, syncPromise, type SetCookie } from "./request.ts";

type CookieOptions = Partial<Omit<SetCookie, "name" | "value">> & { expires?: Date | string | number };

function normalizeExpires(expires: CookieOptions["expires"]): string | undefined {
  if (expires === undefined || expires === null) return undefined;
  const date = expires instanceof Date ? expires : new Date(expires);
  return isNaN(date.getTime()) ? String(expires) : date.toUTCString();
}

export class CookieStore {
  get(name: string | { name: string }) {
    const key = typeof name === "string" ? name : name.name;
    const value = requestCookies().get(key);
    return value === undefined ? undefined : { name: key, value };
  }
  getAll(name?: string | { name: string }) {
    const key = name === undefined ? undefined : typeof name === "string" ? name : name.name;
    return [...requestCookies()].filter(([k]) => !key || k === key).map(([k, v]) => ({ name: k, value: v }));
  }
  has(name: string) {
    return requestCookies().has(name);
  }
  /** Allowed in Server Actions and route handlers (Next.js throws during rendering) */
  set(nameOrOptions: string | (CookieOptions & { name: string; value: string }), value?: string, options: CookieOptions = {}) {
    const opts = typeof nameOrOptions === "object" ? nameOrOptions : { ...options, name: nameOrOptions, value: value ?? "" };
    requestCookies().set(opts.name, String(opts.value ?? ""));
    setCookies().push({ ...opts, value: String(opts.value ?? ""), expires: normalizeExpires(opts.expires), path: opts.path || "/", deleted: false });
    return this;
  }
  delete(nameOrOptions: string | { name: string; path?: string; domain?: string }) {
    const opts = typeof nameOrOptions === "object" ? nameOrOptions : { name: nameOrOptions };
    requestCookies().delete(opts.name);
    setCookies().push({ name: opts.name, value: "", path: opts.path || "/", domain: opts.domain, maxAge: 0, expires: "Thu, 01 Jan 1970 00:00:00 GMT", deleted: true });
    return this;
  }
  clear() {
    for (const key of [...requestCookies().keys()]) this.delete(key);
    return this;
  }
  get size() {
    return requestCookies().size;
  }
  toString() {
    return [...requestCookies()].map(([k, v]) => serializeCookie({ name: k, value: v }).split(";")[0]).join("; ");
  }
  [Symbol.iterator]() {
    return this.getAll().map((c) => [c.name, c] as const)[Symbol.iterator]();
  }
}

export function cookies() {
  return syncPromise(new CookieStore());
}

/** Read-only request headers */
export function headers() {
  const h = new Headers();
  for (const [k, v] of Object.entries(requestHeaders())) {
    try {
      h.set(k, v);
    } catch {}
  }
  const readOnly = new Proxy(h, {
    get(target, prop) {
      if (prop === "set" || prop === "append" || prop === "delete") {
        return () => {
          throw new Error("headers() is read-only. Set response headers in a route handler or middleware instead.");
        };
      }
      const v = (target as any)[prop];
      return typeof v === "function" ? v.bind(target) : v;
    },
  });
  return syncPromise(readOnly);
}

const DRAFT_COOKIE = "__prerender_bypass";

export function draftMode() {
  const store = {
    get isEnabled() {
      return requestCookies().has(DRAFT_COOKIE) || parseCookieHeader(requestHeaders().cookie).has(DRAFT_COOKIE);
    },
    enable() {
      new CookieStore().set({ name: DRAFT_COOKIE, value: crypto.randomUUID(), httpOnly: true, sameSite: "none", secure: true, path: "/" });
    },
    disable() {
      new CookieStore().delete(DRAFT_COOKIE);
    },
  };
  return syncPromise(store);
}

export default { cookies, headers, draftMode };
