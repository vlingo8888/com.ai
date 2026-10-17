// Imported first so module load failures below are still reported in the overlay
import { reportIssue, clearBlockingErrors } from "./overlay.js";
import React from "react";
import ReactDOM from "react-dom/client";

// Bind React and ReactDOM to global scope for SWC classic runtime
window.React = React;
window.ReactDOM = ReactDOM;

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

// Neutral Dark Query & Action Debugger Engine
window.__NATA_LOGS__ = [];
window.__NATA_CURRENT_TAB__ = "all";
window.__NATA_EXPANDED_ITEMS__ = new Set();
window.__NATA_IS_DRAWER_OPEN__ = false;
window.__NATA_IS_MAXIMIZED__ = false;

function escapeHtml(str) {
  if (str === null || str === undefined) return '';
  return String(str)
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#039;');
}

function highlightSql(sql) {
  if (!sql) return '';
  const safe = escapeHtml(sql);
  const sqlRegex = /('(?:[^'\\]|\\.)*'|"(?:[^"\\]|\\.)*")|(\$\d+)|\b(SELECT|FROM|WHERE|AND|OR|NOT|IN|LIKE|ILIKE|BETWEEN|IS|NULL|JOIN|INNER\s+JOIN|LEFT\s+JOIN|RIGHT\s+JOIN|FULL\s+JOIN|CROSS\s+JOIN|ON|GROUP\s+BY|ORDER\s+BY|ASC|DESC|LIMIT|OFFSET|HAVING|UNION|ALL|AS|DISTINCT|CASE|WHEN|THEN|ELSE|END|INSERT\s+INTO|VALUES|UPDATE|SET|DELETE\s+FROM|DELETE|RETURNING|CREATE\s+TABLE|ALTER\s+TABLE|DROP\s+TABLE|BEGIN|COMMIT|ROLLBACK|COUNT|SUM|AVG|MIN|MAX|EXISTS|COALESCE)\b|\b(\d+(?:\.\d+)?)\b/gi;
  return safe.replace(sqlRegex, function (match, str, param, kw, num) {
    if (str) return '<span style="color:#34d399;">' + str + '</span>';
    if (param) return '<span style="color:#c084fc;font-weight:600;">' + param + '</span>';
    if (kw) return '<span style="color:#38bdf8;font-weight:600;">' + kw + '</span>';
    if (num) return '<span style="color:#fbbf24;">' + num + '</span>';
    return match;
  });
}

function syntaxHighlightJson(json) {
  if (json === undefined || json === null) return '<span style="color:#71717a;">null</span>';
  let str = '';
  try {
    str = JSON.stringify(json, null, 2);
  } catch (e) {
    str = String(json);
  }
  const safe = escapeHtml(str);
  return safe.replace(/("(\\u[a-zA-Z0-9]{4}|\\[^u]|[^\\"])*"(\s*:)?|\b(true|false|null)\b|-?\d+(?:\.\d*)?(?:[eE][+\-]?\d+)?)/g, function (match) {
    let cls = 'color:#fbbf24;';
    if (/^"/.test(match)) {
      if (/:$/.test(match)) {
        cls = 'color:#93c5fd;font-weight:500;';
      } else {
        cls = 'color:#34d399;';
      }
    } else if (/true|false/.test(match)) {
      cls = 'color:#c084fc;font-weight:600;';
    } else if (/null/.test(match)) {
      cls = 'color:#71717a;';
    }
    return '<span style="' + cls + '">' + match + '</span>';
  });
}

window.__NATA_COPY__ = function(text, btnId) {
  navigator.clipboard.writeText(text);
  const btn = document.getElementById(btnId);
  if (btn) {
    const originalText = btn.innerHTML;
    btn.innerHTML = '<span style="color:#34d399;">✓ Copied!</span>';
    setTimeout(() => { btn.innerHTML = originalText; }, 1400);
  }
};

