/**
 * Pure Native React App Router Server-Side Rendering (SSR) Evaluator
 * Executes cascading layouts and async pages on the server and emits rendered HTML markup.
 */

const SSR_DELIM_START = "__NATA_SSR_OUT_START__";
const SSR_DELIM_END = "__NATA_SSR_OUT_END__";

interface SegmentFilePaths {
  folder: string;
  layout?: string | null;
  template?: string | null;
  error?: string | null;
  loading?: string | null;
  not_found?: string | null;
}

interface SsrInput {
  pagePath: string;
  layoutPaths: string[];
  segmentsFiles?: SegmentFilePaths[];
  globalErrorPath?: string | null;
  params: Record<string, any>;
  searchParams: Record<string, any>;
  cookiesStr: string;
  headers: Record<string, string>;
  urlPath: string;
}

// 1. Browser & DOM Environment Mocking for Server Context
if (typeof globalThis.window === "undefined") {
  (globalThis as any).window = {
    location: {
      pathname: "/",
      search: "",
      hash: "",
      origin: "http://localhost:3000",
      href: "http://localhost:3000/",
    },
    addEventListener: () => {},
    removeEventListener: () => {},
    dispatchEvent: () => true,
    matchMedia: () => ({
      matches: false,
      addListener: () => {},
      removeListener: () => {},
      addEventListener: () => {},
      removeEventListener: () => {},
      dispatchEvent: () => true,
    }),
    requestAnimationFrame: (cb: any) => setTimeout(cb, 0),
    cancelAnimationFrame: (id: any) => clearTimeout(id),
  };
}

if (typeof globalThis.document === "undefined") {
  (globalThis as any).document = {
    createElement: () => ({ setAttribute: () => {}, appendChild: () => {}, style: {} }),
    head: { appendChild: () => {} },
    body: { appendChild: () => {} },
    cookie: "",
    addEventListener: () => {},
    removeEventListener: () => {},
  };
}

if (typeof globalThis.navigator === "undefined") {
  (globalThis as any).navigator = {
    userAgent: "Mozilla/5.0 (Server-Side-Rendering; NATA Engine)",
    clipboard: { writeText: async () => {} },
  };
}

if (typeof globalThis.localStorage === "undefined") {
  const memoryStore = new Map<string, string>();
  (globalThis as any).localStorage = {
    getItem: (k: string) => memoryStore.get(k) ?? null,
    setItem: (k: string, v: string) => memoryStore.set(k, String(v)),
    removeItem: (k: string) => memoryStore.delete(k),
    clear: () => memoryStore.clear(),
  };
}

if (typeof globalThis.sessionStorage === "undefined") {
  (globalThis as any).sessionStorage = (globalThis as any).localStorage;
}

