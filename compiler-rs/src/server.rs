use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, State,
    },
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{Html, IntoResponse},
    routing::{get, post},
    Json, Router,
};
use std::{collections::VecDeque, path::PathBuf, sync::Arc};
use tokio::sync::{broadcast, RwLock};
use tower_http::compression::Predicate;
use tower_http::cors::CorsLayer;

use crate::{
    bundler::ClientTransformer,
    router::AppRouter,
    rpc::{RpcExecutor, RpcPayload},
    ssr::{SsrEngine, SsrOutput, SsrRequest},
};

#[derive(Clone)]
pub struct AppState {
    pub root_dir: PathBuf,
    pub router: Arc<tokio::sync::RwLock<AppRouter>>,
    pub hmr_tx: broadcast::Sender<String>,
    pub target: String,
    pub query_logs: Arc<RwLock<VecDeque<serde_json::Value>>>,
    pub hmr_version: Arc<RwLock<HmrVersion>>,
}

/// Last file-change event, polled via `/_nata/hmr_version` by clients whose tunnel drops WebSockets
#[derive(Default, Clone, serde::Serialize)]
pub struct HmrVersion {
    pub version: u64,
    pub last_type: Option<String>,
    pub last_file: Option<String>,
}

/// Arguments and results shown in the dev panel's log: a big one (a list of thousands of rows) is
/// summarized, or the log (500 entries) and the HMR channel would hold megabytes of copies
fn log_preview(value: &serde_json::Value) -> serde_json::Value {
    const MAX_BYTES: usize = 8 * 1024;
    let text = value.to_string();
    if text.len() <= MAX_BYTES {
        return value.clone();
    }
    let mut end = MAX_BYTES;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    serde_json::Value::String(format!("{}… ({} KB, truncated in the dev log)", &text[..end], text.len() / 1024))
}

/// Mirrors watcher events (not query logs) from the HMR broadcast into `HmrVersion`
fn spawn_hmr_version_tracker(hmr_tx: &broadcast::Sender<String>, hmr_version: Arc<RwLock<HmrVersion>>) {
    let mut rx = hmr_tx.subscribe();
    tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(msg) => {
                    // Query logs (the most frequent, and the biggest) are not parsed
                    if msg.contains(r#""type":"query_log""#) {
                        continue;
                    }
                    let Ok(event) = serde_json::from_str::<serde_json::Value>(&msg) else { continue };
                    let event_type = event.get("type").and_then(|t| t.as_str()).unwrap_or("");
                    if event_type != "reload" && event_type != "RELOAD" && event_type != "css" {
                        continue;
                    }
                    let mut v = hmr_version.write().await;
                    v.version += 1;
                    v.last_type = Some(event_type.to_string());
                    v.last_file = event.get("file").and_then(|f| f.as_str()).map(|f| f.to_string());
                }
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    });
}

pub struct DevServer;

/// What the HTTP app serves
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ServeMode {
    /// `com dev`: everything, compiled on demand, with HMR
    Dev,
    /// Internal server of `com build`: modules compiled for production (the bundler and the
    /// prerenderer fetch from it), no HMR
    Build,
    /// `com start`: built assets and pages; modules are no longer compiled for the browser
    Start,
}

impl DevServer {
    pub async fn run(root_dir: PathBuf, initial_port: u16, target: String) -> Result<(), Box<dyn std::error::Error>> {
        let state = Self::prepare(root_dir, target).await;
        let root_dir = state.root_dir.clone();
        // Start native file system watcher for live HMR & CSS hot reload
        crate::watcher::ProjectWatcher::start(&root_dir, state.hmr_tx.clone(), state.router.clone());
        // Over a tunnel every byte counts (react-dom alone is ~590 KB). Only JS and CSS are compressed:
        // streamed pages, Flight payloads and Server Action responses must keep flushing chunk by chunk.
        // Brotli's default (quality 11) costs ~1s of CPU for react-dom; quality 4 is ~50x faster and
        // within ~15% of its size. Requests from this machine are not compressed at all (see below)
        let app = Self::app(state, ServeMode::Dev).layer(
            tower_http::compression::CompressionLayer::new().br(true).gzip(true).quality(tower_http::CompressionLevel::Precise(4)).compress_when(
                tower_http::compression::predicate::SizeAbove::new(1024).and(
                    |_: axum::http::StatusCode, _: axum::http::Version, headers: &HeaderMap, _: &axum::http::Extensions| {
                        headers.get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).is_some_and(|ct| {
                            ct.starts_with("application/javascript") || ct.starts_with("text/javascript") || ct.starts_with("text/css")
                        })
                    },
                ),
            ),
        )
        .layer(axum::middleware::map_request(skip_local_compression));

        // Auto-find next available port if in use
        let mut port = initial_port;
        let listener = loop {
            match tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port)).await {
                Ok(l) => {
                    if port != initial_port {
                        crate::term::warn(&format!("Port {} is in use, using {} instead", initial_port, port));
                    }
                    break l;
                }
                Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
                    port += 1;
                    if port > initial_port + 100 {
                        return Err(format!("Could not find an available port between {} and {}", initial_port, port).into());
                    }
                }
                Err(e) => return Err(e.into()),
            }
        };

        if std::env::var("COM_MANAGED").is_err() {
            println!("   - Local:        http://localhost:{}\n", port);
        }
        crate::term::ready();

        axum::serve(listener, app).await?;
        Ok(())
    }

    /// Setup shared by every mode: runtime files, the worker, next.config, the route table
    pub async fn prepare(root_dir: PathBuf, target: String) -> AppState {
        let root_dir = crate::rpc::canonicalize(&root_dir).unwrap_or(root_dir);
        if let Some(major) = crate::module_graph::check_react_support(&root_dir).await {
            crate::term::warn(&format!(
                "React {major} found and the framework's React {} could not be installed (offline?): Server Components, Server Actions and streaming need React 19, so pages render on the client. Upgrade with: bun add react@19 react-dom@19 react-server-dom-webpack@19",
                crate::framework_react::VERSION
            ));
        }
        // Pre-create .nata runtime and shim files
        RpcExecutor::ensure_runtime_env(&root_dir);
        // Start the Bun worker now so the first render / Server Action does not pay its startup
        crate::worker_pool::prewarm(&crate::rpc::dunce_canonicalize(&root_dir));
        crate::rsc::prewarm(&root_dir);
        {
            let root = root_dir.clone();
            tokio::spawn(async move {
                crate::next_config::ensure_loaded(&root).await;
            });
        }

        if target == "zalo" {
            let _ = crate::zmp::ZmpConfigGenerator::ensure_app_config(&root_dir);
            crate::zmp::ZmpConfigGenerator::ensure_hr_config(&root_dir);
        }

        let router = Arc::new(tokio::sync::RwLock::new(AppRouter::scan(&root_dir)));
        let (hmr_tx, _) = broadcast::channel::<String>(100);
        let query_logs = Arc::new(RwLock::new(VecDeque::with_capacity(500)));
        let hmr_version = Arc::new(RwLock::new(HmrVersion::default()));
        spawn_hmr_version_tracker(&hmr_tx, hmr_version.clone());

        AppState { root_dir, router, hmr_tx, target, query_logs, hmr_version }
    }

    pub fn app(state: AppState, mode: ServeMode) -> Router {
        let mut app = Router::new()
            .route("/_next/image", get(next_image_handler))
            .route("/_nata/fonts.css", get(|State(state): State<AppState>| async move {
                let (css, _) = crate::font_loader::stylesheet(&state.root_dir).await;
                ([(header::CONTENT_TYPE, "text/css; charset=utf-8"), (header::CACHE_CONTROL, "no-cache")], css)
            }))
            .route("/_nata/fonts/{*path}", get(|State(state): State<AppState>, Path(path): Path<String>| async move {
                crate::font_loader::serve(&state.root_dir, path.trim_start_matches('/')).await
            }))
            .route("/_nata/media/{*path}", get(|State(state): State<AppState>, Path(path): Path<String>| async move {
                crate::image_optimizer::media(&state.root_dir, path.trim_start_matches('/')).await
            }))
            .route("/favicon.ico", get(favicon_handler))
            .route("/robots.txt", get(robots_handler))
            .route("/sitemap.xml", get(sitemap_handler))
            .route("/manifest.webmanifest", get(manifest_handler))
            .route("/manifest.json", get(manifest_handler))
            .route("/icon", get(icon_handler))
            .route("/apple-icon", get(apple_icon_handler))
            .route("/opengraph-image", get(og_image_handler))
            .route("/twitter-image", get(twitter_image_handler))
            .route("/_nata/rpc", post(rpc_handler))
            .route("/app-config.json", get(zmp_app_config_handler))
            .route("/app.config.json", get(zmp_app_config_handler))
            .route("/hr.config.json", get(zmp_hr_config_handler))
            .route("/hrr.config.json", get(zmp_hr_config_handler))
            .route("/zmp.json", get(zmp_app_config_handler));

        if mode == ServeMode::Dev {
            app = app
                .route("/_hmr", get(ws_hmr_handler))
                .route("/_nata/hmr_version", get(hmr_version_handler))
                .route("/_nata/client.js", get(client_runtime_handler))
                .route("/_nata/overlay.js", get(overlay_handler))
                .route("/_nata/debug.js", get(|| async { javascript_response(crate::client::DEBUG_JS) }))
                .route("/_nata/logs", get(logs_handler).delete(clear_logs_handler))
                .route("/_nata/route_info", get(route_info_handler))
                .route("/_nata/styles.css", get(styles_handler))
                .route("/assets/app.css", get(zmp_css_handler))
                .route("/assets/app.js", get(zmp_js_handler))
                .route("/src/app.js", get(zmp_js_handler));
        }

        // Browser modules compiled on demand: what `com build` bundles. `com start` serves the bundles
        // (and never compiles project files for the browser)
        if mode != ServeMode::Start {
            app = app
                .route("/_nata/image-module/{*path}", get(image_module_handler))
                .route(
                    "/_nata/refresh.js",
                    get(|| async {
                        javascript_response(if crate::module_graph::refresh_enabled() {
                            crate::client::REFRESH_JS
                        } else {
                            "self.$RefreshReg$ = () => {};\nself.$RefreshSig$ = () => (type) => type;\n"
                        })
                    }),
                )
                .route(crate::rsc::ROUTER_URL, get(|| async { javascript_response(crate::rsc::router_source().into_owned()) }))
                .route("/_nata/rsc/boundaries.js", get(|| async { javascript_response(crate::worker_pool::RSC_BOUNDARIES) }))
                .route("/_nata/deps/{*spec}", get(deps_handler))
                .route("/_nata/css/{*path}", get(css_module_handler))
                .route("/_nata/shims/{*path}", get(shims_handler))
                .route("/_bundle/{*path}", get(bundle_handler));
        } else {
            app = app
                .route("/_nata/static/{*path}", get(crate::prod::static_handler))
                .route("/_nata/health", get(crate::prod::health_handler));
        }

        let app = app.fallback(app_router_fallback_handler).layer(axum::middleware::from_fn(log_request));
        // Dev: the Zalo preview and tunnels load from other origins. Production: same-origin only
        let app = if mode == ServeMode::Dev { app.layer(CorsLayer::permissive()) } else { app };
        app.with_state(state)
    }
}

/// Page and API requests only, like Next.js: dev internals, bundles and static files stay quiet
fn is_loggable_request(path: &str) -> bool {
    const INTERNAL: &[&str] = &["/_hmr", "/_nata/", "/_bundle/", "/assets/", "/src/app.js"];
    let last_segment = path.rsplit('/').next().unwrap_or("");
    !INTERNAL.iter().any(|p| path.starts_with(p)) && !last_segment.contains('.') && !path.contains('@')
}

