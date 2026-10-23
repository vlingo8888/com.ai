// Com.AI.VN client router for React Server Components, served at /_nata/router.js.
//
// - Hydrates the server-rendered document from the Flight payload inlined in it (self.__nata_f).
// - Navigates by fetching the next page's payload (`RSC: 1`) and rendering it in a transition. The
//   request carries the segment keys the page already shows (`Nata-Router-State`); the server leaves
//   those layouts out and they are reused as-is, so shared layouts keep their state.
// - Server Actions: `callServer` posts the encoded arguments (`Next-Action: <id>`); a revalidation
//   returns the refreshed page in the same response.
// - Link prefetching (viewport + idle, skipped on Save-Data / 2G), scroll restoration, HMR.
// - Zalo Mini Apps (`__NATA_CONFIG__.mode === "zalo"`): no server in the WebView, so the page payload
//   comes from the backend (`origin`), the location is virtual (kept in history.state) and the page
//   renders into the app container instead of hydrating a server-rendered document.

// First: React Refresh must hook into React before react-dom loads
import "./refresh.js";
import { reportIssue, clearBlockingErrors } from "./overlay.js";
import { mountDebugUi, renderLogs } from "./debug.js";
import * as React from "react";
import * as ReactDOMClient from "react-dom/client";
// Relative to this module: in Zalo the document is not served by the dev server
import { createFromFetch, createFromReadableStream, encodeReply } from "./deps/react-server-dom-webpack/client.browser";
import { AppRoot } from "./rsc/boundaries.js";

const CONFIG = window.__NATA_CONFIG__ || {};
const IS_ZALO = CONFIG.mode === "zalo";
const ORIGIN = (CONFIG.origin || "").replace(/\/$/, "");
// next.config basePath: app paths given to the router get it in front
const BASE_PATH = CONFIG.basePath || "";
const withBasePath = (href) =>
  BASE_PATH && typeof href === "string" && href.startsWith("/") && !href.startsWith("//") && href !== BASE_PATH && !href.startsWith(BASE_PATH + "/") ? BASE_PATH + href : href;

// Code compiled for the classic JSX runtime, or written against a global React
window.React ??= React;
window.ReactDOM ??= ReactDOMClient;

const h = React.createElement;

// Client references: ids are module URLs, loaded by the `__webpack_*` globals the document defines
// before any module runs (the Flight client reads them as it loads)
const clientModules = self.__nata_modules || new Map();

// Every `/_bundle/` module the page has loaded, client references or not: a component imported by
// another client component is only ever fetched by the browser's module loader, and HMR must still
// hot-update it. The default resource timing buffer (250 entries) is far below a real page's modules
const loadedModules = new Set();
try {
  performance.setResourceTimingBufferSize(100000);
  const track = (entries) => {
    for (const entry of entries) {
      const path = new URL(entry.name).pathname;
      if (path.startsWith("/_bundle/")) loadedModules.add(path);
    }
  };
  track(performance.getEntriesByType("resource"));
  new PerformanceObserver((list) => track(list.getEntries())).observe({ type: "resource", buffered: true });
} catch {}

// ---------- router state ----------

let current = null;
let setState = null;
let pendingScroll = null;
let navigationId = 0;
const PREFETCH_TTL = 30_000;
const prefetched = new Map();

const routerState = (keys) => encodeURIComponent(JSON.stringify(keys));
const pathOf = (url) => url.pathname + url.search;

// The App Router location: the real URL, or in Zalo a virtual one kept in history.state
const VIRTUAL_BASE = "http://zalo.local";
function currentUrl() {
  if (!IS_ZALO) return new URL(location.href);
  return new URL((history.state && history.state.path) || CONFIG.start || "/", VIRTUAL_BASE);
}
function writeHistory(method, url, extra) {
  const state = { __nata: true, key: Math.random().toString(36).slice(2), ...extra };
  if (IS_ZALO) {
    history[method]({ ...state, path: pathOf(url) + url.hash }, "", location.href);
    window.__NATA_LOCATION__ = { pathname: url.pathname, search: url.search, href: url.href };
  } else {
    history[method](state, "", url.href);
  }
}

function fetchModel(path, keys, fresh = false, from = null) {
  // Zalo packages built with `com build --target zalo` carry the payloads of their static routes
  const prerendered = IS_ZALO && !fresh && window.__NATA_PRERENDERED__ && window.__NATA_PRERENDERED__[path];
  if (prerendered) return createFromFetch(fetch("./" + prerendered), { callServer });
  const headers = { RSC: "1", "Nata-Router-State": routerState(keys) };
  // Where the navigation comes from: intercepting routes (modals) depend on it
  if (from) headers["Nata-Url"] = encodeURIComponent(from);
  const response = fetch(ORIGIN + path, { headers }).then((res) => {
    syncFonts(res.headers.get("x-nata-fonts"));
    return res;
  });
  return createFromFetch(response, { callServer });
}

