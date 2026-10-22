// React Server Components renderer: runs in the worker's react-server layer (loaded as
// `rsc:<root>/.nata/rsc_runtime.ts`, compiled by the engine), so `react` here is React's server build
// and every page, layout and Server Component runs on the server for real.
//
// The Flight model is `{ nodes, keys, title, head }`: one node per route folder (root to leaf), then
// the page. A folder's node is its layout around the folder's template / error / loading / not-found
// boundaries and an <Outlet> for the next node (see boundaries.js). The client router passes the keys
// of the nodes it already has; those come back as `null` and the client keeps its own (P1.6).

import * as React from "react";
import {
  decodeAction,
  decodeFormState,
  decodeReply,
  registerClientReference,
  renderToReadableStream,
} from "react-server-dom-webpack/server.edge";
import { resolveCascadeMetadata } from "./metadata_runtime.ts";
import { installFetchCache } from "./next/cache-runtime.ts";
import { dynamicProps, markDynamic, noteRevalidate, noteTags, renderInfo } from "./next/request.ts";

// One import at a time for modules a request has not loaded yet (serial_import.ts)
const importModule = (specifier: string): Promise<any> => ((globalThis as any).__nata_import ?? ((s: string) => import(s)))(specifier);

// fetch(url, { cache: "force-cache", next: { revalidate, tags } }) goes through the data cache
installFetchCache();

export { decodeAction, decodeFormState, decodeReply };

const h = React.createElement;

interface SegmentInput {
  key: string;
  layout?: string | null;
  template?: string | null;
  error?: string | null;
  loading?: string | null;
  notFound?: string | null;
  forbidden?: string | null;
  unauthorized?: string | null;
  /** Parallel routes: `@name` folders of this layout, resolved for the URL */
  slots?: SlotInput[];
}

interface SlotInput {
  name: string;
  segments: SegmentInput[];
  page: string;
  params: Record<string, unknown>;
}

export interface RscInput {
  pathname: string;
  search: string;
  params: Record<string, unknown>;
  searchParams: Record<string, unknown>;
  segments: SegmentInput[];
  page: string | null;
  pageKey: string;
  globalError?: string | null;
  /** How many leading nodes the client router already has (sent back as null) */
  skip?: number;
  /** Render a fallback UI of this segment (`fallback`, not-found by default) in place of its children */
  notFoundAt?: number | null;
  fallback?: "notFound" | "forbidden" | "unauthorized";
  /** Per node: the URL segments below it (`useSelectedLayoutSegments`) */
  selected?: string[][];
}

const BOUNDARIES = "/_nata/rsc/boundaries.js";
const clientRef = (name: string) =>
  registerClientReference(
    function () {
      throw new Error(`${name} is a client component`);
    },
    BOUNDARIES,
    name,
  );
const Outlet = clientRef("Outlet");
const ErrorBoundary = clientRef("ErrorBoundary");
const NotFoundBoundary = clientRef("NotFoundBoundary");
const LoadingBoundary = clientRef("LoadingBoundary");
const TemplateBoundary = clientRef("TemplateBoundary");

/**
 * Every client reference resolves to its browser module: ids are the URLs the browser imports
 * (`/_bundle/app/form.tsx`, `/_nata/deps/<pkg>`), and each module is its own chunk
 */
export const clientManifest = new Proxy(
  {},
  {
    get(_target, key) {
      if (typeof key !== "string") return undefined;
      const hash = key.lastIndexOf("#");
      const id = hash === -1 ? key : key.slice(0, hash);
      const name = hash === -1 ? "*" : key.slice(hash + 1);
      return { id, chunks: [id, id], name };
    },
  },
);

async function load(specifier: string | null | undefined) {
  if (!specifier) return null;
  return await importModule(specifier);
}

const componentOf = (mod: any, hint: RegExp) => {
  if (!mod) return null;
  if (mod.default) return mod.default;
  const entry = Object.entries(mod).find(([k, v]) => typeof v === "function" && hint.test(k));
  return entry ? entry[1] : null;
};