async fn log_request(req: axum::extract::Request, next: axum::middleware::Next) -> axum::response::Response {
    let path = req.uri().path().to_string();
    if !is_loggable_request(&path) {
        return next.run(req).await;
    }
    let method = req.method().to_string();
    let started = std::time::Instant::now();
    let response = next.run(req).await;
    crate::term::request(&method, &path, response.status().as_u16(), started.elapsed().as_secs_f64() * 1000.0);
    response
}


/// A browser on this machine reads responses at memory speed: compressing them only costs CPU on every
/// reload. Requests through a tunnel or from the LAN (another Host, or forwarded) stay compressed
async fn skip_local_compression(mut req: axum::extract::Request) -> axum::extract::Request {
    if is_local_request(req.headers()) {
        req.headers_mut().remove(header::ACCEPT_ENCODING);
    }
    req
}

fn is_local_request(headers: &HeaderMap) -> bool {
    let forwarded = ["x-forwarded-for", "x-forwarded-host", "forwarded", "x-real-ip"].iter().any(|h| headers.contains_key(*h));
    let host = headers.get(header::HOST).and_then(|h| h.to_str().ok()).unwrap_or("");
    let name = match host.rsplit_once(':') {
        Some((name, port)) if !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit()) => name,
        _ => host,
    };
    !forwarded && matches!(name, "localhost" | "127.0.0.1" | "[::1]")
}

async fn ws_hmr_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_hmr_socket(socket, state.hmr_tx))
}

async fn handle_hmr_socket(mut socket: WebSocket, hmr_tx: broadcast::Sender<String>) {
    let mut rx = hmr_tx.subscribe();
    loop {
        match rx.recv().await {
            Ok(msg) => {
                if socket.send(Message::Text(msg.into())).await.is_err() {
                    break;
                }
            }
            Err(broadcast::error::RecvError::Lagged(_)) => continue,
            Err(broadcast::error::RecvError::Closed) => break,
        }
    }
}

async fn hmr_version_handler(State(state): State<AppState>) -> impl IntoResponse {
    let mut headers = HeaderMap::new();
    headers.insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    let snapshot = state.hmr_version.read().await.clone();
    (StatusCode::OK, headers, Json(snapshot)).into_response()
}

fn javascript_response(body: impl Into<axum::body::Body>) -> axum::response::Response {
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, "application/javascript; charset=utf-8".parse().unwrap());
    headers.insert(header::CACHE_CONTROL, "no-cache".parse().unwrap());
    (StatusCode::OK, headers, body.into()).into_response()
}

async fn client_runtime_handler() -> impl IntoResponse {
    javascript_response(crate::client::CLIENT_RUNTIME_JS)
}

async fn overlay_handler() -> impl IntoResponse {
    javascript_response(crate::client::DEV_OVERLAY_JS)
}

/// A CSS file imported by a component, as a JS module (see `stylesheet::css_module_js`)
async fn css_module_handler(State(state): State<AppState>, Path(path): Path<String>) -> impl IntoResponse {
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, "application/javascript; charset=utf-8".parse().unwrap());
    headers.insert(header::CACHE_CONTROL, "no-cache".parse().unwrap());
    let rel = path.trim_start_matches('/');
    match crate::css_compiler::stylesheet::css_module_js(&state.root_dir, rel) {
        Ok(js) => (StatusCode::OK, headers, js).into_response(),
        Err(message) => {
            crate::term::error(&format!("Could not load ./{rel}"), Some(&message), None);
            let js = format!(
                "const __e = new Error({});\nif (window.__NATA_REPORT_ISSUE__) window.__NATA_REPORT_ISSUE__(__e, \"runtime\");\nexport default {{}};\n",
                serde_json::to_string(&message).unwrap_or_default()
            );
            (StatusCode::OK, headers, js).into_response()
        }
    }
}

/// Browser build of an npm dependency (see `deps.rs`); revalidated with an ETag naming its version
async fn deps_handler(State(state): State<AppState>, Path(spec): Path<String>, req_headers: HeaderMap) -> impl IntoResponse {
    let spec = spec.trim_start_matches('/').trim_end_matches(".js");
    match crate::deps::resolve(&state.root_dir, spec).await {
        crate::deps::DepResult::Module { code, etag } => {
            let mut headers = HeaderMap::new();
            headers.insert(header::CONTENT_TYPE, "application/javascript; charset=utf-8".parse().unwrap());
            headers.insert(header::CACHE_CONTROL, "no-cache".parse().unwrap());
            if let Ok(value) = HeaderValue::from_str(&etag) {
                headers.insert(header::ETAG, value);
            }
            let not_modified = req_headers.get(header::IF_NONE_MATCH).and_then(|v| v.to_str().ok()) == Some(etag.as_str());
            if not_modified {
                return (StatusCode::NOT_MODIFIED, headers).into_response();
            }
            (StatusCode::OK, headers, code).into_response()
        }
        crate::deps::DepResult::Fallback(url) => axum::response::Redirect::temporary(&url).into_response(),
        crate::deps::DepResult::Error(message) => {
            crate::term::error(&format!("Could not prepare dependency {spec}"), Some(&message), None);
            // A module that reports through the dev overlay instead of an opaque network error
            let payload = serde_json::json!({ "message": message });
            let js = format!(
                "const __e = new Error({}.message);\nif (window.__NATA_REPORT_ISSUE__) window.__NATA_REPORT_ISSUE__(__e, \"runtime\");\nthrow __e;\n",
                payload
            );
            let mut headers = HeaderMap::new();
            headers.insert(header::CONTENT_TYPE, "application/javascript; charset=utf-8".parse().unwrap());
            (StatusCode::OK, headers, js).into_response()
        }
    }
}

async fn rpc_handler(
    State(state): State<AppState>,
    req_headers: HeaderMap,
    Json(mut payload): Json<RpcPayload>,
) -> impl IntoResponse {
    let mod_name = payload.module.clone();
    let action_name = payload.action.clone().unwrap_or_else(|| "default".to_string());
    let rpc_start = std::time::Instant::now();

    if payload.cookies.is_none() {
        if let Some(cookie_hdr) = req_headers.get("x-nata-cookies").or_else(|| req_headers.get(header::COOKIE)) {
            if let Ok(c_str) = cookie_hdr.to_str() {
                payload.cookies = Some(c_str.to_string());
            }
        }
    }

    let dev_panel = !crate::mode::is_production();
    match RpcExecutor::execute_full(&state.root_dir, payload.clone()).await {
        Ok(output) => {
            let rpc_duration = (rpc_start.elapsed().as_secs_f64() * 1000.0 * 100.0).round() / 100.0;
            let now_iso = format_iso_now();
            let rpc_id = format!("rpc_{:x}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos());
            let rpc_log = serde_json::json!({
                "id": rpc_id,
                "type": "rpc",
                "module": mod_name,
                "action": action_name,
                "parameters": payload.args.as_ref().map(log_preview),
                "response": log_preview(&output.data),
                "duration_ms": rpc_duration,
                "status": "success",
                "error": null,
                "timestamp": now_iso,
                "queries": output.queries
            });

            // Store logs into ring buffer
            // The dev panel's log: not kept (nor broadcast) under `com start`
            if dev_panel {
                let mut logs = state.query_logs.write().await;
                for q in &output.queries {
                    if logs.len() >= 500 {
                        logs.pop_front();
                    }
                    logs.push_back(q.clone());
                }
                if logs.len() >= 500 {
                    logs.pop_front();
                }
                logs.push_back(rpc_log.clone());
            }

            // Real-time broadcast to connected browsers
            let ws_event = serde_json::json!({
                "type": "query_log",
                "log": rpc_log,
                "queries": output.queries
            });
            if dev_panel {
                let _ = state.hmr_tx.send(ws_event.to_string());
            }

            let mut res_headers = HeaderMap::new();
            res_headers.insert(header::CONTENT_TYPE, "application/json; charset=utf-8".parse().unwrap());

            if !output.set_cookies.is_empty() {
                if let Ok(set_cookies_json) = serde_json::to_string(&output.set_cookies) {
                    if let Ok(val) = HeaderValue::from_str(&set_cookies_json) {
                        res_headers.insert("x-nata-set-cookies", val);
                    }
                }
                for sc in &output.set_cookies {
                    let mut cookie_hdr = format!("{}={}", escape_cookie_val(&sc.name), escape_cookie_val(&sc.value));
                    cookie_hdr.push_str(&format!("; Path={}", sc.path.as_deref().unwrap_or("/")));
                    if let Some(max_age) = sc.max_age {
                        cookie_hdr.push_str(&format!("; Max-Age={}", max_age));
                    }
                    if let Some(ref exp) = sc.expires {
                        cookie_hdr.push_str(&format!("; Expires={}", exp));
                    }
                    if let Some(ref dom) = sc.domain {
                        cookie_hdr.push_str(&format!("; Domain={}", dom));
                    }
                    if let Some(ref ss) = sc.same_site {
                        cookie_hdr.push_str(&format!("; SameSite={}", ss));
                    } else {
                        cookie_hdr.push_str("; SameSite=Lax");
                    }
                    if let Ok(hv) = HeaderValue::from_str(&cookie_hdr) {
                        res_headers.append(header::SET_COOKIE, hv);
                    }
                }
            }

            (StatusCode::OK, res_headers, Json(output.data)).into_response()
        }
        Err(err) => {
            let rpc_duration = (rpc_start.elapsed().as_secs_f64() * 1000.0 * 100.0).round() / 100.0;
            let now_iso = format_iso_now();
            let rpc_id = format!("rpc_{:x}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos());
            let rpc_log = serde_json::json!({
                "id": rpc_id,
                "type": "rpc",
                "module": mod_name.clone(),
                "action": action_name.clone(),
                "parameters": payload.args.as_ref().map(log_preview),
                "response": null,
                "duration_ms": rpc_duration,
                "status": "error",
                "error": err,
                "timestamp": now_iso,
                "queries": []
            });

            // The dev panel's log: not kept (nor broadcast) under `com start`
            if dev_panel {
                let mut logs = state.query_logs.write().await;
                if logs.len() >= 500 {
                    logs.pop_front();
                }
                logs.push_back(rpc_log.clone());
            }

            let ws_event = serde_json::json!({
                "type": "query_log",
                "log": rpc_log,
                "queries": []
            });
            if dev_panel {
                let _ = state.hmr_tx.send(ws_event.to_string());
            }

            // The failure itself was already printed by the RPC log block (`dev_log::render_rpc`)
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": err })),
            )
                .into_response()
        }
    }
}

async fn logs_handler(State(state): State<AppState>) -> impl IntoResponse {
    let logs = state.query_logs.read().await;
    let list: Vec<serde_json::Value> = logs.iter().cloned().collect();
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, "application/json; charset=utf-8".parse().unwrap());
    headers.insert(header::CACHE_CONTROL, "no-cache".parse().unwrap());
    (StatusCode::OK, headers, Json(serde_json::json!({ "logs": list }))).into_response()
}

async fn clear_logs_handler(State(state): State<AppState>) -> impl IntoResponse {
    let mut logs = state.query_logs.write().await;
    logs.clear();
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, "application/json; charset=utf-8".parse().unwrap());
    (StatusCode::OK, headers, Json(serde_json::json!({ "cleared": true }))).into_response()
}

fn format_iso_now() -> String {
    let now = std::time::SystemTime::now();
    let duration = now.duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    let secs = duration.as_secs();
    let millis = duration.subsec_millis();
    let days = secs / 86400;
    let rem_secs = secs % 86400;
    let hours = rem_secs / 3600;
    let mins = (rem_secs % 3600) / 60;
    let s = rem_secs % 60;

    let mut year = 1970;
    let mut day_count = days as i64;
    loop {
        let is_leap = (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0);
        let days_in_year = if is_leap { 366 } else { 365 };
        if day_count < days_in_year {
            break;
        }
        day_count -= days_in_year;
        year += 1;
    }
    let is_leap = (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0);
    let month_days = [
        31, if is_leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31
    ];
    let mut month = 1;
    for &d in &month_days {
        if day_count < d {
            break;
        }
        day_count -= d;
        month += 1;
    }
    let day = day_count + 1;
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z", year, month, day, hours, mins, s, millis)
}

