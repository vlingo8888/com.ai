// Imported first so module load failures below are still reported in the overlay
import { reportIssue, clearBlockingErrors } from "./overlay.js";
import { mountDebugUi, renderLogs } from "./debug.js";
import React from "react";
import ReactDOM from "react-dom/client";

// Bind React and ReactDOM to global scope for SWC classic runtime
window.React = React;
window.ReactDOM = ReactDOM;
// Modules compiled with React Refresh registration run fine without the refresh runtime
window.$RefreshReg$ = window.$RefreshReg$ || (() => {});
window.$RefreshSig$ = window.$RefreshSig$ || (() => (type) => type);

// Runtime config injected by the HTML shell (web) or /assets/app.js bootstrap (zalo)
const NATA_CONFIG = window.__NATA_CONFIG__ || {};
const IS_ZALO = NATA_CONFIG.mode === "zalo";
const ORIGIN = (NATA_CONFIG.origin || "").replace(/\/$/, "");

// Location abstraction: web mirrors window.location, zalo keeps a virtual (memory) location
// so the Zalo container URL is never touched.
function parseLocation(url) {
  const u = new URL(url, "http://nata.local");
  return { pathname: u.pathname || "/", search: u.search, hash: u.hash };
}

function readInitialZaloPath() {
  const hash = window.location.hash || "";
  return hash.startsWith("#/") ? hash.slice(1) : "/";
}

window.__NATA_LOCATION__ = IS_ZALO
  ? parseLocation(readInitialZaloPath())
  : parseLocation(window.location.pathname + window.location.search + window.location.hash);

function currentPathname() {
  return window.__NATA_LOCATION__.pathname;
}

function currentSearch() {
  return window.__NATA_LOCATION__.search;
}

const moduleCache = new Map();


async function loadModule(specifier, timestamp = null) {
  let clean = specifier.trim();
  if (clean.startsWith("/_bundle/")) {
    clean = clean.slice(9);
  }
  const normKey = clean.replace(/^\/+/, '').split('?')[0];

  if (!timestamp && moduleCache.has(normKey)) {
    return moduleCache.get(normKey);
  }

  const targetUrl = ORIGIN + "/_bundle/" + normKey + (timestamp ? "?t=" + timestamp : "");
  try {
    const mod = await import(targetUrl);
    moduleCache.set(normKey, mod);
    return mod;
  } catch (err) {
    throw err;
  }
}

let currentRoot = null;
let currentRenderId = 0;

function unwrapHtmlBody(node) {
  if (!node) return node;
  if (Array.isArray(node)) {
    const unwrapped = node.map(unwrapHtmlBody).filter(Boolean);
    if (unwrapped.length === 0) return null;
    if (unwrapped.length === 1) return unwrapped[0];
    return React.createElement(React.Fragment, null, ...unwrapped);
  }
  if (React.isValidElement(node)) {
    const type = node.type;
    const props = node.props || {};
    if (type === 'html' || (typeof type === 'string' && type.toLowerCase() === 'html')) {
      if (props.lang && typeof document !== 'undefined' && document.documentElement) {
        document.documentElement.lang = props.lang;
      }
      if (props.className && typeof document !== 'undefined' && document.documentElement) {
        document.documentElement.className = props.className;
      }
      return unwrapHtmlBody(props.children);
    }
    if (type === 'body' || (typeof type === 'string' && type.toLowerCase() === 'body')) {
      if (props.className && typeof document !== 'undefined' && document.body) {
        document.body.className = props.className;
      }
      return unwrapHtmlBody(props.children);
    }
    if (type === 'head' || (typeof type === 'string' && type.toLowerCase() === 'head')) {
      return null;
    }
    return node;
  }
  return node;
}

// Next 15 passes `params`/`searchParams` as promises, Next 14 code reads them synchronously: return a
// plain object that is also awaitable. `then` resolves with a *copy without `then`*; resolving with the
// object itself would make Promise resolution call `then` again, forever (hanging the page).
function awaitableRecord(values) {
  const plain = { ...(values || {}) };
  const record = { ...plain };
  Object.defineProperty(record, 'then', {
    value: (resolve, reject) => Promise.resolve(plain).then(resolve, reject),
    enumerable: false,
    writable: true,
    configurable: true,
  });
  return record;
}

