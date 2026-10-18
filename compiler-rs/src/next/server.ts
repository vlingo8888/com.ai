// next/server: NextRequest, NextResponse, userAgent(), after(), connection() — the shapes Next.js
// passes to route handlers and middleware.

import { afterQueue, markDynamic, parseCookieHeader, serializeCookie, type SetCookie } from "./request.ts";

export class RequestCookies {
  private map: Map<string, string>;
  constructor(private headers: Headers) {
    this.map = parseCookieHeader(headers.get("cookie"));
  }
  get(name: string | { name: string }) {
    const key = typeof name === "string" ? name : name.name;
    const value = this.map.get(key);
    return value === undefined ? undefined : { name: key, value };
  }
  getAll(name?: string | { name: string }) {
    const key = name === undefined ? undefined : typeof name === "string" ? name : name.name;
    return [...this.map].filter(([k]) => !key || k === key).map(([k, v]) => ({ name: k, value: v }));
  }
  has(name: string) {
    return this.map.has(name);
  }
  /** Changes the request seen by later code (middleware → page) */
  set(nameOrCookie: string | { name: string; value: string }, value?: string) {
    const c = typeof nameOrCookie === "object" ? nameOrCookie : { name: nameOrCookie, value: value ?? "" };
    this.map.set(c.name, c.value);
    this.sync();
    return this;
  }
  delete(names: string | string[]) {
    const list = Array.isArray(names) ? names : [names];
    const result = list.map((n) => this.map.delete(n));
    this.sync();
    return Array.isArray(names) ? result : result[0];
  }
  clear() {
    this.map.clear();
    this.sync();
    return this;
  }
  get size() {
    return this.map.size;
  }
  [Symbol.iterator]() {
    return [...this.map].map(([name, value]) => [name, { name, value }] as const)[Symbol.iterator]();
  }
  toString() {
    return [...this.map].map(([k, v]) => `${encodeURIComponent(k)}=${encodeURIComponent(v)}`).join("; ");
  }
  private sync() {
    try {
      this.headers.set("cookie", this.toString());
    } catch {}
  }
}

export class ResponseCookies {
  private map = new Map<string, SetCookie>();
  constructor(private headers: Headers) {
    for (const line of headers.getSetCookie?.() ?? []) {
      const [pair] = line.split(";");
      const idx = pair.indexOf("=");
      if (idx > 0) this.map.set(decodeURIComponent(pair.slice(0, idx)), { name: decodeURIComponent(pair.slice(0, idx)), value: decodeURIComponent(pair.slice(idx + 1)) });
    }
  }
  get(name: string | { name: string }) {
    return this.map.get(typeof name === "string" ? name : name.name);
  }
  getAll(name?: string) {
    return [...this.map.values()].filter((c) => !name || c.name === name);
  }
  has(name: string) {
    return this.map.has(name);
  }
  set(nameOrCookie: string | (SetCookie & { expires?: any }), value?: string, options: Partial<SetCookie> & { expires?: any } = {}) {
    const c: any = typeof nameOrCookie === "object" ? { ...nameOrCookie } : { ...options, name: nameOrCookie, value: value ?? "" };
    if (c.expires instanceof Date) c.expires = c.expires.toUTCString();
    else if (typeof c.expires === "number") c.expires = new Date(c.expires).toUTCString();
    this.map.set(c.name, c);
    this.flush();
    return this;
  }
  delete(nameOrCookie: string | { name: string; path?: string; domain?: string }) {
    const c = typeof nameOrCookie === "object" ? nameOrCookie : { name: nameOrCookie };
    this.map.set(c.name, { ...c, value: "", maxAge: 0, expires: "Thu, 01 Jan 1970 00:00:00 GMT", deleted: true });
    this.flush();
    return this;
  }
  toString() {
    return [...this.map.values()].map(serializeCookie).join(", ");
  }
  /** Cookies are Set-Cookie headers of the response itself */
  private flush() {
    try {
      this.headers.delete("set-cookie");
      for (const c of this.map.values()) this.headers.append("set-cookie", serializeCookie(c));
    } catch {}
  }
}

/** `request.nextUrl`: a URL with `clone()` and the base path */
export class NextURL extends URL {
  basePath = "";
  buildId?: string;
  locale?: string;
  clone() {
    const copy = new NextURL(this.href);
    copy.basePath = this.basePath;
    return copy;
  }
}

export class NextRequest extends Request {
  readonly nextUrl: NextURL;
  readonly cookies: RequestCookies;
  readonly ip?: string;
  readonly geo: { city?: string; country?: string; region?: string; latitude?: string; longitude?: string };

  constructor(input: RequestInfo | URL, init: RequestInit & { ip?: string; geo?: any } = {}) {
    super(input as any, init);
    const url = typeof input === "string" ? input : input instanceof URL ? input.href : (input as Request).url;
    this.nextUrl = new NextURL(url);
    this.cookies = new RequestCookies(this.headers);
    this.ip = init.ip ?? this.headers.get("x-forwarded-for")?.split(",")[0].trim() ?? undefined;
    this.geo = init.geo ?? {};
  }
}

type MiddlewareInit = ResponseInit & { request?: { headers?: HeadersInit } };

/** Request headers set by middleware travel to the page as `x-middleware-request-*` (like Next.js) */
function forwardRequestHeaders(headers: Headers, init?: MiddlewareInit) {
  if (!init?.request?.headers) return;
  const requestHeaders = new Headers(init.request.headers);
  const names: string[] = [];
  requestHeaders.forEach((value, key) => {
    headers.set(`x-middleware-request-${key}`, value);
    names.push(key);
  });
  headers.set("x-middleware-override-headers", names.join(","));
}

