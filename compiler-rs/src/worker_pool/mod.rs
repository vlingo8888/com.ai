//! Persistent Bun worker per project, replacing one `bun -e <runner>` process per SSR render,
//! Server Action and route handler.
//!
//! Transport is binary frames over the worker's stdin/stdout (see `worker.ts` for the layout), so
//! request/response bodies travel as raw bytes and many requests are in flight on one process at once;
//! the worker isolates their state with AsyncLocalStorage.
//!
//! Lifecycle:
//! - started in the background when the dev server boots (`prewarm`), or on the first request;
//! - server code changes do not restart it: modules are loaded through the engine (`Bun.plugin` asks
//!   for `rsc:` / `ssr:` modules with a `CALL` frame, see `module_server.rs`), and the module graph
//!   gives changed files new versioned specifiers. `restart` remains for environment/config changes,
//!   and recycles the process after many invalidations so stale module versions do not pile up;
//! - stopped after `COM_WORKER_IDLE` seconds without requests (default 600, `0` = never), and started
//!   again on demand, so an idle `com dev` costs no worker memory.
//!
//! `COM_WORKER=0` disables the pool; callers then run their runner scripts in one-off processes.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::{oneshot, Semaphore};

const WORKER_SOURCE: &str = include_str!("worker.ts");
const SSR_RUNTIME: &str = include_str!("../ssr/runtime.ts");
const ROUTE_RUNTIME: &str = include_str!("../route_handler/runtime.ts");
const RPC_RUNTIME: &str = include_str!("../rpc_runtime.ts");
const DEPS_BUNDLER: &str = include_str!("../deps_bundler.ts");
const METADATA_RUNTIME: &str = include_str!("../ssr/metadata_runtime.ts");
const RSC_RUNTIME: &str = include_str!("../rsc/rsc_runtime.ts");
const RSC_SSR: &str = include_str!("../rsc/ssr_runtime.ts");
const RSC_RENDER: &str = include_str!("../rsc/render.ts");
pub const RSC_BOUNDARIES: &str = include_str!("../rsc/boundaries.js");

const KIND_REQUEST: u8 = 1;
const KIND_RESULT: u8 = 2;
/// Part of a streamed response (worker -> engine), before its `RESULT`
const KIND_CHUNK: u8 = 3;
const KIND_READY: u8 = 4;
const KIND_SHUTDOWN: u8 = 5;
/// A request from the worker to the engine (load a module), answered with `REPLY`
const KIND_CALL: u8 = 6;
const KIND_REPLY: u8 = 7;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);
const STARTUP_TIMEOUT: Duration = Duration::from_secs(30);
const SHUTDOWN_GRACE: Duration = Duration::from_millis(800);
const DRAIN_TIMEOUT: Duration = Duration::from_secs(10);

/// Result of one worker command
#[derive(Debug, Clone)]
pub struct Reply {
    /// `{"ok", "output", "code", "logs": {"stdout", "stderr"}, "error"?}` (see `worker.ts`)
    pub meta: serde_json::Value,
    /// Raw response body (route handlers), empty otherwise
    pub body: Vec<u8>,
}

impl Reply {
    pub fn output(&self) -> &serde_json::Value {
        &self.meta["output"]
    }
    pub fn code(&self) -> i64 {
        self.meta["code"].as_i64().unwrap_or(1)
    }
    pub fn stdout(&self) -> &str {
        self.meta["logs"]["stdout"].as_str().unwrap_or("")
    }
    pub fn stderr(&self) -> &str {
        self.meta["logs"]["stderr"].as_str().unwrap_or("")
    }
    /// Error raised by the worker itself (unknown command, runtime failed to import)
    pub fn error(&self) -> Option<&str> {
        if self.meta["ok"].as_bool() == Some(false) {
            Some(self.meta["error"].as_str().unwrap_or("worker command failed"))
        } else {
            None
        }
    }
}

/// Output of a runner script, shaped like `std::process::Output`
#[derive(Debug, Clone)]
pub struct ScriptOutput {
    pub stdout: String,
    pub stderr: String,
    pub success: bool,
}

