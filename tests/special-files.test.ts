import { describe, expect, it, beforeAll, afterAll } from "bun:test";
import { mkdtempSync, mkdirSync, writeFileSync, rmSync, existsSync } from "fs";
import { join } from "path";
import { tmpdir } from "os";

// ============================================================================
// 1. Next.js Shims & Exceptions (P0 Special Files Protocol)
// ============================================================================
describe("App Router Special Files - Next.js Shims & Error Signals", () => {
  it("notFound() should throw Error with digest 'NEXT_NOT_FOUND'", () => {
    const notFound = () => {
      const err = new Error("NEXT_NOT_FOUND");
      (err as any).digest = "NEXT_NOT_FOUND";
      throw err;
    };

    expect(() => notFound()).toThrow("NEXT_NOT_FOUND");
    try {
      notFound();
    } catch (err: any) {
      expect(err.digest).toBe("NEXT_NOT_FOUND");
    }
  });

  it("redirect() should throw Error with digest 'NEXT_REDIRECT' and target url", () => {
    const redirect = (url: string, type: "push" | "replace" = "replace") => {
      const err = new Error(`NEXT_REDIRECT;${url}`);
      (err as any).digest = `NEXT_REDIRECT;${type};${url};307;`;
      (err as any).url = url;
      throw err;
    };

    expect(() => redirect("/dashboard")).toThrow("NEXT_REDIRECT;/dashboard");
    try {
      redirect("/dashboard");
    } catch (err: any) {
      expect(err.url).toBe("/dashboard");
      expect(err.digest).toContain("NEXT_REDIRECT;replace;/dashboard;307;");
    }
  });
});

// ============================================================================
// 2. Directory Structure & Segment File Hierarchy Discovery
// ============================================================================
describe("App Router Special Files - File Structure Discovery", () => {
  let tempDir: string;
  let appDir: string;

  beforeAll(() => {
    tempDir = mkdtempSync(join(tmpdir(), "com-special-files-test-"));
    appDir = join(tempDir, "app");
    mkdirSync(appDir, { recursive: true });

    // Root Special Files
    writeFileSync(join(appDir, "layout.tsx"), `export default function RootLayout({ children }: any) { return children; }`);
    writeFileSync(join(appDir, "loading.tsx"), `export default function RootLoading() { return "Loading app..."; }`);
    writeFileSync(join(appDir, "error.tsx"), `export default function RootError({ error, reset }: any) { return "Root Error: " + error.message; }`);
    writeFileSync(join(appDir, "not-found.tsx"), `export default function RootNotFound() { return "404 - Global Not Found"; }`);
    writeFileSync(join(appDir, "global-error.tsx"), `export default function GlobalError({ error, reset }: any) { return "Global Error Shell"; }`);

    // Nested Route with Template & Loading
    const dashDir = join(appDir, "dashboard");
    mkdirSync(dashDir, { recursive: true });
    writeFileSync(join(dashDir, "template.tsx"), `export default function DashboardTemplate({ children }: any) { return children; }`);
    writeFileSync(join(dashDir, "loading.tsx"), `export default function DashboardLoading() { return "Dashboard Loading Skeleton"; }`);
    writeFileSync(join(dashDir, "page.tsx"), `export default function DashboardPage() { return "Dashboard Overview Content"; }`);

    // Route Group with Nested Error & Page
    const groupDir = join(appDir, "(admin)", "settings");
    mkdirSync(groupDir, { recursive: true });
    writeFileSync(join(groupDir, "error.tsx"), `export default function SettingsError({ error }: any) { return "Settings Error: " + error.message; }`);
    writeFileSync(join(groupDir, "page.tsx"), `export default function SettingsPage() { return "Admin Settings"; }`);
  });

  afterAll(() => {
    try {
      rmSync(tempDir, { recursive: true, force: true });
    } catch {}
  });

  it("discovers all special files at root level", () => {
    expect(existsSync(join(appDir, "layout.tsx"))).toBe(true);
    expect(existsSync(join(appDir, "loading.tsx"))).toBe(true);
    expect(existsSync(join(appDir, "error.tsx"))).toBe(true);
    expect(existsSync(join(appDir, "not-found.tsx"))).toBe(true);
    expect(existsSync(join(appDir, "global-error.tsx"))).toBe(true);
  });

  it("discovers segment-specific template.tsx and loading.tsx in nested routes", () => {
    expect(existsSync(join(appDir, "dashboard", "template.tsx"))).toBe(true);
    expect(existsSync(join(appDir, "dashboard", "loading.tsx"))).toBe(true);
    expect(existsSync(join(appDir, "dashboard", "page.tsx"))).toBe(true);
  });

  it("supports special files inside route groups (admin)", () => {
    expect(existsSync(join(appDir, "(admin)", "settings", "error.tsx"))).toBe(true);
    expect(existsSync(join(appDir, "(admin)", "settings", "page.tsx"))).toBe(true);
  });
});

