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

export { decodeAction, decodeFormState, decodeReply };

const h = React.createElement;

interface SegmentInput {
  key: string;
  layout?: string | null;
  template?: string | null;
  error?: string | null;
  loading?: string | null;
  notFound?: string | null;
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
  /** Render the not-found UI of this segment in place of its children */
  notFoundAt?: number | null;
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
  return await import(specifier);
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
  const searchParams = asyncProps({ ...input.searchParams });

  const segmentModules = await Promise.all(
    input.segments.map(async (seg) => ({
      layout: await load(seg.layout),
      template: await load(seg.template),
      error: await load(seg.error),
      loading: await load(seg.loading),
      notFound: await load(seg.notFound),
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

    let inner: any;
    if (notFoundAt === i) {
      const NotFound = componentOf(mods.notFound, /NotFound/);
      inner = NotFound ? h(NotFound, {}) : h(DefaultNotFound, {});
    } else {
      inner = h(Outlet, { depth: i + 1 });
    }
    const NotFound = componentOf(mods.notFound, /NotFound/);
    if (NotFound && notFoundAt !== i) inner = h(NotFoundBoundary, { notFound: h(NotFound, {}), depth }, inner);
    const Loading = componentOf(mods.loading, /Loading/);
    if (Loading) inner = h(LoadingBoundary, { fallback: h(Loading, {}), depth }, inner);
    const ErrorComponent = componentOf(mods.error, /Error/);
    if (ErrorComponent) inner = h(ErrorBoundary, { errorComponent: ErrorComponent, depth }, inner);
    const Template = componentOf(mods.template, /Template/);
    if (Template) inner = h(TemplateBoundary, { depth }, h(Template, {}, inner));

    const Layout = componentOf(mods.layout, /Layout/);
    let node: any = Layout ? h(Layout, { params }, inner) : inner;
    if (i === 0) {
      // A root layout renders <html>; without one, provide the document like Next's default
      if (!Layout) node = h("html", { lang: "en" }, h("body", null, node));
      const GlobalError = componentOf(globalErrorMod, /Error/);
      if (GlobalError) node = h(ErrorBoundary, { errorComponent: GlobalError, depth: 0 }, node);
    }
    nodes.push(node);
  }
  if (input.segments.length === 0) nodes.push(h("html", { lang: "en" }, h("body", null, h(Outlet, { depth: 1 }))));

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
  cascade.push(notFoundAt === null ? pageMod : segmentModules[notFoundAt].notFound);
  const { resolvedTitle, headHtml } = await resolveCascadeMetadata(cascade, {
    params: input.params,
    searchParams: input.searchParams,
  });

  const skip = Math.min(input.skip ?? 0, nodes.length - 1);
  return {
    // Keyed by segment: Flight treats the list like children, and a different segment at a depth
    // is a different subtree (remounted), as in Next.js
    nodes: nodes.map((node: any, i) => (i < skip ? null : React.isValidElement(node) ? React.cloneElement(node, { key: keys[i] ?? String(i) }) : node)),
    keys,
    params: input.params,
    selected: input.selected ?? [],
    title: resolvedTitle ?? (notFoundAt === null ? null : "404: This page could not be found."),
    head: headHtml,
  };
}

function DefaultNotFound() {
  return h(
    "div",
    { style: { fontFamily: "system-ui, sans-serif", height: "100vh", display: "flex", alignItems: "center", justifyContent: "center" } },
    h("h1", { style: { fontSize: 24, fontWeight: 500, paddingRight: 23, marginRight: 20, borderRight: "1px solid rgba(0,0,0,.3)" } }, "404"),
    h("h2", { style: { fontSize: 14, fontWeight: 400 } }, "This page could not be found."),
  );
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