/// `Unavailable` means "use a one-off process instead"; `Failed` means the worker took the request
/// but died or timed out
#[derive(Debug)]
pub enum WorkerError {
    Unavailable(String),
    Failed(String),
}

impl std::fmt::Display for WorkerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WorkerError::Unavailable(e) | WorkerError::Failed(e) => f.write_str(e),
        }
    }
}

/// One event of a streamed command: body chunks (the first may carry response head metadata), then
/// the final result
#[derive(Debug)]
pub enum StreamEvent {
    Chunk { meta: serde_json::Value, body: Vec<u8> },
    End(Reply),
}

enum PendingEntry {
    Once(oneshot::Sender<Reply>),
    Stream(tokio::sync::mpsc::UnboundedSender<StreamEvent>),
}

type Pending = Mutex<HashMap<u32, PendingEntry>>;

struct Handle {
    stdin: Arc<tokio::sync::Mutex<ChildStdin>>,
    child: tokio::sync::Mutex<Child>,
    pending: Arc<Pending>,
    alive: Arc<AtomicBool>,
    next_id: AtomicU32,
    inflight: AtomicUsize,
    last_used: Mutex<Instant>,
    limiter: Semaphore,
}

impl Handle {
    fn is_alive(&self) -> bool {
        self.alive.load(Ordering::SeqCst)
    }
}

type Slot = Arc<tokio::sync::Mutex<Option<Arc<Handle>>>>;

fn slots() -> &'static Mutex<HashMap<PathBuf, Slot>> {
    static SLOTS: OnceLock<Mutex<HashMap<PathBuf, Slot>>> = OnceLock::new();
    SLOTS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn slot_for(root: &Path) -> Slot {
    let mut map = slots().lock().unwrap_or_else(|e| e.into_inner());
    map.entry(root.to_path_buf()).or_default().clone()
}

pub fn enabled() -> bool {
    !matches!(std::env::var("COM_WORKER").as_deref(), Ok("0") | Ok("false") | Ok("off"))
}

fn env_number(name: &str, default: u64) -> u64 {
    std::env::var(name).ok().and_then(|v| v.trim().parse().ok()).unwrap_or(default)
}

/// Same environment the per-request runners received: PATH with Bun locations, project `.env*` files
pub fn apply_runner_env(cmd: &mut Command, root: &Path) {
    if let Ok(curr_path) = std::env::var("PATH") {
        let mut paths = vec![];
        if let Some(userprofile) = std::env::var_os("USERPROFILE") {
            paths.push(PathBuf::from(userprofile).join(".bun/bin").to_string_lossy().to_string());
        }
        if let Some(home) = std::env::var_os("HOME") {
            paths.push(PathBuf::from(home).join(".bun/bin").to_string_lossy().to_string());
        }
        #[cfg(unix)]
        {
            paths.push("/opt/homebrew/bin".to_string());
            paths.push("/usr/local/bin".to_string());
        }
        paths.push(curr_path);
        let sep = if cfg!(windows) { ";" } else { ":" };
        cmd.env("PATH", paths.join(sep));
    }

    // next.config `env` (once the config is loaded)
    if let Some(config) = crate::next_config::for_root(root) {
        for (k, v) in config["env"].as_object().into_iter().flatten() {
            if let Some(v) = v.as_str() {
                cmd.env(k, v);
            }
        }
    }
    // Next.js precedence; the process environment wins (inherited, so file values never override it)
    for (k, v) in crate::env::load_files(root) {
        if std::env::var_os(&k).is_none() {
            cmd.env(k, v);
        }
    }

    cmd.env("NODE_ENV", "development");
    // The dev server renders SQL (including errors) from the query log itself
    cmd.env("COM_DEV_QUERY_LOG", "1");
}

/// `instrumentation.(ts|js)` at the project root or in `src/`
pub fn instrumentation_file(root: &Path) -> Option<PathBuf> {
    [root.to_path_buf(), root.join("src")]
        .iter()
        .flat_map(|dir| ["ts", "js", "mjs"].map(|ext| dir.join(format!("instrumentation.{ext}"))))
        .find(|p| p.is_file())
}

