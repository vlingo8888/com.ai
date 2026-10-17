// Worker commands of the React Server Components pipeline: `rsc` (a page, as an HTML document or as a
// Flight payload for the client router) and `action` (a Server Action, called by the client router or
// posted by a <form> before JavaScript loaded). Results stream back to the engine chunk by chunk.

import { clientRenderedDocument, loadClientModule, renderHtml, ssrModules } from "./rsc_ssr.ts";

const g = globalThis as any;

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
  return chunk.startsWith("/") ? loadClientModule(chunk) : loadActionModule(chunk);
};
g.__webpack_require__ = (id: string) => (id.startsWith("/") ? ssrModules.get(id) : g.__NATA_ACTIONS__?.get(id));
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
const isNotFound = (error: any) => digest(error) === "NEXT_NOT_FOUND";
const isRedirect = (error: any) => digest(error).startsWith("NEXT_REDIRECT");
function redirectLocation(error: any) {
  const parts = digest(error).split(";");
  return parts.length >= 4 ? parts[2] : parts.slice(1).join(";");
}

const HTML = { "content-type": "text/html; charset=utf-8" };
const FLIGHT = { "content-type": "text/x-component", vary: "RSC, Nata-Router-State" };

function headStartFor(input: RenderInput, rendered: { head: string; title: string | null }) {
  const title = rendered.title && !rendered.head.includes("<title>") ? `<title>${rendered.title}</title>` : "";
  return input.headStart + title + rendered.head;
}

/** The deepest segment at or above `from` that has a not-found.tsx (the root otherwise) */
function notFoundSegment(route: any, from: number) {
  for (let i = Math.min(from, route.segments.length - 1); i >= 0; i--) if (route.segments[i].notFound) return i;
  return 0;
}

async function renderDocument(rsc: any, input: RenderInput, route: any, formState?: unknown): Promise<StreamResult> {
  let rendered;
  try {
    rendered = await rsc.renderRsc(route);
  } catch (error) {
    return await shellError(rsc, input, route, error, formState);
  }
  const status = route.notFoundAt != null ? 404 : 200;
  try {
    const stream = await renderHtml(rendered.stream, { headStart: headStartFor(input, rendered), bootstrap: input.bootstrap, formState });
    return { head: { status, headers: HTML }, stream };
  } catch (error) {
    return await shellError(rsc, input, route, error, formState);
  }
}

async function shellError(rsc: any, input: RenderInput, route: any, error: any, formState?: unknown): Promise<StreamResult> {
  if (isRedirect(error)) return { head: { status: 307, headers: { location: redirectLocation(error) } } };
  if (isNotFound(error)) {
    if (route.notFoundAt == null) return renderDocument(rsc, input, { ...route, notFoundAt: notFoundSegment(route, route.segments.length - 1) }, formState);
    if (route.notFoundAt > 0) return renderDocument(rsc, input, { ...route, notFoundAt: notFoundSegment(route, route.notFoundAt - 1) }, formState);
  }
  // Server rendering failed outside any Suspense boundary: send the payload alone and let the browser
  // render it, so error.tsx (a client boundary) shows the error like it does in Next.js
  console.error(error);
  try {
    const rendered = await rsc.renderRsc(route);
    const stream = clientRenderedDocument(rendered.stream, { headStart: headStartFor(input, rendered), bootstrap: input.bootstrap });
    return { head: { status: route.notFoundAt != null ? 404 : 200, headers: HTML }, stream };
  } catch (fatal) {
    throw fatal;
  }
}

export async function renderRoute(input: RenderInput): Promise<StreamResult> {
  setCookies(input.cookies);
  const rsc = await import(input.runtime);
  if (input.mode === "flight") {
    const { stream } = await rsc.renderRsc(input.route);
    return { head: { status: 200, headers: FLIGHT }, stream };
  }
  return renderDocument(rsc, input, input.route);
}

async function formDataOf(body: Uint8Array, contentType: string) {
  return await new Response(body, { headers: { "content-type": contentType } }).formData();
}

export async function runAction(input: RenderInput, body: Uint8Array): Promise<StreamResult> {
  setCookies(input.cookies);
  g.__NATA_REVALIDATED__ = false;
  const rsc = await import(input.runtime);
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