export async function runSsr(input: SsrInput) {
  const startTime = performance.now();
  let redirectUrl: string | null = null;
  let notFoundEncountered = false;

  try {
    // 2. Setup Server Context (Cookies, Headers, Location)
    const cookieMap = new Map<string, string>();
    if (input.cookiesStr && typeof input.cookiesStr === "string") {
      for (const pair of input.cookiesStr.split(";")) {
        const idx = pair.indexOf("=");
        if (idx !== -1) {
          const k = decodeURIComponent(pair.slice(0, idx).trim());
          const v = decodeURIComponent(pair.slice(idx + 1).trim());
          if (k) cookieMap.set(k, v);
        }
      }
    }
    (globalThis as any).__NATA_COOKIES__ = cookieMap;
    (globalThis as any).__NATA_SET_COOKIES__ = [];

    (globalThis as any).window.location.pathname = input.urlPath;
    (globalThis as any).window.location.href = `http://localhost:3000${input.urlPath}`;

    // 3. Load React & ReactDOMServer
    let React: any;
    let ReactDOMServer: any;

    try {
      React = await import("react");
      ReactDOMServer = await import("react-dom/server");
    } catch {
      // Fallback for dynamic esm loading
      React = (globalThis as any).React || (await import("https://esm.sh/react@18.3.1"));
      ReactDOMServer = (globalThis as any).ReactDOMServer || (await import("https://esm.sh/react-dom@18.3.1/server"));
    }

    (globalThis as any).React = React;

    // 4. Load Page Component
    const pageMod = await import(input.pagePath);
    let Page = pageMod.default;
    if (!Page) {
      const entry = Object.entries(pageMod).find(([k, v]) => typeof v === "function" && (/^[A-Z]/.test(k) || k.endsWith("Page")));
      Page = entry ? entry[1] : Object.values(pageMod).find((v) => typeof v === "function");
    }

    if (!Page) {
      throw new Error(`No React component or default export found in page: ${input.pagePath}`);
    }

    // Extract metadata
    let title: string | undefined = undefined;
    if (pageMod.metadata && typeof pageMod.metadata === "object") {
      title = pageMod.metadata.title;
    }

    // Helper to unwrap <html>, <head>, <body> tags for RootLayout rendering inside #root
    const unwrapHtmlBody = (node: any): any => {
      if (!node) return node;
      if (Array.isArray(node)) {
        const unwrapped = node.map(unwrapHtmlBody).filter(Boolean);
        if (unwrapped.length === 0) return null;
        if (unwrapped.length === 1) return unwrapped[0];
        return React.createElement(React.Fragment, null, ...unwrapped);
      }
      if (React.isValidElement(node)) {
        const type = (node as any).type;
        const props: any = (node as any).props || {};
        if (type === "html" || (typeof type === "string" && type.toLowerCase() === "html")) {
          return unwrapHtmlBody(props.children);
        }
        if (type === "body" || (typeof type === "string" && type.toLowerCase() === "body")) {
          return unwrapHtmlBody(props.children);
        }
        if (type === "head" || (typeof type === "string" && type.toLowerCase() === "head")) {
          return null;
        }
        return node;
      }
      return node;
    };

    // Helper to evaluate and make a React element out of async/sync components
    const makeElement = (Comp: any, props: any, children?: any) => {
      try {
        if (typeof Comp !== "function") return null;
        const WrappedComp = (p: any) => {
          try {
            const res = Comp(p);
            if (res && typeof res.then === "function") {
              return res;
            }
            return unwrapHtmlBody(res);
          } catch (err: any) {
            if (err?.digest?.startsWith("NEXT_REDIRECT") || err?.name === "RedirectError") {
              redirectUrl = err.url || err.message;
              return null;
            }
            if (err?.digest === "NEXT_NOT_FOUND" || err?.message === "NEXT_NOT_FOUND" || err?.message?.includes("404 Not Found")) {
              notFoundEncountered = true;
              return null;
            }
            throw err;
          }
        };
        return React.createElement(WrappedComp, { ...props, ...(children ? { children } : {}) });
      } catch (err: any) {
        if (err?.digest?.startsWith("NEXT_REDIRECT") || err?.name === "RedirectError") {
          redirectUrl = err.url || err.message;
          return null;
        }
        if (err?.digest === "NEXT_NOT_FOUND" || err?.message === "NEXT_NOT_FOUND" || err?.message?.includes("404 Not Found")) {
          notFoundEncountered = true;
          return null;
        }
        throw err;
      }
    };

    const loadComp = async (filePath: string | null | undefined) => {
      if (!filePath) return null;
      try {
        const mod = await import(filePath);
        let comp = mod.default;
        if (!comp) {
          const entry = Object.entries(mod).find(([k, v]) => typeof v === "function" && /^[A-Z]/.test(k));
          comp = entry ? entry[1] : Object.values(mod).find((v) => typeof v === "function");
        }
        return comp;
      } catch (err) {
        console.warn(`[NATA SSR] Failed to load module ${filePath}:`, err);
        return null;
      }
    };

    // 5. Build Component Tree from Page outwards through Segments
    let currentTree = makeElement(Page, {
      params: input.params,
      searchParams: input.searchParams,
    });

    if (input.segmentsFiles && input.segmentsFiles.length > 0) {
      // Wrap from leaf segment outwards to root segment
      for (let i = input.segmentsFiles.length - 1; i >= 0; i--) {
        const seg = input.segmentsFiles[i];

        // 5a. Wrap with loading if present (React.Suspense)
        if (seg.loading) {
          try {
            const Loading = await loadComp(seg.loading);
            if (Loading) {
              const loadingEl = React.createElement(Loading, {});
              currentTree = React.createElement(React.Suspense, { fallback: loadingEl }, currentTree);
            }
          } catch (loadingErr) {
            console.warn(`[NATA SSR] Loading fallback error in ${seg.loading}:`, loadingErr);
          }
        }

        // 5b. Wrap with template if present
        if (seg.template) {
          try {
            const Template = await loadComp(seg.template);
            if (Template) {
              currentTree = makeElement(Template, { params: input.params, searchParams: input.searchParams }, currentTree);
            }
          } catch (templateErr) {
            console.warn(`[NATA SSR] Template error in ${seg.template}:`, templateErr);
          }
        }

        // 5c. Wrap with layout if present
        if (seg.layout) {
          try {
            const layoutMod = await import(seg.layout);
            let Layout = layoutMod.default;
            if (!Layout) {
              const entry = Object.entries(layoutMod).find(([k, v]) => typeof v === "function" && (/^[A-Z]/.test(k) || k.endsWith("Layout")));
              Layout = entry ? entry[1] : Object.values(layoutMod).find((v) => typeof v === "function");
            }

            if (layoutMod.metadata && typeof layoutMod.metadata === "object" && !title) {
              title = layoutMod.metadata.title;
            }

            if (Layout) {
              currentTree = makeElement(
                Layout,
                { params: input.params, searchParams: input.searchParams },
                currentTree
              );
            }
          } catch (layoutErr: any) {
            if (layoutErr?.digest?.startsWith("NEXT_REDIRECT") || layoutErr?.name === "RedirectError") {
              redirectUrl = layoutErr.url || layoutErr.message;
              break;
            }
            if (layoutErr?.digest === "NEXT_NOT_FOUND" || layoutErr?.message === "NEXT_NOT_FOUND") {
              notFoundEncountered = true;
              break;
            }
            console.warn(`[NATA SSR] Layout evaluation warning in ${seg.layout}:`, layoutErr?.message || layoutErr);
          }
        }
      }
    } else {
      // Legacy fallback: wrap with layouts in reverse order
      for (let i = input.layoutPaths.length - 1; i >= 0; i--) {
        const layoutPath = input.layoutPaths[i];
        try {
          const layoutMod = await import(layoutPath);
          let Layout = layoutMod.default;
          if (!Layout) {
            const entry = Object.entries(layoutMod).find(([k, v]) => typeof v === "function" && (/^[A-Z]/.test(k) || k.endsWith("Layout")));
            Layout = entry ? entry[1] : Object.values(layoutMod).find((v) => typeof v === "function");
          }

          if (layoutMod.metadata && typeof layoutMod.metadata === "object" && !title) {
            title = layoutMod.metadata.title;
          }

          if (Layout) {
            currentTree = makeElement(
              Layout,
              { params: input.params, searchParams: input.searchParams },
              currentTree
            );
          }
        } catch (layoutErr: any) {
          if (layoutErr?.digest?.startsWith("NEXT_REDIRECT") || layoutErr?.name === "RedirectError") {
            redirectUrl = layoutErr.url || layoutErr.message;
            break;
          }
          if (layoutErr?.digest === "NEXT_NOT_FOUND" || layoutErr?.message === "NEXT_NOT_FOUND") {
            notFoundEncountered = true;
            break;
          }
          console.warn(`[NATA SSR] Layout evaluation warning in ${layoutPath}:`, layoutErr?.message || layoutErr);
        }
      }
    }

    if (redirectUrl) {
      const output = {
        html: "",
        initial_state: { params: input.params, searchParams: input.searchParams },
        title: title || null,
        set_cookies: (globalThis as any).__NATA_SET_COOKIES__ || [],
        status_code: 307,
        redirect_url: redirectUrl,
        render_time_ms: performance.now() - startTime,
        mode: "Full",
        error: null,
      };
      console.log(SSR_DELIM_START + JSON.stringify(output) + SSR_DELIM_END);
      return;
    }

    // Helper to render Not Found UI
    const renderNotFoundTree = async () => {
      let NotFoundComp = null;
      if (input.segmentsFiles) {
        for (let i = input.segmentsFiles.length - 1; i >= 0; i--) {
          if (input.segmentsFiles[i].not_found) {
            NotFoundComp = await loadComp(input.segmentsFiles[i].not_found);
            if (NotFoundComp) break;
          }
        }
      }
      let nfTree: any = null;
      if (NotFoundComp) {
        nfTree = makeElement(NotFoundComp, { params: input.params, searchParams: input.searchParams });
        const rootLayout = input.segmentsFiles?.[0]?.layout || input.layoutPaths?.[0];
        if (rootLayout) {
          const RootLayout = await loadComp(rootLayout);
          if (RootLayout) {
            nfTree = makeElement(RootLayout, { params: input.params, searchParams: input.searchParams }, nfTree);
          }
        }
      } else {
        nfTree = React.createElement("div", {
          style: {
            fontFamily: "system-ui, -apple-system, sans-serif",
            height: "100vh",
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            flexDirection: "column",
            background: "#09090b",
            color: "#f4f4f5"
          }
        }, React.createElement("h1", { style: { fontSize: "2rem", fontWeight: 700 } }, "404 - Not Found"));
      }
      return nfTree;
    };

    if (notFoundEncountered) {
      currentTree = await renderNotFoundTree();
    }

    // 6. Render to HTML String
    let renderedHtml = "";
    try {
      if (currentTree) {
        if (typeof ReactDOMServer.renderToString === "function") {
          renderedHtml = ReactDOMServer.renderToString(currentTree);
        } else if (typeof ReactDOMServer.renderToStaticMarkup === "function") {
          renderedHtml = ReactDOMServer.renderToStaticMarkup(currentTree);
        }
      }
    } catch (renderErr: any) {
      if (renderErr?.digest === "NEXT_NOT_FOUND" || renderErr?.message === "NEXT_NOT_FOUND" || renderErr?.message?.includes("404 Not Found")) {
        notFoundEncountered = true;
        const nfTree = await renderNotFoundTree();
        renderedHtml = typeof ReactDOMServer.renderToString === "function"
          ? ReactDOMServer.renderToString(nfTree)
          : ReactDOMServer.renderToStaticMarkup(nfTree);
      } else if (input.globalErrorPath) {
        try {
          const GlobalError = await loadComp(input.globalErrorPath);
          if (GlobalError) {
            const geTree = React.createElement(GlobalError, { error: renderErr, reset: () => {} });
            renderedHtml = typeof ReactDOMServer.renderToString === "function"
              ? ReactDOMServer.renderToString(geTree)
              : ReactDOMServer.renderToStaticMarkup(geTree);
          } else {
            throw renderErr;
          }
        } catch (geErr) {
          throw renderErr;
        }
      } else {
        throw renderErr;
      }
    }

    if (redirectUrl) {
      const output = {
        html: "",
        initial_state: { params: input.params, searchParams: input.searchParams },
        title: title || null,
        set_cookies: (globalThis as any).__NATA_SET_COOKIES__ || [],
        status_code: 307,
        redirect_url: redirectUrl,
        render_time_ms: performance.now() - startTime,
        mode: "Full",
        error: null,
      };
      console.log(SSR_DELIM_START + JSON.stringify(output) + SSR_DELIM_END);
      return;
    }

    const output = {
      html: renderedHtml,
      initial_state: { params: input.params, searchParams: input.searchParams },
      title: title || null,
      set_cookies: (globalThis as any).__NATA_SET_COOKIES__ || [],
      status_code: notFoundEncountered ? 404 : 200,
      redirect_url: null,
      render_time_ms: performance.now() - startTime,
      mode: "Full",
      error: null,
    };

    console.log(SSR_DELIM_START + JSON.stringify(output) + SSR_DELIM_END);
  } catch (err: any) {
    const output = {
      html: "",
      initial_state: { params: input.params, searchParams: input.searchParams },
      title: null,
      set_cookies: (globalThis as any).__NATA_SET_COOKIES__ || [],
      status_code: 500,
      redirect_url: redirectUrl,
      render_time_ms: performance.now() - startTime,
      mode: "ClientOnly",
      error: err?.message || String(err),
    };
    console.log(SSR_DELIM_START + JSON.stringify(output) + SSR_DELIM_END);
  }
}