/// Writes the worker and the runtimes it imports into `.nata/`
pub fn write_runtime_files(root: &Path) -> std::io::Result<PathBuf> {
    let nata_dir = root.join(".nata");
    std::fs::create_dir_all(&nata_dir)?;
    for (name, source) in [
        ("worker.ts", WORKER_SOURCE),
        ("ssr_runtime.ts", SSR_RUNTIME),
        ("route_handler_runtime.ts", ROUTE_RUNTIME),
        ("rpc_runtime.ts", RPC_RUNTIME),
        ("deps_bundler.ts", DEPS_BUNDLER),
        ("metadata_runtime.ts", METADATA_RUNTIME),
        ("rsc_runtime.ts", RSC_RUNTIME),
        ("rsc_ssr.ts", RSC_SSR),
        ("rsc_render.ts", RSC_RENDER),
        ("rsc_boundaries.js", RSC_BOUNDARIES),
    ] {
        let path = nata_dir.join(name);
        // Skip identical content so file watchers and Bun's cache see no change
        if std::fs::read_to_string(&path).ok().as_deref() != Some(source) {
            std::fs::write(&path, source)?;
        }
    }
    Ok(nata_dir.join("worker.ts"))
}

// ---------- framing ----------

fn encode_frame(kind: u8, id: u32, meta: &serde_json::Value, body: &[u8]) -> Vec<u8> {
    let meta_bytes = serde_json::to_vec(meta).unwrap_or_else(|_| b"{}".to_vec());
    let length = 1 + 4 + 4 + meta_bytes.len() + body.len();
    let mut frame = Vec::with_capacity(4 + length);
    frame.extend_from_slice(&(length as u32).to_le_bytes());
    frame.push(kind);
    frame.extend_from_slice(&id.to_le_bytes());
    frame.extend_from_slice(&(meta_bytes.len() as u32).to_le_bytes());
    frame.extend_from_slice(&meta_bytes);
    frame.extend_from_slice(body);
    frame
}

async fn read_frame(stdout: &mut ChildStdout) -> std::io::Result<(u8, u32, serde_json::Value, Vec<u8>)> {
    let mut len_buf = [0u8; 4];
    stdout.read_exact(&mut len_buf).await?;
    let length = u32::from_le_bytes(len_buf) as usize;
    let mut buf = vec![0u8; length];
    stdout.read_exact(&mut buf).await?;
    if length < 9 {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "short frame"));
    }
    let kind = buf[0];
    let id = u32::from_le_bytes([buf[1], buf[2], buf[3], buf[4]]);
    let meta_len = u32::from_le_bytes([buf[5], buf[6], buf[7], buf[8]]) as usize;
    let meta_end = (9 + meta_len).min(length);
    let meta = serde_json::from_slice(&buf[9..meta_end]).unwrap_or(serde_json::Value::Null);
    let body = buf[meta_end..].to_vec();
    Ok((kind, id, meta, body))
}

// ---------- process management ----------