fn escape_cookie_val(s: &str) -> String {
    s.replace(';', "%3B").replace(',', "%2C").replace(' ', "%20").replace('=', "%3D")
}

/// Root layout used to wrap the global not-found page (both SSR fallback and client navigation)
fn root_layout_files(router: &AppRouter) -> Vec<PathBuf> {
    router
        .routes
        .first()
        .and_then(|r| r.layout_files.first().cloned())
        .into_iter()
        .collect()
}

async fn route_info_handler(
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let path = params.get("path").cloned().unwrap_or_else(|| "/".to_string());
    let router_guard = state.router.read().await;

    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, "application/json; charset=utf-8".parse().unwrap());
    headers.insert(header::CACHE_CONTROL, "no-cache".parse().unwrap());

    if let Some((route, route_params)) = router_guard.match_route(&path) {
        let info = ClientTransformer::route_info_json(
            &route.page_file,
            &route.layout_files,
            &route.segments_files,
            route.global_error_file.as_ref(),
            &route.params_json(&route_params),
        );
        (StatusCode::OK, headers, Json(info)).into_response()
    } else {
        let not_found_file = router_guard
            .routes
            .iter()
            .find_map(|r| {
                r.segments_files.iter().find_map(|s| s.not_found.as_ref().map(|p| p.to_string_lossy().replace('\\', "/")))
            });
        let err_msg = if router_guard.routes.is_empty() {
            "Không tìm thấy cấu trúc App Router (thư mục 'app/' hoặc 'src/app/'). Hãy đảm bảo lệnh 'com dev' được chạy đúng bên trong thư mục dự án."
        } else {
            "Không tìm thấy trang cho đường dẫn này (404 Not Found)."
        };

        (
            StatusCode::NOT_FOUND,
            headers,
            Json(serde_json::json!({
                "found": false,
                "not_found_file": not_found_file,
                "layout_files": root_layout_files(&router_guard),
                "error": err_msg
            })),
        )
            .into_response()
    }
}

async fn styles_handler(
    State(state): State<AppState>,
) -> impl IntoResponse {
    // Compiled Tailwind stylesheet; with the browser fallback, the raw CSS for `text/tailwindcss`
    let css = crate::css_compiler::stylesheet::project_stylesheet(&state.root_dir).await.style_content();
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        "text/css; charset=utf-8".parse().unwrap(),
    );
    headers.insert(
        header::CACHE_CONTROL,
        "no-cache".parse().unwrap(),
    );
    (StatusCode::OK, headers, css).into_response()
}

async fn favicon_handler(
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Some((p, mime)) = crate::metadata::MetadataEngine::find_route_asset(&state.root_dir, "icon") {
        if let Ok(bytes) = tokio::fs::read(p).await {
            let mut headers = HeaderMap::new();
            headers.insert(header::CONTENT_TYPE, mime.parse().unwrap());
            return (StatusCode::OK, headers, bytes).into_response();
        }
    }

    let public_fav = state.root_dir.join("public").join("favicon.ico");
    let root_fav = state.root_dir.join("favicon.ico");
    let target = if public_fav.exists() {
        Some(public_fav)
    } else if root_fav.exists() {
        Some(root_fav)
    } else {
        None
    };

    if let Some(p) = target {
        if let Ok(bytes) = tokio::fs::read(p).await {
            let mut headers = HeaderMap::new();
            headers.insert(header::CONTENT_TYPE, "image/x-icon".parse().unwrap());
            return (StatusCode::OK, headers, bytes).into_response();
        }
    }

    let default_svg = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100"><text y=".9em" font-size="90">⚡</text></svg>"#;
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, "image/svg+xml".parse().unwrap());
    (StatusCode::OK, headers, default_svg.as_bytes().to_vec()).into_response()
}

async fn robots_handler(
    State(state): State<AppState>,
    req: axum::extract::Request,
) -> impl IntoResponse {
    let host = req.headers().get(header::HOST).and_then(|v| v.to_str().ok()).unwrap_or("localhost:3000");
    let base_url = format!("http://{}", host);
    let (status, headers, body) = crate::metadata::MetadataEngine::handle_robots(&state.root_dir, Some(&base_url)).await;
    (status, headers, body)
}

async fn sitemap_handler(
    State(state): State<AppState>,
    req: axum::extract::Request,
) -> impl IntoResponse {
    let host = req.headers().get(header::HOST).and_then(|v| v.to_str().ok()).unwrap_or("localhost:3000");
    let base_url = format!("http://{}", host);
    let (status, headers, body) = crate::metadata::MetadataEngine::handle_sitemap(&state.root_dir, Some(&base_url)).await;
    (status, headers, body)
}

async fn manifest_handler(
    State(state): State<AppState>,
) -> impl IntoResponse {
    let (status, headers, body) = crate::metadata::MetadataEngine::handle_manifest(&state.root_dir).await;
    (status, headers, body)
}

async fn icon_handler(State(state): State<AppState>) -> impl IntoResponse {
    if let Some((path, mime)) = crate::metadata::MetadataEngine::find_route_asset(&state.root_dir, "icon") {
        if let Ok(bytes) = tokio::fs::read(path).await {
            let mut headers = HeaderMap::new();
            headers.insert(header::CONTENT_TYPE, mime.parse().unwrap());
            return (StatusCode::OK, headers, bytes).into_response();
        }
    }
    StatusCode::NOT_FOUND.into_response()
}

async fn apple_icon_handler(State(state): State<AppState>) -> impl IntoResponse {
    if let Some((path, mime)) = crate::metadata::MetadataEngine::find_route_asset(&state.root_dir, "apple-icon") {
        if let Ok(bytes) = tokio::fs::read(path).await {
            let mut headers = HeaderMap::new();
            headers.insert(header::CONTENT_TYPE, mime.parse().unwrap());
            return (StatusCode::OK, headers, bytes).into_response();
        }
    }
    StatusCode::NOT_FOUND.into_response()
}

async fn og_image_handler(State(state): State<AppState>) -> impl IntoResponse {
    if let Some((path, mime)) = crate::metadata::MetadataEngine::find_route_asset(&state.root_dir, "opengraph-image") {
        if let Ok(bytes) = tokio::fs::read(path).await {
            let mut headers = HeaderMap::new();
            headers.insert(header::CONTENT_TYPE, mime.parse().unwrap());
            return (StatusCode::OK, headers, bytes).into_response();
        }
    }
    StatusCode::NOT_FOUND.into_response()
}

async fn twitter_image_handler(State(state): State<AppState>) -> impl IntoResponse {
    if let Some((path, mime)) = crate::metadata::MetadataEngine::find_route_asset(&state.root_dir, "twitter-image") {
        if let Ok(bytes) = tokio::fs::read(path).await {
            let mut headers = HeaderMap::new();
            headers.insert(header::CONTENT_TYPE, mime.parse().unwrap());
            return (StatusCode::OK, headers, bytes).into_response();
        }
    }
    StatusCode::NOT_FOUND.into_response()
}

async fn zmp_app_config_handler(State(state): State<AppState>) -> impl IntoResponse {
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, "application/json; charset=utf-8".parse().unwrap());
    headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*".parse().unwrap());

    let candidates = ["app-config.json", "zmp.json", "app.config.json"];
    for cand in &candidates {
        let p = state.root_dir.join(cand);
        if p.exists() {
            if let Ok(content) = tokio::fs::read_to_string(&p).await {
                if let Ok(mut json_val) = serde_json::from_str::<serde_json::Value>(&content) {
                    // Force full screen without header and preserve bottom safe area
                    if let Some(app_obj) = json_val.get_mut("app").and_then(|v| v.as_object_mut()) {
                        app_obj.insert("actionBarHidden".to_string(), serde_json::json!(true));
                        app_obj.insert("statusBar".to_string(), serde_json::json!("transparent"));
                        app_obj.insert("hideIOSSafeAreaBottom".to_string(), serde_json::json!(false));
                        app_obj.insert("hideAndroidBottomNavigationBar".to_string(), serde_json::json!(false));
                    }
                    if let Ok(serialized) = serde_json::to_string_pretty(&json_val) {
                        return (StatusCode::OK, headers, serialized).into_response();
                    }
                }
                return (StatusCode::OK, headers, content).into_response();
            }
        }
    }

    let mut default_config = crate::zmp::ZmpConfigGenerator::ensure_app_config(&state.root_dir).unwrap_or_default();
    default_config.app.action_bar_hidden = true;
    default_config.app.hide_ios_safe_area_bottom = false;
    default_config.app.hide_android_bottom_navigation_bar = false;
    default_config.app.status_bar = "transparent".to_string();
    let json_str = serde_json::to_string_pretty(&default_config).unwrap_or_else(|_| "{}".to_string());
    (StatusCode::OK, headers, json_str).into_response()
}

async fn zmp_hr_config_handler(State(state): State<AppState>) -> impl IntoResponse {
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, "application/json; charset=utf-8".parse().unwrap());
    headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*".parse().unwrap());
    headers.insert(header::ACCESS_CONTROL_ALLOW_METHODS, "GET, HEAD, OPTIONS".parse().unwrap());

    // 1. Check hrr.config.json or hr.config.json on disk
    let hrr_path = state.root_dir.join("hrr.config.json");
    if hrr_path.exists() {
        if let Ok(content) = tokio::fs::read_to_string(&hrr_path).await {
            return (StatusCode::OK, headers, content).into_response();
        }
    }
    let hr_path = state.root_dir.join("hr.config.json");
    if hr_path.exists() {
        if let Ok(content) = tokio::fs::read_to_string(&hr_path).await {
            return (StatusCode::OK, headers, content).into_response();
        }
    }

    // 2. Otherwise, check app-config.json for listCSS and listJS / listSyncJS
    let mut list_css = Vec::new();
    let mut list_js = Vec::new();

    let app_config_path = state.root_dir.join("app-config.json");
    if app_config_path.exists() {
        if let Ok(content) = tokio::fs::read_to_string(&app_config_path).await {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(css_arr) = parsed.get("listCSS").and_then(|v| v.as_array()) {
                    for item in css_arr {
                        if let Some(s) = item.as_str() {
                            let src = if s.starts_with('/') { s.to_string() } else { format!("/{}", s) };
                            list_css.push(serde_json::json!({ "src": src }));
                        } else if item.is_object() {
                            list_css.push(item.clone());
                        }
                    }
                }
                if let Some(js_arr) = parsed.get("listSyncJS").or_else(|| parsed.get("listJS")).and_then(|v| v.as_array()) {
                    for item in js_arr {
                        if let Some(s) = item.as_str() {
                            let src = if s.starts_with('/') { s.to_string() } else { format!("/{}", s) };
                            list_js.push(serde_json::json!({ "src": src, "type": "text/javascript", "async": true }));
                        } else if item.is_object() {
                            list_js.push(item.clone());
                        }
                    }
                }
            }
        }
    }

    if list_css.is_empty() {
        list_css.push(serde_json::json!({ "src": "/assets/app.css" }));
    }

    if list_js.is_empty() {
        list_js.push(serde_json::json!({
            "src": "/assets/app.js",
            "type": "text/javascript",
            "async": true
        }));
    }

    let response_json = serde_json::json!({
        "listCSS": list_css,
        "listJS": list_js
    });

    (StatusCode::OK, headers, response_json.to_string()).into_response()
}