/** Next.js 15 passes params as a Promise; the properties stay readable for older code */
function asyncProps<T extends object>(value: T): Promise<T> & T {
  return Object.assign(Promise.resolve(value), value);
}

function setRequestContext(input: RscInput) {
  (globalThis as any).__NATA_PARAMS__ = input.params;
  (globalThis as any).__NATA_LOCATION__ = {
    pathname: input.pathname,
    search: input.search,
    href: `http://localhost${input.pathname}${input.search}`,
  };
}

export async function buildModel(input: RscInput) {
  setRequestContext(input);
  const params = asyncProps({ ...input.params });
  const searchParams = dynamicProps({ ...input.searchParams }, "searchParams");
  // revalidatePath(pathname) purges the cached page and the data it rendered
  noteTags([`_N_T_${input.pathname}`]);

  const segmentModules = await Promise.all(
    input.segments.map(async (seg) => ({
      layout: await load(seg.layout),
      template: await load(seg.template),
      error: await load(seg.error),
      loading: await load(seg.loading),
      notFound: await load(seg.notFound),
      forbidden: await load(seg.forbidden),
      unauthorized: await load(seg.unauthorized),
    })),
  );
  const notFoundAt = input.notFoundAt ?? null;
  const pageMod = notFoundAt === null ? await load(input.page) : null;
  const globalErrorMod = await load(input.globalError);

  const keys = input.segments.map((s) => s.key);
  if (notFoundAt === null) keys.push(input.pageKey);
  else keys.splice(notFoundAt + 1);

  const nodes: unknown[] = [];
  const lastSegment = notFoundAt ?? input.segments.length - 1;
  for (let i = 0; i <= lastSegment; i++) {
    const mods = segmentModules[i];
    // Boundaries read the key of the segment below them on the client (it changes while this layout
    // is reused); `depth` is that segment's index
    const depth = i + 1;

    const fallbackUi = (kind: "notFound" | "forbidden" | "unauthorized") => {
      const Component = componentOf(mods[kind], /NotFound|Forbidden|Unauthorized/);
      return Component ? h(Component, {}) : null;
    };
    let inner: any;
    if (notFoundAt === i) {
      const kind = input.fallback ?? "notFound";
      inner = fallbackUi(kind) ?? h(DefaultFallback, { kind });
    } else {
      inner = h(Outlet, { depth: i + 1 });
    }
    const fallbacks = { notFound: fallbackUi("notFound"), forbidden: fallbackUi("forbidden"), unauthorized: fallbackUi("unauthorized") };
    if (notFoundAt !== i && (fallbacks.notFound || fallbacks.forbidden || fallbacks.unauthorized)) {
      inner = h(NotFoundBoundary, { ...fallbacks, depth }, inner);
    }
    const Loading = componentOf(mods.loading, /Loading/);
    if (Loading) inner = h(LoadingBoundary, { fallback: h(Loading, {}), depth }, inner);
    const ErrorComponent = componentOf(mods.error, /Error/);
    if (ErrorComponent) inner = h(ErrorBoundary, { errorComponent: ErrorComponent, depth }, inner);
    const Template = componentOf(mods.template, /Template/);
    if (Template) inner = h(TemplateBoundary, { depth }, h(Template, {}, inner));

    const Layout = componentOf(mods.layout, /Layout/);
    // Parallel routes: each slot is a prop of the layout (`@team` → `team`)
    const slotProps: Record<string, unknown> = {};
    for (const slot of input.segments[i].slots ?? []) slotProps[slot.name] = await buildSlot(slot, params);
    let node: any = Layout ? h(Layout, { params, ...slotProps }, inner) : inner;
    if (i === 0) {
      // A root layout renders <html>; without one, provide the document like Next's default
      if (!Layout) node = h("html", { lang: "en" }, h("body", null, node));
      const GlobalError = componentOf(globalErrorMod, /Error/);
      if (GlobalError) node = h(ErrorBoundary, { errorComponent: GlobalError, depth: 0 }, node);
    }
    nodes.push(node);
  }
  if (input.segments.length === 0) nodes.push(h("html", { lang: "en" }, h("body", null, h(Outlet, { depth: 1 }))));

  if (notFoundAt === null) await checkStaticParams(pageMod, segmentModules.map((m) => m.layout), input.params);
  applySegmentConfig([...segmentModules.map((m) => m.layout), pageMod]);

  if (notFoundAt === null) {
    const Page = componentOf(pageMod, /Page$|^[A-Z]/);
    if (!Page) throw new Error(`No React component or default export found in page: ${input.page}`);
    nodes.push(h(Page, { params, searchParams }));
  }

  // Metadata: layouts root to leaf, then the page (or the not-found UI)
  const cascade = segmentModules
    .slice(0, lastSegment + 1)
    .map((m) => m.layout)
    .filter(Boolean);
  cascade.push(notFoundAt === null ? pageMod : segmentModules[notFoundAt][input.fallback ?? "notFound"]);
  const { resolvedTitle, headHtml } = await resolveCascadeMetadata(cascade, {
    params: input.params,
    searchParams: dynamicProps({ ...input.searchParams }, "searchParams"),
  });

  const skip = Math.min(input.skip ?? 0, nodes.length - 1);
  return {
    // Keyed by segment: Flight treats the list like children, and a different segment at a depth
    // is a different subtree (remounted), as in Next.js
    nodes: nodes.map((node: any, i) => (i < skip ? null : React.isValidElement(node) ? React.cloneElement(node, { key: keys[i] ?? String(i) }) : node)),
    keys,
    params: input.params,
    selected: input.selected ?? [],
    title: resolvedTitle ?? (notFoundAt === null ? null : FALLBACK_TEXT[input.fallback ?? "notFound"].title),
    head: headHtml,
  };
}