async fn spawn(root: &Path) -> Result<Arc<Handle>, String> {
    let script_path = write_runtime_files(root).map_err(|e| format!("could not write worker: {e}"))?;
    // A new process has an empty module registry: server modules start again from version 0 state
    crate::module_graph::for_root(root).write().unwrap_or_else(|e| e.into_inner()).reset_server_layers();

    let mut cmd = Command::new(crate::rpc::find_bun_bin());
    // --smol: a more aggressive GC and smaller heap; dev traffic is light, memory matters more
    if !matches!(std::env::var("COM_WORKER_SMOL").as_deref(), Ok("0") | Ok("false")) {
        cmd.arg("--smol");
    }
    cmd.arg(&script_path)
        .current_dir(root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .kill_on_drop(true);
    apply_runner_env(&mut cmd, root);

    let mut child = cmd.spawn().map_err(|e| format!("could not spawn bun: {e}"))?;
    let stdin = child.stdin.take().ok_or("worker stdin unavailable")?;
    let mut stdout = child.stdout.take().ok_or("worker stdout unavailable")?;

    // Wait for the ready frame so startup failures surface here, not as a request timeout
    match tokio::time::timeout(STARTUP_TIMEOUT, read_frame(&mut stdout)).await {
        Ok(Ok((KIND_READY, _, _, _))) => {}
        Ok(Ok(_)) => return Err("worker sent an unexpected first frame".to_string()),
        Ok(Err(e)) => return Err(format!("worker exited during startup: {e}")),
        Err(_) => return Err("worker did not start in time".to_string()),
    }

    let pending: Arc<Pending> = Arc::new(Mutex::new(HashMap::new()));
    let alive = Arc::new(AtomicBool::new(true));
    let stdin = Arc::new(tokio::sync::Mutex::new(stdin));
    {
        let pending = pending.clone();
        let alive = alive.clone();
        let stdin = stdin.clone();
        let root = root.to_path_buf();
        tokio::spawn(async move {
            loop {
                match read_frame(&mut stdout).await {
                    Ok((KIND_RESULT, id, meta, body)) => {
                        let entry = pending.lock().unwrap_or_else(|e| e.into_inner()).remove(&id);
                        match entry {
                            Some(PendingEntry::Once(tx)) => {
                                let _ = tx.send(Reply { meta, body });
                            }
                            Some(PendingEntry::Stream(tx)) => {
                                let _ = tx.send(StreamEvent::End(Reply { meta, body }));
                            }
                            None => {}
                        }
                    }
                    Ok((KIND_CHUNK, id, meta, body)) => {
                        let map = pending.lock().unwrap_or_else(|e| e.into_inner());
                        if let Some(PendingEntry::Stream(tx)) = map.get(&id) {
                            let _ = tx.send(StreamEvent::Chunk { meta, body });
                        }
                    }
                    Ok((KIND_CALL, id, meta, body)) => {
                        let stdin = stdin.clone();
                        let root = root.clone();
                        tokio::spawn(async move {
                            let cmd = meta["cmd"].as_str().unwrap_or("").to_string();
                            let (reply_meta, reply_body) =
                                crate::module_server::handle_call(root, cmd, meta["input"].clone(), body).await;
                            let frame = encode_frame(KIND_REPLY, id, &reply_meta, &reply_body);
                            let mut stdin = stdin.lock().await;
                            let _ = stdin.write_all(&frame).await;
                            let _ = stdin.flush().await;
                        });
                    }
                    Ok(_) => {}
                    Err(_) => break,
                }
            }
            // Process gone: dropping the senders fails every waiting request
            alive.store(false, Ordering::SeqCst);
            pending.lock().unwrap_or_else(|e| e.into_inner()).clear();
        });
    }

    // instrumentation.ts: `register()` finishes before the worker takes requests
    if let Some(file) = instrumentation_file(root) {
        if let Some(specifier) = crate::module_graph::server_module_specifier(root, &file) {
            let id = u32::MAX;
            let (tx, rx) = oneshot::channel();
            pending.lock().unwrap_or_else(|e| e.into_inner()).insert(id, PendingEntry::Once(tx));
            let frame = encode_frame(KIND_REQUEST, id, &serde_json::json!({ "cmd": "instrumentation", "input": { "specifier": specifier } }), &[]);
            {
                let mut stdin = stdin.lock().await;
                let _ = stdin.write_all(&frame).await;
                let _ = stdin.flush().await;
            }
            match tokio::time::timeout(STARTUP_TIMEOUT, rx).await {
                Ok(Ok(reply)) => {
                    crate::ssr::worker::print_logs(reply.stdout(), reply.stderr());
                    if let Some(err) = reply.error() {
                        crate::term::error("instrumentation.ts register() failed", Some(err), None);
                    }
                }
                _ => crate::term::warn("instrumentation.ts register() did not finish in time"),
            }
        }
    }

    let concurrency = env_number("COM_WORKER_CONCURRENCY", 32).max(1) as usize;
    ensure_idle_reaper();
    Ok(Arc::new(Handle {
        stdin,
        child: tokio::sync::Mutex::new(child),
        pending,
        alive,
        next_id: AtomicU32::new(1),
        inflight: AtomicUsize::new(0),
        last_used: Mutex::new(Instant::now()),
        limiter: Semaphore::new(concurrency),
    }))
}

async fn shutdown(handle: Arc<Handle>) {
    let deadline = Instant::now() + DRAIN_TIMEOUT;
    while handle.inflight.load(Ordering::SeqCst) > 0 && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    {
        let mut stdin = handle.stdin.lock().await;
        let _ = stdin.write_all(&encode_frame(KIND_SHUTDOWN, 0, &serde_json::json!({}), &[])).await;
        let _ = stdin.flush().await;
    }
    let mut child = handle.child.lock().await;
    // Graceful exit lets PGlite flush; kill only if it hangs
    if tokio::time::timeout(SHUTDOWN_GRACE, child.wait()).await.is_err() {
        let _ = child.kill().await;
    }
}

/// Decrements the in-flight counter however the request ends
struct InflightGuard(Arc<Handle>);

impl Drop for InflightGuard {
    fn drop(&mut self) {
        self.0.inflight.fetch_sub(1, Ordering::SeqCst);
        *self.0.last_used.lock().unwrap_or_else(|e| e.into_inner()) = Instant::now();
    }
}

async fn acquire(root: &Path) -> Result<InflightGuard, WorkerError> {
    let slot = slot_for(root);
    let mut guard = slot.lock().await;
    let needs_spawn = guard.as_ref().map_or(true, |h| !h.is_alive());
    if needs_spawn {
        let crashed = guard.is_some();
        let handle = spawn(root).await.map_err(WorkerError::Unavailable)?;
        *guard = Some(handle);
        if crashed {
            crate::term::warn("Worker restarted");
        }
    }
    let handle = guard.as_ref().expect("worker started above").clone();
    // Counted while the slot is still locked, so a concurrent restart drains this request
    handle.inflight.fetch_add(1, Ordering::SeqCst);
    Ok(InflightGuard(handle))
}

/// Runs a worker command (`ssr`, `route`, `rpc`, `eval`), starting the worker if needed
pub async fn call(root: &Path, cmd: &str, input: serde_json::Value, body: Vec<u8>) -> Result<Reply, WorkerError> {
    if !enabled() {
        return Err(WorkerError::Unavailable("disabled by COM_WORKER=0".to_string()));
    }
    let guard = acquire(root).await?;
    let handle = guard.0.clone();
    let _permit = handle.limiter.acquire().await.map_err(|e| WorkerError::Failed(e.to_string()))?;

    let id = handle.next_id.fetch_add(1, Ordering::SeqCst);
    let (tx, rx) = oneshot::channel();
    handle.pending.lock().unwrap_or_else(|e| e.into_inner()).insert(id, PendingEntry::Once(tx));
    send_request(&handle, id, cmd, input, &body).await?;

    match tokio::time::timeout(REQUEST_TIMEOUT, rx).await {
        Ok(Ok(reply)) => Ok(reply),
        Ok(Err(_)) => Err(WorkerError::Failed("worker exited while handling the request".to_string())),
        Err(_) => {
            handle.pending.lock().unwrap_or_else(|e| e.into_inner()).remove(&id);
            Err(WorkerError::Failed(format!("request timed out after {}s", REQUEST_TIMEOUT.as_secs())))
        }
    }
}

async fn send_request(handle: &Arc<Handle>, id: u32, cmd: &str, input: serde_json::Value, body: &[u8]) -> Result<(), WorkerError> {
    let frame = encode_frame(KIND_REQUEST, id, &serde_json::json!({ "cmd": cmd, "input": input }), body);
    let mut stdin = handle.stdin.lock().await;
    let written = async {
        stdin.write_all(&frame).await?;
        stdin.flush().await
    }
    .await;
    if let Err(e) = written {
        handle.pending.lock().unwrap_or_else(|e| e.into_inner()).remove(&id);
        handle.alive.store(false, Ordering::SeqCst);
        return Err(WorkerError::Failed(format!("worker write failed: {e}")));
    }
    Ok(())
}

/// A streamed worker command: the receiver yields body chunks as the worker produces them, then the
/// final `Reply`. The worker stays counted as busy (so a restart drains it) until the stream ends.
pub struct WorkerStream {
    pub events: tokio::sync::mpsc::UnboundedReceiver<StreamEvent>,
    _guard: InflightGuard,
}

/// Runs a command whose output is streamed (`rsc`): see `WorkerStream`
pub async fn call_stream(root: &Path, cmd: &str, input: serde_json::Value, body: Vec<u8>) -> Result<WorkerStream, WorkerError> {
    if !enabled() {
        return Err(WorkerError::Unavailable("disabled by COM_WORKER=0".to_string()));
    }
    let guard = acquire(root).await?;
    let handle = guard.0.clone();
    let id = handle.next_id.fetch_add(1, Ordering::SeqCst);
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    handle.pending.lock().unwrap_or_else(|e| e.into_inner()).insert(id, PendingEntry::Stream(tx));
    send_request(&handle, id, cmd, input, &body).await?;
    Ok(WorkerStream { events: rx, _guard: guard })
}

/// Restarts the project's worker so changed modules are re-imported; running requests finish first
pub async fn restart(root: &Path) {
    if !enabled() {
        return;
    }
    let slot = slot_for(root);
    let mut guard = slot.lock().await;
    if let Some(old) = guard.take() {
        shutdown(old).await;
    }
    match spawn(root).await {
        Ok(handle) => *guard = Some(handle),
        Err(e) => crate::term::warn(&format!("Could not restart worker: {e}")),
    }
}

/// Starts the worker in the background so the first request does not pay the startup cost
pub fn prewarm(root: &Path) {
    if !enabled() {
        return;
    }
    let root = root.to_path_buf();
    tokio::spawn(async move {
        let slot = slot_for(&root);
        let mut guard = slot.lock().await;
        if guard.is_none() {
            match spawn(&root).await {
                Ok(handle) => *guard = Some(handle),
                Err(e) => crate::term::warn(&format!("Worker unavailable, falling back to per-request processes: {e}")),
            }
        }
    });
}

/// Stops workers that have been idle longer than `COM_WORKER_IDLE` seconds
fn ensure_idle_reaper() {
    static STARTED: OnceLock<()> = OnceLock::new();
    let idle_secs = env_number("COM_WORKER_IDLE", 600);
    if idle_secs == 0 || STARTED.set(()).is_err() {
        return;
    }
    let idle = Duration::from_secs(idle_secs);
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(15).min(idle)).await;
            let all: Vec<Slot> = slots().lock().unwrap_or_else(|e| e.into_inner()).values().cloned().collect();
            for slot in all {
                let Ok(mut guard) = slot.try_lock() else { continue };
                let expired = guard.as_ref().is_some_and(|h| {
                    h.inflight.load(Ordering::SeqCst) == 0
                        && h.last_used.lock().map(|t| t.elapsed() >= idle).unwrap_or(false)
                });
                if expired {
                    if let Some(handle) = guard.take() {
                        shutdown(handle).await;
                    }
                }
            }
        }
    });
}

