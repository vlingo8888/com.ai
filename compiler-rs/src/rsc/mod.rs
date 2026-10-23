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

/// Dev-only parts of the router: React Refresh, the error overlay, the debug drawer and the HMR socket
const ROUTER_DEV_IMPORTS: &str = "import \"./refresh.js\";\nimport { reportIssue, clearBlockingErrors } from \"./overlay.js\";\nimport { mountDebugUi, renderLogs } from \"./debug.js\";\n";
const ROUTER_PROD_STUBS: &str = "const reportIssue = (error) => console.error(error), clearBlockingErrors = () => {}, mountDebugUi = () => {}, renderLogs = () => {};\n";

/// The router for the current mode: in production without the dev-only modules (the bundler then
/// drops the HMR code, which nothing calls)
pub fn router_source() -> std::borrow::Cow<'static, str> {
    if !crate::mode::is_production() {
        return ROUTER_SOURCE.into();
    }
    ROUTER_SOURCE.replacen(ROUTER_DEV_IMPORTS, ROUTER_PROD_STUBS, 1).replacen("\nconnectHmr();\n", "\n", 1).into()
}

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
    /// Request headers (lower-case names), for headers()
    pub headers: HashMap<String, String>,
    /// Segment keys the client router already has (`Nata-Router-State`)
    pub router_state: Vec<String>,
    pub mode: Mode,
    /// Root segment files for the not-found page when no route matched
    pub not_found_segments: Vec<SegmentFiles>,
    /// Client navigations: the page the browser is on (`Nata-Url`) and its route, for intercepting
    /// routes
    pub navigated_from: Option<(String, RouteEntry, HashMap<String, String>)>,
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