async fn zmp_css_handler(State(state): State<AppState>) -> impl IntoResponse {
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, "text/css; charset=utf-8".parse().unwrap());
    headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*".parse().unwrap());

    // If assets/app.css exists on disk, serve it
    let file_path = state.root_dir.join("assets/app.css");
    if file_path.exists() {
        if let Ok(content) = tokio::fs::read_to_string(&file_path).await {
            return (StatusCode::OK, headers, content).into_response();
        }
    }

    // Otherwise serve compiled custom CSS (Tailwind / globals.css)
    let stylesheet = crate::css_compiler::stylesheet::project_stylesheet(&state.root_dir).await;
    (StatusCode::OK, headers, stylesheet.css().to_string()).into_response()
}

async fn zmp_js_handler(State(state): State<AppState>) -> impl IntoResponse {
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, "application/javascript; charset=utf-8".parse().unwrap());
    headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*".parse().unwrap());

    // 1. If assets/app.js or src/app.js exists on disk, serve it
    for cand in &["assets/app.js", "src/app.js", "src/app.ts", "app.js"] {
        let p = state.root_dir.join(cand);
        if p.exists() {
            if let Ok(content) = tokio::fs::read_to_string(&p).await {
                return (StatusCode::OK, headers, content).into_response();
            }
        }
    }

    // 2. Otherwise serve the bootstrap that loads the shared client runtime in Zalo (memory router) mode
    let backend_url = std::env::var("COM_BACKEND_URL").unwrap_or_default();
    let compiled_css = crate::css_compiler::stylesheet::project_stylesheet(&state.root_dir).await.is_compiled();
    (StatusCode::OK, headers, zmp_bootstrap_js(&backend_url, !compiled_css)).into_response()
}

/// Classic script loaded by the Zalo container (via `hr.config.json`). The container owns the HTML page,
/// so this injects the import map, styles and dev tooling, then boots `/_nata/client.js` client-only.
fn zmp_bootstrap_js(backend_url: &str, tailwind_in_browser: bool) -> String {
    let bootstrap_js = r###"// Com.AI.VN Zalo Mini App bootstrap (client-side rendering only)
(function() {
  // 1. Resolve Server Origin accurately from script tag URL
  var currentScript = document.currentScript || document.querySelector('script[src*="app.js"]');
  var scriptOrigin = currentScript ? new URL(currentScript.src, window.location.href).origin : "";
  var configuredBackend = "__COM_BACKEND_PLACEHOLDER__";
  var backendOrigin = (configuredBackend && !configuredBackend.startsWith("__") ? configuredBackend : (window.__COM_BACKEND_URL__ || ""));
  var serverOrigin = (scriptOrigin && !scriptOrigin.includes("h5.zdn.vn")) ? scriptOrigin : (backendOrigin || (window.location.origin && !window.location.origin.includes("h5.zdn.vn") && !window.location.origin.startsWith("file:") ? window.location.origin : ""));
  if (!backendOrigin && serverOrigin) {
    backendOrigin = serverOrigin;
  }
  window.__COM_BACKEND_URL__ = backendOrigin;

  // 2. Dismiss Zalo loading indicator
  function dismissLoading() {
    try {
      if (window.ZaloJavaScriptBridge && typeof window.ZaloJavaScriptBridge.closeLoading === 'function') {
        window.ZaloJavaScriptBridge.closeLoading();
      }
    } catch(e) {}
  }
  dismissLoading();

  // 3. Hook fetch to automatically resolve relative URLs (e.g. /_nata/rpc, /api) against backendOrigin
  if (backendOrigin && !window.__nata_fetch_hooked) {
    window.__nata_fetch_hooked = true;
    var originalFetch = window.fetch;
    // "//host/x" is another site without its scheme (zmp-sdk logs to //h5.zalo.me/log/…), not a path of the app
    var appPath = function(u) { return u.charAt(0) === "/" && u.charAt(1) !== "/"; };
    window.fetch = function(input, init) {
      if (typeof input === "string" && appPath(input)) {
        input = backendOrigin + input;
      } else if (input && typeof input.url === "string" && appPath(input.url)) {
        input = new Request(backendOrigin + input.url, input);
      }
      return originalFetch.call(this, input, init);
    };
  }

  // 4. Mobile DevTools (Eruda) for on-device debugging: ~500 KB from a CDN, so only on request
  // (COM_ERUDA=1 / `com dev --eruda`, or `?eruda` in the page URL)
  var wantEruda = __COM_ERUDA__ || /[?&]eruda\b/.test(window.location.search);
  if (wantEruda && !window.__eruda_injected) {
    window.__eruda_injected = true;
    var erudaScript = document.createElement("script");
    erudaScript.src = "https://cdn.jsdelivr.net/npm/eruda";
    erudaScript.onload = function() {
      if (window.eruda) {
        window.eruda.init({ tool: ["console", "network", "elements", "resources", "info"] });
        // Eruda replaces window.fetch; re-wrap it so the overlay still attributes network errors
        if (typeof window.__NATA_INSTRUMENT_FETCH__ === "function") window.__NATA_INSTRUMENT_FETCH__();
        console.log("[ZaloMiniApp] Eruda Mobile Console ready on device");
      }
    };
    document.head.appendChild(erudaScript);
  }

  // 5. Inject Stylesheet if absent
  if (!document.getElementById("_zmp_styles") && serverOrigin) {
    var link = document.createElement("link");
    link.id = "_zmp_styles";
    link.rel = "stylesheet";
    link.href = serverOrigin + "/assets/app.css";
    document.head.appendChild(link);
  }

  // 5b. Inject Safe-Area Bottom Protection Rules (Full screen & né bottom bar)
  if (!document.getElementById("_zmp_safe_area")) {
    var safeAreaStyle = document.createElement("style");
    safeAreaStyle.id = "_zmp_safe_area";
    safeAreaStyle.textContent = [
      ":root {",
      "  --sat: env(safe-area-inset-top, 0px);",
      "  --sab: env(safe-area-inset-bottom, 24px);",
      "  --sal: env(safe-area-inset-left, 0px);",
      "  --sar: env(safe-area-inset-right, 0px);",
      "}",
      "html, body {",
      "  margin: 0 !important;",
      "  padding: 0 !important;",
      "  width: 100% !important;",
      "  min-height: 100vh !important;",
      "  overflow-x: hidden !important;",
      "}",
      "#app, #root {",
      "  min-height: 100vh !important;",
      "  width: 100% !important;",
      "  box-sizing: border-box !important;",
      "  padding-bottom: max(env(safe-area-inset-bottom, 0px), 24px) !important;",
      "}"
    ].join("\n");
    document.head.appendChild(safeAreaStyle);
  }

  // 6. Tailwind browser engine, only when the stylesheet could not be compiled on the server
  if (__COM_TAILWIND_BROWSER__ && !document.getElementById("_zmp_tailwind")) {
    var tw = document.createElement("script");
    tw.id = "_zmp_tailwind";
    tw.src = "https://cdn.jsdelivr.net/npm/@tailwindcss/browser@4";
    document.head.appendChild(tw);
  }

  // 7. Inject Import Map before any module is resolved
  if (!document.querySelector('script[type="importmap"]')) {
    var im = document.createElement("script");
    im.type = "importmap";
    im.textContent = JSON.stringify(__COM_IMPORT_MAP_PLACEHOLDER__);
    document.head.appendChild(im);
  }

  // 8. Boot the shared App Router runtime in client-only Zalo mode
  function bootApp() {
    if (!document.getElementById("app") && !document.getElementById("root")) {
      var container = document.createElement("div");
      container.id = "app";
      document.body.appendChild(container);
    }
    var rsc = __COM_RSC__;
    window.__NATA_CONFIG__ = { mode: "zalo", ssr: false, rsc: rsc, origin: serverOrigin, initial: null, start: __COM_ZALO_START__ };
    if (rsc) {
      // React Server Components router: client references are module URLs on the dev server
      var modules = self.__nata_modules = new Map();
      self.__webpack_chunk_load__ = function(id) { return import(serverOrigin + id).then(function(m) { modules.set(id, m); }); };
      self.__webpack_require__ = function(id) { return modules.get(id); };
      self.__webpack_require__.u = function(c) { return c; };
      self.__webpack_get_script_filename__ = function(c) { return serverOrigin + c; };
    }
    // The overlay loads first so a runtime that cannot load (tunnel/CDN down) is still reported on screen
    import(serverOrigin + "/_nata/overlay.js").then(function(overlay) {
      return import(serverOrigin + (rsc ? "/_nata/router.js" : "/_nata/client.js")).catch(function(err) {
        overlay.reportIssue(err, "runtime");
      });
    }).catch(function(err) {
      console.error("[Com.AI.VN] Could not load the dev overlay:", err);
    }).then(dismissLoading);
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", bootApp);
  } else {
    bootApp();
  }
})();
"###;
    // Shims must point at the dev server, which the bootstrap only learns at runtime
    let import_map = ClientTransformer::import_map("__COM_SERVER_ORIGIN__");
    let import_map_js = serde_json::to_string(&import_map)
        .unwrap_or_else(|_| "{}".to_string())
        .replace("\"__COM_SERVER_ORIGIN__", "serverOrigin + \"");
    bootstrap_js
        .replace("__COM_BACKEND_PLACEHOLDER__", backend_url)
        .replace("__COM_IMPORT_MAP_PLACEHOLDER__", &import_map_js)
        .replace("__COM_TAILWIND_BROWSER__", if tailwind_in_browser { "true" } else { "false" })
        .replace("__COM_ERUDA__", if std::env::var("COM_ERUDA").is_ok_and(|v| v == "1" || v == "true") { "true" } else { "false" })
        .replace("__COM_RSC__", if crate::module_graph::rsc_enabled() { "true" } else { "false" })
        // `com dev --target zalo --start /staff`: the page the mini app opens on (one project, several mini apps)
        .replace(
            "__COM_ZALO_START__",
            &serde_json::to_string(&std::env::var("COM_ZALO_START").ok().filter(|p| p.starts_with('/')).unwrap_or_else(|| "/".into())).unwrap_or_else(|_| "\"/\"".into()),
        )
}