// ---------- runner scripts (tests, fallback) ----------

/// One-off `bun -e <script>` (or Node as a last resort): the pre-worker behaviour, kept as a fallback
pub async fn spawn_once(root: &Path, script: &str) -> Result<ScriptOutput, String> {
    let _ = write_runtime_files(root);
    let mut cmd = Command::new(crate::rpc::find_bun_bin());
    cmd.arg("-e").arg(script).current_dir(root);
    apply_runner_env(&mut cmd, root);
    let output = match cmd.output().await {
        Ok(out) => out,
        Err(_) => Command::new("node")
            .arg("--input-type=module")
            .arg("-e")
            .arg(script)
            .current_dir(root)
            .output()
            .await
            .map_err(|e| format!("Neither bun nor node runtime could be spawned: {e}"))?,
    };
    Ok(ScriptOutput {
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        success: output.status.success(),
    })
}

/// Runs an arbitrary runner script: in the worker (`eval`) when available, else in a one-off process
pub async fn run(root: &Path, script: &str) -> Result<ScriptOutput, String> {
    match call(root, "eval", serde_json::json!({ "script": script }), Vec::new()).await {
        Ok(reply) => Ok(ScriptOutput {
            stdout: reply.stdout().to_string(),
            stderr: reply.stderr().to_string(),
            success: reply.code() == 0 && reply.error().is_none(),
        }),
        Err(WorkerError::Failed(e)) => Ok(ScriptOutput { stdout: String::new(), stderr: e, success: false }),
        Err(WorkerError::Unavailable(_)) => spawn_once(root, script).await,
    }
}
