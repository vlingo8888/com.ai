/**
 * Host of `com-compiler.wasm` (compiler-rs/wasm): loads the engine as a WASI reactor and gives it what
 * the native engine gets from the OS.
 *
 * - Files: the WASI implementation passed in (in the browser, the bun-rust kernel's VFS; in tests,
 *   `node:wasi` on a real folder). File changes are reported with `fileChanged`.
 * - Child processes (the Bun workers the engine starts): `spawn`, e.g. a bun-rust process worker.
 * - HTTP: `fetch(request)` instead of a socket, e.g. from the Service Worker that serves the preview.
 * - HMR: `onHmr` instead of the `/_hmr` WebSocket.
 *
 * The engine never blocks: it is driven by `com_tick`, often while a job is in flight or right after
 * an event (a file change waits out the watcher's debounce on an engine timer), once a second when idle
 * (workers exiting after a while, caches flushing). `dispose()` stops it.
 */

export interface ChildProcess {
  write(data: Uint8Array): void;
  closeStdin(): void;
  kill(): void;
}

export interface ChildEvents {
  stdout(data: Uint8Array): void;
  stderr(data: Uint8Array): void;
  exit(code: number): void;
}

export interface SpawnRequest {
  argv: string[];
  /** `null` removes the variable */
  env: Record<string, string | null>;
  cwd: string | null;
}

export interface ComHostOptions {
  /** `com-compiler.wasm`: URL, bytes or a compiled module */
  wasm: string | URL | BufferSource | WebAssembly.Module;
  /** `wasi_snapshot_preview1` imports, bound to the file system the engine sees */
  wasiImports: WebAssembly.ModuleImports;
  /** Called once instantiated, e.g. `wasi.initialize(instance)` (runs the reactor's `_initialize`) */
  initialize?(instance: WebAssembly.Instance): void;
  /** Starts a child process; without it, the engine's workers fail to start (compiling still works) */
  spawn?(request: SpawnRequest, events: ChildEvents): ChildProcess;
}

interface Exports {
  memory: WebAssembly.Memory;
  com_alloc(len: number): number;
  com_free(ptr: number, len: number): void;
  com_es5(ptr: number, len: number): number;
  com_tailwind(ptr: number, len: number): number;
  com_routes(ptr: number, len: number): number;
  com_dev_start(ptr: number, len: number): number;
  com_request(ptr: number, len: number): number;
  com_tick(): number;
  com_job_take(id: number): number;
  com_hmr_take(): number;
  com_file_changed(ptr: number, len: number): void;
  com_wake(pid: number): void;
}

/** Response header with the app's `Set-Cookie` values (a JSON array), which `fetch()` returns */
export const SET_COOKIE_HEADER = "x-com-set-cookie";
/**
 * Request header with the request's cookies: a Request made by script cannot carry `Cookie` in
 * browsers (a forbidden header), so hosts put them here and the engine gets them as `cookie`
 */
export const COOKIE_HEADER = "x-com-cookie";

/** A request's headers for the engine, `cookie` restored from COOKIE_HEADER */
function requestHeaders(request: Request): [string, string][] {
  const headers: [string, string][] = [];
  for (const [name, value] of request.headers) {
    headers.push([name === COOKIE_HEADER ? "cookie" : name, value]);
  }
  return headers;
}

/** WASI `EAGAIN`: no data yet / still running (platform/host.rs `PENDING`) */
const PENDING = -6;
/** `ENOSYS`, `EBADF` */
const ENOSYS = -52;
/** Frequent ticks after an event, for the engine's debounce timers (the watcher waits up to ~1s) */
const BUSY_WINDOW_MS = 3000;
const IDLE_TICK_MS = 1000;
const EBADF = -8;
/** Stderr kept unread per child: the engine never reads an inherited stderr (a long-lived worker's) */
const STDERR_CAP = 256 * 1024;

interface Child {
  process: ChildProcess;
  stdout: Uint8Array[];
  stderr: Uint8Array[];
  /** Bytes queued in `stderr` */
  stderrBytes: number;
  stdoutDone: boolean;
  stderrDone: boolean;
  exitCode: number | null;
  /** The engine has seen the exit code (process_wait) */
  waited: boolean;
}

const encoder = new TextEncoder();
const decoder = new TextDecoder();

export class ComEngine {
  private exports!: Exports;
  private children = new Map<number, Child>();
  private nextPid = 1;
  private jobs = new Map<number, (bytes: Uint8Array) => void>();
  private timer: ReturnType<typeof setTimeout> | null = null;
  /** Ticks stay frequent until then (ms): the engine has timers running after an event */
  private busyUntil = 0;
  private disposed = false;
  private hmrListeners = new Set<(message: unknown) => void>();

  private constructor(private options: ComHostOptions) {}