function makeParams(paramsObj) {
  return awaitableRecord(paramsObj);
}

function makeSearchParams(searchParamsObj) {
  return awaitableRecord(searchParamsObj);
}

// Route components (page, layouts, templates, loading, error, not-found). An `async` component — a Server
// Component until RSC lands (ROADMAP P1.5) — suspends like Next.js: it throws its pending promise, so
// the nearest <Suspense> (from loading.tsx) shows its fallback until the data is there. renderRoute
// creates new wrappers on every navigation/refresh, so the cached result never outlives its route.
function makeComponent(Comp) {
  if (!Comp) return () => null;
  let pending = null;
  return function DynamicComponentWrapper(props = {}) {
    if (pending) {
      if (pending.status === "fulfilled") return pending.value;
      if (pending.status === "rejected") throw pending.value;
      throw pending.promise;
    }
    const res = Comp(props || {});
    if (res && typeof res.then === "function") {
      const entry = { status: "pending", value: undefined, promise: null };
      entry.promise = Promise.resolve(res).then(
        (value) => { entry.status = "fulfilled"; entry.value = unwrapHtmlBody(value); },
        (error) => { entry.status = "rejected"; entry.value = error; },
      );
      pending = entry;
      throw entry.promise;
    }
    return unwrapHtmlBody(res);
  };
}

class DevRootBoundary extends React.Component {
  constructor(props) {
    super(props);
    this.state = { error: null };
  }
  static getDerivedStateFromError(error) {
    return { error };
  }
  componentDidCatch(error) {
    reportIssue(error, "runtime");
  }
  render() {
    return this.state.error ? null : this.props.children;
  }
}

class ErrorBoundary extends React.Component {
  constructor(props) {
    super(props);
    this.state = { hasError: false, error: null };
  }
  static getDerivedStateFromError(error) {
    return { hasError: true, error };
  }
  componentDidCatch(error, info) {
    // Handled by the route's error.tsx, so it only counts as an issue (like Next.js)
    reportIssue(error, "issue");
  }
  render() {
    if (this.state.hasError) {
      if (this.props.fallback) {
        const Fallback = this.props.fallback;
        return React.createElement(Fallback, {
          error: this.state.error,
          reset: () => this.setState({ hasError: false, error: null })
        });
      }
      return React.createElement("div", {
        style: { padding: "2rem", color: "#ef4444", fontFamily: "system-ui" }
      }, React.createElement("h2", { style: { fontWeight: 600 } }, "Application Error"),
         React.createElement("pre", { style: { marginTop: "0.5rem", fontSize: "0.875rem" } }, this.state.error?.message || String(this.state.error)),
         React.createElement("button", {
           onClick: () => this.setState({ hasError: false, error: null }),
           style: { marginTop: "1rem", padding: "0.5rem 1rem", background: "#ef4444", color: "white", borderRadius: "0.375rem", border: "none", cursor: "pointer" }
         }, "Try again")
      );
    }
    return this.props.children;
  }
}

class NotFoundBoundary extends React.Component {
  constructor(props) {
    super(props);
    this.state = { isNotFound: false };
  }
  static getDerivedStateFromError(error) {
    if (error?.digest === "NEXT_NOT_FOUND" || error?.message === "NEXT_NOT_FOUND" || (error?.message && error.message.includes("404 Not Found"))) {
      return { isNotFound: true };
    }
    throw error;
  }
  render() {
    if (this.state.isNotFound) {
      if (this.props.fallback) {
        const Fallback = this.props.fallback;
        return React.createElement(Fallback, {});
      }
      return React.createElement("div", {
        style: { minHeight: "60vh", display: "flex", alignItems: "center", justifyContent: "center", flexDirection: "column" }
      }, React.createElement("h1", { style: { fontSize: "2rem", fontWeight: 700 } }, "404 - Page Not Found"));
    }
    return this.props.children;
  }
}

