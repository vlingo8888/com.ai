/**
 * Smoke test of `com-compiler.wasm` outside the browser: `node:wasi` gives it the real file system,
 * and child processes are real Bun processes, so the result should match the native engine.
 *
 *   bun scripts/build-wasm.ts && bun build compiler-rs/wasm/smoke.ts --target node --outfile /tmp/smoke.mjs && node /tmp/smoke.mjs
 */
import { spawn } from "node:child_process";
import { mkdtempSync, mkdirSync, readFileSync, realpathSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { WASI } from "node:wasi";
import { ComEngine } from "./host.ts";

const wasmPath = process.argv[2] ?? join(import.meta.dirname, "../target/wasm32-wasip1/release/com_compiler_wasm.wasm");
const dev = process.argv.includes("--dev");

function check(name: string, ok: boolean, detail?: unknown) {
  console.log(`${ok ? "✓" : "✗"} ${name}${ok || detail === undefined ? "" : `: ${typeof detail === "string" ? detail : JSON.stringify(detail).slice(0, 400)}`}`);
  if (!ok) process.exitCode = 1;
}

// Canonical: the engine watches the real path (macOS: /var → /private/var)
const project = realpathSync(mkdtempSync(join(tmpdir(), "com-wasm-")));
mkdirSync(join(project, "app/about"), { recursive: true });
mkdirSync(join(project, "app/api/hello"), { recursive: true });
writeFileSync(join(project, "package.json"), JSON.stringify({ name: "smoke", private: true, dependencies: {} }));
writeFileSync(join(project, "app/layout.tsx"), `export default function Layout({ children }: { children: React.ReactNode }) {\n  return <html><body>{children}</body></html>;\n}\n`);
writeFileSync(join(project, "app/page.tsx"), `export default function Page() {\n  return <h1 className="text-xl font-bold">Hello from com.wasm</h1>;\n}\n`);
writeFileSync(join(project, "app/about/page.tsx"), `export default function About() {\n  return <p>About</p>;\n}\n`);
writeFileSync(join(project, "app/api/hello/route.ts"), `export function GET() {\n  return Response.json({ hello: "wasm" });\n}\n`);

const wasi = new WASI({ version: "preview1", args: ["com-compiler"], env: process.env as Record<string, string>, preopens: { "/": "/" } });
const started = performance.now();
const engine = await ComEngine.load({
  wasm: readFileSync(wasmPath),
  wasiImports: wasi.wasiImport as WebAssembly.ModuleImports,
  initialize: (instance) => wasi.initialize(instance),
  spawn(request, events) {
    const env = { ...process.env };
    for (const [k, v] of Object.entries(request.env)) v === null ? delete env[k] : (env[k] = v);
    const child = spawn(request.argv[0], request.argv.slice(1), { cwd: request.cwd ?? undefined, env, stdio: "pipe" });
    child.stdout.on("data", (d: Buffer) => events.stdout(new Uint8Array(d)));
    child.stderr.on("data", (d: Buffer) => events.stderr(new Uint8Array(d)));
    child.on("exit", (code) => events.exit(code ?? 1));
    child.on("error", () => events.exit(127));
    return { write: (d) => child.stdin.write(d), closeStdin: () => child.stdin.end(), kill: () => child.kill() };
  },
});
console.log(`loaded ${(readFileSync(wasmPath).length / 1024 / 1024).toFixed(1)} MB in ${(performance.now() - started).toFixed(0)}ms`);

const [es5] = engine.es5([{ path: "app/page.tsx", content: readFileSync(join(project, "app/page.tsx"), "utf8") }]);
check("es5 compiles TSX to CommonJS", !!es5.es5 && es5.es5.includes("exports") && !es5.error, es5);

const tw = engine.tailwind({ css: '@import "tailwindcss";', files: [{ path: "app/page.tsx", content: 'className="text-xl font-bold"' }] });
check("tailwind generates the used classes", !!tw.css && tw.css.includes(".text-xl") && tw.css.includes(".font-bold"), tw);

const routes = engine.routes(project).map((r) => r.pattern).sort();
check("routes scans the App Router", JSON.stringify(routes) === JSON.stringify(["/", "/about", "/api/hello"]), routes);

if (dev) {
  await engine.devStart({ dir: project });
  check("dev server starts", true);
  for (const path of ["/_nata/client.js", "/_bundle/app/page.tsx", "/api/hello", "/"]) {
    const t = performance.now();
    const res = await engine.fetch(new Request(`http://localhost${path}`));
    const body = await res.text();
    check(`GET ${path} → ${res.status} (${(performance.now() - t).toFixed(0)}ms, ${body.length} B)`, res.ok, body.slice(0, 300));
  }
  const hmr = new Promise((resolve) => engine.onHmr(resolve));
  writeFileSync(join(project, "app/page.tsx"), `export default function Page() {\n  return <h1>Changed</h1>;\n}\n`);
  engine.fileChanged(join(project, "app/page.tsx"));
  const message = await Promise.race([hmr, new Promise((r) => setTimeout(() => r(null), 5000))]);
  check("file change sends an HMR message", message !== null, message);
}
engine.dispose();
process.exit(process.exitCode ?? 0);