async fn shims_handler(
    Path(path): Path<String>,
) -> impl IntoResponse {
    let clean = path.trim_start_matches('/').trim_end_matches(".js");
    let js_code = match clean {
        "core" => {
            r##"export class HttpError extends Error {
  constructor(message, statusCode = 400) {
    super(message);
    this.name = "HttpError";
    this.statusCode = statusCode;
  }
}
export const pubsub = {
  subscribe: (channel, listener) => () => {},
  publish: async (channel, event, data) => {}
};
export const upload = async (file, options = {}) => ({
  url: typeof file === "string" ? file : "https://mock.cdn/upload.png",
  key: "uploads/file.png"
});
export const connections = {
  executeCapability: async (connectionId, capabilityKey, args) => ({ success: true }),
  listConnections: async () => []
};
export const cookies = async () => ({
  get: () => undefined,
  set: () => {},
  delete: () => {},
  has: () => false
});
export const db = new Proxy({}, {
  get: () => () => ({ selectAll: () => ({ execute: async () => [] }) })
});
export const sql = (strings, ...values) => strings.join("");
export default { HttpError, pubsub, upload, connections, cookies, db, sql };
"##
        }
        "next/headers" | "headers" => {
            r##"function parseBrowserCookies() {
  const map = new Map();
  if (typeof document === "undefined" || !document.cookie) return map;
  const pairs = document.cookie.split(";");
  for (const pair of pairs) {
    const idx = pair.indexOf("=");
    if (idx === -1) continue;
    const key = decodeURIComponent(pair.slice(0, idx).trim());
    const val = decodeURIComponent(pair.slice(idx + 1).trim());
    if (key) map.set(key, val);
  }
  return map;
}

class CookieStore {
  get(name) {
    const cookies = parseBrowserCookies();
    if (cookies.has(name)) {
      return { name, value: cookies.get(name) };
    }
    if (typeof localStorage !== "undefined") {
      const lsVal = localStorage.getItem(name);
      if (lsVal !== null) {
        return { name, value: lsVal };
      }
    }
    if (typeof sessionStorage !== "undefined") {
      const ssVal = sessionStorage.getItem(name);
      if (ssVal !== null) {
        return { name, value: ssVal };
      }
    }
    return undefined;
  }

  getAll(name) {
    const cookies = parseBrowserCookies();
    const result = [];
    for (const [k, v] of cookies.entries()) {
      if (!name || k === name) {
        result.push({ name: k, value: v });
      }
    }
    return result;
  }

  has(name) {
    return this.get(name) !== undefined;
  }

  set(nameOrOptions, value, options = {}) {
    if (typeof document === "undefined") return;
    let name = nameOrOptions;
    let val = value;
    let opts = options;
    if (typeof nameOrOptions === "object" && nameOrOptions !== null) {
      name = nameOrOptions.name;
      val = nameOrOptions.value;
      opts = nameOrOptions;
    }
    if (!name) return;

    let cookieStr = `${encodeURIComponent(name)}=${encodeURIComponent(val || "")}`;
    if (opts.path) cookieStr += `; Path=${opts.path}`;
    else cookieStr += "; Path=/";

    if (opts.maxAge !== undefined) cookieStr += `; Max-Age=${opts.maxAge}`;
    if (opts.expires) cookieStr += `; Expires=${opts.expires instanceof Date ? opts.expires.toUTCString() : opts.expires}`;
    if (opts.domain) cookieStr += `; Domain=${opts.domain}`;
    if (opts.secure) cookieStr += "; Secure";
    if (opts.sameSite) cookieStr += `; SameSite=${opts.sameSite}`;
    else cookieStr += "; SameSite=Lax";

    document.cookie = cookieStr;
    if (typeof localStorage !== "undefined" && val) {
      try { localStorage.setItem(name, val); } catch {}
    }
  }

  delete(nameOrOptions) {
    const name = typeof nameOrOptions === "object" ? nameOrOptions?.name : nameOrOptions;
    if (!name || typeof document === "undefined") return;
    document.cookie = `${encodeURIComponent(name)}=; Path=/; Max-Age=0; Expires=Thu, 01 Jan 1970 00:00:00 GMT`;
    if (typeof localStorage !== "undefined") {
      try { localStorage.removeItem(name); } catch {}
    }
  }

  clear() {
    const cookies = parseBrowserCookies();
    for (const k of cookies.keys()) {
      this.delete(k);
    }
  }

  get size() {
    return parseBrowserCookies().size;
  }

  toString() {
    return typeof document !== "undefined" ? document.cookie : "";
  }
}

export const cookies = async () => new CookieStore();

export const headers = async () => {
  const h = new Headers();
  if (typeof window !== "undefined") {
    h.set("host", window.location.host);
    h.set("user-agent", navigator.userAgent);
    h.set("referer", window.location.href);
    if (typeof document !== "undefined" && document.cookie) h.set("cookie", document.cookie);
  }
  return h;
};

export const draftMode = () => ({
  isEnabled: false,
  enable: () => {},
  disable: () => {}
});

export default { cookies, headers, draftMode };
"##
        }
        "next/link" | "link" => crate::next_shims::LINK_JS,
        "next/image" | "image" => crate::next_shims::IMAGE_JS,
        "next/dynamic" | "dynamic" => crate::next_shims::DYNAMIC_JS,
        "next/script" | "script" => crate::next_shims::SCRIPT_JS,
        "next/navigation" | "navigation" => {
            r##"import React from "react";
import { SelectedLayoutContext } from "/_nata/rsc/boundaries.js";

/** The URL segments below the calling layout (React Server Components router) */
export const useSelectedLayoutSegments = () => React.useContext(SelectedLayoutContext);
export const useSelectedLayoutSegment = () => {
  const segments = useSelectedLayoutSegments();
  return segments.length ? segments[0] : null;
};
// The client runtime keeps the App Router location here (virtual in Zalo Mini Apps); app paths are
// reported without the next.config basePath, like Next.js
const nataLocation = () => {
  if (typeof window === "undefined") return { pathname: "/", search: "" };
  const location = window.__NATA_LOCATION__ || window.location;
  const base = (window.__NATA_CONFIG__ || {}).basePath || "";
  if (!base || !location.pathname.startsWith(base)) return location;
  return { pathname: location.pathname.slice(base.length) || "/", search: location.search, href: location.href };
};

export const useRouter = () => ({
  push: (url) => {
    if (typeof window !== "undefined" && typeof window.__NATA_NAVIGATE__ === "function") {
      window.__NATA_NAVIGATE__(url);
    } else if (typeof window !== "undefined") {
      window.location.href = url;
    }
  },
  replace: (url) => {
    if (typeof window !== "undefined" && typeof window.__NATA_NAVIGATE__ === "function") {
      window.__NATA_NAVIGATE__(url, { replace: true });
    } else if (typeof window !== "undefined") {
      window.location.replace(url);
    }
  },
  back: () => { if (typeof window !== "undefined") window.history.back(); },
  forward: () => { if (typeof window !== "undefined") window.history.forward(); },
  refresh: () => {
    if (typeof window !== "undefined" && window.__NATA_ROUTER__) {
      window.__NATA_ROUTER__.refresh();
    } else if (typeof window !== "undefined" && typeof window.__NATA_NAVIGATE__ === "function") {
      window.__NATA_NAVIGATE__(nataLocation().pathname + nataLocation().search, { replace: true });
    } else if (typeof window !== "undefined") {
      window.location.reload();
    }
  },
  prefetch: (url) => {
    if (typeof window !== "undefined" && window.__NATA_ROUTER__) window.__NATA_ROUTER__.prefetch(url);
  }
});

export const usePathname = () => {
  const [pathname, setPathname] = React.useState(() => nataLocation().pathname);
  React.useEffect(() => {
    const handler = () => setPathname(nataLocation().pathname);
    window.addEventListener("popstate", handler);
    window.addEventListener("_nata_navigate", handler);
    return () => {
      window.removeEventListener("popstate", handler);
      window.removeEventListener("_nata_navigate", handler);
    };
  }, []);
  return pathname;
};

export const useSearchParams = () => {
  const [searchParams, setSearchParams] = React.useState(() => new URLSearchParams(nataLocation().search));
  React.useEffect(() => {
    const handler = () => setSearchParams(new URLSearchParams(nataLocation().search));
    window.addEventListener("popstate", handler);
    window.addEventListener("_nata_navigate", handler);
    return () => {
      window.removeEventListener("popstate", handler);
      window.removeEventListener("_nata_navigate", handler);
    };
  }, []);
  return searchParams;
};

export const useParams = () => {
  const getParams = () => (typeof window !== "undefined" && window.__NATA_PARAMS__ && typeof window.__NATA_PARAMS__ === "object") ? window.__NATA_PARAMS__ : {};
  const [params, setParams] = React.useState(getParams);
  React.useEffect(() => {
    const handler = (e) => {
      if (e && e.detail && e.detail.params) {
        setParams(e.detail.params);
      } else {
        setParams(getParams());
      }
    };
    window.addEventListener("popstate", handler);
    window.addEventListener("_nata_navigate", handler);
    return () => {
      window.removeEventListener("popstate", handler);
      window.removeEventListener("_nata_navigate", handler);
    };
  }, []);
  return params;
};

export const redirect = (url, type = "replace") => {
  if (typeof window !== "undefined") {
    if (typeof window.__NATA_NAVIGATE__ === "function") {
      window.__NATA_NAVIGATE__(url, { replace: type !== "push" });
    } else {
      window.location.href = url;
    }
  }
};

export const notFound = () => {
  const err = new Error("NEXT_NOT_FOUND");
  err.digest = "NEXT_NOT_FOUND";
  throw err;
};
const httpFallback = (status) => {
  const err = new Error("NEXT_HTTP_ERROR_FALLBACK;" + status);
  err.digest = "NEXT_HTTP_ERROR_FALLBACK;" + status;
  throw err;
};
export const forbidden = () => httpFallback(403);
export const unauthorized = () => httpFallback(401);
export const RedirectType = { push: "push", replace: "replace" };
export const permanentRedirect = (url, type) => redirect(url, type);
export const unstable_rethrow = (error) => {
  if (error && typeof error.digest === "string" && /^(NEXT_REDIRECT|NEXT_NOT_FOUND|NEXT_HTTP_ERROR_FALLBACK)/.test(error.digest)) throw error;
};
export default { useRouter, usePathname, useSearchParams, useParams, redirect, permanentRedirect, notFound, forbidden, unauthorized, unstable_rethrow, RedirectType, useSelectedLayoutSegment, useSelectedLayoutSegments };
"##
        }
        "next/router" | "router" => crate::next_shims::ROUTER_JS,
        "next/head" | "head" => crate::next_shims::HEAD_JS,
        "next/server" | "server" => {
            r##"export class NextResponse extends Response {
  static json(data, init) {
    const headers = new Headers(init?.headers);
    if (!headers.has("content-type")) headers.set("content-type", "application/json; charset=utf-8");
    return new NextResponse(JSON.stringify(data), { ...init, headers });
  }
  static redirect(url, init) {
    const status = typeof init === "number" ? init : init?.status || 307;
    const headers = new Headers(typeof init === "object" ? init?.headers : undefined);
    headers.set("location", String(url));
    return new NextResponse(null, { status, headers });
  }
  static rewrite(url, init) {
    const headers = new Headers(init?.headers);
    headers.set("x-middleware-rewrite", String(url));
    return new NextResponse(null, { ...init, headers });
  }
  static next(init) {
    const headers = new Headers(init?.headers);
    headers.set("x-middleware-next", "1");
    return new NextResponse(null, { ...init, headers });
  }
}
export class NextRequest extends Request {
  constructor(input, init) {
    super(input, init);
    this.nextUrl = new URL(typeof input === "string" ? input : input?.url || "/");
    this.cookies = new Map();
  }
}
export default { NextResponse, NextRequest };
"##
        }
        "next-intl/server" | "next-intl" => {
            r##"const _messagesCache = new Map();

export async function getLocale() {
  if (typeof document !== "undefined" && document.cookie) {
    const match = document.cookie.match(/(?:^|;\s*)NEXT_LOCALE=([^;]+)/);
    if (match) return decodeURIComponent(match[1]);
  }
  if (typeof localStorage !== "undefined") {
    try {
      const val = localStorage.getItem("NEXT_LOCALE");
      if (val) return val;
    } catch {}
  }
  if (typeof navigator !== "undefined" && navigator.language) {
    return navigator.language.split("-")[0] || "vi";
  }
  return "vi";
}

export async function getMessages(opts) {
  const locale = (opts && opts.locale) || (await getLocale());
  if (_messagesCache.has(locale)) {
    return _messagesCache.get(locale);
  }

  const candidatePaths = [
    `/config/locales/${locale}.json`,
    `/messages/${locale}.json`,
    `/locales/${locale}.json`,
    `/config/locales/vi.json`,
    `/messages/vi.json`,
    `/config/locales/en.json`,
    `/messages/en.json`
  ];

  if (typeof window !== "undefined") {
    for (const path of candidatePaths) {
      try {
        const res = await fetch(path);
        if (res.ok) {
          const data = await res.json();
          if (data && typeof data === "object") {
            _messagesCache.set(locale, data);
            return data;
          }
        }
      } catch {}
    }
  }

  return {};
}

function getNestedValue(obj, keyPath) {
  if (!obj || !keyPath) return undefined;
  const parts = keyPath.split(".");
  let curr = obj;
  for (const p of parts) {
    if (curr && typeof curr === "object" && p in curr) {
      curr = curr[p];
    } else {
      return undefined;
    }
  }
  return curr;
}

export async function getTranslations(optsOrNamespace) {
  const namespace = typeof optsOrNamespace === "string" ? optsOrNamespace : optsOrNamespace?.namespace;
  const locale = (typeof optsOrNamespace === "object" && optsOrNamespace?.locale) || (await getLocale());
  const messages = (typeof optsOrNamespace === "object" && optsOrNamespace?.messages) || (await getMessages({ locale }));
  const scopeObj = namespace ? getNestedValue(messages, namespace) || {} : messages;

  function t(key, params) {
    let raw = getNestedValue(scopeObj, key) || getNestedValue(messages, key) || key;
    if (typeof raw !== "string") {
      if (raw === undefined || raw === null) return key;
      return String(raw);
    }
    let res = raw;
    if (params && typeof params === "object") {
      for (const [k, v] of Object.entries(params)) {
        res = res.replace(new RegExp(`\\{${k}\\}`, "g"), String(v));
      }
    }
    return res;
  }

  t.raw = (key) => getNestedValue(scopeObj, key) || key;
  t.rich = (key, values) => t(key, values);
  t.has = (key) => getNestedValue(scopeObj, key) !== undefined;

  return t;
}

export function getRequestConfig(fn) {
  return fn;
}

export function setRequestLocale(locale) {
  if (typeof document !== "undefined") {
    document.cookie = `NEXT_LOCALE=${encodeURIComponent(locale)}; Path=/; SameSite=Lax`;
    try { localStorage.setItem("NEXT_LOCALE", locale); } catch {}
  }
}

export async function getTimeZone() {
  try {
    return Intl.DateTimeFormat().resolvedOptions().timeZone || "UTC";
  } catch {
    return "UTC";
  }
}

export async function getNow() {
  return new Date();
}

export async function getFormatter() {
  return {
    dateTime: (date, opts) => new Intl.DateTimeFormat(undefined, opts).format(date),
    number: (num, opts) => new Intl.NumberFormat(undefined, opts).format(num),
    relativeTime: (val, unit, opts) => new Intl.RelativeTimeFormat(undefined, opts).format(val, unit)
  };
}

export const getExtracted = () => ({});

export default {
  getLocale,
  getMessages,
  getTranslations,
  getRequestConfig,
  setRequestLocale,
  getTimeZone,
  getNow,
  getFormatter,
  getExtracted
};
"##
        }
        _ => {
            r##"export default {};"##
        }
    };

    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        "application/javascript; charset=utf-8".parse().unwrap(),
    );
    headers.insert(
        header::CACHE_CONTROL,
        "no-cache".parse().unwrap(),
    );
    (StatusCode::OK, headers, js_code).into_response()
}