function setMetaTag(name, content, isProperty = false) {
  if (!content) return;
  const attr = isProperty ? "property" : "name";
  let el = document.querySelector(`meta[${attr}="${name}"]`);
  if (!el) {
    el = document.createElement("meta");
    el.setAttribute(attr, name);
    document.head.appendChild(el);
  }
  el.setAttribute("content", content);
}

function setLinkTag(rel, href) {
  if (!href) return;
  let el = document.querySelector(`link[rel="${rel}"]`);
  if (!el) {
    el = document.createElement("link");
    el.setAttribute("rel", rel);
    document.head.appendChild(el);
  }
  el.setAttribute("href", href);
}

async function updateClientMetadata(pageMod, layoutModules, routeProps) {
  try {
    const modules = [...(layoutModules || []), pageMod];
    let resolvedTitle = null;
    let titleTemplate = null;
    let merged = {};

    for (const mod of modules) {
      if (!mod) continue;
      let meta = null;
      if (typeof mod.generateMetadata === "function") {
        try {
          meta = await mod.generateMetadata(routeProps, Promise.resolve(merged));
        } catch (e) {}
      } else if (mod.metadata) {
        meta = typeof mod.metadata === "function" ? await mod.metadata() : mod.metadata;
      }
      if (!meta || typeof meta !== "object") continue;

      if (meta.title !== undefined) {
        if (typeof meta.title === "string") {
          resolvedTitle = titleTemplate && titleTemplate.includes("%s") ? titleTemplate.replace("%s", meta.title) : meta.title;
        } else if (typeof meta.title === "object") {
          if (meta.title.absolute) {
            resolvedTitle = meta.title.absolute;
          } else if (meta.title.default) {
            resolvedTitle = titleTemplate && titleTemplate.includes("%s") ? titleTemplate.replace("%s", meta.title.default) : meta.title.default;
          }
          if (meta.title.template) {
            titleTemplate = meta.title.template;
          }
        }
      }

      if (meta.description) merged.description = meta.description;
      if (meta.openGraph) merged.openGraph = { ...(merged.openGraph || {}), ...meta.openGraph };
      if (meta.twitter) merged.twitter = { ...(merged.twitter || {}), ...meta.twitter };
      if (meta.alternates) merged.alternates = { ...(merged.alternates || {}), ...meta.alternates };
      if (meta.icons) merged.icons = meta.icons;
      if (meta.robots) merged.robots = meta.robots;
    }

    if (resolvedTitle) {
      document.title = resolvedTitle;
    }
    if (merged.description) {
      setMetaTag("description", merged.description);
    }
    if (merged.alternates && merged.alternates.canonical) {
      setLinkTag("canonical", merged.alternates.canonical);
    }
    if (merged.openGraph) {
      setMetaTag("og:title", merged.openGraph.title || resolvedTitle, true);
      if (merged.openGraph.description || merged.description) {
        setMetaTag("og:description", merged.openGraph.description || merged.description, true);
      }
      if (merged.openGraph.url) {
        setMetaTag("og:url", merged.openGraph.url, true);
      }
      if (merged.openGraph.images) {
        const img = Array.isArray(merged.openGraph.images) ? merged.openGraph.images[0] : merged.openGraph.images;
        const url = typeof img === "string" ? img : img?.url;
        if (url) setMetaTag("og:image", url, true);
      }
    }
    if (merged.twitter) {
      setMetaTag("twitter:card", merged.twitter.card || "summary_large_image");
      setMetaTag("twitter:title", merged.twitter.title || merged.openGraph?.title || resolvedTitle);
      if (merged.twitter.description || merged.description) {
        setMetaTag("twitter:description", merged.twitter.description || merged.description);
      }
    }
  } catch (err) {
    console.warn("[NATA Metadata] Client meta update error:", err);
  }
}

