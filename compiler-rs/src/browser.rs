//! The engine as `com-compiler.wasm`: what the exports of `compiler-rs/wasm` (a WASI reactor) call.
//! The host (the bun-rust kernel in the browser, or any WASI runtime) keeps one instance alive:
//!
//! - One-shot commands, the same JSON as the native CLI's stdin/stdout: `es5`, `tailwind`, `routes`.
//! - `com dev` without a socket: `dev_start` prepares the project and the dev app; `request` queues a
//!   request for it. Both return a job id; the host drives the runtime with `tick` and collects the
//!   result with `job_take`. HMR messages (the `/_hmr` WebSocket natively) wait in `hmr_take`; file
//!   changes come in through `file_changed`, child process output through `wake` (platform/host.rs).
//!
//! Nothing here blocks: a job waiting on a child process parks until the host wakes it, so the host
//! can await the process (a worker) between ticks.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

use axum::body::Body;
use axum::http::{HeaderName, HeaderValue, Method, Request};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tower::ServiceExt;

use crate::platform::notify::{event, EventKind};

/// Polls of the scheduler per `tick`: enough for every ready task to run, without ever parking
/// (a park on WASI would block the host's thread until the next timer)
const TICK_YIELDS: usize = 64;

/// Body of a response that is not JSON
const MAX_BODY: usize = 256 * 1024 * 1024;

struct Engine {
    runtime: tokio::runtime::Runtime,
    /// Jobs hold `Rc`s (one thread): they run on this set, driven by `tick`
    local: tokio::task::LocalSet,
    jobs: Rc<RefCell<HashMap<u32, Option<Vec<u8>>>>>,
    next_job: u32,
    app: Rc<RefCell<Option<axum::Router>>>,
    hmr: Rc<RefCell<Option<broadcast::Receiver<String>>>>,
}

thread_local! {
    static ENGINE: RefCell<Option<Engine>> = const { RefCell::new(None) };
}

fn with_engine<R>(f: impl FnOnce(&mut Engine) -> R) -> R {
    ENGINE.with(|cell| {
        let mut slot = cell.borrow_mut();
        let engine = slot.get_or_insert_with(|| Engine {
            runtime: tokio::runtime::Builder::new_current_thread().enable_time().build().expect("tokio runtime"),
            local: tokio::task::LocalSet::new(),
            jobs: Rc::default(),
            next_job: 1,
            app: Rc::default(),
            hmr: Rc::default(),
        });
        f(engine)
    })
}

fn json_error(message: impl std::fmt::Display) -> Vec<u8> {
    serde_json::json!({ "error": message.to_string() }).to_string().into_bytes()
}

// ---------- one-shot commands ----------

/// `com-compiler es5`: `[{path, content}]` → `[{path, es5, error?}]`
pub fn es5(input: &[u8]) -> Vec<u8> {
    match serde_json::from_slice::<Vec<crate::swc_compiler::cjs::Es5Input>>(input) {
        Ok(files) => serde_json::to_vec(&crate::swc_compiler::cjs::compile_all_cjs(files)).unwrap_or_else(json_error),
        Err(e) => json_error(e),
    }
}

/// `com-compiler tailwind`: `{css, files: [{path, content}], minify}` → `{css}` or `{error}`
pub fn tailwind(input: &[u8]) -> Vec<u8> {
    #[derive(Deserialize)]
    struct Input {
        css: String,
        files: Vec<tailwind_rs::FileItem>,
        #[serde(default)]
        minify: bool,
    }
    let input: Input = match serde_json::from_slice(input) {
        Ok(input) => input,
        Err(e) => return json_error(e),
    };
    let tokens: Vec<String> = input
        .files
        .iter()
        .flat_map(|f| tailwind_rs::scanner::extract_file_candidates(&f.path, &f.content))
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    let output = match tailwind_rs::engine::compile_stylesheet(&input.css, &tokens, input.minify) {
        Ok(css) => serde_json::json!({ "css": css }),
        Err(error) => serde_json::json!({ "error": error }),
    };
    output.to_string().into_bytes()
}

/// `com-compiler routes`: the project directory → the App Router's routes
pub fn routes(dir: &[u8]) -> Vec<u8> {
    let dir = PathBuf::from(String::from_utf8_lossy(dir).into_owned());
    serde_json::to_vec(&crate::router::AppRouter::scan(&dir).routes).unwrap_or_else(json_error)
}

// ---------- com dev ----------

fn spawn_job(future: impl std::future::Future<Output = Vec<u8>> + 'static) -> u32 {
    with_engine(|engine| {
        let id = engine.next_job;
        engine.next_job = engine.next_job.wrapping_add(1).max(1);
        engine.jobs.borrow_mut().insert(id, None);
        let jobs = engine.jobs.clone();
        engine.local.spawn_local(async move {
            let result = future.await;
            jobs.borrow_mut().insert(id, Some(result));
        });
        id
    })
}

#[derive(Deserialize)]
struct DevOptions {
    dir: PathBuf,
    #[serde(default = "default_target")]
    target: String,
}

fn default_target() -> String {
    "web".into()
}

