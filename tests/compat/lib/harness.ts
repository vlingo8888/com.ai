// Shared tooling for the compatibility suite (tests/compat/run.ts) and the benchmark (scripts/bench.ts):
// start `com dev` on a project, drive headless Chrome over the DevTools protocol, measure what the
// performance budgets in ROADMAP.md talk about.

import { cpSync, existsSync, mkdtempSync, readFileSync, rmSync } from "fs";
import { tmpdir } from "os";
import { join, resolve } from "path";

export const repoRoot = resolve(import.meta.dir, "../../..");

export const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

export function median(values: number[]): number {
  const sorted = [...values].sort((a, b) => a - b);
  return sorted[Math.floor(sorted.length / 2)] ?? 0;
}

export function percentile(values: number[], p: number): number {
  const sorted = [...values].sort((a, b) => a - b);
  return sorted[Math.min(sorted.length - 1, Math.floor(sorted.length * p))] ?? 0;
}

/** Engine binary: --engine / COM_COMPILER_PATH / local release build */
export function findEngine(explicit?: string): string {
  const candidates = [explicit, process.env.COM_COMPILER_PATH, join(repoRoot, "compiler-rs/target/release/com-compiler")];
  const found = candidates.find((c) => c && existsSync(c));
  if (!found) throw new Error("com-compiler not found: run `bun run build:rust` or pass --engine <path>");
  return found;
}

export function findChrome(): string {
  const candidates = [
    process.env.CHROME_PATH,
    "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
    "/usr/bin/google-chrome",
    "/usr/bin/google-chrome-stable",
    "/usr/bin/chromium",
    "/usr/bin/chromium-browser",
  ];
  const found = candidates.find((c) => c && existsSync(c));
  if (!found) throw new Error("Chrome not found: set CHROME_PATH");
  return found;
}

export async function freePort(): Promise<number> {
  const server = Bun.listen({ hostname: "127.0.0.1", port: 0, socket: { data() {} } });
  const port = server.port;
  server.stop(true);
  return port;
}

/** Copies a project to a temp dir so generated files (.nata, node_modules, data) never touch the repo */
export function copyToTemp(projectDir: string, label: string): string {
  const dir = mkdtempSync(join(tmpdir(), `com-compat-${label}-`));
  cpSync(projectDir, dir, {
    recursive: true,
    filter: (src) => !/[\\/](\.nata|data|compat\.json)$/.test(src) && !src.includes(`${projectDir}/node_modules`),
  });
  return dir;
}

export interface DevServer {
  url: string;
  dir: string;
  port: number;
  /** ` ✓ Ready in …` as printed by the engine (ms since `com dev` started) */
  readyMs: number;
  log: () => string;
  pids: () => number[];
  stop: () => Promise<void>;
}

/** Starts `com dev` through the real CLI (bin/cli.ts), like a user would */
export async function startDev(dir: string, opts: { engine: string; target?: "web" | "zalo"; env?: Record<string, string> }): Promise<DevServer> {
  const port = await freePort();
  let output = "";
  const proc = Bun.spawn(["bun", join(repoRoot, "bin/cli.ts"), "dev", "--dir", dir, "--port", String(port), "--target", opts.target ?? "web", "--no-tunnel"], {
    cwd: repoRoot,
    stdin: "ignore",
    stdout: "pipe",
    stderr: "pipe",
    env: { ...process.env, NO_COLOR: "1", COM_COMPILER_PATH: opts.engine, ...opts.env },
  });
  const collect = async (stream: ReadableStream<Uint8Array>) => {
    const decoder = new TextDecoder();
    for await (const chunk of stream) output += decoder.decode(chunk);
  };
  collect(proc.stdout);
  collect(proc.stderr);

  const url = `http://localhost:${port}`;
  const deadline = Date.now() + 180_000;
  let readyMs = NaN;
  while (Date.now() < deadline) {
    const ready = output.match(/Ready in ([\d.]+)(ms|s)/);
    if (ready) {
      readyMs = Number(ready[1]) * (ready[2] === "s" ? 1000 : 1);
      try {
        await fetch(`${url}/_nata/client.js`);
        break;
      } catch {}
    }
    if (proc.exitCode !== null) throw new Error(`com dev exited early:\n${output}`);
    await sleep(100);
  }
  if (Number.isNaN(readyMs)) throw new Error(`com dev did not become ready:\n${output}`);

  const pids = () => {
    // The CLI, the engine and the Bun worker of this project
    const list = Bun.spawnSync(["pgrep", "-f", dir]).stdout.toString().trim().split("\n").filter(Boolean).map(Number);
    return [proc.pid, ...list.filter((p) => p !== proc.pid)];
  };
  return {
    url,
    dir,
    port,
    readyMs,
    log: () => output,
    pids,
    stop: async () => {
      for (const pid of pids()) {
        try { process.kill(pid, "SIGTERM"); } catch {}
      }
      await sleep(300);
      for (const pid of pids()) {
        try { process.kill(pid, "SIGKILL"); } catch {}
      }
    },
  };
}

/** Resident memory of the engine + worker (not the Bun CLI wrapper, which just waits) */
export function serverRssMb(server: DevServer): number {
  let kb = 0;
  for (const pid of server.pids().slice(1)) {
    const out = Bun.spawnSync(["ps", "-o", "rss=", "-p", String(pid)]).stdout.toString().trim();
    kb += Number(out) || 0;
  }
  return Math.round(kb / 1024);
}

