// Worker commands of the React Server Components pipeline: `rsc` (a page, as an HTML document or as a
// Flight payload for the client router) and `action` (a Server Action, called by the client router or
// posted by a <form> before JavaScript loaded). Results stream back to the engine chunk by chunk.

import { reportRequestError } from "./next/request.ts";

const g = globalThis as any;

// The SSR runtime (rsc_ssr.ts) is loaded in the `ssr:` layer, through the engine like the client
// components it renders, so it gets the same React (the framework's own, see framework_react.rs)
let ssr: any;

// ---------- module loading for Flight ----------
// Client references (ids are browser URLs, starting with "/") load in the SSR layer; Server Action ids
// (hashes) load their module in the react-server layer, which registers them in __NATA_ACTIONS__.

async function loadActionModule(id: string) {
  if (g.__NATA_ACTIONS__?.has(id)) return;
  const { meta } = await g.__NATA_CALL_ENGINE__("action_module", { id });
  if (!meta.specifier) throw new Error(`Server Action "${id}" was not found. It may have been removed or renamed; reload the page.`);
  await import(meta.specifier);
}

g.__webpack_chunk_load__ = (chunk: string) => {
  if (typeof chunk !== "string" || chunk === "") return Promise.resolve();
  return chunk.startsWith("/") ? ssr.loadClientModule(chunk) : loadActionModule(chunk);
};
g.__webpack_require__ = (id: string) => (id.startsWith("/") ? ssr.ssrModules.get(id) : g.__NATA_ACTIONS__?.get(id));
g.__webpack_require__.u = (chunk: string) => chunk;
g.__webpack_get_script_filename__ = (chunk: string) => chunk;

const serverManifest = new Proxy(
  {},
  { get: (_target, id) => (typeof id === "string" ? { id, chunks: [id, ""], name: "" } : undefined) },
);

// ---------- requests ----------

interface RenderInput {
  /** `rsc:` specifier of rsc_runtime.ts */
  runtime: string;
  mode: "html" | "flight";
  route: any;
  headStart: string;
  bootstrap: string;
  cookies?: string;
  /** Request headers (lower-case names), for headers() */
  headers?: Record<string, string>;
  /** next.config basePath: links rendered on the server get it too */
  basePath?: string;
  /** next/image settings (same srcset as the browser) */
  images?: unknown;
  /** Server Action id from the `Next-Action` header (client router calls) */
  actionId?: string | null;
  contentType?: string;
}

export interface StreamResult {
  head: { status: number; headers: Record<string, string> };
  stream?: ReadableStream<Uint8Array>;
}

function setCookies(header: string | undefined) {
  const map = new Map<string, string>();
  for (const pair of (header ?? "").split(";")) {
    const idx = pair.indexOf("=");
    if (idx === -1) continue;
    try {
      const key = decodeURIComponent(pair.slice(0, idx).trim());
      if (key) map.set(key, decodeURIComponent(pair.slice(idx + 1).trim()));
    } catch {}
  }
  g.__NATA_COOKIES__ = map;
  g.__NATA_SET_COOKIES__ = [];
}

const digest = (error: any): string => (error && typeof error.digest === "string" ? error.digest : "");
/** notFound() / forbidden() / unauthorized(): the fallback UI and status the error asks for */
function httpFallback(error: any): { kind: "notFound" | "forbidden" | "unauthorized"; status: number } | null {
  const d = digest(error);
  if (d === "NEXT_NOT_FOUND" || d === "NEXT_HTTP_ERROR_FALLBACK;404") return { kind: "notFound", status: 404 };
  if (d === "NEXT_HTTP_ERROR_FALLBACK;403") return { kind: "forbidden", status: 403 };
  if (d === "NEXT_HTTP_ERROR_FALLBACK;401") return { kind: "unauthorized", status: 401 };
  return null;
}
const FALLBACK_STATUS = { notFound: 404, forbidden: 403, unauthorized: 401 } as const;
const isRedirect = (error: any) => digest(error).startsWith("NEXT_REDIRECT");
function redirectLocation(error: any) {
  const parts = digest(error).split(";");
  return parts.length >= 4 ? parts[2] : parts.slice(1).join(";");
}

const HTML = { "content-type": "text/html; charset=utf-8" };
const FLIGHT = { "content-type": "text/x-component", vary: "RSC, Nata-Router-State" };

function headStartFor(input: RenderInput, rendered: { head: string; title: string | null; fonts?: string }) {
  const title = rendered.title && !rendered.head.includes("<title>") ? `<title>${rendered.title}</title>` : "";
  return input.headStart + (rendered.fonts ?? "") + title + rendered.head;
}

/**
 * next/font CSS, known once the page's modules are loaded (the compiler records each loader call):
 * inlined in the document head; the client router compares the version on navigations
 */
async function fontStyles(): Promise<{ tag: string; version: string }> {
  const { meta, body } = await g.__NATA_CALL_ENGINE__("font_css", {});
  const css = new TextDecoder().decode(body);
  const version = String(meta.version ?? 0);
  return { tag: css ? `<style id="_nata_fonts" data-v="${version}">${css}</style>` : "", version };
}

/** The deepest segment at or above `from` with a UI for `kind` (not-found.tsx, ...), the root otherwise */
function fallbackSegment(route: any, from: number, kind: string) {
  for (let i = Math.min(from, route.segments.length - 1); i >= 0; i--) if (route.segments[i][kind]) return i;
  return 0;
}