const FALLBACK_TEXT = {
  notFound: { status: "404", message: "This page could not be found.", title: "404: This page could not be found." },
  forbidden: { status: "403", message: "This page is forbidden.", title: "403: This page is forbidden." },
  unauthorized: { status: "401", message: "You're not authorized to access this page.", title: "401: Unauthorized." },
};

/** Next.js's built-in 404 / 403 / 401 page */
function DefaultFallback({ kind }: { kind: keyof typeof FALLBACK_TEXT }) {
  const text = FALLBACK_TEXT[kind];
  return h(
    "div",
    { style: { fontFamily: "system-ui, sans-serif", height: "100vh", display: "flex", alignItems: "center", justifyContent: "center" } },
    h("h1", { style: { fontSize: 24, fontWeight: 500, paddingRight: 23, marginRight: 20, borderRight: "1px solid rgba(0,0,0,.3)" } }, text.status),
    h("h2", { style: { fontSize: 14, fontWeight: 400 } }, text.message),
  );
}

/** A slot's own tree: its layouts (and their boundaries) around its page or default.tsx */
async function buildSlot(slot: SlotInput, parentParams: Promise<any> & Record<string, unknown>) {
  const params = asyncProps({ ...parentParams, ...slot.params });
  const Page = componentOf(await load(slot.page), /Page$|Default$|^[A-Z]/);
  let node: any = Page ? h(Page, { params }) : null;
  for (let i = slot.segments.length - 1; i >= 0; i--) {
    const seg = slot.segments[i];
    const [layout, loading, error, notFound, template] = await Promise.all([load(seg.layout), load(seg.loading), load(seg.error), load(seg.notFound), load(seg.template)]);
    const NotFound = componentOf(notFound, /NotFound/);
    if (NotFound) node = h(NotFoundBoundary, { notFound: h(NotFound, {}), depth: -1 }, node);
    const Loading = componentOf(loading, /Loading/);
    if (Loading) node = h(LoadingBoundary, { fallback: h(Loading, {}), depth: -1 }, node);
    const ErrorComponent = componentOf(error, /Error/);
    if (ErrorComponent) node = h(ErrorBoundary, { errorComponent: ErrorComponent, depth: -1 }, node);
    const Template = componentOf(template, /Template/);
    if (Template) node = h(Template, {}, node);
    const Layout = componentOf(layout, /Layout/);
    if (Layout) node = h(Layout, { params }, node);
  }
  return node;
}