export async function timeRequests(url: string, count: number, init?: RequestInit): Promise<number[]> {
  const times: number[] = [];
  for (let i = 0; i < count; i++) {
    const started = performance.now();
    const res = await fetch(url, init);
    await res.arrayBuffer();
    times.push(performance.now() - started);
  }
  return times;
}

// ---------- Chrome DevTools protocol ----------

export class Browser {
  private constructor(private proc: ReturnType<typeof Bun.spawn>, private profile: string, readonly port: number) {}

  static async launch(): Promise<Browser> {
    const port = await freePort();
    const profile = mkdtempSync(join(tmpdir(), "com-compat-chrome-"));
    const proc = Bun.spawn(
      [findChrome(), "--headless=new", "--disable-gpu", "--no-first-run", "--no-default-browser-check", `--user-data-dir=${profile}`, `--remote-debugging-port=${port}`, "about:blank"],
      { stdout: "ignore", stderr: "ignore" },
    );
    const browser = new Browser(proc, profile, port);
    for (let i = 0; i < 100; i++) {
      try {
        await fetch(`http://127.0.0.1:${port}/json/version`);
        return browser;
      } catch {
        await sleep(100);
      }
    }
    throw new Error("Chrome did not start");
  }

  async newPage(): Promise<Page> {
    const res = await fetch(`http://127.0.0.1:${this.port}/json/new?about:blank`, { method: "PUT" });
    const target = await res.json();
    return Page.connect(target.webSocketDebuggerUrl);
  }

  close() {
    this.proc.kill();
    try { rmSync(this.profile, { recursive: true, force: true }); } catch {}
  }
}

export interface PageReport {
  status: number | null;
  /** Visible text of the app root */
  text: string;
  /** Overlay state: "N Issues" badge text, or the open dialog's label + title */
  overlay: string | null;
  /** Hosts other than the dev server that the page contacted */
  externalHosts: string[];
  /** Transferred bytes of JavaScript (encoded, i.e. what crossed the network) */
  jsBytes: number;
  exceptions: string[];
}

export class Page {
  private id = 0;
  private pending = new Map<number, (v: any) => void>();
  private listeners: ((m: any) => void)[] = [];

  private constructor(private ws: WebSocket) {
    ws.onmessage = (e) => {
      const m = JSON.parse(String(e.data));
      if (m.id && this.pending.has(m.id)) {
        this.pending.get(m.id)!(m.result ?? m.error);
        this.pending.delete(m.id);
      }
      for (const l of this.listeners) l(m);
    };
  }

  static async connect(wsUrl: string): Promise<Page> {
    const ws = new WebSocket(wsUrl);
    await new Promise((r, j) => { ws.onopen = r; ws.onerror = j; });
    const page = new Page(ws);
    await page.send("Network.enable");
    await page.send("Runtime.enable");
    await page.send("Page.enable");
    await page.send("Network.setCacheDisabled", { cacheDisabled: true });
    return page;
  }

  send(method: string, params: any = {}): Promise<any> {
    return new Promise((r) => {
      const i = ++this.id;
      this.pending.set(i, r);
      this.ws.send(JSON.stringify({ id: i, method, params }));
    });
  }

  async evaluate<T = any>(expression: string): Promise<T> {
    const r = await this.send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true });
    return r?.result?.value as T;
  }

  /** Navigates and collects network/exception data until the page has settled */
  async load(url: string, settleMs = 2500): Promise<PageReport> {
    const origin = new URL(url).host;
    let status: number | null = null;
    let jsBytes = 0;
    const hosts = new Set<string>();
    const exceptions: string[] = [];
    const scriptRequests = new Set<string>();
    const listener = (m: any) => {
      if (m.method === "Network.requestWillBeSent") {
        const host = new URL(m.params.request.url).host;
        if (host && host !== origin && !host.startsWith("127.0.0.1") && !/^\d+\.\d+\.\d+\.\d+:/.test(host)) hosts.add(host);
      }
      if (m.method === "Network.responseReceived") {
        if (m.params.type === "Document" && status === null) status = m.params.response.status;
        if (m.params.type === "Script") scriptRequests.add(m.params.requestId);
      }
      if (m.method === "Network.loadingFinished" && scriptRequests.has(m.params.requestId)) jsBytes += m.params.encodedDataLength;
      if (m.method === "Runtime.exceptionThrown") exceptions.push(String(m.params.exceptionDetails.exception?.description ?? m.params.exceptionDetails.text).split("\n")[0]);
    };
    this.listeners.push(listener);
    await this.send("Page.navigate", { url });
    await sleep(settleMs);
    this.listeners = this.listeners.filter((l) => l !== listener);
    const state = await this.evaluate<{ text: string; overlay: string | null }>(OVERLAY_STATE);
    return { status, text: state.text, overlay: state.overlay, externalHosts: [...hosts], jsBytes, exceptions };
  }

  close() {
    this.ws.close();
  }
}

export const OVERLAY_STATE = `(() => {
  const host = document.querySelector("nata-dev-overlay");
  const q = (s) => host && host.shadowRoot && host.shadowRoot.querySelector(s);
  const dialog = q(".dialog") ? (q(".label")?.textContent || "") + ": " + (q(".title")?.textContent || "") : null;
  const root = document.getElementById("root") || document.getElementById("app") || document.body;
  return { text: (root.innerText || "").replace(/\\s+/g, " ").trim(), overlay: dialog || q(".badge")?.textContent || null };
})()`;

export function readJson<T>(path: string): T {
  return JSON.parse(readFileSync(path, "utf-8")) as T;
}