/** The page uses a font the document does not have yet (next/font): fetch the font stylesheet */
async function syncFonts(version) {
  if (!version) return;
  let style = document.getElementById("_nata_fonts");
  if (style && style.dataset.v === version) return;
  try {
    const css = await (await fetch(ORIGIN + "/_nata/fonts.css")).text();
    if (!style) {
      style = document.createElement("style");
      style.id = "_nata_fonts";
      document.head.appendChild(style);
    }
    style.textContent = css;
    style.dataset.v = version;
  } catch {}
}

/** Nodes the server left out (same key as what is shown) are reused, keeping their state */
function merge(model) {
  const nodes = model.nodes.map((node, i) => (node === null && current && current.keys[i] === model.keys[i] ? current.nodes[i] : node));
  return { ...model, nodes };
}

function commit(model, scroll) {
  current = merge(model);
  pendingScroll = scroll;
  React.startTransition(() => setState(current));
}

function saveScroll() {
  const state = history.state && history.state.__nata ? history.state : { __nata: true, key: Math.random().toString(36).slice(2) };
  history.replaceState({ ...state, scroll: window.scrollY }, "", location.href);
}

async function navigate(href, { replace = false, scroll = true, pop = false } = {}) {
  href = pop ? href : withBasePath(href);
  const here = currentUrl();
  const url = new URL(href, here);
  if (url.origin !== here.origin || !current) {
    if (!IS_ZALO) location.assign(url.href);
    else if (!current) return;
    else window.open(url.href, "_blank");
    return;
  }
  if (!pop && pathOf(url) === pathOf(here) && url.hash) {
    writeHistory(replace ? "replaceState" : "pushState", url, {});
    document.getElementById(decodeURIComponent(url.hash.slice(1)))?.scrollIntoView();
    return;
  }
  if (!pop) saveScroll();
  const id = ++navigationId;
  let model;
  try {
    model = await (takePrefetched(pathOf(url)) || fetchModel(pathOf(url), current.keys, false, pop ? null : pathOf(here)));
  } catch (error) {
    console.error("[router] client navigation failed, loading the page instead:", error);
    if (IS_ZALO) reportIssue(error, "runtime");
    else location.assign(url.href);
    return;
  }
  if (id !== navigationId) return; // a newer navigation started
  if (!pop) writeHistory(replace ? "replaceState" : "pushState", url, { scroll: 0 });
  else if (IS_ZALO) window.__NATA_LOCATION__ = { pathname: url.pathname, search: url.search, href: url.href };
  commit(model, pop ? (history.state && history.state.scroll) || 0 : scroll ? (url.hash ? url.hash : 0) : null);
}

function prefetch(href) {
  href = withBasePath(href);
  const conn = navigator.connection;
  if (!current || (conn && (conn.saveData || /2g/.test(conn.effectiveType || "")))) return;
  const url = new URL(href, currentUrl());
  if (url.origin !== currentUrl().origin) return;
  const path = pathOf(url);
  const entry = prefetched.get(path);
  if (entry && Date.now() - entry.at < PREFETCH_TTL) return;
  const keys = current.keys;
  const promise = fetchModel(path, keys);
  promise.catch(() => prefetched.delete(path));
  prefetched.set(path, { at: Date.now(), keys: JSON.stringify(keys), promise });
}

function takePrefetched(path) {
  const entry = prefetched.get(path);
  prefetched.delete(path);
  if (!entry || Date.now() - entry.at > PREFETCH_TTL || entry.keys !== JSON.stringify(current.keys)) return null;
  return entry.promise;
}

async function refresh() {
  prefetched.clear();
  try {
    commit(await fetchModel(pathOf(currentUrl()), [], true), null);
  } catch (error) {
    if (IS_ZALO) reportIssue(error, "runtime");
    else location.reload();
  }
}

// ---------- Server Actions ----------

async function callServer(id, args) {
  const response = fetch(ORIGIN + pathOf(currentUrl()), {
    method: "POST",
    headers: { Accept: "text/x-component", "Next-Action": id, "Nata-Router-State": routerState(current ? current.keys : []) },
    body: await encodeReply(args),
  });
  const result = await createFromFetch(response, { callServer });
  if (result.redirect) navigate(result.redirect, { replace: true });
  else if (result.f) {
    prefetched.clear();
    commit(result.f, null);
  }
  return result.a;
}
window.__NATA_CALL_SERVER__ = callServer;

// ---------- the root ----------

