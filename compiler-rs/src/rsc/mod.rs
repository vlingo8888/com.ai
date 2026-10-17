//! React Server Components pipeline (P1.5–P1.7): page requests, client-router navigations (`RSC: 1`)
//! and Server Actions are rendered by the worker (`rsc_runtime.ts` in the react-server layer, SSR in
//! `ssr_runtime.ts`) and streamed to the browser as they render: the response starts as soon as the
//! shell is ready, Suspense boundaries fill in afterwards.

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use axum::body::{Body, Bytes};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use serde_json::{json, Value};

use crate::resolver::Layer;
use crate::router::types::{RouteEntry, SegmentFiles};
use crate::worker_pool::{StreamEvent, WorkerError};

/// The browser runtime of the RSC router
pub const ROUTER_URL: &str = "/_nata/router.js";
pub const ROUTER_SOURCE: &str = include_str!("../client/router.js");

/// What the page request asks for
pub enum Mode {
    /// A full HTML document
    Html,
    /// The Flight payload only, for the client router
    Flight,
    /// A Server Action: `id` from `Next-Action` (client router), or `None` for a form posted without JS
    Action { id: Option<String>, content_type: String, body: Vec<u8> },
}

pub struct PageRequest<'a> {
    pub pathname: &'a str,
    /// Raw query string, without `?`
    pub query: &'a str,
    pub route: Option<&'a RouteEntry>,
    pub params: HashMap<String, String>,
    pub cookies: Option<String>,
    /// Segment keys the client router already has (`Nata-Router-State`)
    pub router_state: Vec<String>,
    pub mode: Mode,
    /// Root segment files for the not-found page when no route matched
    pub not_found_segments: Vec<SegmentFiles>,
}

/// `app/blog/[slug]` with `slug=hello` → `app/blog/hello`: identifies a segment's rendered output
pub fn segment_key(folder: &Path, params: &HashMap<String, String>) -> String {
    let folder = folder.to_string_lossy().replace('\\', "/");
    let parts: Vec<String> = folder
        .split('/')
        .map(|part| {
            let name = part.trim_start_matches('[').trim_end_matches(']').trim_start_matches("...");
            if part.starts_with('[') && part.ends_with(']') {
                params.get(name).cloned().unwrap_or_default()
            } else {
                part.to_string()
            }
        })
        .collect();
    parts.join("/")
}

fn query_json(query: &str) -> Value {
    let mut map = serde_json::Map::new();
    for pair in query.split('&').filter(|p| !p.is_empty()) {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        let k = crate::router::types::percent_decode(&k.replace('+', " "));
        let v = Value::String(crate::router::types::percent_decode(&v.replace('+', " ")));
        match map.get_mut(&k) {
            Some(Value::Array(list)) => list.push(v),
            Some(existing) => *existing = Value::Array(vec![existing.clone(), v]),
            None => {
                map.insert(k, v);
            }
        }
    }
    Value::Object(map)
}

fn abs(root: &Path, p: &Path) -> PathBuf {
    let joined = if p.is_absolute() { p.to_path_buf() } else { root.join(p) };
    joined.canonicalize().unwrap_or(joined)
}

/// The route as the worker's `buildModel` takes it: segment files as versioned `rsc:` specifiers
pub fn route_input(root: &Path, req: &PageRequest) -> Value {
    let graph = crate::module_graph::for_root(root);
    let graph = graph.read().unwrap_or_else(|e| e.into_inner());
    let spec = |p: &Option<PathBuf>| p.as_ref().map(|p| graph.specifier(&abs(root, p), Layer::Rsc));
    let segments_files = match req.route {
        Some(route) => &route.segments_files,
        None => &req.not_found_segments,
    };
    let segments: Vec<Value> = segments_files
        .iter()
        .map(|s| {
            json!({
                "key": segment_key(&s.folder, &req.params),
                "layout": spec(&s.layout),
                "template": spec(&s.template),
                "error": spec(&s.error),
                "loading": spec(&s.loading),
                "notFound": spec(&s.not_found),
            })
        })
        .collect();
    let keys: Vec<&str> = segments.iter().filter_map(|s| s["key"].as_str()).collect();
    // Per node, the URL segments below it: what useSelectedLayoutSegments() returns in its layout
    let own_segment: Vec<Option<String>> = segments_files
        .iter()
        .enumerate()
        .map(|(i, s)| {
            // The root folder adds no URL segment; route groups and parallel slots neither
            let folder_last = s.folder.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            if i == 0 || folder_last.starts_with('(') || folder_last.starts_with('@') {
                return None;
            }
            let key = segment_key(&s.folder, &req.params);
            key.rsplit('/').next().filter(|last| !last.is_empty()).map(|last| last.to_string())
        })
        .collect();
    let selected: Vec<Vec<String>> = (0..=own_segment.len())
        .map(|i| own_segment.iter().skip(i + 1).flatten().cloned().collect())
        .collect();
    // Layouts the client already shows come back empty; the page is always rendered
    let skip = req.router_state.iter().zip(keys.iter()).take_while(|(a, b)| a == b).count();
    let search = if req.query.is_empty() { String::new() } else { format!("?{}", req.query) };
    let params = req.route.map(|r| r.params_json(&req.params)).unwrap_or_else(|| json!({}));
    json!({
        "pathname": req.pathname,
        "search": search,
        "params": params,
        "searchParams": query_json(req.query),
        "segments": segments,
        "page": req.route.map(|r| graph.specifier(&abs(root, &r.page_file), Layer::Rsc)),
        "pageKey": format!("{}{}", req.route.map(|r| r.page_file.to_string_lossy().replace('\\', "/")).unwrap_or_default(), search),
        "globalError": req.route.and_then(|r| r.global_error_file.as_ref()).map(|p| graph.specifier(&abs(root, p), Layer::Rsc)),
        "skip": skip,
        "selected": selected,
        "notFoundAt": if req.route.is_none() { json!(0) } else { Value::Null },
    })
}