export class NextResponse<Body = unknown> extends Response {
  readonly cookies: ResponseCookies;

  constructor(body?: BodyInit | null, init: ResponseInit = {}) {
    super(body, init);
    this.cookies = new ResponseCookies(this.headers);
  }

  static json<T = unknown>(data: T, init: ResponseInit = {}): NextResponse<T> {
    const headers = new Headers(init.headers);
    if (!headers.has("content-type")) headers.set("content-type", "application/json");
    return new NextResponse(JSON.stringify(data), { ...init, headers });
  }

  static redirect(url: string | URL, init?: number | ResponseInit): NextResponse {
    const status = typeof init === "number" ? init : init?.status ?? 307;
    const headers = new Headers(typeof init === "object" ? init.headers : undefined);
    headers.set("location", String(url));
    return new NextResponse(null, { ...(typeof init === "object" ? init : {}), status, headers });
  }

  static rewrite(destination: string | URL, init?: MiddlewareInit): NextResponse {
    const headers = new Headers(init?.headers);
    headers.set("x-middleware-rewrite", String(destination));
    forwardRequestHeaders(headers, init);
    return new NextResponse(null, { ...init, headers });
  }

  static next(init?: MiddlewareInit): NextResponse {
    const headers = new Headers(init?.headers);
    headers.set("x-middleware-next", "1");
    forwardRequestHeaders(headers, init);
    return new NextResponse(null, { ...init, headers });
  }
}

/** Parses a User-Agent header the way Next.js reports it (subset of ua-parser-js) */
export function userAgentFromString(ua: string | undefined | null) {
  ua = ua || "";
  const pick = (re: RegExp) => ua!.match(re);
  const browser =
    (pick(/(Edg|EdgA|EdgiOS)\/([\d.]+)/) && { name: "Edge", version: pick(/(?:Edg|EdgA|EdgiOS)\/([\d.]+)/)![1] }) ||
    (pick(/OPR\/([\d.]+)/) && { name: "Opera", version: pick(/OPR\/([\d.]+)/)![1] }) ||
    (pick(/Zalo[ /]([\d.]+)/i) && { name: "Zalo", version: pick(/Zalo[ /]([\d.]+)/i)![1] }) ||
    (pick(/(?:Chrome|CriOS)\/([\d.]+)/) && { name: "Chrome", version: pick(/(?:Chrome|CriOS)\/([\d.]+)/)![1] }) ||
    (pick(/(?:Firefox|FxiOS)\/([\d.]+)/) && { name: "Firefox", version: pick(/(?:Firefox|FxiOS)\/([\d.]+)/)![1] }) ||
    (pick(/Version\/([\d.]+).*Safari/) && { name: "Safari", version: pick(/Version\/([\d.]+)/)![1] }) || { name: undefined, version: undefined };
  const os =
    (pick(/Windows NT ([\d.]+)/) && { name: "Windows", version: pick(/Windows NT ([\d.]+)/)![1] }) ||
    (pick(/(?:iPhone|iPad|iPod).*OS ([\d_]+)/) && { name: "iOS", version: pick(/OS ([\d_]+)/)![1].replace(/_/g, ".") }) ||
    (pick(/Mac OS X ([\d_.]+)/) && { name: "Mac OS", version: pick(/Mac OS X ([\d_.]+)/)![1].replace(/_/g, ".") }) ||
    (pick(/Android ([\d.]+)/) && { name: "Android", version: pick(/Android ([\d.]+)/)![1] }) ||
    (pick(/Linux/) && { name: "Linux", version: undefined }) || { name: undefined, version: undefined };
  const type = /iPad|Tablet/i.test(ua) ? "tablet" : /Mobi|iPhone|Android/i.test(ua) ? "mobile" : undefined;
  const engine = /AppleWebKit/.test(ua) ? (/Chrome|Edg|OPR/.test(ua) ? "Blink" : "WebKit") : /Gecko\//.test(ua) ? "Gecko" : undefined;
  return {
    ua,
    isBot: /bot|crawler|spider|crawling|googlebot|bingbot|slurp|facebookexternalhit/i.test(ua),
    browser: { ...browser, major: browser.version?.split(".")[0] },
    device: { type, vendor: /iPhone|iPad|Mac/.test(ua) ? "Apple" : undefined, model: undefined },
    engine: { name: engine, version: undefined },
    os,
    cpu: { architecture: /arm64|aarch64/i.test(ua) ? "arm64" : /x86_64|Win64|x64/i.test(ua) ? "amd64" : undefined },
  };
}

export function userAgent(request: { headers: Headers }) {
  return userAgentFromString(request.headers.get("user-agent"));
}

/** Runs `task` after the response has been sent (logging, analytics) */
export function after(task: Promise<unknown> | (() => unknown)) {
  afterQueue().push(typeof task === "function" ? task : () => task);
}

/** Marks the render as depending on the request (always dynamic in development) */
export async function connection(): Promise<void> {
  markDynamic("connection()");
}

export const URLPattern = (globalThis as any).URLPattern;

export { ImageResponse } from "./og.ts";

export default { NextRequest, NextResponse, NextURL, RequestCookies, ResponseCookies, userAgent, userAgentFromString, after, connection };