/// ES module served in place of a file that failed to compile: it hands the error to the dev overlay
/// ("Build Error", shown on web and inside Zalo alike) and then fails the import.
fn build_error_module(err: &crate::swc_compiler::CompileError) -> String {
    let payload = serde_json::to_string(err).unwrap_or_else(|_| "{}".to_string());
    format!(
        "const __nataBuildError = {payload};\n\
         if (typeof window !== \"undefined\" && typeof window.__NATA_BUILD_ERROR__ === \"function\") window.__NATA_BUILD_ERROR__(__nataBuildError);\n\
         const __nataErr = new SyntaxError(__nataBuildError.message);\n\
         __nataErr.__nataBuildError = __nataBuildError;\n\
         throw __nataErr;\n"
    )
}

async fn bundle_handler(
    State(state): State<AppState>,
    Path(path): Path<String>,
    request_headers: HeaderMap,
) -> impl IntoResponse {
    let clean_path = path.trim_start_matches('/');
    let tsconfig = crate::resolver::TsConfig::load_from_dir(&state.root_dir);

    match crate::resolver::PathResolver::resolve_file_with_config(&state.root_dir, clean_path, &tsconfig) {
        Some(resolved) => match tokio::fs::read_to_string(&resolved.absolute_path).await {
            Ok(content) => {
                let mut js_headers = HeaderMap::new();
                js_headers.insert(header::CONTENT_TYPE, "application/javascript; charset=utf-8".parse().unwrap());
                js_headers.insert(header::CACHE_CONTROL, "no-cache".parse().unwrap());

                if crate::module_graph::rsc_enabled() {
                    // SWC (on a cache miss) and the cache lookup are CPU work: off the request threads
                    let (root, file) = (state.root_dir.clone(), resolved.absolute_path.clone());
                    let compiled = tokio::task::spawn_blocking(move || {
                        crate::module_graph::compile(&root, &file, crate::resolver::Layer::Client, &content)
                            .map(|compiled| crate::resolver::ImportResolver::soften_remote_named_imports(&compiled.code))
                    })
                    .await;
                    let Ok(compiled) = compiled else {
                        return (StatusCode::INTERNAL_SERVER_ERROR, "Compilation was interrupted").into_response();
                    };
                    return match compiled {
                        Ok(js) => {
                            // A reload revalidates every module: unchanged ones answer 304 without a body
                            let etag = format!("\"{}\"", &blake3::hash(js.as_bytes()).to_hex()[..16]);
                            let fresh = request_headers.get(header::IF_NONE_MATCH).and_then(|v| v.to_str().ok()) == Some(etag.as_str());
                            js_headers.insert(header::ETAG, etag.parse().unwrap());
                            if fresh {
                                return (StatusCode::NOT_MODIFIED, js_headers).into_response();
                            }
                            (StatusCode::OK, js_headers, js).into_response()
                        }
                        Err(err) => {
                            crate::term::error(&err.location(), Some(&err.message), Some(&err.frame));
                            js_headers.insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
                            (StatusCode::OK, js_headers, build_error_module(&err)).into_response()
                        }
                    };
                }

                let cache = crate::compile_cache::CompileCache::for_root(&state.root_dir);
                let cache_key = crate::compile_cache::CompileCache::key(
                    &resolved.relative_path,
                    &content,
                    &crate::compile_cache::tsconfig_fingerprint(&state.root_dir),
                );
                if let Some(compiled_js) = cache.as_ref().and_then(|c| c.get(&cache_key)) {
                    return (StatusCode::OK, js_headers, compiled_js).into_response();
                }

                let transformed = crate::resolver::PathResolver::transform_source_with_config(
                    &content,
                    &resolved.relative_path,
                    &tsconfig,
                );

                // Compile TypeScript / TSX directly via SWC Native Rust engine
                match crate::swc_compiler::SwcCompiler::compile(&transformed, &resolved.relative_path) {
                    Ok(compiled_js) => {
                        let compiled_js = crate::resolver::ImportResolver::soften_remote_named_imports(&compiled_js);
                        if let Some(cache) = &cache {
                            cache.put(&cache_key, &compiled_js);
                        }
                        (StatusCode::OK, js_headers, compiled_js).into_response()
                    }
                    Err(mut err) => {
                        // Resolver rewrites keep line numbers, so show the frame from the file the user wrote
                        if let (Some(line), Some(col)) = (err.line, err.column) {
                            if content.lines().count() == transformed.lines().count() {
                                err.frame = crate::term::code_frame(&content, line, col, 2);
                            }
                        }
                        crate::term::error(&err.location(), Some(&err.message), Some(&err.frame));
                        let mut headers = HeaderMap::new();
                        headers.insert(header::CONTENT_TYPE, "application/javascript; charset=utf-8".parse().unwrap());
                        headers.insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
                        (StatusCode::OK, headers, build_error_module(&err)).into_response()
                    }
                }
            }
            Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Failed to read file").into_response(),
        },
        None => (StatusCode::NOT_FOUND, format!("Module not found: {}", clean_path)).into_response(),
    }
}

/// The nearest loading.tsx of a route, server-rendered on its own (loading UIs are plain synchronous
/// components); empty when the route has none or it cannot render
async fn loading_placeholder(root: &std::path::Path, segments: &[crate::router::types::SegmentFiles], path: &str) -> String {
    let Some(loading_file) = segments.iter().rev().find_map(|s| s.loading.clone()) else {
        return String::new();
    };
    let request = SsrRequest {
        path: path.to_string(),
        page_file: loading_file,
        layout_files: Vec::new(),
        segments_files: Vec::new(),
        global_error_file: None,
        params: serde_json::json!({}),
        search_params: serde_json::json!({}),
        cookies: None,
        headers: None,
    };
    SsrEngine::render(root, request).await.html
}

/// The Zalo Mini App document for the browser preview: the same bootstrap the device runs
/// (`/assets/app.js`), with the mock Zalo bridge when it is enabled (`com dev --zmp-mock`)
fn zalo_preview_document() -> String {
    format!(
        "<!DOCTYPE html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><title>Zalo Mini App</title>{}</head><body><div id=\"app\"></div><script src=\"/assets/app.js\"></script></body></html>",
        crate::zmp::ZmpMockBridge::script_tag()
    )
}

/// `/_next/image?url=&w=&q=`: the next/image optimizer
async fn next_image_handler(State(state): State<AppState>, req: axum::extract::Request) -> axum::response::Response {
    let accept = req.headers().get(header::ACCEPT).and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
    crate::image_optimizer::handle(&state.root_dir, req.uri().query().unwrap_or(""), &accept).await
}

/// A statically imported image, as the ES module the browser loads
async fn image_module_handler(State(state): State<AppState>, Path(path): Path<String>) -> axum::response::Response {
    let file = state.root_dir.join(path.trim_start_matches('/'));
    let inside = crate::rpc::canonicalize(&file).ok().zip(crate::rpc::canonicalize(&state.root_dir).ok()).is_some_and(|(f, r)| f.starts_with(r));
    if !inside || !crate::image_optimizer::is_image(&file) || !file.is_file() {
        return StatusCode::NOT_FOUND.into_response();
    }
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, "application/javascript; charset=utf-8".parse().unwrap());
    headers.insert(header::CACHE_CONTROL, "no-cache".parse().unwrap());
    (StatusCode::OK, headers, crate::image_optimizer::static_import_module(&state.root_dir, &file)).into_response()
}

/// What a page request asks the React Server Components pipeline for: the document, the Flight
/// payload (`RSC: 1`, client navigations) or a Server Action (POST)
async fn rsc_page_request<'a>(
    req: axum::extract::Request,
    path: &'a str,
    query: &'a str,
    route: Option<&'a crate::router::types::RouteEntry>,
    params: std::collections::HashMap<String, String>,
    not_found_segments: Vec<crate::router::types::SegmentFiles>,
) -> crate::rsc::PageRequest<'a> {
    let headers = req.headers().clone();
    let header_str = |name: &str| headers.get(name).and_then(|v| v.to_str().ok()).map(|s| s.to_string());
    let router_state = header_str("nata-router-state")
        .map(|v| crate::router::types::percent_decode(&v))
        .and_then(|v| serde_json::from_str::<Vec<String>>(&v).ok())
        .unwrap_or_default();
    let mode = if req.method() == axum::http::Method::POST {
        let content_type = header_str("content-type").unwrap_or_default();
        let body = axum::body::to_bytes(req.into_body(), 50 * 1024 * 1024).await.unwrap_or_default().to_vec();
        crate::rsc::Mode::Action { id: header_str("next-action"), content_type, body }
    } else if header_str("rsc").as_deref() == Some("1") {
        crate::rsc::Mode::Flight
    } else {
        crate::rsc::Mode::Html
    };
    let request_headers = headers
        .iter()
        .filter_map(|(k, v)| v.to_str().ok().map(|v| (k.as_str().to_lowercase(), v.to_string())))
        .collect();
    crate::rsc::PageRequest {
        pathname: path,
        query,
        route,
        params,
        cookies: header_str("cookie"),
        headers: request_headers,
        router_state,
        mode,
        not_found_segments,
        navigated_from: None,
    }
}