window.__NATA_TOGGLE_ITEM__ = function(id) {
  if (window.__NATA_EXPANDED_ITEMS__.has(id)) {
    window.__NATA_EXPANDED_ITEMS__.delete(id);
  } else {
    window.__NATA_EXPANDED_ITEMS__.add(id);
  }
  renderLogs();
};

window.__NATA_TOGGLE_DEBUGGER__ = function() {
  const drawer = document.getElementById('_nata_debug_drawer');
  if (!drawer) return;
  window.__NATA_IS_DRAWER_OPEN__ = !window.__NATA_IS_DRAWER_OPEN__;
  drawer.style.display = window.__NATA_IS_DRAWER_OPEN__ ? 'flex' : 'none';
  if (window.__NATA_IS_DRAWER_OPEN__) {
    renderLogs();
    setTimeout(() => {
      const searchInput = document.getElementById('_dbg_search');
      if (searchInput) searchInput.focus();
    }, 50);
  }
};

window.__NATA_TOGGLE_EXPAND__ = function() {
  const drawer = document.getElementById('_nata_debug_drawer');
  const btn = document.getElementById('_dbg_expand_btn');
  if (!drawer) return;
  window.__NATA_IS_MAXIMIZED__ = !window.__NATA_IS_MAXIMIZED__;
  drawer.style.height = window.__NATA_IS_MAXIMIZED__ ? '85vh' : '460px';
  if (btn) btn.textContent = window.__NATA_IS_MAXIMIZED__ ? '⤓' : '⤢';
};

window.__NATA_SET_TAB__ = function(tabName) {
  window.__NATA_CURRENT_TAB__ = tabName;
  ['all', 'sql', 'rpc', 'error'].forEach(t => {
    const el = document.getElementById('_tab_btn_' + t);
    if (el) {
      if (t === tabName) {
        el.style.background = '#27272a';
        el.style.color = '#fafafa';
      } else {
        el.style.background = 'transparent';
        el.style.color = '#a1a1aa';
      }
    }
  });
  renderLogs();
};

window.__NATA_FILTER_LOGS__ = function() {
  renderLogs();
};

window.__NATA_CLEAR_LOGS__ = async function() {
  window.__NATA_LOGS__ = [];
  window.__NATA_EXPANDED_ITEMS__.clear();
  try {
    await fetch(ORIGIN + '/_nata/logs', { method: 'DELETE' });
  } catch (e) {}
  renderLogs();
};

window.__NATA_EXPORT_LOGS__ = function() {
  const dataStr = 'data:text/json;charset=utf-8,' + encodeURIComponent(JSON.stringify(window.__NATA_LOGS__, null, 2));
  const downloadAnchor = document.createElement('a');
  downloadAnchor.setAttribute('href', dataStr);
  downloadAnchor.setAttribute('download', 'nata-query-logs-' + Date.now() + '.json');
  document.body.appendChild(downloadAnchor);
  downloadAnchor.click();
  downloadAnchor.remove();
};

function updatePillBadges() {
  const total = window.__NATA_LOGS__.length;
  const errors = window.__NATA_LOGS__.filter(l => l.status === 'error').length;
  const pillStats = document.getElementById('_nata_pill_stats');
  const pillErr = document.getElementById('_nata_pill_err');
  if (pillStats) {
    pillStats.textContent = total + ' ' + (total === 1 ? 'log' : 'logs');
  }
  if (pillErr) {
    if (errors > 0) {
      pillErr.style.display = 'inline-block';
      pillErr.textContent = errors + ' err';
    } else {
      pillErr.style.display = 'none';
    }
  }
}