async function renderRoute(entryStr, layoutFiles, params = {}, timestamp = null, segmentFiles = [], globalErrorFile = null) {
  const renderId = ++currentRenderId;
  window.__NATA_PARAMS__ = params || {};

  const searchParamsObj = typeof window !== 'undefined'
    ? Object.fromEntries(new URLSearchParams(currentSearch()))
    : {};

  const safeParams = makeParams(params);
  const safeSearchParams = makeSearchParams(searchParamsObj);
  const routeProps = {
    params: safeParams,
    searchParams: safeSearchParams
  };

  window.dispatchEvent(new CustomEvent("_nata_navigate", { detail: { params: window.__NATA_PARAMS__, searchParams: searchParamsObj, pathname: currentPathname() } }));

  try {
    // Parallel preload all segment modules
    const allModules = [entryStr];
    if (Array.isArray(segmentFiles)) {
      for (const s of segmentFiles) {
        if (s.layout) allModules.push(s.layout);
        if (s.template) allModules.push(s.template);
        if (s.error) allModules.push(s.error);
        if (s.loading) allModules.push(s.loading);
        if (s.not_found) allModules.push(s.not_found);
      }
    } else if (Array.isArray(layoutFiles)) {
      allModules.push(...layoutFiles);
    }
    if (globalErrorFile) allModules.push(globalErrorFile);

    await Promise.all(allModules.map(f => loadModule(f, timestamp).catch(() => null)));

    const pageMod = await loadModule(entryStr, timestamp);
    const Page = pageMod.default || Object.values(pageMod).find(v => typeof v === 'function');

    if (!Page) {
      throw new Error("No default export or React component found in " + entryStr);
    }

    const loadedLayoutMods = [];
    if (Array.isArray(segmentFiles)) {
      for (const s of segmentFiles) {
        if (s.layout) {
          try {
            const lm = await loadModule(s.layout, timestamp);
            if (lm) loadedLayoutMods.push(lm);
          } catch (e) {}
        }
      }
    } else if (Array.isArray(layoutFiles)) {
      for (const lf of layoutFiles) {
        try {
          const lm = await loadModule(lf, timestamp);
          if (lm) loadedLayoutMods.push(lm);
        } catch (e) {}
      }
    }
    updateClientMetadata(pageMod, loadedLayoutMods, routeProps);

    let RootComponent = makeComponent(Page);

    if (Array.isArray(segmentFiles) && segmentFiles.length > 0) {
      for (let i = segmentFiles.length - 1; i >= 0; i--) {
        const seg = segmentFiles[i];

        if (seg.not_found) {
          try {
            const nfMod = await loadModule(seg.not_found, timestamp);
            const NF = nfMod.default || Object.values(nfMod).find(v => typeof v === 'function');
            if (NF) {
              const CurrentChild = RootComponent;
              const WrappedNF = makeComponent(NF);
              RootComponent = (props = {}) => React.createElement(NotFoundBoundary, { fallback: WrappedNF }, React.createElement(CurrentChild, props));
            }
          } catch (e) {}
        }

        if (seg.loading) {
          try {
            const loadMod = await loadModule(seg.loading, timestamp);
            const Loading = loadMod.default || Object.values(loadMod).find(v => typeof v === 'function');
            if (Loading) {
              const CurrentChild = RootComponent;
              const WrappedLoading = makeComponent(Loading);
              RootComponent = (props = {}) => React.createElement(React.Suspense, { fallback: React.createElement(WrappedLoading, props) }, React.createElement(CurrentChild, props));
            }
          } catch (e) {}
        }

        if (seg.error) {
          try {
            const errMod = await loadModule(seg.error, timestamp);
            const ErrComp = errMod.default || Object.values(errMod).find(v => typeof v === 'function');
            if (ErrComp) {
              const CurrentChild = RootComponent;
              const WrappedErr = makeComponent(ErrComp);
              RootComponent = (props = {}) => React.createElement(ErrorBoundary, { fallback: WrappedErr }, React.createElement(CurrentChild, props));
            }
          } catch (e) {}
        }

        if (seg.template) {
          try {
            const tplMod = await loadModule(seg.template, timestamp);
            const Tpl = tplMod.default || Object.values(tplMod).find(v => typeof v === 'function');
            if (Tpl) {
              const CurrentChild = RootComponent;
              const WrappedTpl = makeComponent(Tpl);
              RootComponent = (props = {}) => React.createElement(WrappedTpl, { ...routeProps, ...(props || {}), key: currentPathname(), children: React.createElement(CurrentChild, props) });
            }
          } catch (e) {}
        }

        if (seg.layout) {
          try {
            const layoutMod = await loadModule(seg.layout, timestamp);
            let Layout = layoutMod.default;
            if (!Layout) {
              const entry = Object.entries(layoutMod).find(([k, v]) => typeof v === 'function' && (/^[A-Z]/.test(k) || k.endsWith('Layout')));
              Layout = entry ? entry[1] : undefined;
            }
            if (!Layout) {
              Layout = Object.values(layoutMod).find(v => typeof v === 'function');
            }
            if (Layout) {
              const CurrentChild = RootComponent;
              const WrappedLayout = makeComponent(Layout);
              RootComponent = (props = {}) => React.createElement(WrappedLayout, { ...routeProps, ...(props || {}), children: React.createElement(CurrentChild, props) });
            }
          } catch (layoutErr) {
            console.warn("Could not wrap layout:", seg.layout, layoutErr);
          }
        }
      }
    } else {
      // Fallback legacy layout wrapping
      for (let i = layoutFiles.length - 1; i >= 0; i--) {
        try {
          const layoutMod = await loadModule(layoutFiles[i], timestamp);
          let Layout = layoutMod.default;
          if (!Layout) {
            const entry = Object.entries(layoutMod).find(([k, v]) => typeof v === 'function' && (/^[A-Z]/.test(k) || k.endsWith('Layout')));
            Layout = entry ? entry[1] : undefined;
          }
          if (!Layout) {
            Layout = Object.values(layoutMod).find(v => typeof v === 'function');
          }
          if (Layout) {
            const CurrentChild = RootComponent;
            const WrappedLayout = makeComponent(Layout);
            RootComponent = (props = {}) => React.createElement(WrappedLayout, { ...routeProps, ...(props || {}), children: React.createElement(CurrentChild, props) });
          }
        } catch (layoutErr) {
          console.warn("Could not wrap layout:", layoutFiles[i], layoutErr);
        }
      }
    }

    if (globalErrorFile) {
      try {
        const geMod = await loadModule(globalErrorFile, timestamp);
        const GE = geMod.default || Object.values(geMod).find(v => typeof v === 'function');
        if (GE) {
          const CurrentChild = RootComponent;
          const WrappedGE = makeComponent(GE);
          RootComponent = (props = {}) => React.createElement(ErrorBoundary, { fallback: WrappedGE }, React.createElement(CurrentChild, props));
        }
      } catch (e) {}
    }

    // Render crashes that no error.tsx caught become "Unhandled Runtime Error" instead of a blank page
    const AppRoot = RootComponent;
    RootComponent = (props = {}) => React.createElement(DevRootBoundary, { key: renderId }, React.createElement(AppRoot, props));

    if (renderId === currentRenderId) {
      const rootEl = document.getElementById("root") || document.getElementById("app");
      const isSsr = NATA_CONFIG.ssr !== false && rootEl && rootEl.getAttribute("data-nata-ssr") === "true";

      if (!currentRoot) {
        if (isSsr && typeof ReactDOM.hydrateRoot === "function") {
          try {
            currentRoot = ReactDOM.hydrateRoot(rootEl, React.createElement(RootComponent, routeProps), {
              onRecoverableError(error) {
                reportIssue(error, "issue");
              }
            });
            rootEl.removeAttribute("data-nata-ssr");
          } catch (hydrateErr) {
            console.warn("[NATA SSR] Hydration mismatch/error, falling back to createRoot:", hydrateErr);
            rootEl.innerHTML = "";
            currentRoot = ReactDOM.createRoot(rootEl);
            currentRoot.render(React.createElement(RootComponent, routeProps));
          }
        } else {
          currentRoot = ReactDOM.createRoot(rootEl);
          currentRoot.render(React.createElement(RootComponent, routeProps));
        }
        if (window.requestIdleCallback) window.requestIdleCallback(mountDebugUi);
        else setTimeout(mountDebugUi, 200);
      } else {
        currentRoot.render(React.createElement(RootComponent, routeProps));
      }
    }
  } catch (err) {
    reportIssue(err, "runtime");
  }
}