/// Page, route handler and static file requests: the middleware first (it may respond, rewrite or
/// override request headers), then routing
async fn app_router_fallback_handler(State(state): State<AppState>, mut req: axum::extract::Request) -> axum::response::Response {
    use crate::middleware::Outcome;
    // next.config: basePath, trailingSlash, redirects, beforeFiles rewrites; headers() for the response
    let config = if crate::next_config::find(&state.root_dir).is_some() { crate::next_config::ensure_loaded(&state.root_dir).await } else { None };
    let mut config_headers = Vec::new();
    if let Some(config) = &config {
        let path = req.uri().path().to_string();
        let query = req.uri().query().unwrap_or("").to_string();
        match crate::next_config::before_routing(config, &path, &query, req.headers()) {
            crate::next_config::Decision::Redirect { status, location } => {
                return axum::response::Response::builder()
                    .status(StatusCode::from_u16(status).unwrap_or(StatusCode::TEMPORARY_REDIRECT))
                    .header(header::LOCATION, location)
                    .body(axum::body::Body::empty())
                    .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response());
            }
            crate::next_config::Decision::NotFound => return (StatusCode::NOT_FOUND, "Not Found").into_response(),
            crate::next_config::Decision::Continue { path_and_query } => {
                let routed = path_and_query.split('?').next().unwrap_or("/").to_string();
                config_headers = crate::next_config::response_headers(config, &routed, &query, req.headers());
                if path_and_query.starts_with("http://") || path_and_query.starts_with("https://") {
                    let mut response = proxy_external(&state.root_dir, req, &path_and_query).await;
                    crate::middleware::apply_response_headers(&mut response, &config_headers);
                    return response;
                }
                if let Ok(uri) = path_and_query.parse::<axum::http::Uri>() {
                    *req.uri_mut() = uri;
                }
            }
        }
    }
    let outcome = crate::middleware::run(&state.root_dir, req.method().as_str(), req.uri(), req.headers()).await;
    let set_request_headers = |req: &mut axum::extract::Request, headers: &[(String, String)]| {
        for (k, v) in headers {
            if let (Ok(name), Ok(value)) = (header::HeaderName::from_bytes(k.as_bytes()), HeaderValue::from_str(v)) {
                req.headers_mut().insert(name, value);
            }
        }
    };
    let response_headers = match outcome {
        None => Vec::new(),
        Some(Outcome::Respond(response)) => return response,
        Some(Outcome::Next { request_headers, response_headers }) => {
            set_request_headers(&mut req, &request_headers);
            response_headers
        }
        Some(Outcome::Rewrite { path_and_query, request_headers, response_headers }) => {
            set_request_headers(&mut req, &request_headers);
            if let Ok(uri) = path_and_query.parse::<axum::http::Uri>() {
                *req.uri_mut() = uri;
            }
            response_headers
        }
    };
    // afterFiles / fallback rewrites: only for paths no page, route handler or file answers
    if let Some(config) = &config {
        let path = req.uri().path().to_string();
        let unmatched = state.router.read().await.match_route(&path).is_none() && !static_file_exists(&state.root_dir, &path);
        if unmatched {
            let query = req.uri().query().unwrap_or("").to_string();
            if let Some(destination) = crate::next_config::rewrite_unmatched(config, &path, &query, req.headers()) {
                if destination.starts_with("http://") || destination.starts_with("https://") {
                    let mut response = proxy_external(&state.root_dir, req, &destination).await;
                    crate::middleware::apply_response_headers(&mut response, &config_headers);
                    crate::middleware::apply_response_headers(&mut response, &response_headers);
                    return response;
                }
                if let Ok(uri) = destination.parse::<axum::http::Uri>() {
                    *req.uri_mut() = uri;
                }
            }
        }
    }
    let mut response = route_request(state, req).await.into_response();
    crate::middleware::apply_response_headers(&mut response, &config_headers);
    crate::middleware::apply_response_headers(&mut response, &response_headers);
    response
}

fn static_file_exists(root: &std::path::Path, path: &str) -> bool {
    let rel = path.trim_start_matches('/');
    !rel.is_empty() && !rel.contains("..") && (root.join("public").join(rel).is_file() || root.join(rel).is_file())
}

/// An external rewrite destination: the worker fetches it (the engine has no HTTP client)
async fn proxy_external(root: &std::path::Path, req: axum::extract::Request, url: &str) -> axum::response::Response {
    let method = req.method().to_string();
    let headers: Vec<(String, String)> = req
        .headers()
        .iter()
        .filter(|(k, _)| *k != header::HOST)
        .filter_map(|(k, v)| v.to_str().ok().map(|v| (k.as_str().to_string(), v.to_string())))
        .collect();
    let body = axum::body::to_bytes(req.into_body(), 50 * 1024 * 1024).await.unwrap_or_default().to_vec();
    match crate::worker_pool::call(root, "proxy", serde_json::json!({ "url": url, "method": method, "headers": headers }), body).await {
        Ok(reply) if reply.error().is_none() => {
            let out = reply.output();
            let mut builder = axum::response::Response::builder().status(StatusCode::from_u16(out["status"].as_u64().unwrap_or(502) as u16).unwrap_or(StatusCode::BAD_GATEWAY));
            for pair in out["headers"].as_array().into_iter().flatten() {
                if let (Some(k), Some(v)) = (pair[0].as_str(), pair[1].as_str()) {
                    if let (Ok(name), Ok(value)) = (header::HeaderName::from_bytes(k.as_bytes()), HeaderValue::from_str(v)) {
                        builder = builder.header(name, value);
                    }
                }
            }
            builder.body(axum::body::Body::from(reply.body)).unwrap_or_else(|_| StatusCode::BAD_GATEWAY.into_response())
        }
        Ok(reply) => (StatusCode::BAD_GATEWAY, format!("Proxy to {url} failed: {}", reply.error().unwrap_or(""))).into_response(),
        Err(e) => (StatusCode::BAD_GATEWAY, format!("Proxy to {url} failed: {e}")).into_response(),
    }
}