function renderLogs() {
  const container = document.getElementById('_dbg_logs_container');
  if (!container) return;

  const searchInput = document.getElementById('_dbg_search');
  const query = (searchInput ? searchInput.value : '').trim().toLowerCase();
  const tab = window.__NATA_CURRENT_TAB__ || 'all';

  // Update tab counter numbers
  const allCount = window.__NATA_LOGS__.length;
  const sqlCount = window.__NATA_LOGS__.filter(l => l.type === 'sql').length;
  const rpcCount = window.__NATA_LOGS__.filter(l => l.type === 'rpc').length;
  const errCount = window.__NATA_LOGS__.filter(l => l.status === 'error').length;

  const tabCntAll = document.getElementById('_tab_cnt_all');
  const tabCntSql = document.getElementById('_tab_cnt_sql');
  const tabCntRpc = document.getElementById('_tab_cnt_rpc');
  const tabCntErr = document.getElementById('_tab_cnt_err');
  if (tabCntAll) tabCntAll.textContent = '(' + allCount + ')';
  if (tabCntSql) tabCntSql.textContent = '(' + sqlCount + ')';
  if (tabCntRpc) tabCntRpc.textContent = '(' + rpcCount + ')';
  if (tabCntErr) tabCntErr.textContent = '(' + errCount + ')';

  updatePillBadges();

  // Filter by tab
  let filtered = window.__NATA_LOGS__.filter(item => {
    if (tab === 'sql') return item.type === 'sql';
    if (tab === 'rpc') return item.type === 'rpc';
    if (tab === 'error') return item.status === 'error';
    return true;
  });

  // Filter by search query
  if (query) {
    filtered = filtered.filter(item => {
      const str = JSON.stringify(item).toLowerCase();
      return str.includes(query);
    });
  }

  if (filtered.length === 0) {
    container.innerHTML = '<div style="display:flex;flex-direction:column;align-items:center;justify-content:center;height:100%;min-height:220px;color:#71717a;gap:10px;user-select:none;">' +
      '<div style="font-size:28px;opacity:0.6;">⚡</div>' +
      '<div style="font-size:14px;font-weight:500;color:#a1a1aa;">No logs to display</div>' +
      '<div style="font-size:12px;color:#71717a;">Execute queries or server actions in your app to inspect them live.</div>' +
    '</div>';
    return;
  }

  let html = '';
  for (let i = filtered.length - 1; i >= 0; i--) {
    const item = filtered[i];
    const isExpanded = window.__NATA_EXPANDED_ITEMS__.has(item.id);
    const isError = item.status === 'error';
    const isSql = item.type === 'sql';
    const isRpc = item.type === 'rpc';
    const duration = Number(item.duration_ms || 0);

    // Latency color styling
    let latColor = '#34d399';
    let latBg = 'rgba(16,185,129,0.12)';
    let latBorder = 'rgba(16,185,129,0.25)';
    if (duration >= 200 || isError) {
      latColor = '#f43f5e';
      latBg = 'rgba(244,63,94,0.12)';
      latBorder = 'rgba(244,63,94,0.25)';
    } else if (duration >= 50) {
      latColor = '#fbbf24';
      latBg = 'rgba(245,158,11,0.12)';
      latBorder = 'rgba(245,158,11,0.25)';
    }

    // Type Badge
    let typeBadge = '';
    if (isError) {
      typeBadge = '<span style="background:rgba(244,63,94,0.15);color:#fda4af;border:1px solid rgba(244,63,94,0.3);padding:2px 7px;border-radius:6px;font-size:10px;font-weight:700;">ERROR</span>';
    } else if (isSql) {
      typeBadge = '<span style="background:rgba(56,189,248,0.12);color:#38bdf8;border:1px solid rgba(56,189,248,0.25);padding:2px 7px;border-radius:6px;font-size:10px;font-weight:700;">SQL</span>';
    } else {
      typeBadge = '<span style="background:rgba(168,85,247,0.12);color:#c084fc;border:1px solid rgba(168,85,247,0.25);padding:2px 7px;border-radius:6px;font-size:10px;font-weight:700;">RPC</span>';
    }

    // Summary Title
    let summaryTitle = '';
    if (isSql) {
      const cleanSql = (item.sql || '').trim().replace(/\s+/g, ' ');
      summaryTitle = highlightSql(cleanSql.length > 120 ? cleanSql.slice(0, 120) + '...' : cleanSql);
    } else if (isRpc) {
      summaryTitle = '<span style="color:#e4e4e7;font-weight:600;">' + escapeHtml(item.module) + '</span> ' +
        '<span style="color:#71717a;">›</span> ' +
        '<span style="color:#38bdf8;font-weight:600;">' + escapeHtml(item.action) + '()</span>';
    }

    // Meta info (row count or nested queries)
    let metaInfo = '';
    if (isSql) {
      metaInfo = '<span style="color:#a1a1aa;font-size:11px;">' + (item.rowCount !== undefined ? item.rowCount + ' row(s)' : 'query') + '</span>';
    } else if (isRpc) {
      const qLen = Array.isArray(item.queries) ? item.queries.length : 0;
      metaInfo = '<span style="color:#a1a1aa;font-size:11px;">' + (qLen > 0 ? qLen + ' SQL query(s)' : 'action') + '</span>';
    }

    const timeStr = item.timestamp ? new Date(item.timestamp).toLocaleTimeString() : '';

    // Card header
    html += '<div style="background:#121215;border:1px solid ' + (isExpanded ? '#3f3f46' : '#27272a') + ';border-radius:10px;overflow:hidden;flex-shrink:0;transition:border-color 0.15s ease;">';
    html += '<div onclick="window.__NATA_TOGGLE_ITEM__(\'' + item.id + '\')" style="padding:10px 14px;cursor:pointer;display:flex;align-items:center;justify-content:space-between;gap:12px;user-select:none;min-height:42px;box-sizing:border-box;background:' + (isExpanded ? '#18181b' : 'transparent') + ';">';
    html += '<div style="display:flex;align-items:center;gap:10px;flex:1;overflow:hidden;min-width:0;">';
    html += '<span style="font-size:10px;color:#71717a;transform:rotate(' + (isExpanded ? '90deg' : '0deg') + ');transition:transform 0.15s ease;flex-shrink:0;">▶</span>';
    html += typeBadge;
    html += '<div style="font-family:ui-monospace,SFMono-Regular,Menlo,monospace;font-size:12px;color:#f4f4f5;white-space:nowrap;overflow:hidden;text-overflow:ellipsis;flex:1;min-width:0;">' + summaryTitle + '</div>';
    html += '</div>';
    html += '<div style="display:flex;align-items:center;gap:10px;flex-shrink:0;">';
    html += metaInfo;
    html += '<span style="background:' + latBg + ';color:' + latColor + ';border:1px solid ' + latBorder + ';padding:2px 8px;border-radius:6px;font-size:11px;font-family:ui-monospace,monospace;font-weight:600;">' + duration + 'ms</span>';
    html += '<span style="color:#71717a;font-size:11px;font-family:ui-monospace,monospace;">' + timeStr + '</span>';
    html += '</div>';
    html += '</div>';

    // Card expanded body
    if (isExpanded) {
      html += '<div style="padding:14px;border-top:1px solid #27272a;display:flex;flex-direction:column;gap:12px;background:#09090b;">';

      // 1. SQL Query Box
      if (isSql && item.sql) {
        const copyBtnId = '_btn_copy_sql_' + item.id;
        const rawSqlEscaped = escapeHtml(item.sql).replace(/'/g, "\\'");
        html += '<div style="display:flex;flex-direction:column;gap:6px;">';
        html += '<div style="display:flex;align-items:center;justify-content:space-between;">';
        html += '<span style="font-size:11px;font-weight:600;color:#94a3b8;text-transform:uppercase;letter-spacing:0.5px;">SQL Statement</span>';
        html += '<button id="' + copyBtnId + '" onclick="window.__NATA_COPY__(\'' + rawSqlEscaped + '\', \'' + copyBtnId + '\')" style="background:#18181b;border:1px solid #27272a;color:#e4e4e7;padding:3px 8px;border-radius:6px;font-size:11px;cursor:pointer;">Copy SQL</button>';
        html += '</div>';
        html += '<pre style="margin:0;background:#121215;padding:12px;border-radius:8px;border:1px solid #27272a;font-family:ui-monospace,SFMono-Regular,Menlo,monospace;font-size:12px;line-height:1.5;color:#f4f4f5;white-space:pre-wrap;word-break:break-all;overflow-x:auto;">' + highlightSql(item.sql) + '</pre>';
        html += '</div>';
      }

      // 2. Parameters / Arguments Box
      const params = isSql ? item.parameters : item.parameters;
      if (params !== undefined && params !== null && (Array.isArray(params) ? params.length > 0 : Object.keys(params).length > 0)) {
        const copyBtnId = '_btn_copy_param_' + item.id;
        const rawParamsEscaped = escapeHtml(JSON.stringify(params, null, 2)).replace(/'/g, "\\'");
        html += '<div style="display:flex;flex-direction:column;gap:6px;">';
        html += '<div style="display:flex;align-items:center;justify-content:space-between;">';
        html += '<span style="font-size:11px;font-weight:600;color:#94a3b8;text-transform:uppercase;letter-spacing:0.5px;">Parameters / Arguments</span>';
        html += '<button id="' + copyBtnId + '" onclick="window.__NATA_COPY__(\'' + rawParamsEscaped + '\', \'' + copyBtnId + '\')" style="background:#18181b;border:1px solid #27272a;color:#e4e4e7;padding:3px 8px;border-radius:6px;font-size:11px;cursor:pointer;">Copy Params</button>';
        html += '</div>';
        html += '<pre style="margin:0;background:#121215;padding:12px;border-radius:8px;border:1px solid #27272a;font-family:ui-monospace,SFMono-Regular,Menlo,monospace;font-size:12px;line-height:1.5;color:#f4f4f5;white-space:pre-wrap;word-break:break-all;overflow-x:auto;">' + syntaxHighlightJson(params) + '</pre>';
        html += '</div>';
      }

      // 3. Response / Result Payload Box
      if (item.response !== undefined && item.response !== null) {
        const copyBtnId = '_btn_copy_res_' + item.id;
        const rawResEscaped = escapeHtml(JSON.stringify(item.response, null, 2)).replace(/'/g, "\\'");
        const isArr = Array.isArray(item.response);
        const countLabel = isArr ? ' (' + item.response.length + ' rows)' : '';
        html += '<div style="display:flex;flex-direction:column;gap:6px;">';
        html += '<div style="display:flex;align-items:center;justify-content:space-between;">';
        html += '<span style="font-size:11px;font-weight:600;color:#94a3b8;text-transform:uppercase;letter-spacing:0.5px;">Response / Result' + countLabel + '</span>';
        html += '<button id="' + copyBtnId + '" onclick="window.__NATA_COPY__(\'' + rawResEscaped + '\', \'' + copyBtnId + '\')" style="background:#18181b;border:1px solid #27272a;color:#e4e4e7;padding:3px 8px;border-radius:6px;font-size:11px;cursor:pointer;">Copy Response</button>';
        html += '</div>';
        html += '<pre style="margin:0;background:#121215;padding:12px;border-radius:8px;border:1px solid #27272a;font-family:ui-monospace,SFMono-Regular,Menlo,monospace;font-size:12px;line-height:1.5;color:#f4f4f5;white-space:pre-wrap;word-break:break-all;max-height:260px;overflow-y:auto;">' + syntaxHighlightJson(item.response) + '</pre>';
        html += '</div>';
      }

      // 4. Nested SQL Queries (for RPC actions)
      if (isRpc && Array.isArray(item.queries) && item.queries.length > 0) {
        html += '<div style="display:flex;flex-direction:column;gap:6px;">';
        html += '<span style="font-size:11px;font-weight:600;color:#38bdf8;text-transform:uppercase;letter-spacing:0.5px;">Executed SQL Queries (' + item.queries.length + ')</span>';
        html += '<div style="display:flex;flex-direction:column;gap:6px;">';
        for (const q of item.queries) {
          html += '<div style="background:#18181b;padding:8px 12px;border-radius:6px;border:1px solid #27272a;font-family:ui-monospace,SFMono-Regular,Menlo,monospace;font-size:11px;">';
          html += '<div style="display:flex;justify-content:space-between;margin-bottom:4px;">';
          html += '<span style="color:#38bdf8;font-weight:600;">SQL</span>';
          html += '<span style="color:#34d399;font-weight:600;">' + (q.duration_ms || 0) + 'ms</span>';
          html += '</div>';
          html += '<div style="color:#f4f4f5;white-space:pre-wrap;word-break:break-all;">' + highlightSql(q.sql || '') + '</div>';
          if (q.parameters && q.parameters.length > 0) {
            html += '<div style="color:#a1a1aa;margin-top:4px;font-size:10px;">Params: ' + escapeHtml(JSON.stringify(q.parameters)) + '</div>';
          }
          html += '</div>';
        }
        html += '</div>';
        html += '</div>';
      }

      // 5. Error Info Box (if status === 'error')
      if (isError && item.error) {
        html += '<div style="background:rgba(244,63,94,0.1);border:1px solid rgba(244,63,94,0.3);border-radius:8px;padding:12px;color:#fda4af;font-size:12px;">';
        html += '<div style="font-weight:700;color:#f43f5e;margin-bottom:4px;">Error Details</div>';
        html += '<pre style="margin:0;font-family:ui-monospace,monospace;white-space:pre-wrap;word-break:break-all;color:#fca5a5;">' + escapeHtml(item.error) + '</pre>';
        html += '</div>';
      }

      html += '</div>';
    }

    html += '</div>';
  }

  container.innerHTML = html;
}

// Global keyboard shortcuts for DevTools
window.addEventListener('keydown', (e) => {
  // Ctrl+Shift+D or Cmd+Shift+D to toggle debugger
  if ((e.ctrlKey || e.metaKey) && e.shiftKey && (e.key === 'D' || e.key === 'd')) {
    e.preventDefault();
    window.__NATA_TOGGLE_DEBUGGER__();
  }
  // Escape to close debugger
  if (e.key === 'Escape' && window.__NATA_IS_DRAWER_OPEN__) {
    window.__NATA_TOGGLE_DEBUGGER__();
  }
  // / or Ctrl+K / Cmd+K to focus search input when open
  if ((e.key === '/' || ((e.ctrlKey || e.metaKey) && (e.key === 'k' || e.key === 'K'))) && window.__NATA_IS_DRAWER_OPEN__) {
    const searchInput = document.getElementById('_dbg_search');
    if (searchInput && document.activeElement !== searchInput) {
      e.preventDefault();
      searchInput.focus();
      searchInput.select();
    }
  }
});

// Fetch initial logs on mount
fetch(ORIGIN + '/_nata/logs').then(r => r.json()).then(data => {
  if (data && Array.isArray(data.logs)) {
    window.__NATA_LOGS__ = data.logs;
    renderLogs();
  }
}).catch(() => {});

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
  } else if (data.type === 'reload' || data.type === 'RELOAD') {
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
  ws.onmessage = async (e) => {
    try {
      await handleHmrMessage(JSON.parse(e.data));
    } catch (err) {
      console.error('[HMR] Message processing error:', err);
    }
  };
  ws.onclose = () => {
    setDevStatus('#ef4444');
    if (!opened) startHmrPolling();
  };
}
connectHmr();