  static async load(options: ComHostOptions): Promise<ComEngine> {
    const engine = new ComEngine(options);
    const module = await compile(options.wasm);
    const instance = await WebAssembly.instantiate(module, {
      wasi_snapshot_preview1: options.wasiImports,
      com_host: engine.hostImports(),
    });
    engine.exports = instance.exports as unknown as Exports;
    options.initialize?.(instance);
    return engine;
  }

  // ---------- one-shot commands (same JSON as `com-compiler es5 | tailwind`) ----------

  es5(files: { path: string; content: string }[]): { path: string; es5?: string; error?: string }[] {
    return JSON.parse(decoder.decode(this.call("com_es5", encoder.encode(JSON.stringify(files)))));
  }

  tailwind(input: { css: string; files: { path: string; content: string }[]; minify?: boolean }): { css?: string; error?: string } {
    return JSON.parse(decoder.decode(this.call("com_tailwind", encoder.encode(JSON.stringify(input)))));
  }

  routes(dir: string): { pattern: string; page_file: string; is_api: boolean }[] {
    return JSON.parse(decoder.decode(this.call("com_routes", encoder.encode(dir))));
  }

  // ---------- com dev ----------

  async devStart(options: { dir: string; target?: "web" | "zalo" }): Promise<void> {
    const result = JSON.parse(decoder.decode(await this.job(this.exports.com_dev_start(...this.write(encoder.encode(JSON.stringify(options)))))));
    if (result.error) throw new Error(result.error);
  }

  /** A request to the dev app, as the native engine would get it on its port */
  async fetch(request: Request): Promise<Response> {
    const url = new URL(request.url);
    const head = encoder.encode(
      JSON.stringify({ method: request.method, url: url.pathname + url.search, headers: requestHeaders(request) }),
    );
    const body = request.body ? new Uint8Array(await request.arrayBuffer()) : new Uint8Array();
    const frame = new Uint8Array(4 + head.length + body.length);
    new DataView(frame.buffer).setUint32(0, head.length, true);
    frame.set(head, 4);
    frame.set(body, 4 + head.length);
    const out = await this.job(this.exports.com_request(...this.write(frame)));
    const headLength = new DataView(out.buffer, out.byteOffset).getUint32(0, true);
    const { status, headers } = JSON.parse(decoder.decode(out.subarray(4, 4 + headLength))) as { status: number; headers: [string, string][] };
    const nullBody = status === 204 || status === 304 || request.method === "HEAD";
    // A Response made by script drops Set-Cookie in browsers (a forbidden response header): the
    // cookies the app sets are kept as JSON in SET_COOKIE_HEADER for the host to pass on
    const cookies = headers.filter(([name]) => name.toLowerCase() === "set-cookie").map(([, value]) => value);
    const kept = headers.filter(([name]) => name.toLowerCase() !== "set-cookie");
    if (cookies.length) kept.push([SET_COOKIE_HEADER, JSON.stringify(cookies)]);
    // `out` is already a copy out of wasm memory (read): the body is a view of it, not a second copy
    return new Response(nullBody ? null : out.subarray(4 + headLength), { status, headers: kept });
  }

  /** A file of the project was created, changed or removed (absolute path, as the engine sees it) */
  fileChanged(path: string, kind: "create" | "modify" | "remove" = "modify"): void {
    this.exports.com_file_changed(...this.write(encoder.encode(JSON.stringify({ path, kind }))));
    this.pump();
  }

  /** HMR messages (what the native engine sends on `/_hmr`) */
  onHmr(listener: (message: unknown) => void): () => void {
    this.hmrListeners.add(listener);
    return () => this.hmrListeners.delete(listener);
  }

  // ---------- plumbing ----------

  private bytes(): Uint8Array {
    return new Uint8Array(this.exports.memory.buffer);
  }

  private write(data: Uint8Array): [number, number] {
    const ptr = this.exports.com_alloc(data.length);
    this.bytes().set(data, ptr);
    return [ptr, data.length];
  }

  /** Reads and frees an output (`[u32 length][bytes]`) */
  private read(ptr: number): Uint8Array {
    const length = new DataView(this.exports.memory.buffer).getUint32(ptr, true);
    const data = this.bytes().slice(ptr + 4, ptr + 4 + length);
    this.exports.com_free(ptr, 4 + length);
    return data;
  }

  private call(name: "com_es5" | "com_tailwind" | "com_routes", input: Uint8Array): Uint8Array {
    return this.read(this.exports[name](...this.write(input)));
  }

  private job(id: number): Promise<Uint8Array> {
    return new Promise((resolve) => {
      this.jobs.set(id, resolve);
      this.pump();
    });
  }

  /** Stops ticking (pending jobs never finish) */
  dispose(): void {
    this.disposed = true;
    if (this.timer) clearTimeout(this.timer);
    this.timer = null;
  }