async function renderDocument(rsc: any, input: RenderInput, route: any, formState?: unknown): Promise<StreamResult> {
  let rendered;
  try {
    rendered = await rsc.renderRsc(route);
    rendered.fonts = (await fontStyles()).tag;
  } catch (error) {
    return await shellError(rsc, input, route, error, formState);
  }
  const status = route.notFoundAt != null ? FALLBACK_STATUS[(route.fallback ?? "notFound") as keyof typeof FALLBACK_STATUS] : 200;
  try {
    const stream = await ssr.renderHtml(rendered.stream, { headStart: headStartFor(input, rendered), bootstrap: input.bootstrap, formState });
    return { head: { status, headers: HTML }, stream };
  } catch (error) {
    return await shellError(rsc, input, route, error, formState);
  }
}

async function shellError(rsc: any, input: RenderInput, route: any, error: any, formState?: unknown): Promise<StreamResult> {
  if (isRedirect(error)) return { head: { status: 307, headers: { location: redirectLocation(error) } } };
  const fallback = httpFallback(error);
  if (fallback) {
    if (route.notFoundAt == null) {
      return renderDocument(rsc, input, { ...route, fallback: fallback.kind, notFoundAt: fallbackSegment(route, route.segments.length - 1, fallback.kind) }, formState);
    }
    if (route.notFoundAt > 0) {
      return renderDocument(rsc, input, { ...route, fallback: fallback.kind, notFoundAt: fallbackSegment(route, route.notFoundAt - 1, fallback.kind) }, formState);
    }
  }
  // Server rendering failed outside any Suspense boundary: send the payload alone and let the browser
  // render it, so error.tsx (a client boundary) shows the error like it does in Next.js
  console.error(error);
  await reportRequestError(error, { routeType: "render" });
  try {
    const rendered = await rsc.renderRsc(route);
    rendered.fonts = (await fontStyles()).tag;
    const stream = ssr.clientRenderedDocument(rendered.stream, { headStart: headStartFor(input, rendered), bootstrap: input.bootstrap });
    // Like Next.js: an error outside every Suspense boundary is a 500, whatever error.tsx shows
    return { head: { status: 500, headers: HTML }, stream };
  } catch (fatal) {
    throw fatal;
  }
}

export async function renderRoute(input: RenderInput): Promise<StreamResult> {
  setCookies(input.cookies);
  g.__NATA_IMAGES__ = input.images ?? {};
  g.__NATA_BASE_PATH__ = input.basePath ?? "";
  g.__NATA_HEADERS__ = input.headers ?? {};
  const rsc = await import(input.runtime);
  ssr = await import(input.ssrRuntime);
  if (input.mode === "flight") {
    const { stream } = await rsc.renderRsc(input.route);
    const { version } = await fontStyles();
    return { head: { status: 200, headers: { ...FLIGHT, "x-nata-fonts": version } }, stream };
  }
  return renderDocument(rsc, input, input.route);
}

async function formDataOf(body: Uint8Array, contentType: string) {
  return await new Response(body, { headers: { "content-type": contentType } }).formData();
}

export async function runAction(input: RenderInput, body: Uint8Array): Promise<StreamResult> {
  setCookies(input.cookies);
  g.__NATA_IMAGES__ = input.images ?? {};
  g.__NATA_HEADERS__ = input.headers ?? {};
  g.__NATA_REVALIDATED__ = false;
  const rsc = await import(input.runtime);
  ssr = await import(input.ssrRuntime);
  const contentType = input.contentType ?? "";

  if (input.actionId) {
    // Called by the client router: arguments encoded with encodeReply
    await loadActionModule(input.actionId);
    const action = g.__NATA_ACTIONS__.get(input.actionId);
    const payload = contentType.startsWith("multipart/form-data") ? await formDataOf(body, contentType) : new TextDecoder().decode(body);
    const args = await rsc.decodeReply(payload, serverManifest);
    let result: Promise<unknown>;
    let redirect: string | null = null;
    try {
      result = Promise.resolve(await action.apply(null, args));
    } catch (error) {
      if (isRedirect(error)) {
        redirect = redirectLocation(error);
        result = Promise.resolve(undefined);
      } else {
        await reportRequestError(error, { routeType: "action" });
        result = Promise.reject(error);
        result.catch(() => {});
      }
    }
    // revalidatePath() / revalidateTag() / refresh(): the new page travels in the same response
    const tree = g.__NATA_REVALIDATED__ && !redirect ? await rsc.buildModel(input.route) : null;
    const stream = rsc.renderValue({ a: result, f: tree, redirect });
    return { head: { status: 200, headers: FLIGHT }, stream };
  }

  // A <form action={serverAction}> submitted before JavaScript loaded (progressive enhancement)
  const formData = await formDataOf(body, contentType);
  const action = await rsc.decodeAction(formData, serverManifest);
  if (!action) return renderDocument(rsc, input, input.route);
  let result: unknown;
  try {
    result = await action();
  } catch (error) {
    if (isRedirect(error)) return { head: { status: 303, headers: { location: redirectLocation(error) } } };
    throw error;
  }
  const formState = await rsc.decodeFormState(result, formData, serverManifest);
  return renderDocument(rsc, input, input.route, formState);
}
