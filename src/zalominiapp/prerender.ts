// Prerendering for `com build --target zalo` (ROADMAP P1.9): the React Server Components payload of
// every static route (no dynamic segment) is rendered once at build time and shipped in the Mini App
// package, so the first screen of those routes does not wait for the backend.

import { existsSync, mkdirSync, readdirSync, statSync, writeFileSync } from "fs";
import { join } from "path";

const PAGE = /^page\.(tsx|ts|jsx|js)$/;

/** URL paths of the routes without dynamic segments (`app/(shop)/cart/page.tsx` -> `/cart`) */
export function staticRoutes(projectDir: string): string[] {
  const appDir = [join(projectDir, "app"), join(projectDir, "src", "app")].find((d) => existsSync(d));
  if (!appDir) return [];
  const routes: string[] = [];
  const walk = (dir: string, segments: string[]) => {
    for (const name of readdirSync(dir).sort()) {
      const full = join(dir, name);
      if (statSync(full).isDirectory()) {
        // Private folders, parallel slots, intercepting routes and dynamic segments are not static pages
        if (name.startsWith("_") || name.startsWith("@") || name.startsWith("(.") || name.includes("[")) continue;
        walk(full, name.startsWith("(") && name.endsWith(")") ? segments : [...segments, name]);
      } else if (PAGE.test(name)) {
        routes.push("/" + segments.join("/"));
      }
    }
  };
  walk(appDir, []);
  return routes.sort();
}

/** `/` -> `index.rsc`, `/blog/intro` -> `blog__intro.rsc` */
export const payloadFileName = (route: string) => (route === "/" ? "index" : route.slice(1).replace(/\//g, "__")) + ".rsc";

async function freePort(): Promise<number> {
  const server = Bun.serve({ port: 0, fetch: () => new Response() });
  const port = server.port;
  server.stop(true);
  return port;
}

export interface PrerenderResult {
  /** route -> path of its payload inside the package */
  manifest: Record<string, string>;
  failed: string[];
}

/** Renders the payloads with the engine (a temporary dev server) into `<outputDir>/assets/rsc/` */
export async function prerenderPayloads(projectDir: string, outputDir: string, engine: string): Promise<PrerenderResult> {
  const routes = staticRoutes(projectDir);
  const result: PrerenderResult = { manifest: {}, failed: [] };
  if (routes.length === 0) return result;

  const port = await freePort();
  const proc = Bun.spawn([engine, "dev", "--dir", projectDir, "--port", String(port), "--target", "web"], {
    cwd: projectDir,
    stdin: "ignore",
    stdout: "ignore",
    stderr: "ignore",
    env: { ...process.env, COM_MANAGED: "1" },
  });
  const origin = `http://127.0.0.1:${port}`;
  try {
    for (let i = 0; i < 300; i++) {
      try {
        if ((await fetch(origin + "/_nata/client.js")).ok) break;
      } catch {}
      await Bun.sleep(100);
    }
    const payloadDir = join(outputDir, "assets", "rsc");
    mkdirSync(payloadDir, { recursive: true });
    for (const route of routes) {
      try {
        const res = await fetch(origin + route, { headers: { RSC: "1" } });
        if (!res.ok || !(res.headers.get("content-type") || "").includes("text/x-component")) throw new Error(`HTTP ${res.status}`);
        const file = payloadFileName(route);
        writeFileSync(join(payloadDir, file), new Uint8Array(await res.arrayBuffer()));
        result.manifest[route] = `assets/rsc/${file}`;
      } catch {
        result.failed.push(route);
      }
    }
    writeFileSync(join(payloadDir, "manifest.json"), JSON.stringify(result.manifest, null, 2));
  } finally {
    proc.kill();
  }
  return result;
}