const routeInfoCache = new Map();

function hardNavigate(targetUrl) {
  // Zalo container must never leave the mini app page, so only web falls back to a full load
  if (!IS_ZALO) window.location.href = targetUrl;
}

async function fetchRouteInfo(targetPath, timestamp = null) {
  const res = await fetch(`${ORIGIN}/_nata/route_info?path=${encodeURIComponent(targetPath)}${timestamp ? "&t=" + timestamp : ""}`);
  const info = await res.json().catch(() => null);
  return { ok: res.ok, info };
}

async function renderRouteInfo(info, timestamp = null) {
  if (info && info.found) {
    await renderRoute(info.page_file, info.layout_files || [], info.params || {}, timestamp, info.segments_files || [], info.global_error_file);
    return true;
  }
  if (info && info.not_found_file) {
    await renderRoute(info.not_found_file, info.layout_files || [], {}, timestamp, [], null);
    return true;
  }
  return false;
}

async function navigateTo(targetUrl, options = {}) {
  try {
    const base = IS_ZALO ? "http://nata.local" + currentPathname() : window.location.href;
    const urlObj = new URL(targetUrl, base);
    const targetPath = urlObj.pathname;
    const fullPath = urlObj.pathname + urlObj.search + urlObj.hash;

    window.__NATA_LOCATION__ = parseLocation(fullPath);
    if (!options.fromPopState) {
      // Zalo keeps its own URL; the virtual path rides in history.state so the Zalo back button works
      const state = { __nata: fullPath };
      const url = IS_ZALO ? window.location.href : fullPath;
      if (options.replace) {
        window.history.replaceState(state, "", url);
      } else {
        window.history.pushState(state, "", url);
      }
    }

    window.dispatchEvent(new CustomEvent("_nata_navigate", { detail: { url: targetUrl, pathname: targetPath } }));

    let info = routeInfoCache.get(targetPath);
    if (!info) {
      const result = await fetchRouteInfo(targetPath);
      info = result.info;
      if (!result.ok && !IS_ZALO) {
        hardNavigate(targetUrl);
        return;
      }
      if (info && info.found) {
        routeInfoCache.set(targetPath, info);
      }
    }

    if (!(await renderRouteInfo(info))) {
      if (IS_ZALO) {
        throw new Error((info && info.error) || ("Không tìm thấy route: " + targetPath));
      }
      hardNavigate(targetUrl);
      return;
    }
    if (!options.fromPopState) window.scrollTo(0, 0);
  } catch (err) {
    console.warn("SPA navigation fallback:", err);
    if (IS_ZALO) {
      reportIssue(err, "runtime");
    } else {
      hardNavigate(targetUrl);
    }
  }
}