/// The route as the worker's `buildModel` takes it: segment files as versioned `rsc:` specifiers,
/// with the parallel slots of each layout and, for client navigations, intercepting routes applied
pub fn route_input(root: &Path, req: &PageRequest) -> Value {
    let root_c = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let app_dir = ["app", "src/app"].iter().map(|d| root_c.join(d)).find(|d| d.is_dir()).unwrap_or_else(|| root_c.join("app"));
    let graph = crate::module_graph::for_root(root);
    let graph = graph.read().unwrap_or_else(|e| e.into_inner());
    let spec = |p: &Option<PathBuf>| p.as_ref().map(|p| graph.specifier(&abs(root, p), Layer::Rsc));

    // Soft navigation to an intercepted URL: render the intercepting page (in a slot of the page the
    // browser is on, or in place of the target)
    let interception = match (&req.mode, &req.navigated_from) {
        (Mode::Flight, Some((from, _, _))) => crate::router::parallel::find_interception(&root_c, &app_dir, req.pathname, from),
        _ => None,
    };
    let (route, params, pathname): (Option<&RouteEntry>, HashMap<String, String>, String) = match (&interception, &req.navigated_from) {
        (Some(i), Some((from, from_route, from_params))) if i.slot.is_some() => (Some(from_route), from_params.clone(), from.split('?').next().unwrap_or("/").to_string()),
        _ => (req.route, req.params.clone(), req.pathname.to_string()),
    };
    let mut segments_files: Vec<SegmentFiles> = match route {
        Some(route) => route.segments_files.clone(),
        None => req.not_found_segments.clone(),
    };
    let mut page = route.map(|r| r.page_file.clone());
    let mut page_params = params.clone();
    if let Some(i) = interception.as_ref().filter(|i| i.slot.is_none()) {
        // `(..)photo` inside `feed/`: the intercepting folders hang under the folder holding them
        let holder = i.sub.segments.first().map(|s| s.folder.parent().map(Path::to_path_buf).unwrap_or_default()).unwrap_or_default();
        segments_files.retain(|s| holder.starts_with(&s.folder));
        segments_files.extend(i.sub.segments.iter().cloned());
        page = Some(i.sub.page.clone());
        page_params = i.sub.params.clone();
    }

    let url_parts: Vec<&str> = pathname.split('/').filter(|p| !p.is_empty()).collect();
    let segment_json = |s: &SegmentFiles, key: String, slots: Value| {
        json!({
            "key": key,
            "layout": spec(&s.layout),
            "template": spec(&s.template),
            "error": spec(&s.error),
            "loading": spec(&s.loading),
            "notFound": spec(&s.not_found),
            "forbidden": spec(&s.forbidden),
            "unauthorized": spec(&s.unauthorized),
            "slots": slots,
        })
    };
    let sub_json = |sub: &crate::router::parallel::SubRoute| {
        json!({
            "segments": sub.segments.iter().map(|s| segment_json(s, String::new(), json!([]))).collect::<Vec<_>>(),
            "page": graph.specifier(&abs(root, &sub.page), Layer::Rsc),
            "params": sub.params,
        })
    };
    let segments: Vec<Value> = segments_files
        .iter()
        .map(|s| {
            let folder_abs = abs(root, &s.folder);
            let consumed = folder_abs.strip_prefix(&app_dir).map(|rel| {
                rel.iter().filter_map(|c| c.to_str()).filter(|c| !(c.starts_with('(') && c.ends_with(')')) && !c.starts_with('@')).count()
            }).unwrap_or(0).min(url_parts.len());
            let mut slots = crate::router::parallel::match_slots(&root_c, &folder_abs, &url_parts[consumed..]);
            if let Some((slot_folder, slot_name)) = interception.as_ref().and_then(|i| i.slot.as_ref()) {
                if abs(root, slot_folder) == folder_abs {
                    let sub = interception.as_ref().unwrap().sub.clone();
                    slots.retain(|(n, _)| n != slot_name);
                    slots.push((slot_name.clone(), sub));
                }
            }
            // The slots' pages are part of the layout's identity: a slot change re-renders it
            let mut key = segment_key(&s.folder, &params);
            for (name, sub) in &slots {
                key.push_str(&format!("|@{name}={}", sub.page.to_string_lossy()));
                for (k, v) in &sub.params {
                    key.push_str(&format!(";{k}={v}"));
                }
            }
            let slots_json: Vec<Value> = slots.iter().map(|(name, sub)| {
                let mut v = sub_json(sub);
                v["name"] = json!(name);
                v
            }).collect();
            segment_json(s, key, Value::Array(slots_json))
        })
        .collect();
    let keys: Vec<&str> = segments.iter().filter_map(|s| s["key"].as_str()).collect();
    // Layouts the client already shows come back empty; the page is always rendered
    let skip = req.router_state.iter().zip(keys.iter()).take_while(|(a, b)| a == b).count();
    let search = if req.query.is_empty() { String::new() } else { format!("?{}", req.query) };
    let params_value = match (route, interception.as_ref().filter(|i| i.slot.is_none())) {
        (_, Some(i)) => json!(i.sub.params),
        (Some(r), None) => r.params_json(&page_params),
        (None, None) => json!({}),
    };
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
            let key = segment_key(&s.folder, &params);
            key.rsplit('/').next().filter(|last| !last.is_empty()).map(|last| last.to_string())
        })
        .collect();
    let selected: Vec<Vec<String>> = (0..=own_segment.len()).map(|i| own_segment.iter().skip(i + 1).flatten().cloned().collect()).collect();
    json!({
        "pathname": req.pathname,
        "search": search,
        "params": params_value,
        "searchParams": query_json(req.query),
        "segments": segments,
        "page": page.as_ref().map(|p| graph.specifier(&abs(root, p), Layer::Rsc)),
        "pageKey": format!("{}{}", page.map(|p| p.to_string_lossy().replace('\\', "/")).unwrap_or_default(), search),
        "globalError": route.and_then(|r| r.global_error_file.as_ref()).map(|p| graph.specifier(&abs(root, p), Layer::Rsc)),
        "skip": skip,
        "selected": selected,
        "notFoundAt": if route.is_none() { json!(0) } else { Value::Null },
    })
}

