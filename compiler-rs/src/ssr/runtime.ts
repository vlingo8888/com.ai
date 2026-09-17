/**
 * Pure Native React App Router Server-Side Rendering (SSR) Evaluator
 * Executes cascading layouts and async pages on the server and emits rendered HTML markup.
 */

const SSR_DELIM_START = "__NATA_SSR_OUT_START__";
const SSR_DELIM_END = "__NATA_SSR_OUT_END__";

interface SsrInput {
  pagePath: string;
  layoutPaths: string[];
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
            throw err;
          }
        };
        return React.createElement(WrappedComp, { ...props, ...(children ? { children } : {}) });
      } catch (err: any) {
        if (err?.digest?.startsWith("NEXT_REDIRECT") || err?.name === "RedirectError") {
          redirectUrl = err.url || err.message;
          return null;
        }
        throw err;
      }
    };

    // 5. Build Component Tree from Page outwards through Layouts
    let currentTree = makeElement(Page, {
      params: input.params,
      searchParams: input.searchParams,
    });

    // Wrap with layouts in reverse order (leaf layout -> root layout)
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
        console.warn(`[NATA SSR] Layout evaluation warning in ${layoutPath}:`, layoutErr?.message || layoutErr);
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

    // 6. Render to HTML String
    let renderedHtml = "";
    if (currentTree) {
      if (typeof ReactDOMServer.renderToString === "function") {
        renderedHtml = ReactDOMServer.renderToString(currentTree);
      } else if (typeof ReactDOMServer.renderToStaticMarkup === "function") {
        renderedHtml = ReactDOMServer.renderToStaticMarkup(currentTree);
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
