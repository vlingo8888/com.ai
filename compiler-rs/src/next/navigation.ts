// next/navigation on the server: control flow (notFound, forbidden, unauthorized, redirect) thrown as
// errors with Next.js digests, and the hooks client components call while being server-rendered.

import * as React from "react";
// The URL segments below the calling layout come from the React Server Components router
import { SelectedLayoutContext } from "../rsc_boundaries.js";

const g = globalThis as any;

export enum RedirectType {
  push = "push",
  replace = "replace",
}

function controlError(message: string, digest: string) {
  const err: any = new Error(message);
  err.digest = digest;
  return err;
}

export function redirect(url: string, type: RedirectType | "push" | "replace" = RedirectType.replace): never {
  const err = controlError("NEXT_REDIRECT", `NEXT_REDIRECT;${type};${url};307;`);
  err.url = url;
  throw err;
}

export function permanentRedirect(url: string, type: RedirectType | "push" | "replace" = RedirectType.replace): never {
  const err = controlError("NEXT_REDIRECT", `NEXT_REDIRECT;${type};${url};308;`);
  err.url = url;
  throw err;
}

export function notFound(): never {
  throw controlError("NEXT_NOT_FOUND", "NEXT_NOT_FOUND");
}

/** Renders forbidden.tsx with status 403 */
export function forbidden(): never {
  throw controlError("NEXT_HTTP_ERROR_FALLBACK;403", "NEXT_HTTP_ERROR_FALLBACK;403");
}

/** Renders unauthorized.tsx with status 401 */
export function unauthorized(): never {
  throw controlError("NEXT_HTTP_ERROR_FALLBACK;401", "NEXT_HTTP_ERROR_FALLBACK;401");
}

/** Re-throws errors Next.js uses for control flow, so a try/catch does not swallow them */
export function unstable_rethrow(error: unknown): void {
  const digest = (error as any)?.digest;
  if (typeof digest === "string" && /^(NEXT_REDIRECT|NEXT_NOT_FOUND|NEXT_HTTP_ERROR_FALLBACK)/.test(digest)) throw error;
}

const location = () => g.__NATA_LOCATION__ || { pathname: "/", search: "" };

export function usePathname(): string {
  return location().pathname;
}

export function useSearchParams() {
  return new URLSearchParams(location().search);
}

export function useParams() {
  return g.__NATA_PARAMS__ || {};
}

export function useRouter() {
  const noop = () => {};
  return { push: noop, replace: noop, back: noop, forward: noop, refresh: noop, prefetch: noop };
}

export function useServerInsertedHTML(_callback: () => unknown) {}

export function useSelectedLayoutSegments(): string[] {
  return React.useContext(SelectedLayoutContext as React.Context<string[]>) ?? [];
}

export function useSelectedLayoutSegment(): string | null {
  const segments = useSelectedLayoutSegments();
  return segments.length ? segments[0] : null;
}

export default {
  redirect,
  permanentRedirect,
  notFound,
  forbidden,
  unauthorized,
  unstable_rethrow,
  usePathname,
  useSearchParams,
  useParams,
  useRouter,
  useServerInsertedHTML,
  useSelectedLayoutSegment,
  useSelectedLayoutSegments,
  RedirectType,
};