const router = {
  push: (href, options = {}) => navigate(href, options),
  replace: (href, options = {}) => navigate(href, { ...options, replace: true }),
  back: () => history.back(),
  forward: () => history.forward(),
  refresh,
  prefetch,
};
window.__NATA_ROUTER__ = router;
// For debugging in the console: the rendered segment nodes and their keys
Object.defineProperty(router, "state", { get: () => current });
window.__NATA_NAVIGATE__ = (href, options = {}) => navigate(href, options);

function Router({ initial }) {
  const [state, set] = React.useState(initial);
  setState = set;
  React.useEffect(() => {
    window.__NATA_PARAMS__ = state.params || {};
    if (state.title) document.title = state.title;
    window.dispatchEvent(new CustomEvent("_nata_navigate", { detail: { params: state.params || {} } }));
    if (typeof pendingScroll === "number") window.scrollTo(0, pendingScroll);
    else if (typeof pendingScroll === "string") document.getElementById(decodeURIComponent(pendingScroll.slice(1)))?.scrollIntoView();
    pendingScroll = null;
  }, [state]);
  const nodes = IS_ZALO ? [withoutDocument(state.nodes[0]), ...state.nodes.slice(1)] : state.nodes;
  return h(AppRoot, { nodes, keys: state.keys, selected: state.selected, router });
}

/**
 * Zalo renders into the app container: the root layout's <html> and <body> become a fragment, their
 * attributes go to the real document elements
 */
function withoutDocument(node) {
  if (!node || node.type !== "html") return node;
  const { children, className, lang } = node.props;
  if (lang) document.documentElement.lang = lang;
  if (className) document.documentElement.className = className;
  const parts = React.Children.toArray(children).flatMap((child) => {
    if (!child || child.type === "head") return [];
    if (child.type === "body") {
      if (child.props.className) document.body.className = child.props.className;
      return [child.props.children];
    }
    return [child];
  });
  return h(React.Fragment, null, ...parts);
}

window.addEventListener("popstate", () => navigate(currentUrl().href, { pop: true }));
let scrollFrame = 0;
window.addEventListener("scroll", () => {
  cancelAnimationFrame(scrollFrame);
  scrollFrame = requestAnimationFrame(() => history.state && history.state.__nata && saveScroll());
}, { passive: true });

/** The payload inlined in the document, as a stream (chunks keep arriving until the HTML is parsed) */
function inlineFlight() {
  const encoder = new TextEncoder();
  return new ReadableStream({
    start(controller) {
      const push = (chunk) => controller.enqueue(encoder.encode(chunk));
      const queue = (self.__nata_f = self.__nata_f || []);
      queue.forEach(push);
      queue.push = push;
      if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", () => controller.close());
      else controller.close();
    },
  });
}

const rootOptions = {
  onUncaughtError: (error) => {
    console.error(error);
    reportIssue(error, "runtime");
  },
  onCaughtError: (error) => {
    console.error(error);
    reportIssue(error, "issue");
  },
  onRecoverableError: (error) => {
    console.warn(error);
    reportIssue(error, "issue");
  },
};

async function start() {
  history.scrollRestoration = "manual";
  if (!history.state || !history.state.__nata) {
    history.replaceState({ __nata: true, key: "initial", scroll: window.scrollY, ...(IS_ZALO ? { path: "/" } : {}) }, "", location.href);
  }
  if (IS_ZALO) return startZalo();
  try {
    current = await createFromReadableStream(inlineFlight(), { callServer });
  } catch (error) {
    console.error("[router] could not read the page payload:", error);
    reportIssue(error, "runtime");
    return;
  }
  const app = h(Router, { initial: current });
  if (self.__nata_csr) {
    // Server rendering failed: render the payload here (error boundaries show the error)
    ReactDOMClient.createRoot(document, rootOptions).render(app);
  } else {
    ReactDOMClient.hydrateRoot(document, app, { ...rootOptions, formState: current.formState || null });
  }
  if (window.requestIdleCallback) window.requestIdleCallback(mountDebugUi);
  else setTimeout(mountDebugUi, 200);
}

/** Zalo: fetch the payload of the virtual location from the backend and render it client-side */
async function startZalo() {
  const url = currentUrl();
  window.__NATA_LOCATION__ = { pathname: url.pathname, search: url.search, href: url.href };
  try {
    current = await fetchModel(pathOf(url), []);
  } catch (error) {
    reportIssue(error, "runtime");
    return;
  }
  let container = document.getElementById("app") || document.getElementById("root");
  if (!container) {
    container = document.createElement("div");
    container.id = "app";
    document.body.appendChild(container);
  }
  ReactDOMClient.createRoot(container, rootOptions).render(h(Router, { initial: current }));
  if (window.requestIdleCallback) window.requestIdleCallback(mountDebugUi);
  else setTimeout(mountDebugUi, 200);
}