/// Module loading for Flight in the browser: client reference ids are module URLs, each its own chunk.
/// Defined before any module runs (the Flight client reads them when it loads).
///
/// A chunk that fails to load in development is usually the dev server mid-rebuild (a plugin install
/// changes files in several waves and restarts the worker). The browser remembers a failed module for the
/// life of the document, so retrying `import` can't help: wait until the server serves the module AND has
/// been quiet for 3s (`/_nata/hmr_version` unchanged), then reload — at most 3 times a minute, so a real
/// error still shows in the overlay.
const WEBPACK_GLOBALS: &str = "(function(){var m=self.__nata_modules=new Map();var K='nata:chunk-reload';function recover(id,e){var s={};try{s=JSON.parse(sessionStorage.getItem(K)||'{}')||{}}catch(_){}if(!s.t||Date.now()-s.t>60000)s={n:0,t:Date.now()};if(s.n>=3)return Promise.reject(e);var tries=0,last=null,since=0;return new Promise(function(_,reject){(function poll(){Promise.all([fetch(id,{cache:'no-store'}),fetch('/_nata/hmr_version',{cache:'no-store'}).then(function(r){return r.json()})]).then(function(a){if(!a[0].ok)throw 0;var v=a[1]&&a[1].version;if(v!==last){last=v;since=Date.now();throw 0}if(Date.now()-since<3000)throw 0;s.n++;try{sessionStorage.setItem(K,JSON.stringify(s))}catch(_){}location.reload()}).catch(function(){if(++tries>120)reject(e);else setTimeout(poll,1000)})})()})}self.__webpack_chunk_load__=function(id){return import(id).then(function(x){m.set(id,x)},function(e){return recover(id,e)})};self.__webpack_require__=function(id){return m.get(id)};self.__webpack_require__.u=function(c){return c};self.__webpack_get_script_filename__=function(c){return c}})();";

/// Everything the document needs before the page's own `<head>` content: styles, import map, config
/// next/image settings shared by the server render and the browser (same srcset on both sides)
pub fn images_client_config(root: &Path) -> Value {
    let config = crate::next_config::for_root(root);
    let images = config.as_ref().map(|c| c["images"].clone()).unwrap_or(Value::Null);
    let base = crate::next_config::base_path(root);
    json!({
        "path": images["path"].as_str().map(String::from).unwrap_or_else(|| format!("{base}/_next/image")),
        "deviceSizes": images["deviceSizes"],
        "imageSizes": images["imageSizes"],
        "unoptimized": images["unoptimized"],
    })
}

pub fn head_start(root: &Path, stylesheet_tags: &str, target: &str) -> String {
    let build = crate::build::manifest::current();
    // Production build: bundles instead of the module URLs of development (no import map)
    let module_globals = match &build {
        Some(manifest) => crate::build::manifest::head_globals(manifest),
        None => {
            let import_map = serde_json::to_string(&crate::bundler::ClientTransformer::import_map("")).unwrap_or_else(|_| "{}".into());
            format!("<script type=\"importmap\">{import_map}</script><script>{WEBPACK_GLOBALS}</script>")
        }
    };
    // Links prefetch in production, like Next.js (in development only when asked)
    let config = serde_json::to_string(&json!({ "mode": "web", "rsc": true, "origin": "", "basePath": crate::next_config::base_path(root), "prefetch": build.is_some() }))
        .unwrap_or_else(|_| "{}".into())
        .replace("</", "<\\/");
    let images = serde_json::to_string(&images_client_config(root)).unwrap_or_else(|_| "{}".into()).replace("</", "<\\/");
    let backend = serde_json::to_string(&std::env::var("COM_BACKEND_URL").unwrap_or_default()).unwrap_or_default();
    let zmp = if target == "zalo" { crate::zmp::ZmpMockBridge::script_tag() } else { String::new() };
    format!(
        "<meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
         <script>window.__NATA_CONFIG__={config};window.__NATA_IMAGES__={images};window.__COM_BACKEND_URL__={backend};self.__nata_f=self.__nata_f||[];</script>\
         {module_globals}{zmp}{stylesheet_tags}"
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
        // next/og renders with Satori: install it when the app generates images
        if project_mentions(&root, &["next/og", "ImageResponse"]) {
            let _ = crate::deps::ensure_dev_packages(&root, &["satori"]).await;
        }
    });
}

/// Whether a source file of the app mentions one of `needles` (cheap scan of `app/` and `src/`)
fn project_mentions(root: &Path, needles: &[&str]) -> bool {
    ["app", "src"].iter().any(|dir| {
        walkdir::WalkDir::new(root.join(dir))
            .max_depth(12)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|x| matches!(x.to_str(), Some("ts" | "tsx" | "js" | "jsx"))))
            .any(|e| std::fs::read_to_string(e.path()).is_ok_and(|text| needles.iter().any(|n| text.contains(n))))
    })
}

