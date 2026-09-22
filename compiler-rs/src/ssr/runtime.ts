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
    let React: any = (globalThis as any).React;
    let ReactDOMServer: any = (globalThis as any).ReactDOMServer;

    if (!React || !ReactDOMServer) {
      try {
        const r = await import("react");
        React = r.default || r;
        const rd = await import("react-dom/server");
        ReactDOMServer = rd.default || rd;
      } catch {
        try {
          const timeoutPromise = new Promise((_, reject) => setTimeout(() => reject(new Error("Network timeout")), 1500));
          const netPromise = Promise.all([
            import("https://esm.sh/react@18.3.1"),
            import("https://esm.sh/react-dom@18.3.1/server"),
          ]);
          const [r, rd]: any = await Promise.race([netPromise, timeoutPromise]);
          React = r.default || r;
          ReactDOMServer = rd.default || rd;
        } catch {
          // Fallback mock React if offline
        }
      }
    }

    if (!React || typeof React.createElement !== "function") {
      if (React && React.default && typeof React.default.createElement === "function") {
        React = React.default;
      } else {
        React = {
          createElement: (type: any, props: any, ...children: any[]) => ({ type, props: { ...props, children } }),
          Fragment: "Fragment",
          isValidElement: (node: any) => node && typeof node === "object" && "type" in node,
          Suspense: ({ children }: any) => children,
        };
      }
    }

    if (!ReactDOMServer || typeof ReactDOMServer.renderToString !== "function") {
      if (ReactDOMServer && ReactDOMServer.default && typeof ReactDOMServer.default.renderToString === "function") {
        ReactDOMServer = ReactDOMServer.default;
      } else {
        ReactDOMServer = {
          renderToString: () => "<div>App</div>",
          renderToStaticMarkup: () => "<div>App</div>",
        };
      }
    }

    (globalThis as any).React = React;
    (globalThis as any).ReactDOMServer = ReactDOMServer;

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

    // Helpers for Metadata and Head HTML Resolution
    const escapeHtml = (str: any): string => {
      if (str === null || str === undefined) return "";
      return String(str)
        .replace(/&/g, "&amp;")
        .replace(/</g, "&lt;")
        .replace(/>/g, "&gt;")
        .replace(/"/g, "&quot;")
        .replace(/'/g, "&#039;");
    };

    const generateHeadHtml = (resolvedTitle: string | null, meta: Record<string, any>): string => {
      const tags: string[] = [];
      if (resolvedTitle) {
        tags.push(`<title>${escapeHtml(resolvedTitle)}</title>`);
      }
      if (meta.description) {
        tags.push(`<meta name="description" content="${escapeHtml(meta.description)}">`);
      }
      if (meta.applicationName) {
        tags.push(`<meta name="application-name" content="${escapeHtml(meta.applicationName)}">`);
      }
      if (meta.generator) {
        tags.push(`<meta name="generator" content="${escapeHtml(meta.generator)}">`);
      }
      if (meta.keywords) {
        const kw = Array.isArray(meta.keywords) ? meta.keywords.join(", ") : String(meta.keywords);
        tags.push(`<meta name="keywords" content="${escapeHtml(kw)}">`);
      }
      if (meta.referrer) {
        tags.push(`<meta name="referrer" content="${escapeHtml(meta.referrer)}">`);
      }
      if (meta.creator) {
        tags.push(`<meta name="creator" content="${escapeHtml(meta.creator)}">`);
      }
      if (meta.publisher) {
        tags.push(`<meta name="publisher" content="${escapeHtml(meta.publisher)}">`);
      }
      if (meta.themeColor) {
        if (typeof meta.themeColor === "string") {
          tags.push(`<meta name="theme-color" content="${escapeHtml(meta.themeColor)}">`);
        } else if (Array.isArray(meta.themeColor)) {
          for (const tc of meta.themeColor) {
            if (tc && tc.color) {
              const media = tc.media ? ` media="${escapeHtml(tc.media)}"` : "";
              tags.push(`<meta name="theme-color"${media} content="${escapeHtml(tc.color)}">`);
            }
          }
        }
      }
      if (meta.robots) {
        let robotsStr = "";
        if (typeof meta.robots === "string") {
          robotsStr = meta.robots;
        } else if (typeof meta.robots === "object") {
          const parts: string[] = [];
          parts.push(meta.robots.index === false ? "noindex" : "index");
          parts.push(meta.robots.follow === false ? "nofollow" : "follow");
          if (meta.robots.nocache) parts.push("nocache");
          if (meta.robots.noarchive) parts.push("noarchive");
          if (meta.robots.noimageindex) parts.push("noimageindex");
          robotsStr = parts.join(", ");
        }
        if (robotsStr) {
          tags.push(`<meta name="robots" content="${escapeHtml(robotsStr)}">`);
        }
      }
      if (meta.alternates && typeof meta.alternates === "object") {
        if (meta.alternates.canonical) {
          tags.push(`<link rel="canonical" href="${escapeHtml(meta.alternates.canonical)}">`);
        }
        if (meta.alternates.languages && typeof meta.alternates.languages === "object") {
          for (const [lang, url] of Object.entries(meta.alternates.languages)) {
            tags.push(`<link rel="alternate" hreflang="${escapeHtml(lang)}" href="${escapeHtml(url)}">`);
          }
        }
      }
      if (meta.manifest) {
        tags.push(`<link rel="manifest" href="${escapeHtml(meta.manifest)}">`);
      }
      if (meta.icons) {
        if (typeof meta.icons === "string") {
          tags.push(`<link rel="icon" href="${escapeHtml(meta.icons)}">`);
        } else if (typeof meta.icons === "object") {
          const renderIconGroup = (rel: string, val: any) => {
            if (!val) return;
            if (typeof val === "string") {
              tags.push(`<link rel="${rel}" href="${escapeHtml(val)}">`);
            } else if (Array.isArray(val)) {
              for (const item of val) {
                if (typeof item === "string") {
                  tags.push(`<link rel="${rel}" href="${escapeHtml(item)}">`);
                } else if (item && item.url) {
                  const sizes = item.sizes ? ` sizes="${escapeHtml(item.sizes)}"` : "";
                  const type = item.type ? ` type="${escapeHtml(item.type)}"` : "";
                  tags.push(`<link rel="${rel}" href="${escapeHtml(item.url)}"${sizes}${type}>`);
                }
              }
            } else if (val.url) {
              const sizes = val.sizes ? ` sizes="${escapeHtml(val.sizes)}"` : "";
              const type = val.type ? ` type="${escapeHtml(val.type)}"` : "";
              tags.push(`<link rel="${rel}" href="${escapeHtml(val.url)}"${sizes}${type}>`);
            }
          };
          renderIconGroup("icon", meta.icons.icon);
          renderIconGroup("apple-touch-icon", meta.icons.apple);
          renderIconGroup("shortcut icon", meta.icons.shortcut);
        }
      }
      if (meta.openGraph && typeof meta.openGraph === "object") {
        const og = meta.openGraph;
        const ogTitle = og.title || resolvedTitle;
        if (ogTitle) tags.push(`<meta property="og:title" content="${escapeHtml(ogTitle)}">`);
        const ogDesc = og.description || meta.description;
        if (ogDesc) tags.push(`<meta property="og:description" content="${escapeHtml(ogDesc)}">`);
        if (og.url) tags.push(`<meta property="og:url" content="${escapeHtml(og.url)}">`);
        if (og.siteName) tags.push(`<meta property="og:site_name" content="${escapeHtml(og.siteName)}">`);
        tags.push(`<meta property="og:type" content="${escapeHtml(og.type || "website")}">`);
        if (og.locale) tags.push(`<meta property="og:locale" content="${escapeHtml(og.locale)}">`);

        if (og.images) {
          const renderOgImg = (img: any) => {
            if (typeof img === "string") {
              tags.push(`<meta property="og:image" content="${escapeHtml(img)}">`);
            } else if (img && typeof img === "object") {
              if (img.url) tags.push(`<meta property="og:image" content="${escapeHtml(img.url)}">`);
              if (img.width) tags.push(`<meta property="og:image:width" content="${escapeHtml(img.width)}">`);
              if (img.height) tags.push(`<meta property="og:image:height" content="${escapeHtml(img.height)}">`);
              if (img.alt) tags.push(`<meta property="og:image:alt" content="${escapeHtml(img.alt)}">`);
            }
          };
          if (Array.isArray(og.images)) {
            og.images.forEach(renderOgImg);
          } else {
            renderOgImg(og.images);
          }
        }
      }
      if (meta.twitter && typeof meta.twitter === "object") {
        const tw = meta.twitter;
        tags.push(`<meta name="twitter:card" content="${escapeHtml(tw.card || "summary_large_image")}">`);
        const twTitle = tw.title || meta.openGraph?.title || resolvedTitle;
        if (twTitle) tags.push(`<meta name="twitter:title" content="${escapeHtml(twTitle)}">`);
        const twDesc = tw.description || meta.openGraph?.description || meta.description;
        if (twDesc) tags.push(`<meta name="twitter:description" content="${escapeHtml(twDesc)}">`);
        if (tw.site) tags.push(`<meta name="twitter:site" content="${escapeHtml(tw.site)}">`);
        if (tw.creator) tags.push(`<meta name="twitter:creator" content="${escapeHtml(tw.creator)}">`);
        if (tw.images) {
          const imgs = Array.isArray(tw.images) ? tw.images : [tw.images];
          for (const img of imgs) {
            const url = typeof img === "string" ? img : img?.url;
            if (url) tags.push(`<meta name="twitter:image" content="${escapeHtml(url)}">`);
          }
        }
      }
      if (meta.other && typeof meta.other === "object") {
        for (const [k, v] of Object.entries(meta.other)) {
          if (v !== undefined && v !== null) {
            if (Array.isArray(v)) {
              for (const item of v) {
                tags.push(`<meta name="${escapeHtml(k)}" content="${escapeHtml(item)}">`);
              }
            } else {
              tags.push(`<meta name="${escapeHtml(k)}" content="${escapeHtml(v)}">`);
            }
          }
        }
      }
      return tags.join("\n  ");
    };

    const resolveCascadeMetadata = async (modules: any[], routeContext: { params: any; searchParams: any }) => {
      let mergedMeta: Record<string, any> = {};
      let titleTemplate: string | null = null;
      let resolvedTitle: string | null = null;

      for (const mod of modules) {
        if (!mod) continue;
        let meta: any = null;
        if (typeof mod.generateMetadata === "function") {
          try {
            meta = await mod.generateMetadata(routeContext, Promise.resolve(mergedMeta));
          } catch (err) {
            console.warn("[NATA Metadata] Error in generateMetadata:", err);
          }
        } else if (mod.metadata) {
          meta = typeof mod.metadata === "function" ? await mod.metadata() : mod.metadata;
        }

        if (!meta || typeof meta !== "object") continue;

        if (meta.title !== undefined) {
          if (typeof meta.title === "string") {
            if (titleTemplate) {
              resolvedTitle = titleTemplate.includes("%s") ? titleTemplate.replace("%s", meta.title) : meta.title;
            } else {
              resolvedTitle = meta.title;
            }
          } else if (typeof meta.title === "object" && meta.title !== null) {
            if (meta.title.absolute) {
              resolvedTitle = meta.title.absolute;
            } else if (meta.title.default) {
              if (titleTemplate) {
                resolvedTitle = titleTemplate.includes("%s") ? titleTemplate.replace("%s", meta.title.default) : meta.title.default;
              } else {
                resolvedTitle = meta.title.default;
              }
            }
            if (meta.title.template) {
              titleTemplate = meta.title.template;
            }
          }
        }

        for (const key of ["description", "applicationName", "generator", "referrer", "creator", "publisher", "manifest", "themeColor"]) {
          if (meta[key] !== undefined) {
            mergedMeta[key] = meta[key];
          }
        }
        if (meta.keywords !== undefined) mergedMeta.keywords = meta.keywords;
        if (meta.authors !== undefined) mergedMeta.authors = meta.authors;
        if (meta.icons !== undefined) mergedMeta.icons = meta.icons;
        if (meta.robots !== undefined) mergedMeta.robots = meta.robots;

        if (meta.openGraph && typeof meta.openGraph === "object") {
          mergedMeta.openGraph = { ...(mergedMeta.openGraph || {}), ...meta.openGraph };
        }
        if (meta.twitter && typeof meta.twitter === "object") {
          mergedMeta.twitter = { ...(mergedMeta.twitter || {}), ...meta.twitter };
        }
        if (meta.alternates && typeof meta.alternates === "object") {
          mergedMeta.alternates = { ...(mergedMeta.alternates || {}), ...meta.alternates };
        }
        if (meta.other && typeof meta.other === "object") {
          mergedMeta.other = { ...(mergedMeta.other || {}), ...meta.other };
        }
      }

      if (resolvedTitle) {
        mergedMeta.title = resolvedTitle;
      }

      const headHtml = generateHeadHtml(resolvedTitle, mergedMeta);
      return { resolvedTitle, mergedMeta, headHtml };
    };

    // Extract metadata cascade across layout chain and page
    const cascadeModules: any[] = [];
    if (input.segmentsFiles && input.segmentsFiles.length > 0) {
      for (const seg of input.segmentsFiles) {
        if (seg.layout) {
          try {
            const m = await import(seg.layout);
            if (m) cascadeModules.push(m);
          } catch {}
        }
      }
    } else if (input.layoutPaths && input.layoutPaths.length > 0) {
      for (const lp of input.layoutPaths) {
        try {
          const m = await import(lp);
          if (m) cascadeModules.push(m);
        } catch {}
      }
    }
    cascadeModules.push(pageMod);

    const { resolvedTitle, mergedMeta, headHtml } = await resolveCascadeMetadata(
      cascadeModules,
      { params: input.params, searchParams: input.searchParams }
    );

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
        title: resolvedTitle || null,
        metadata: mergedMeta || null,
        head_tags: headHtml || null,
        set_cookies: (globalThis as any).__NATA_SET_COOKIES__ || [],
        status_code: 307,
        redirect_url: redirectUrl,
        render_time_ms: performance.now() - startTime,
        mode: "Full",
        error: null,
      };
      console.log(SSR_DELIM_START + JSON.stringify(output) + SSR_DELIM_END);
      return output;
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
        title: resolvedTitle || null,
        metadata: mergedMeta || null,
        head_tags: headHtml || null,
        set_cookies: (globalThis as any).__NATA_SET_COOKIES__ || [],
        status_code: 307,
        redirect_url: redirectUrl,
        render_time_ms: performance.now() - startTime,
        mode: "Full",
        error: null,
      };
      console.log(SSR_DELIM_START + JSON.stringify(output) + SSR_DELIM_END);
      return output;
    }

    const output = {
      html: renderedHtml,
      initial_state: { params: input.params, searchParams: input.searchParams },
      title: resolvedTitle || null,
      metadata: mergedMeta || null,
      head_tags: headHtml || null,
      set_cookies: (globalThis as any).__NATA_SET_COOKIES__ || [],
      status_code: notFoundEncountered ? 404 : 200,
      redirect_url: null,
      render_time_ms: performance.now() - startTime,
      mode: "Full",
      error: null,
    };

    console.log(SSR_DELIM_START + JSON.stringify(output) + SSR_DELIM_END);
    return output;
  } catch (err: any) {
    const output = {
      html: "",
      initial_state: { params: input.params, searchParams: input.searchParams },
      title: null,
      metadata: null,
      head_tags: null,
      set_cookies: (globalThis as any).__NATA_SET_COOKIES__ || [],
      status_code: 500,
      redirect_url: redirectUrl,
      render_time_ms: performance.now() - startTime,
      mode: "ClientOnly",
      error: err?.message || String(err),
    };
    console.log(SSR_DELIM_START + JSON.stringify(output) + SSR_DELIM_END);
    return output;
  }
}