/**
 * Route segment config (`dynamic`, `revalidate`, `fetchCache`) of the layouts and the page, the
 * deepest segment winning: how `fetch` caches during this render. `runtime`, `preferredRegion` and
 * `maxDuration` are accepted and have no effect in development.
 */
function applySegmentConfig(modules: any[]) {
  const config: { dynamic?: string; revalidate?: number | false; fetchCache?: string } = {};
  for (const mod of modules) {
    if (!mod) continue;
    if (typeof mod.dynamic === "string") config.dynamic = mod.dynamic;
    if (typeof mod.revalidate === "number" || mod.revalidate === false) config.revalidate = mod.revalidate;
    if (typeof mod.fetchCache === "string") config.fetchCache = mod.fetchCache;
  }
  (globalThis as any).__NATA_SEGMENT_CONFIG__ = config;
  if (config.dynamic === "force-dynamic") markDynamic("dynamic = \"force-dynamic\"");
  if (config.dynamic === "force-static" || config.dynamic === "error") renderInfo().forceStatic = true;
  noteRevalidate(config.revalidate);
}

/**
 * `export const dynamicParams = false` with `generateStaticParams`: params outside the generated list
 * are a 404, like Next.js (the default, `true`, renders them on demand)
 */
async function checkStaticParams(pageMod: any, layouts: any[], params: Record<string, unknown>) {
  const dynamicParams = [...layouts, pageMod].reduce((value, mod) => (mod && typeof mod.dynamicParams === "boolean" ? mod.dynamicParams : value), true);
  if (dynamicParams || typeof pageMod?.generateStaticParams !== "function") return;
  const list = await pageMod.generateStaticParams({ params });
  const same = (a: unknown, b: unknown) => (Array.isArray(a) || Array.isArray(b) ? [a].flat().join("/") === [b].flat().join("/") : String(a) === String(b));
  const allowed = Array.isArray(list) && list.some((entry: Record<string, unknown>) => Object.entries(entry).every(([k, v]) => same(params[k], v)));
  if (!allowed) {
    const err: any = new Error("NEXT_NOT_FOUND");
    err.digest = "NEXT_NOT_FOUND";
    throw err;
  }
}

/**
 * `com build`: the params to prerender, from `generateStaticParams` of the layouts and the page, top
 * down (a layout's params are passed to the segments below it). `null` when none defines it.
 */
export async function staticParams(specifiers: (string | null)[]) {
  let list: Record<string, unknown>[] = [{}];
  let defined = false;
  for (const specifier of specifiers) {
    const mod = specifier ? await importModule(specifier) : null;
    if (typeof mod?.generateStaticParams !== "function") continue;
    defined = true;
    const next: Record<string, unknown>[] = [];
    for (const parent of list) {
      const generated = await mod.generateStaticParams({ params: parent });
      for (const params of Array.isArray(generated) ? generated : []) next.push({ ...parent, ...params });
    }
    list = next;
  }
  return defined ? list : null;
}

export function onFlightError(error: any) {
  if (error && typeof error.digest === "string") return error.digest;
  return undefined;
}

/** Renders the route to a Flight stream; `head` and `title` are also returned for the HTML document */
export async function renderRsc(input: RscInput, extra: Record<string, unknown> = {}) {
  const model = await buildModel(input);
  const stream = renderToReadableStream({ ...model, ...extra }, clientManifest, { onError: onFlightError });
  return { stream, head: model.head, title: model.title };
}

/** Flight stream of an arbitrary value (Server Action results) */
export function renderValue(value: unknown) {
  return renderToReadableStream(value, clientManifest, { onError: onFlightError });
}