/// Module loading for Flight in the browser: client reference ids are module URLs, each its own chunk.
/// Defined before any module runs (the Flight client reads them when it loads).
const WEBPACK_GLOBALS: &str = "(function(){var m=self.__nata_modules=new Map();self.__webpack_chunk_load__=function(id){return import(id).then(function(x){m.set(id,x)})};self.__webpack_require__=function(id){return m.get(id)};self.__webpack_require__.u=function(c){return c};self.__webpack_get_script_filename__=function(c){return c}})();";

/// Everything the document needs before the page's own `<head>` content: styles, import map, config
pub fn head_start(stylesheet_tags: &str, target: &str) -> String {
    let import_map = serde_json::to_string(&crate::bundler::ClientTransformer::import_map("")).unwrap_or_else(|_| "{}".into());
    let config = serde_json::to_string(&json!({ "mode": "web", "rsc": true, "origin": "" }))
        .unwrap_or_else(|_| "{}".into())
        .replace("</", "<\\/");
    let backend = serde_json::to_string(&std::env::var("COM_BACKEND_URL").unwrap_or_default()).unwrap_or_default();
    let zmp = if target == "zalo" {
        format!("<script>{}</script>", crate::zmp::ZmpMockBridge::get_browser_mock_script())
    } else {
        String::new()
    };
    format!(
        "<meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
         <script type=\"importmap\">{import_map}</script>\
         <script>window.__NATA_CONFIG__={config};window.__COM_BACKEND_URL__={backend};self.__nata_f=self.__nata_f||[];{WEBPACK_GLOBALS}</script>\
         {zmp}{stylesheet_tags}"
    )
}

static FLIGHT_DEPS_READY: std::sync::OnceLock<tokio::sync::OnceCell<bool>> = std::sync::OnceLock::new();

/// Installs the packages React Server Components need in the background when `com dev` starts, so
/// the first page request does not wait for them
pub fn prewarm(root: &Path) {
    if !crate::module_graph::rsc_enabled() {
        return;
    }
    let root = root.to_path_buf();
    tokio::spawn(async move {
        let _ = ensure_flight_runtime(&root).await;
    });
}

/// The Flight runtime must match the project's React version exactly
async fn ensure_flight_runtime(root: &Path) -> Result<(), String> {
    let cell = FLIGHT_DEPS_READY.get_or_init(tokio::sync::OnceCell::new);
    let ok = *cell
        .get_or_init(|| async {
            let react = match crate::deps::ensure_installed(root, "react").await {
                Some(v) => v,
                None => return false,
            };
            // React 18 has no Flight runtime to install (the engine falls back at startup)
            if !react.starts_with(|c: char| c.is_ascii_digit()) || react.split('.').next().and_then(|m| m.parse::<u64>().ok()).is_some_and(|m| m < 19) {
                return false;
            }
            if crate::deps::ensure_installed(root, "react-dom").await.is_none() {
                return false;
            }
            // Fast Refresh is a dev nicety: without it, edits to client components reload the page
            if crate::module_graph::refresh_enabled() {
                let _ = crate::deps::ensure_dev_packages(root, &["react-refresh"]).await;
            }
            crate::deps::ensure_version(root, "react-server-dom-webpack", &react).await
        })
        .await;
    if ok {
        Ok(())
    } else {
        Err("React Server Components need react, react-dom and react-server-dom-webpack installed (bun add react react-dom react-server-dom-webpack)".into())
    }
}

