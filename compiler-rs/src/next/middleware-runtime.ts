// Runs the project's middleware (`middleware.ts`, Next.js 16 `proxy.ts`) for the engine and reports
// what it decided: continue (with request headers overridden), rewrite to another path, or respond
// (redirect, JSON, the result of an external rewrite). See middleware.rs.

import { NextRequest } from "./server.ts";
import { afterQueue, parseCookieHeader } from "./request.ts";

const g = globalThis as any;

export async function middlewareConfig(input: { specifier: string }) {
  const mod = await import(input.specifier);
  return mod.config ?? {};
}

// Headers Next.js uses to talk to its router (others, even `x-middleware-*` names, go to the client)
const INTERNAL = /^x-middleware-(next|rewrite|override-headers|request-.+|redirect|skip|set-cookie)$/;

export async function runMiddleware(input: { specifier: string; url: string; method: string; headers: [string, string][] }) {
  const mod = await import(input.specifier);
  const handler = mod.middleware ?? mod.proxy ?? mod.default;
  if (typeof handler !== "function") throw new Error("middleware.ts must export a `middleware` (or `proxy`) function");

  const headers = new Headers(input.headers);
  g.__NATA_HEADERS__ = Object.fromEntries(input.headers.map(([k, v]) => [k.toLowerCase(), v]));
  g.__NATA_COOKIES__ = parseCookieHeader(headers.get("cookie"));
  const request = new NextRequest(input.url, { method: input.method, headers });
  const event = { waitUntil: (promise: Promise<unknown>) => afterQueue().push(() => promise), passThroughOnException() {} };

  const response: Response | null | undefined = await handler(request, event);
  if (!response) return { output: { kind: "next", requestHeaders: [], responseHeaders: [] } };

  const responseHeaders: [string, string][] = [];
  response.headers.forEach((value, key) => {
    if (key !== "set-cookie" && !INTERNAL.test(key)) responseHeaders.push([key, value]);
  });
  for (const cookie of response.headers.getSetCookie?.() ?? []) responseHeaders.push(["set-cookie", cookie]);

  const requestHeaders: [string, string][] = [];
  const overridden = response.headers.get("x-middleware-override-headers");
  for (const name of overridden ? overridden.split(",").filter(Boolean) : []) {
    const value = response.headers.get(`x-middleware-request-${name}`);
    if (value !== null) requestHeaders.push([name, value]);
  }

  const rewrite = response.headers.get("x-middleware-rewrite");
  if (rewrite) {
    const target = new URL(rewrite, request.url);
    if (target.origin === new URL(request.url).origin) {
      return { output: { kind: "rewrite", rewrite: target.pathname + target.search, requestHeaders, responseHeaders } };
    }
    // An external rewrite proxies the other server
    const proxied = await fetch(target, { method: input.method, headers: requestHeaders.length ? { ...Object.fromEntries(headers), ...Object.fromEntries(requestHeaders) } : headers, redirect: "manual" });
    const proxiedHeaders: [string, string][] = [];
    proxied.headers.forEach((value, key) => {
      if (!["content-encoding", "content-length", "transfer-encoding", "connection"].includes(key)) proxiedHeaders.push([key, value]);
    });
    return { output: { kind: "respond", status: proxied.status, responseHeaders: [...proxiedHeaders, ...responseHeaders] }, body: new Uint8Array(await proxied.arrayBuffer()) };
  }
  if (response.headers.get("x-middleware-next")) return { output: { kind: "next", requestHeaders, responseHeaders } };
  return { output: { kind: "respond", status: response.status, responseHeaders }, body: new Uint8Array(await response.arrayBuffer()) };
}