async fn route_request(state: AppState, req: axum::extract::Request) -> impl IntoResponse {
    let path = req.uri().path().to_string();

    // Defense check: ZMP config and assets must never fall through to HTML
    let clean_path = path.trim_end_matches('/');
    if clean_path == "/hr.config.json" || clean_path == "/hrr.config.json" {
        return zmp_hr_config_handler(State(state)).await.into_response();
    }
    if clean_path == "/app-config.json" || clean_path == "/zmp.json" || clean_path == "/app.config.json" {
        return zmp_app_config_handler(State(state)).await.into_response();
    }
    if clean_path == "/assets/app.css" {
        return zmp_css_handler(State(state)).await.into_response();
    }
    if clean_path == "/assets/app.js" || clean_path == "/src/app.js" {
        return zmp_js_handler(State(state)).await.into_response();
    }

    // Redirect CDN internal module assets to esm.sh
    if path.starts_with("/react@")
        || path.starts_with("/react-dom@")
        // esm.sh build paths (`/v135/...`), not app routes such as `/vi`
        || (path.starts_with("/v") && path[2..].split('/').next().is_some_and(|v| !v.is_empty() && v.chars().all(|c| c.is_ascii_digit())) && path[2..].contains('/'))
        || path.starts_with("/node/")
        || path.starts_with("/npm/")
        || path.starts_with("/gh/")
        || (path.contains('@') && (path.ends_with(".mjs") || path.ends_with(".js") || path.ends_with(".map")))
    {
        let redirect_url = format!("https://esm.sh{}", path);
        return axum::response::Redirect::temporary(&redirect_url).into_response();
    }

    // In-memory router matching with zero disk I/O
    let router_guard = state.router.read().await;
    if let Some((route, params)) = router_guard.match_route(&path) {
        if route.is_api {
            let method = req.method().to_string();
            let uri = req.uri().clone();
            let full_url = format!("http://localhost{}", uri);

            let mut query_map = std::collections::HashMap::new();
            if let Some(q) = uri.query() {
                for pair in q.split('&') {
                    if let Some((k, v)) = pair.split_once('=') {
                        query_map.insert(k.to_string(), v.to_string());
                    } else if !pair.is_empty() {
                        query_map.insert(pair.to_string(), String::new());
                    }
                }
            }

            let mut headers_map = std::collections::HashMap::new();
            for (k, v) in req.headers() {
                if let Ok(str_val) = v.to_str() {
                    headers_map.insert(k.as_str().to_lowercase(), str_val.to_string());
                }
            }

            let raw_cookie_str = req
                .headers()
                .get(header::COOKIE)
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string());

            let body_bytes = axum::body::to_bytes(req.into_body(), 50 * 1024 * 1024)
                .await
                .unwrap_or_default();

            let handler_req = crate::route_handler::RouteHandlerRequest {
                path: path.to_string(),
                url: full_url,
                method,
                file_path: route.page_file.clone(),
                params: route.params_json(&params),
                search_params: query_map,
                headers: headers_map,
                cookies: raw_cookie_str,
                body: body_bytes.to_vec(),
            };

            if crate::module_graph::rsc_enabled() {
                return crate::route_handler::worker::RouteHandlerWorker::stream(&state.root_dir, handler_req).await;
            }
            let output = crate::route_handler::RouteHandlerEngine::execute(&state.root_dir, handler_req).await;

            let mut resp_builder = axum::response::Response::builder()
                .status(StatusCode::from_u16(output.status).unwrap_or(StatusCode::OK));

            for (k, v) in &output.headers {
                if let (Ok(hk), Ok(hv)) = (header::HeaderName::from_bytes(k.as_bytes()), HeaderValue::from_str(v)) {
                    resp_builder = resp_builder.header(hk, hv);
                }
            }

            for sc in &output.cookies {
                let mut cookie_hdr = format!("{}={}", escape_cookie_val(&sc.name), escape_cookie_val(&sc.value));
                cookie_hdr.push_str(&format!("; Path={}", sc.path.as_deref().unwrap_or("/")));
                if let Some(max_age) = sc.max_age {
                    cookie_hdr.push_str(&format!("; Max-Age={}", max_age));
                }
                if let Some(ref exp) = sc.expires {
                    cookie_hdr.push_str(&format!("; Expires={}", exp));
                }
                if let Some(ref dom) = sc.domain {
                    cookie_hdr.push_str(&format!("; Domain={}", dom));
                }
                if let Some(ref ss) = sc.same_site {
                    cookie_hdr.push_str(&format!("; SameSite={}", ss));
                } else {
                    cookie_hdr.push_str("; SameSite=Lax");
                }
                if let Ok(hv) = HeaderValue::from_str(&cookie_hdr) {
                    resp_builder = resp_builder.header(header::SET_COOKIE, hv);
                }
            }

            return resp_builder
                .body(axum::body::Body::from(output.body))
                .unwrap_or_else(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Response building failed").into_response());
        }

        // `com start`: prerendered and ISR pages from the page cache
        if crate::prod::active() {
            if let Some(cached) = crate::prod::serve_page(&state, &req.method().clone(), &req.headers().clone(), &path, route).await {
                return cached;
            }
        }

        // A client navigation or HMR refresh (`RSC: 1`) only gets the payload: the document head with
        // the inlined stylesheet is not sent, so the stylesheet is not built for it
        let flight = crate::module_graph::rsc_enabled() && req.method() == axum::http::Method::GET && req.headers().get("rsc").is_some();
        let stylesheet_tags: std::sync::Arc<str> = if flight { "".into() } else { crate::css_compiler::stylesheet::project_head_tags(&state.root_dir).await };

        if crate::module_graph::rsc_enabled() {
            // Zalo Mini Apps render on the device: the browser preview gets the Mini App document,
            // the payload and Server Actions are served to the device like to the web router
            let wants_document = state.target == "zalo" && req.method() == axum::http::Method::GET && req.headers().get("rsc").is_none();
            if wants_document {
                return Html(zalo_preview_document()).into_response();
            }
            let route = route.clone();
            let query = req.uri().query().unwrap_or("").to_string();
            // Client navigations say where they come from (intercepting routes depend on it)
            let navigated_from = req
                .headers()
                .get("nata-url")
                .and_then(|v| v.to_str().ok())
                .map(crate::router::types::percent_decode)
                .and_then(|from| {
                    let (from_route, from_params) = router_guard.match_route(from.split('?').next().unwrap_or("/"))?;
                    Some((from.clone(), from_route.clone(), from_params))
                });
            let mut page_req = rsc_page_request(req, &path, &query, Some(&route), params, Vec::new()).await;
            page_req.navigated_from = navigated_from;
            return crate::rsc::handle(&state.root_dir, &state.target, &stylesheet_tags, page_req).await;
        }

        // Build SSR request context
        let mut query_map = std::collections::HashMap::new();
        if let Some(q) = req.uri().query() {
            for pair in q.split('&') {
                if let Some((k, v)) = pair.split_once('=') {
                    query_map.insert(k.to_string(), v.to_string());
                } else if !pair.is_empty() {
                    query_map.insert(pair.to_string(), String::new());
                }
            }
        }

        let mut headers_map = std::collections::HashMap::new();
        for (k, v) in req.headers() {
            if let Ok(str_val) = v.to_str() {
                headers_map.insert(k.as_str().to_string(), str_val.to_string());
            }
        }

        let raw_cookie_str = req
            .headers()
            .get(header::COOKIE)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        let ssr_req = SsrRequest {
            path: path.to_string(),
            page_file: route.page_file.clone(),
            layout_files: route.layout_files.clone(),
            segments_files: route.segments_files.clone(),
            global_error_file: route.global_error_file.clone(),
            params: route.params_json(&params),
            search_params: serde_json::to_value(&query_map).unwrap_or_else(|_| serde_json::json!({})),
            cookies: raw_cookie_str,
            headers: Some(serde_json::to_value(&headers_map).unwrap_or_else(|_| serde_json::json!({}))),
        };

        // Zalo Mini Apps are client-only on device, so the browser preview skips SSR as well
        let ssr_output = if state.target == "zalo" {
            SsrOutput::client_only(&ssr_req)
        } else {
            SsrEngine::render(&state.root_dir, ssr_req).await
        };

        // Check if SSR returned a redirect
        if let Some(redirect_to) = ssr_output.redirect_url {
            return axum::response::Redirect::temporary(&redirect_to).into_response();
        }

        // Page rendered on the client: show the route's loading.tsx (rendered here) until it mounts
        let loading_html = if ssr_output.html.is_empty() {
            loading_placeholder(&state.root_dir, &route.segments_files, &path).await
        } else {
            String::new()
        };

        let page_title = ssr_output.title.as_deref().unwrap_or("Com.AI.VN Application");
        let head_tags = ssr_output.head_tags.as_deref().unwrap_or("");
        let initial_route = ClientTransformer::route_info_json(
            &route.page_file,
            &route.layout_files,
            &route.segments_files,
            route.global_error_file.as_ref(),
            &route.params_json(&params),
        );
        let html = ClientTransformer::render_ssr_html_shell(
            page_title,
            &initial_route,
            &stylesheet_tags,
            &ssr_output.html,
            &loading_html,
            &ssr_output.initial_state,
            &state.target,
            head_tags,
        );

        let mut response = Html(html).into_response();
        *response.status_mut() = StatusCode::from_u16(ssr_output.status_code).unwrap_or(StatusCode::OK);

        for sc in &ssr_output.set_cookies {
            let mut cookie_hdr = format!("{}={}", escape_cookie_val(&sc.name), escape_cookie_val(&sc.value));
            cookie_hdr.push_str(&format!("; Path={}", sc.path.as_deref().unwrap_or("/")));
            if let Some(max_age) = sc.max_age {
                cookie_hdr.push_str(&format!("; Max-Age={}", max_age));
            }
            if let Some(ref exp) = sc.expires {
                cookie_hdr.push_str(&format!("; Expires={}", exp));
            }
            if let Some(ref dom) = sc.domain {
                cookie_hdr.push_str(&format!("; Domain={}", dom));
            }
            if let Some(ref ss) = sc.same_site {
                cookie_hdr.push_str(&format!("; SameSite={}", ss));
            } else {
                cookie_hdr.push_str("; SameSite=Lax");
            }
            if let Ok(hv) = HeaderValue::from_str(&cookie_hdr) {
                response.headers_mut().append(header::SET_COOKIE, hv);
            }
        }

        return response;
    }

    // Try serving special route assets (e.g. /icon.png, /opengraph-image.png, etc.)
    let clean_path = path.trim_start_matches('/');
    let asset_stem = clean_path.split('.').next().unwrap_or("");
    if asset_stem == "icon" || asset_stem == "apple-icon" || asset_stem == "opengraph-image" || asset_stem == "twitter-image" {
        if let Some((asset_path, mime)) = crate::metadata::MetadataEngine::find_route_asset(&state.root_dir, asset_stem) {
            if let Ok(bytes) = tokio::fs::read(asset_path).await {
                let mut headers = HeaderMap::new();
                headers.insert(header::CONTENT_TYPE, mime.parse().unwrap());
                return (StatusCode::OK, headers, bytes).into_response();
            }
        }
    }

    // Try serving static assets
    let static_candidate = state.root_dir.join(clean_path);
    if static_candidate.exists() && static_candidate.is_file() {
        if let Ok(bytes) = tokio::fs::read(static_candidate).await {
            return (StatusCode::OK, bytes).into_response();
        }
    }

    if crate::module_graph::rsc_enabled() && (state.target != "zalo" || req.headers().get("rsc").is_some()) {
        let root_segment: Vec<_> = router_guard.routes.iter().find_map(|r| r.segments_files.first().cloned()).into_iter().collect();
        drop(router_guard);
        let stylesheet_tags = crate::css_compiler::stylesheet::project_head_tags(&state.root_dir).await;
        let query = req.uri().query().unwrap_or("").to_string();
        let page_req = rsc_page_request(req, &path, &query, None, Default::default(), root_segment).await;
        return crate::rsc::handle(&state.root_dir, &state.target, &stylesheet_tags, page_req).await;
    }

    // Try rendering root not-found.tsx if available
    let not_found_opt = router_guard.routes.iter().find_map(|r| {
        r.segments_files.iter().find_map(|s| s.not_found.clone())
    });

    if let Some(not_found_file) = not_found_opt {
        let stylesheet_tags = crate::css_compiler::stylesheet::project_head_tags(&state.root_dir).await;
        let layout_files = root_layout_files(&router_guard);
        let ssr_req = SsrRequest {
            path: path.to_string(),
            page_file: not_found_file.clone(),
            layout_files: layout_files.clone(),
            segments_files: vec![],
            global_error_file: None,
            params: serde_json::json!({}),
            search_params: serde_json::json!({}),
            cookies: None,
            headers: None,
        };
        let ssr_output = if state.target == "zalo" {
            SsrOutput::client_only(&ssr_req)
        } else {
            SsrEngine::render(&state.root_dir, ssr_req).await
        };
        let page_title = ssr_output.title.as_deref().unwrap_or("404: This page could not be found");
        let head_tags = ssr_output.head_tags.as_deref().unwrap_or("");
        let initial_route = ClientTransformer::route_info_json(&not_found_file, &layout_files, &[], None, &serde_json::json!({}));
        let html = ClientTransformer::render_ssr_html_shell(
            page_title,
            &initial_route,
            &stylesheet_tags,
            &ssr_output.html,
            "",
            &ssr_output.initial_state,
            &state.target,
            head_tags,
        );
        let mut response = Html(html).into_response();
        *response.status_mut() = StatusCode::NOT_FOUND;
        return response;
    }

    (StatusCode::NOT_FOUND, "Route not found in App Router").into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_only_requests_from_this_machine_skip_compression() {
        let headers = |pairs: &[(&'static str, &str)]| {
            let mut map = HeaderMap::new();
            for (name, value) in pairs {
                map.insert(*name, value.parse().unwrap());
            }
            map
        };
        assert!(is_local_request(&headers(&[("host", "localhost:3000")])));
        assert!(is_local_request(&headers(&[("host", "127.0.0.1:3000")])));
        assert!(is_local_request(&headers(&[("host", "[::1]:3000")])));
        // Phones on the LAN, tunnels (even when they rewrite Host to localhost)
        assert!(!is_local_request(&headers(&[("host", "192.168.1.20:3000")])));
        assert!(!is_local_request(&headers(&[("host", "abc.loca.lt")])));
        assert!(!is_local_request(&headers(&[("host", "localhost:3000"), ("x-forwarded-for", "1.2.3.4")])));
    }

    #[test]
    fn test_zmp_bootstrap_boots_shared_runtime_client_only() {
        let js = zmp_bootstrap_js("https://backend.example", false);
        assert!(js.contains(r#"var configuredBackend = "https://backend.example";"#));
        // React Server Components router (on by default), the client-rendered runtime with COM_RSC=0
        assert!(js.contains(r#"import(serverOrigin + (rsc ? "/_nata/router.js" : "/_nata/client.js"))"#));
        assert!(js.contains("var rsc = true;"));
        assert!(js.contains("self.__webpack_chunk_load__ = function(id) { return import(serverOrigin + id)"));
        assert!(js.contains(r#"mode: "zalo", ssr: false"#));
        assert!(js.contains(r#""next/navigation":serverOrigin + "/_nata/shims/next/navigation""#));
        assert!(!js.contains("__COM_IMPORT_MAP_PLACEHOLDER__"));
        assert!(!js.contains("__COM_SERVER_ORIGIN__"));
        assert!(!js.contains("hydrateRoot"));
        assert!(js.contains("if (false && !document.getElementById(\"_zmp_tailwind\"))"));
        // Eruda only on request
        assert!(js.contains("var wantEruda = false || /[?&]eruda\\b/.test(window.location.search);"));
        assert!(!js.contains("__COM_ERUDA__"));
    }

    #[test]
    fn test_zmp_bootstrap_loads_overlay_before_runtime() {
        let js = zmp_bootstrap_js("", false);
        let overlay_at = js.find(r#"import(serverOrigin + "/_nata/overlay.js")"#).expect("overlay import");
        let client_at = js.find(r#"import(serverOrigin + (rsc ? "/_nata/router.js" : "/_nata/client.js"))"#).expect("runtime import");
        assert!(overlay_at < client_at);
    }

    #[test]
    fn test_build_error_module_reports_to_overlay() {
        let err = crate::swc_compiler::SwcCompiler::compile("export const b = ;\n", "app/page.tsx").unwrap_err();
        let js = build_error_module(&err);
        assert!(js.contains("window.__NATA_BUILD_ERROR__(__nataBuildError)"));
        assert!(js.contains(r#""file":"app/page.tsx""#));
        assert!(js.contains(r#""line":1"#));
        assert!(js.contains("throw __nataErr;"));
    }

    #[test]
    fn test_only_page_and_api_requests_are_logged() {
        assert!(is_loggable_request("/"));
        assert!(is_loggable_request("/about"));
        assert!(is_loggable_request("/api/health"));
        assert!(!is_loggable_request("/_bundle/app/page.tsx"));
        assert!(!is_loggable_request("/_nata/rpc"));
        assert!(!is_loggable_request("/_hmr"));
        assert!(!is_loggable_request("/favicon.ico"));
        assert!(!is_loggable_request("/assets/app.js"));
        assert!(!is_loggable_request("/react@18.3.1/es2022/react.mjs"));
    }

    #[test]
    fn test_client_runtime_supports_zalo_memory_router() {
        let js = crate::client::CLIENT_RUNTIME_JS;
        assert!(js.contains("window.__NATA_CONFIG__"));
        assert!(js.contains("window.__NATA_LOCATION__"));
        assert!(js.contains("/_nata/hmr_version"));
        assert!(js.contains(r#"from "./overlay.js""#));
        assert!(!js.contains("showInlineError"));
        assert!(!js.contains("{entry_str}"));
        assert!(!js.contains("{{"));
    }
}