fn error_page(status: StatusCode, message: &str) -> Response {
    let body = format!(
        "<!DOCTYPE html><html><head><meta charset=\"utf-8\"><title>Server Error</title></head><body style=\"font-family:system-ui,sans-serif;padding:2rem\"><h1>Server Error</h1><pre style=\"white-space:pre-wrap\">{}</pre><script type=\"module\" src=\"/_nata/overlay.js\"></script></body></html>",
        escape_html(message)
    );
    let mut response = (status, body).into_response();
    response.headers_mut().insert(header::CONTENT_TYPE, HeaderValue::from_static("text/html; charset=utf-8"));
    response
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn print_reply_logs(reply: &crate::worker_pool::Reply) {
    crate::ssr::worker::print_logs(reply.stdout(), reply.stderr());
}

/// Renders a page (or runs a Server Action) and streams the response
pub async fn handle(root: &Path, target: &str, stylesheet_tags: &str, req: PageRequest<'_>) -> Response {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    crate::rpc::RpcExecutor::ensure_runtime_env(&root);
    if let Err(message) = ensure_flight_runtime(&root).await {
        return error_page(StatusCode::INTERNAL_SERVER_ERROR, &message);
    }
    let _ = crate::worker_pool::write_runtime_files(&root);
    let runtime = crate::module_graph::for_root(&root)
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .specifier(&root.join(".nata/rsc_runtime.ts"), Layer::Rsc);

    let route = route_input(&root, &req);
    let (cmd, mode, action_id, content_type, body) = match req.mode {
        Mode::Html => ("rsc", "html", None, String::new(), Vec::new()),
        Mode::Flight => ("rsc", "flight", None, String::new(), Vec::new()),
        Mode::Action { id, content_type, body } => ("action", "html", id, content_type, body),
    };
    let input = json!({
        "runtime": runtime,
        "mode": mode,
        "route": route,
        "headStart": head_start(stylesheet_tags, target),
        "bootstrap": ROUTER_URL,
        "cookies": req.cookies,
        "actionId": action_id,
        "contentType": content_type,
    });

    let mut stream = match crate::worker_pool::call_stream(&root, cmd, input, body).await {
        Ok(stream) => stream,
        Err(WorkerError::Unavailable(e)) | Err(WorkerError::Failed(e)) => {
            return error_page(StatusCode::INTERNAL_SERVER_ERROR, &format!("The server worker is unavailable: {e}"))
        }
    };

    // The first event is the response head (status, headers), or the final result if it failed early
    let head = match stream.events.recv().await {
        Some(StreamEvent::Chunk { meta, .. }) => meta["head"].clone(),
        Some(StreamEvent::End(reply)) => {
            print_reply_logs(&reply);
            let message = reply.error().unwrap_or("The page did not render").to_string();
            let detail = format!("{message}\n\n{}", reply.stderr());
            return error_page(StatusCode::INTERNAL_SERVER_ERROR, detail.trim());
        }
        None => return error_page(StatusCode::INTERNAL_SERVER_ERROR, "The server worker exited while rendering"),
    };

    let status = head["status"].as_u64().and_then(|s| StatusCode::from_u16(s as u16).ok()).unwrap_or(StatusCode::OK);
    let mut builder = Response::builder().status(status);
    if let Some(headers) = head["headers"].as_object() {
        for (k, v) in headers {
            if let (Ok(name), Some(Ok(value))) = (header::HeaderName::from_bytes(k.as_bytes()), v.as_str().map(HeaderValue::from_str)) {
                builder = builder.header(name, value);
            }
        }
    }
    builder = builder.header(header::CACHE_CONTROL, "no-store, must-revalidate");

    let body_stream = futures_util::stream::unfold(stream, |mut stream| async move {
        loop {
            match stream.events.recv().await {
                Some(StreamEvent::Chunk { body, .. }) if !body.is_empty() => {
                    return Some((Ok::<Bytes, std::io::Error>(Bytes::from(body)), stream))
                }
                Some(StreamEvent::Chunk { .. }) => continue,
                Some(StreamEvent::End(reply)) => {
                    print_reply_logs(&reply);
                    if let Some(err) = reply.error() {
                        crate::term::error("Error while streaming the page", Some(err), None);
                    }
                    return None;
                }
                None => return None,
            }
        }
    });
    builder.body(Body::from_stream(body_stream)).unwrap_or_else(|_| error_page(StatusCode::INTERNAL_SERVER_ERROR, "Invalid response"))
}

/// Resolves a client reference id (a browser URL) to what the SSR layer imports
pub fn client_reference_specifier(root: &Path, id: &str) -> Result<String, String> {
    let clean = id.split('?').next().unwrap_or(id);
    if let Some(rel) = clean.strip_prefix("/_bundle/") {
        let file = abs(root, Path::new(rel));
        if !file.starts_with(root.canonicalize().unwrap_or_else(|_| root.to_path_buf())) || !file.is_file() {
            return Err(format!("client module not found: {clean}"));
        }
        let graph = crate::module_graph::for_root(root);
        let graph = graph.read().unwrap_or_else(|e| e.into_inner());
        return Ok(graph.specifier(&file, Layer::Ssr));
    }
    if clean == "/_nata/rsc/boundaries.js" {
        return Ok(root.join(".nata/rsc_boundaries.js").to_string_lossy().replace('\\', "/"));
    }
    if let Some(spec) = clean.strip_prefix("/_nata/deps/").or_else(|| clean.strip_prefix("/_nata/shims/")) {
        return Ok(spec.to_string());
    }
    Err(format!("unknown client reference: {clean}"))
}