/// Prepares `com dev` for `{dir, target}`; the job's result is `{ok: true}` or `{error}`
pub fn dev_start(input: &[u8]) -> u32 {
    let options: Result<DevOptions, _> = serde_json::from_slice(input);
    let (app, hmr) = with_engine(|e| (e.app.clone(), e.hmr.clone()));
    spawn_job(async move {
        let options = match options {
            Ok(options) => options,
            Err(e) => return json_error(e),
        };
        crate::term::mark_start();
        let (router, hmr_tx) = crate::server::DevServer::start_in_host(options.dir, options.target).await;
        *hmr.borrow_mut() = Some(hmr_tx.subscribe());
        *app.borrow_mut() = Some(router);
        br#"{"ok":true}"#.to_vec()
    })
}

#[derive(Deserialize)]
struct RequestHead {
    method: String,
    url: String,
    #[serde(default)]
    headers: Vec<(String, String)>,
}

#[derive(Serialize)]
struct ResponseHead {
    status: u16,
    headers: Vec<(String, String)>,
}

/// `[u32 LE head length][head JSON][bytes]`: requests and responses in one buffer each
fn split_frame(frame: &[u8]) -> Option<(&[u8], &[u8])> {
    let len = u32::from_le_bytes(frame.get(..4)?.try_into().ok()?) as usize;
    let head = frame.get(4..4 + len)?;
    Some((head, &frame[4 + len..]))
}

fn frame(head: &[u8], body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + head.len() + body.len());
    out.extend_from_slice(&(head.len() as u32).to_le_bytes());
    out.extend_from_slice(head);
    out.extend_from_slice(body);
    out
}

fn error_response(status: u16, message: impl std::fmt::Display) -> Vec<u8> {
    let head = serde_json::to_vec(&ResponseHead { status, headers: vec![("content-type".into(), "text/plain; charset=utf-8".into())] }).unwrap();
    frame(&head, message.to_string().as_bytes())
}

/// A request for the dev app (a frame: head `{method, url, headers}`, then the body); the job's
/// result is the response frame (head `{status, headers}`, then the whole body)
pub fn request(input: &[u8]) -> u32 {
    let parsed = split_frame(input).and_then(|(head, body)| Some((serde_json::from_slice::<RequestHead>(head).ok()?, body.to_vec())));
    let app = with_engine(|e| e.app.clone());
    spawn_job(async move {
        let Some((head, body)) = parsed else { return error_response(400, "malformed request frame") };
        let Some(router) = app.borrow().clone() else { return error_response(503, "com dev is not started (dev_start)") };
        let mut builder = Request::builder().method(Method::from_bytes(head.method.as_bytes()).unwrap_or(Method::GET)).uri(&head.url);
        for (name, value) in &head.headers {
            if let (Ok(name), Ok(value)) = (HeaderName::from_bytes(name.as_bytes()), HeaderValue::from_str(value)) {
                builder = builder.header(name, value);
            }
        }
        let request = match builder.body(Body::from(body)) {
            Ok(request) => request,
            Err(e) => return error_response(400, e),
        };
        let response = match router.oneshot(request).await {
            Ok(response) => response,
            Err(e) => return error_response(500, e),
        };
        let (parts, body) = response.into_parts();
        let headers = parts.headers.iter().map(|(k, v)| (k.to_string(), String::from_utf8_lossy(v.as_bytes()).into_owned())).collect();
        let body = match axum::body::to_bytes(body, MAX_BODY).await {
            Ok(body) => body,
            Err(e) => return error_response(500, e),
        };
        frame(&serde_json::to_vec(&ResponseHead { status: parts.status.as_u16(), headers }).unwrap(), &body)
    })
}

/// Runs every task that can make progress, then returns: the number of finished jobs not taken yet
pub fn tick() -> u32 {
    with_engine(|engine| {
        engine.runtime.block_on(engine.local.run_until(async {
            for _ in 0..TICK_YIELDS {
                tokio::task::yield_now().await;
            }
        }));
        engine.jobs.borrow().values().filter(|r| r.is_some()).count() as u32
    })
}

/// The result of a finished job (and forgets it); `None` while it runs
pub fn job_take(id: u32) -> Option<Vec<u8>> {
    with_engine(|engine| {
        let mut jobs = engine.jobs.borrow_mut();
        match jobs.get(&id) {
            Some(Some(_)) => jobs.remove(&id).flatten(),
            _ => None,
        }
    })
}

/// HMR messages since the last call, as a JSON array of the messages (each itself JSON)
pub fn hmr_take() -> Vec<u8> {
    with_engine(|engine| {
        let mut messages = Vec::new();
        if let Some(rx) = engine.hmr.borrow_mut().as_mut() {
            loop {
                match rx.try_recv() {
                    Ok(message) => messages.push(message),
                    Err(broadcast::error::TryRecvError::Lagged(_)) => continue,
                    Err(_) => break,
                }
            }
        }
        format!("[{}]", messages.join(",")).into_bytes()
    })
}

/// `{path, kind}` with kind `create`, `modify` or `remove`: a change in the host's file system
pub fn file_changed(input: &[u8]) {
    #[derive(Deserialize)]
    struct Change {
        path: PathBuf,
        kind: String,
    }
    let Ok(change) = serde_json::from_slice::<Change>(input) else { return };
    let kind = match change.kind.as_str() {
        "create" => EventKind::Create(event::CreateKind::Any),
        "remove" => EventKind::Remove(event::RemoveKind::Any),
        _ => EventKind::Modify(event::ModifyKind::Any),
    };
    crate::platform::notify::dispatch(change.path, kind);
}

/// Output or exit of child process `pid` (platform/host.rs)
pub fn wake(pid: i32) {
    crate::platform::host::wake(pid);
}