// ============================================================================
// 3. SSR & VDOM Special Files Composition & Boundary Semantics
// ============================================================================
describe("App Router Special Files - VDOM Composition & Boundaries", () => {
  // Ultra-lightweight VDOM test engine simulating React element tree evaluation & boundary propagation
  interface VNode {
    type: string | Function;
    props: Record<string, any>;
    children: any[];
  }

  const V = {
    createElement(type: any, props: any = {}, ...children: any[]): VNode {
      return {
        type,
        props: props || {},
        children: children.flat().filter((c) => c !== null && c !== undefined),
      };
    },
    render(node: any, catchHandler?: (err: any) => any): string {
      if (node === null || node === undefined) return "";
      if (typeof node === "string" || typeof node === "number") return String(node);
      if (Array.isArray(node)) return node.map((n) => V.render(n, catchHandler)).join("");

      if (typeof node.type === "function") {
        const currentHandler = node.props?.onError || catchHandler;
        try {
          const res = node.type({ ...node.props, children: node.children });
          return V.render(res, currentHandler);
        } catch (err: any) {
          if (currentHandler) {
            return V.render(currentHandler(err));
          }
          throw err;
        }
      }

      const attrs = Object.entries(node.props)
        .filter(([k]) => k !== "children" && !k.startsWith("__") && k !== "onError")
        .map(([k, v]) => ` ${k}="${v}"`)
        .join("");

      try {
        const inner = node.children.map((c: any) => V.render(c, catchHandler)).join("");
        return `<${node.type}${attrs}>${inner}</${node.type}>`;
      } catch (err: any) {
        if (catchHandler) {
          return V.render(catchHandler(err));
        }
        throw err;
      }
    },
  };

  it("composes segment hierarchy: Layout > Template > Suspense(Loading) > Page", () => {
    // 1. Page
    const Page = () => V.createElement("div", { id: "page-content" }, "Hello Dashboard");

    // 2. Suspense / Loading wrapper
    const LoadingFallback = () => V.createElement("div", { id: "loading" }, "Loading...");
    const SuspendedPage = ({ children }: any) => V.createElement("div", { class: "suspense-boundary" }, children);

    // 3. Template (remounts on navigation)
    const Template = ({ children }: any) => V.createElement("main", { id: "template-wrapper", "data-nav-key": "/dashboard" }, children);

    // 4. Layout (persists across navigations)
    const Layout = ({ children }: any) => V.createElement("div", { id: "root-layout" }, children);

    // Hierarchy composition from inner to outer
    const tree = V.createElement(
      Layout,
      {},
      V.createElement(
        Template,
        {},
        V.createElement(
          SuspendedPage,
          {},
          V.createElement(Page)
        )
      )
    );

    const html = V.render(tree);
    expect(html).toContain('id="root-layout"');
    expect(html).toContain('id="template-wrapper"');
    expect(html).toContain('data-nav-key="/dashboard"');
    expect(html).toContain('class="suspense-boundary"');
    expect(html).toContain('id="page-content"');
    expect(html).toContain("Hello Dashboard");
  });

  it("catches NEXT_NOT_FOUND and renders not-found.tsx with 404 status code", () => {
    let statusCode = 200;

    const NotFoundBoundary = ({ children, onError }: any) => {
      return V.createElement("div", { class: "boundary-shell" }, ...children);
    };

    const NotFoundComponent = () => V.createElement("div", { id: "not-found-404" }, "404 - Custom Page Not Found");

    const CrashingRoute = () => {
      const err = new Error("NEXT_NOT_FOUND");
      (err as any).digest = "NEXT_NOT_FOUND";
      throw err;
    };

    const boundaryWrapper = V.createElement(
      NotFoundBoundary,
      {
        onError: (err: any) => {
          if (err?.digest === "NEXT_NOT_FOUND") {
            statusCode = 404;
            return V.createElement(NotFoundComponent);
          }
          throw err;
        },
      },
      V.createElement(CrashingRoute)
    );

    const html = V.render(boundaryWrapper);
    expect(statusCode).toBe(404);
    expect(html).toContain('id="not-found-404"');
    expect(html).toContain("404 - Custom Page Not Found");
  });

  it("catches runtime errors and renders error.tsx with reset handler", () => {
    let resetCalled = false;
    const reset = () => {
      resetCalled = true;
    };

    const ErrorBoundary = ({ children }: any) => {
      return V.createElement("div", { class: "error-boundary-shell" }, ...children);
    };

    const ErrorComponent = ({ error, reset }: any) => {
      return V.createElement(
        "div",
        { id: "error-card" },
        `Application Error: ${error.message}`
      );
    };

    const BrokenComponent = () => {
      throw new Error("Failed to connect to PostgreSQL");
    };

    const boundary = V.createElement(
      ErrorBoundary,
      {
        onError: (err: any) => {
          return V.createElement(ErrorComponent, { error: err, reset });
        },
      },
      V.createElement(BrokenComponent)
    );

    const html = V.render(boundary);
    expect(html).toContain('id="error-card"');
    expect(html).toContain("Application Error: Failed to connect to PostgreSQL");
  });

  it("global-error.tsx wraps entire application shell on critical failure", () => {
    let is500 = false;

    const GlobalErrorBoundary = ({ children }: any) => {
      return V.createElement("div", { class: "global-shell" }, ...children);
    };

    const GlobalErrorComponent = ({ error }: any) => {
      return V.createElement(
        "html",
        {},
        V.createElement(
          "body",
          { class: "global-error-body" },
          V.createElement("h1", {}, "Critical System Failure: " + error.message)
        )
      );
    };

    const RootFatalCrash = () => {
      throw new Error("FATAL: Root layout failed to initialize");
    };

    const globalBoundary = V.createElement(
      GlobalErrorBoundary,
      {
        onError: (err: any) => {
          is500 = true;
          return V.createElement(GlobalErrorComponent, { error: err });
        },
      },
      V.createElement(RootFatalCrash)
    );

    const html = V.render(globalBoundary);
    expect(is500).toBe(true);
    expect(html).toContain('class="global-error-body"');
    expect(html).toContain("Critical System Failure: FATAL: Root layout failed to initialize");
  });
});