  /** Ticks now, then keeps ticking: every few ms while busy, backing off to once a second when idle */
  private pump(): void {
    if (this.disposed) return;
    this.busyUntil = Date.now() + BUSY_WINDOW_MS;
    if (this.timer) clearTimeout(this.timer);
    let idle = 0;
    const step = () => {
      this.timer = null;
      if (this.disposed) return;
      this.exports.com_tick();
      const progressed = this.collect() || this.flushHmr();
      idle = progressed ? 0 : idle + 1;
      const busy = this.jobs.size > 0 || Date.now() < this.busyUntil;
      this.timer = setTimeout(step, busy ? Math.min(idle, 50) : IDLE_TICK_MS);
    };
    step();
  }

  private collect(): boolean {
    let done = false;
    for (const [id, resolve] of this.jobs) {
      const ptr = this.exports.com_job_take(id);
      if (ptr === 0) continue;
      this.jobs.delete(id);
      resolve(this.read(ptr));
      done = true;
    }
    return done;
  }

  private flushHmr(): boolean {
    const messages: string[] = JSON.parse(decoder.decode(this.read(this.exports.com_hmr_take())));
    for (const message of messages) for (const listener of this.hmrListeners) listener(message);
    return messages.length > 0;
  }

  private wake(pid: number): void {
    this.exports.com_wake(pid);
    this.pump();
  }

  /** Forgets a child once it exited, the engine saw its exit code and read what it wrote */
  private reap(pid: number): void {
    const child = this.children.get(pid);
    if (child?.waited && child.exitCode !== null && !child.stdout.length && !child.stderr.length) this.children.delete(pid);
  }

  private hostImports(): WebAssembly.ModuleImports {
    const text = (ptr: number, len: number) => decoder.decode(this.bytes().subarray(ptr, ptr + len));
    return {
      process_spawn: (ptr: number, len: number): number => {
        if (!this.options.spawn) return ENOSYS;
        const pid = this.nextPid++;
        const child: Child = {
          process: null!,
          stdout: [],
          stderr: [],
          stderrBytes: 0,
          stdoutDone: false,
          stderrDone: false,
          exitCode: null,
          waited: false,
        };
        this.children.set(pid, child);
        try {
          child.process = this.options.spawn(JSON.parse(text(ptr, len)), {
            stdout: (data) => (child.stdout.push(data), this.wake(pid)),
            stderr: (data) => {
              child.stderr.push(data);
              child.stderrBytes += data.length;
              // Oldest first: logs nobody reads must not grow without bound
              while (child.stderrBytes > STDERR_CAP && child.stderr.length > 1) child.stderrBytes -= child.stderr.shift()!.length;
              this.wake(pid);
            },
            exit: (code) => {
              child.exitCode = code;
              child.stdoutDone = child.stderrDone = true;
              this.wake(pid);
            },
          });
        } catch {
          this.children.delete(pid);
          return ENOSYS;
        }
        return pid;
      },
      process_write: (pid: number, ptr: number, len: number): number => {
        const child = this.children.get(pid);
        if (!child || child.exitCode !== null) return EBADF;
        child.process.write(this.bytes().slice(ptr, ptr + len));
        return len;
      },
      process_close_stdin: (pid: number): void => this.children.get(pid)?.process.closeStdin(),
      process_read: (pid: number, fd: number, ptr: number, cap: number): number => {
        const child = this.children.get(pid);
        if (!child) return EBADF;
        const queue = fd === 1 ? child.stdout : child.stderr;
        const chunk = queue[0];
        if (!chunk) return (fd === 1 ? child.stdoutDone : child.stderrDone) ? 0 : PENDING;
        const n = Math.min(cap, chunk.length);
        this.bytes().set(chunk.subarray(0, n), ptr);
        if (n === chunk.length) queue.shift();
        else queue[0] = chunk.subarray(n);
        if (fd !== 1) child.stderrBytes -= n;
        this.reap(pid);
        return n;
      },
      process_wait: (pid: number): number => {
        const child = this.children.get(pid);
        if (!child) return EBADF;
        if (child.exitCode === null) return PENDING;
        child.waited = true;
        const code = child.exitCode;
        this.reap(pid);
        return code;
      },
      process_kill: (pid: number): void => this.children.get(pid)?.process.kill(),
    };
  }
}

async function compile(source: ComHostOptions["wasm"]): Promise<WebAssembly.Module> {
  if (source instanceof WebAssembly.Module) return source;
  if (typeof source === "string" || source instanceof URL) {
    const res = await fetch(source);
    if (!res.ok) throw new Error(`failed to fetch ${source}: ${res.status}`);
    return WebAssembly.compileStreaming ? WebAssembly.compileStreaming(res) : WebAssembly.compile(await res.arrayBuffer());
  }
  return WebAssembly.compile(source);
}