/// The Flight runtime must match the project's React version exactly. Projects on an older React
/// use the framework's React 19 (installed once per machine, see framework_react.rs).
async fn ensure_flight_runtime(root: &Path) -> Result<(), String> {
    let cell = FLIGHT_DEPS_READY.get_or_init(tokio::sync::OnceCell::new);
    let ok = *cell
        .get_or_init(|| async {
            if crate::framework_react::needed(root) {
                return crate::framework_react::ensure(root).await;
            }
            let react = match crate::deps::ensure_installed(root, "react").await {
                Some(v) => v,
                None => return false,
            };
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
    match page_command(&root, target, stylesheet_tags, req).await {
        Ok((cmd, input, body)) => stream_response(&root, cmd, input, body).await,
        Err(message) => error_page(StatusCode::INTERNAL_SERVER_ERROR, &message),
    }
}

/// A page rendered to the end (prerendering, the page cache of `com start`)
pub struct Rendered {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    /// What the render depended on (see next/request.ts `RenderInfo`)
    pub dynamic: Option<String>,
    pub revalidate: Option<u64>,
    pub tags: Vec<String>,
}

impl Rendered {
    /// Can be served to every visitor: no request-time API, a cacheable status
    pub fn is_static(&self) -> bool {
        self.dynamic.is_none() && matches!(self.status, 200 | 404)
    }
}

/// Renders a page to the end instead of streaming it
pub async fn render_collect(root: &Path, target: &str, stylesheet_tags: &str, req: PageRequest<'_>) -> Result<Rendered, String> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let (cmd, input, body) = page_command(&root, target, stylesheet_tags, req).await?;
    let mut stream = crate::worker_pool::call_stream(&root, cmd, input, body).await.map_err(|e| match e {
        WorkerError::Unavailable(e) | WorkerError::Failed(e) => format!("The server worker is unavailable: {e}"),
    })?;
    let mut rendered = Rendered { status: 200, headers: Vec::new(), body: Vec::new(), dynamic: None, revalidate: None, tags: Vec::new() };
    let mut first = true;
    while let Some(event) = stream.events.recv().await {
        match event {
            StreamEvent::Chunk { meta, body } => {
                if first {
                    first = false;
                    let head = &meta["head"];
                    rendered.status = head["status"].as_u64().unwrap_or(200) as u16;
                    for (k, v) in head["headers"].as_object().into_iter().flatten() {
                        match v {
                            Value::Array(list) => rendered.headers.extend(list.iter().filter_map(|x| x.as_str()).map(|x| (k.clone(), x.to_string()))),
                            other => rendered.headers.extend(other.as_str().map(|x| (k.clone(), x.to_string()))),
                        }
                    }
                }
                rendered.body.extend_from_slice(&body);
            }
            StreamEvent::End(reply) => {
                print_reply_logs(&reply);
                if let Some(err) = reply.error() {
                    return Err(format!("{err}\n{}", reply.stderr()).trim().to_string());
                }
                let render = &reply.meta["render"];
                let force_static = render["forceStatic"].as_bool().unwrap_or(false);
                rendered.dynamic = if force_static { None } else { render["dynamic"].as_str().map(String::from) };
                rendered.revalidate = render["revalidate"].as_f64().filter(|s| *s > 0.0).map(|s| s.ceil() as u64);
                rendered.tags = render["tags"].as_array().into_iter().flatten().filter_map(|t| t.as_str().map(String::from)).collect();
                break;
            }
        }
    }
    if first {
        return Err("The server worker exited while rendering".to_string());
    }
    Ok(rendered)
}

/// `rsc:` specifier of the react-server runtime, its packages installed and runtime files written
pub async fn runtime_specifier(root: &Path) -> Result<String, String> {
    crate::rpc::RpcExecutor::ensure_runtime_env(root);
    ensure_flight_runtime(root).await?;
    let _ = crate::worker_pool::write_runtime_files(root);
    let graph = crate::module_graph::for_root(root);
    let graph = graph.read().unwrap_or_else(|e| e.into_inner());
    Ok(graph.specifier(&root.join(".nata/rsc_runtime.ts"), Layer::Rsc))
}

/// The worker command, input and body for a page request
async fn page_command(root: &Path, target: &str, stylesheet_tags: &str, req: PageRequest<'_>) -> Result<(&'static str, Value, Vec<u8>), String> {
    let root = root.to_path_buf();
    crate::rpc::RpcExecutor::ensure_runtime_env(&root);
    ensure_flight_runtime(&root).await?;
    let _ = crate::worker_pool::write_runtime_files(&root);
    let (runtime, ssr_runtime) = {
        let graph = crate::module_graph::for_root(&root);
        let graph = graph.read().unwrap_or_else(|e| e.into_inner());
        (graph.specifier(&root.join(".nata/rsc_runtime.ts"), Layer::Rsc), graph.specifier(&root.join(".nata/rsc_ssr.ts"), Layer::Ssr))
    };

    let route = route_input(&root, &req);
    let (cmd, mode, action_id, content_type, body) = match req.mode {
        Mode::Html => ("rsc", "html", None, String::new(), Vec::new()),
        Mode::Flight => ("rsc", "flight", None, String::new(), Vec::new()),
        Mode::Action { id, content_type, body } => ("action", "html", id, content_type, body),
    };
    let input = json!({
        "runtime": runtime,
        "ssrRuntime": ssr_runtime,
        "mode": mode,
        "route": route,
        "headStart": head_start(&root, stylesheet_tags, target),
        "images": images_client_config(&root),
        "basePath": crate::next_config::base_path(&root),
        "bootstrap": crate::build::manifest::current().map(|m| m.router.clone()).unwrap_or_else(|| ROUTER_URL.to_string()),
        "cookies": req.cookies,
        "headers": req.headers,
        "actionId": action_id,
        "contentType": content_type,
    });
    Ok((cmd, input, body))
}

/// Runs a streamed worker command (`rsc`, `action`, `route`) and turns it into an HTTP response: the
/// first chunk carries status and headers (a header may be a list, e.g. several `set-cookie`), the
/// following chunks are the body
pub async fn stream_response(root: &Path, cmd: &str, input: Value, body: Vec<u8>) -> Response {
    let mut stream = match crate::worker_pool::call_stream(root, cmd, input, body).await {
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
    let mut has_cache_control = false;
    if let Some(headers) = head["headers"].as_object() {
        for (k, v) in headers {
            let Ok(name) = header::HeaderName::from_bytes(k.as_bytes()) else { continue };
            has_cache_control |= name == header::CACHE_CONTROL;
            let values: Vec<&str> = match v {
                Value::Array(list) => list.iter().filter_map(|x| x.as_str()).collect(),
                other => other.as_str().into_iter().collect(),
            };
            for value in values {
                if let Ok(value) = HeaderValue::from_str(value) {
                    builder = builder.header(name.clone(), value);
                }
            }
        }
    }
    if !has_cache_control {
        builder = builder.header(header::CACHE_CONTROL, "no-store, must-revalidate");
    }

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
    // Fused: body layers (compression) may poll again after the end, which `unfold` panics on
    builder.body(Body::from_stream(futures_util::StreamExt::fuse(body_stream))).unwrap_or_else(|_| error_page(StatusCode::INTERNAL_SERVER_ERROR, "Invalid response"))
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
        // The instance rsc_ssr.ts imports (both are loaded in the SSR layer)
        let file = root.canonicalize().unwrap_or_else(|_| root.to_path_buf()).join(".nata/rsc_boundaries.js");
        let graph = crate::module_graph::for_root(root);
        return Ok(graph.read().unwrap_or_else(|e| e.into_inner()).specifier(&file, Layer::Ssr));
    }
    if let Some(spec) = clean.strip_prefix("/_nata/deps/").or_else(|| clean.strip_prefix("/_nata/shims/")) {
        // Packages load natively in the SSR layer, unless React comes from the framework: then they
        // load through the engine, which resolves `react` to it. Framework shims are files in .nata.
        if clean.starts_with("/_nata/shims/") || crate::framework_react::active_modules_dir(root).is_some() {
            let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
            // A client component inside a package is referenced by its file (client_reference_id), which
            // the package's `exports` may not expose (next-intl's NextIntlClientProvider)
            let path = crate::resolver::oxc::resolvers(&root)
                .resolve(Layer::Ssr, &root.join("package.json"), spec)
                .map(|r| r.path)
                .or_else(|| Some(crate::store::packages_dir(&root).join(spec)).filter(|f| !spec.split('/').any(|s| s == "..") && f.is_file()))
                .ok_or_else(|| format!("cannot resolve {spec}"))?;
            let graph = crate::module_graph::for_root(&root);
            return Ok(graph.read().unwrap_or_else(|e| e.into_inner()).specifier(&path, Layer::Ssr));
        }
        return Ok(spec.to_string());
    }
    Err(format!("unknown client reference: {clean}"))
}