window.__NATA_NAVIGATE__ = navigateTo;

window.addEventListener('popstate', (e) => {
  const currentPath = IS_ZALO
    ? ((e.state && e.state.__nata) || "/")
    : window.location.pathname + window.location.search;
  navigateTo(currentPath, { fromPopState: true });
});

// Diagnostic notice if CDN network is slow

let initialData = {};
try {
  const ssrDataEl = document.getElementById('__NATA_SSR_DATA__');
  if (ssrDataEl && ssrDataEl.textContent) {
    initialData = JSON.parse(ssrDataEl.textContent);
  }
} catch (e) {}

const initialRoute = NATA_CONFIG.initial || null;
const initialParams = (initialData && initialData.params) ? initialData.params : ((initialRoute && initialRoute.params) || {});
window.__NATA_PARAMS__ = initialParams;

// Initial mount: the web shell already knows the matched route (same segments as SSR, so hydration
// matches); zalo resolves its virtual path through route_info.
if (initialRoute) {
  renderRoute(initialRoute.page_file, initialRoute.layout_files || [], initialParams, null, initialRoute.segments_files || [], initialRoute.global_error_file || null);
} else {
  navigateTo(currentPathname() + currentSearch(), { replace: true });
}

async function refreshStyles(now) {
  try {
    const styleTag = document.getElementById('_nata_custom_css');
    const zmpLink = document.getElementById('_zmp_styles');
    if (styleTag) {
      const cssRes = await fetch(ORIGIN + '/_nata/styles.css?t=' + now);
      if (cssRes.ok) {
        const css = await cssRes.text();
        if (styleTag.textContent !== css) styleTag.textContent = css;
      }
    } else if (zmpLink) {
      zmpLink.href = zmpLink.href.split('?')[0] + '?t=' + now;
    }
  } catch (cssErr) {
    console.warn('[HMR:CSS] Hot update fallback reload:', cssErr);
    if (!IS_ZALO) location.reload();
  }
}