// ---------- dev: styles and HMR ----------

async function refreshStyles() {
  const zaloStyles = document.getElementById("_zmp_styles");
  if (zaloStyles) zaloStyles.href = zaloStyles.href.split("?")[0] + "?t=" + Date.now();
  const style = document.getElementById("_nata_custom_css");
  if (!style) return;
  try {
    const res = await fetch(ORIGIN + "/_nata/styles.css?t=" + Date.now());
    if (res.ok) {
      const css = await res.text();
      if (style.textContent !== css) style.textContent = css;
    }
  } catch {}
}

async function onHmr(data) {
  if (data.type === "query_log") {
    window.__NATA_LOGS__ = window.__NATA_LOGS__ || [];
    if (data.log) window.__NATA_LOGS__.push(data.log);
    for (const q of data.queries || []) if (!window.__NATA_LOGS__.some((l) => l.id === q.id)) window.__NATA_LOGS__.push(q);
    renderLogs();
  } else if (data.type === "css") {
    await refreshStyles();
  } else if (data.type === "full_reload") {
    // Many files changed at once (plugin install, checkout): one fresh page load
    location.reload();
  } else if (data.type === "update" && data.module) {
    clearBlockingErrors();
    const { url, client, server, boundary, version } = data.module;
    const reference = [...clientModules.keys()].find((id) => id.split("?")[0] === url);
    const loaded = reference !== undefined || loadedModules.has(url) || document.querySelector(`script[src="${url}"]`);
    if (client && loaded) {
      // Components only: re-run the module and swap implementations, keeping state (Fast Refresh);
      // anything else (hooks, utilities, constants) may have been captured by importers: reload
      if (!boundary || !self.__NATA_REFRESH__) return location.reload();
      try {
        const module = await import(`${ORIGIN}${url}?v=${version}`);
        // Later payloads must resolve this client reference to the new implementation too, or
        // React would see a different component type and remount it
        if (reference !== undefined) clientModules.set(url, module);
      } catch (error) {
        reportIssue(error, "runtime");
        return;
      }
    }
    // Server Components read the new code on the next render: refetch this page's payload
    if (server) await refresh();
    if (client && loaded) self.__NATA_REFRESH__.performReactRefresh();
    await refreshStyles();
  } else if (data.type === "reload" || data.type === "update") {
    clearBlockingErrors();
    const file = "/_bundle/" + (data.file || "");
    if ([...clientModules.keys()].some((id) => id.split("?")[0] === file)) location.reload();
    else await refresh();
    await refreshStyles();
  }
}

function connectHmr() {
  let opened = false;
  const ws = new WebSocket((ORIGIN || location.origin).replace(/^http/, "ws") + "/_hmr");
  ws.onopen = () => (opened = true);
  // One message at a time: concurrent refreshes / reloads raced each other on bursts of changes
  let queue = Promise.resolve();
  ws.onmessage = (event) => {
    const data = JSON.parse(event.data);
    queue = queue.then(() => onHmr(data)).catch((error) => console.error("[HMR]", error));
  };
  ws.onclose = () => {
    const status = document.getElementById("_dev_status");
    if (status) status.style.background = "#ef4444";
    if (!opened) pollHmr();
  };
}

function pollHmr() {
  let last = null;
  const tick = async () => {
    try {
      const data = await (await fetch(ORIGIN + "/_nata/hmr_version", { cache: "no-store" })).json();
      if (last !== null && data.version !== last) await onHmr({ type: data.last_type || "reload", file: data.last_file });
      last = data.version;
    } catch {}
    setTimeout(tick, 1500);
  };
  tick();
}

// Zalo: the container owns the page URL, so a full page load leaves the mini app (the tunnel's web server answers).
// Plain <a href="/x"> links (website sections, rich text, plugin views) navigate inside the app instead; <Link>
// already handles its own clicks (defaultPrevented), other sites / tel: / mailto: / new tabs keep the default.
function interceptZaloLinks(go) {
  document.addEventListener("click", (e) => {
    if (e.defaultPrevented || e.button !== 0 || e.metaKey || e.ctrlKey || e.shiftKey || e.altKey) return;
    const a = e.target instanceof Element ? e.target.closest("a[href]") : null;
    if (!a || a.hasAttribute("download") || (a.getAttribute("target") && a.getAttribute("target") !== "_self")) return;
    const href = a.getAttribute("href") || "";
    if (!href || href.startsWith("#") || href.startsWith("//") || /^[a-z][a-z0-9+.-]*:/i.test(href)) return;
    e.preventDefault();
    go(href);
  });
}
if (IS_ZALO) interceptZaloLinks((href) => navigate(href));

start();
connectHmr();