async function handleHmrMessage(data) {
  const now = data.timestamp || Date.now();

  if (data.type === 'query_log') {
    if (data.log) {
      window.__NATA_LOGS__.push(data.log);
    }
    if (Array.isArray(data.queries)) {
      for (const q of data.queries) {
        if (!window.__NATA_LOGS__.some(l => l.id === q.id)) {
          window.__NATA_LOGS__.push(q);
        }
      }
    }
    renderLogs();
  } else if (data.type === 'css') {
    console.log('[HMR:CSS] Live updating styles:', data.file);
    await refreshStyles(now);
  } else if (data.type === 'full_reload') {
    // Many files changed at once (plugin install, checkout): one fresh page load
    if (!IS_ZALO) location.reload();
  } else if (data.type === 'reload' || data.type === 'RELOAD' || data.type === 'update') {
    console.log('[HMR:RELOAD] Source modified, hot updating:', data.file);
    moduleCache.clear();
    routeInfoCache.clear();
    // Errors from the previous version are stale; the re-render below reports any that remain
    clearBlockingErrors();

    let info = null;
    try {
      info = (await fetchRouteInfo(currentPathname(), now)).info;
    } catch {}

    if (!(await renderRouteInfo(info, now)) && !IS_ZALO) {
      location.reload();
    }
    // Tailwind classes come from scanning source files, so a code change can change the stylesheet
    await refreshStyles(now);
  }
}

// HMR: WebSocket first; tunnels that drop WebSocket upgrades fall back to version polling
function setDevStatus(color) {
  const statusEl = document.getElementById('_dev_status');
  if (statusEl) statusEl.style.background = color;
}

let hmrPolling = false;
function startHmrPolling() {
  if (hmrPolling) return;
  hmrPolling = true;
  let lastVersion = null;
  const tick = async () => {
    try {
      const res = await fetch(ORIGIN + '/_nata/hmr_version', { cache: 'no-store' });
      if (res.ok) {
        const data = await res.json();
        if (lastVersion !== null && data.version !== lastVersion) {
          await handleHmrMessage({ type: data.last_type || 'reload', file: data.last_file, timestamp: Date.now() });
        }
        lastVersion = data.version;
        setDevStatus('#10b981');
      }
    } catch {
      setDevStatus('#ef4444');
    }
    setTimeout(tick, 1500);
  };
  tick();
}

function connectHmr() {
  const httpBase = ORIGIN || location.origin;
  const wsUrl = httpBase.replace(/^http/, 'ws') + '/_hmr';
  let opened = false;
  let ws;
  try {
    ws = new WebSocket(wsUrl);
  } catch {
    startHmrPolling();
    return;
  }
  ws.onopen = () => { opened = true; };
  // One message at a time: concurrent re-renders / reloads raced each other on bursts of changes
  let queue = Promise.resolve();
  ws.onmessage = (e) => {
    queue = queue
      .then(() => handleHmrMessage(JSON.parse(e.data)))
      .catch((err) => console.error('[HMR] Message processing error:', err));
  };
  ws.onclose = () => {
    setDevStatus('#ef4444');
    if (!opened) startHmrPolling();
  };
}
connectHmr();
