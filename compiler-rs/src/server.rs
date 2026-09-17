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
use tower_http::cors::CorsLayer;

use crate::{
    bundler::ClientTransformer,
    router::AppRouter,
    rpc::{RpcExecutor, RpcPayload},
    ssr::{SsrEngine, SsrRequest},
};

#[derive(Clone)]
pub struct AppState {
    pub root_dir: PathBuf,
    pub router: Arc<AppRouter>,
    pub hmr_tx: broadcast::Sender<String>,
    pub target: String,
    pub query_logs: Arc<RwLock<VecDeque<serde_json::Value>>>,
}

pub struct DevServer;

impl DevServer {
    pub async fn run(root_dir: PathBuf, initial_port: u16, target: String) -> Result<(), Box<dyn std::error::Error>> {
        let root_dir = root_dir.canonicalize().unwrap_or(root_dir);
        // Pre-create .nata runtime and shim files
        RpcExecutor::ensure_runtime_env(&root_dir);

        if target == "zalo" {
            let _ = crate::zmp::ZmpConfigGenerator::ensure_app_config(&root_dir);
            crate::zmp::ZmpConfigGenerator::ensure_hr_config(&root_dir);
        }

        let router = Arc::new(AppRouter::scan(&root_dir));
        let (hmr_tx, _) = broadcast::channel::<String>(100);
        let query_logs = Arc::new(RwLock::new(VecDeque::with_capacity(500)));

        // Start native file system watcher for live HMR & CSS hot reload
        crate::watcher::ProjectWatcher::start(&root_dir, hmr_tx.clone());

        let state = AppState {
            root_dir: root_dir.clone(),
            router: router.clone(),
            hmr_tx: hmr_tx.clone(),
            target: target.clone(),
            query_logs: query_logs.clone(),
        };

        let app = Router::new()
            .route("/_hmr", get(ws_hmr_handler))
            .route("/favicon.ico", get(favicon_handler))
            .route("/_nata/rpc", post(rpc_handler))
            .route("/_nata/logs", get(logs_handler).delete(clear_logs_handler))
            .route("/_nata/route_info", get(route_info_handler))
            .route("/_nata/styles.css", get(styles_handler))
            .route("/_nata/shims/{*path}", get(shims_handler))
            .route("/_bundle/{*path}", get(bundle_handler))
            .route("/app-config.json", get(zmp_app_config_handler))
            .route("/app.config.json", get(zmp_app_config_handler))
            .route("/hr.config.json", get(zmp_hr_config_handler))
            .route("/hrr.config.json", get(zmp_hr_config_handler))
            .route("/zmp.json", get(zmp_app_config_handler))
            .route("/assets/app.css", get(zmp_css_handler))
            .route("/assets/app.js", get(zmp_js_handler))
            .route("/src/app.js", get(zmp_js_handler))
            .fallback(get(app_router_fallback_handler))
            .layer(CorsLayer::permissive())
            .with_state(state);

        // Auto-find next available port if in use
        let mut port = initial_port;
        let listener = loop {
            match tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port)).await {
                Ok(l) => {
                    if port != initial_port {
                        println!("  \x1b[33m▲ Port {} is in use, automatically switched to port {}\x1b[0m", initial_port, port);
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

        let (db_status_title, db_status_detail) = detect_database_env(&root_dir);
        let _ = (db_status_title, db_status_detail);

        if std::env::var("COM_MANAGED").is_err() {
            println!("   \x1b[90m-\x1b[0m Local:    \x1b[36mhttp://localhost:{}\x1b[0m", port);
            println!("\n \x1b[32m✓\x1b[0m Ready\n");
        }

        axum::serve(listener, app).await?;
        Ok(())
    }
}

fn detect_database_env(root_dir: &std::path::Path) -> (String, String) {
    let mut db_url = std::env::var("DATABASE_URL").or_else(|_| std::env::var("POSTGRES_URL")).ok();

    if db_url.is_none() {
        for env_file in &[".env.local", ".env.development", ".env"] {
            let p = root_dir.join(env_file);
            if p.exists() {
                if let Ok(content) = std::fs::read_to_string(&p) {
                    for line in content.lines() {
                        let trimmed = line.trim();
                        if trimmed.is_empty() || trimmed.starts_with('#') || !trimmed.contains('=') {
                            continue;
                        }
                        if let Some((key, val)) = trimmed.split_once('=') {
                            let k = key.trim();
                            let v = val.trim().trim_matches('"').trim_matches('\'');
                            if (k == "DATABASE_URL" || k == "POSTGRES_URL") && !v.is_empty() {
                                db_url = Some(v.to_string());
                                break;
                            }
                        }
                    }
                }
            }
            if db_url.is_some() {
                break;
            }
        }
    }

    if let Some(url) = db_url {
        // Mask password in URL
        let re = regex::Regex::new(r"://([^:@]+):([^@]+)@").unwrap();
        let masked = re.replace(&url, "://$1:***@").to_string();
        ("\x1b[1;32m● PostgreSQL\x1b[0m".to_string(), format!("\x1b[90m({})\x1b[0m", masked))
    } else {
        ("\x1b[36m○ PGlite Embedded\x1b[0m".to_string(), "\x1b[90m(data/app.db - No DATABASE_URL in .env)\x1b[0m".to_string())
    }
}

async fn ws_hmr_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_hmr_socket(socket, state.hmr_tx))
}

async fn handle_hmr_socket(mut socket: WebSocket, hmr_tx: broadcast::Sender<String>) {
    let mut rx = hmr_tx.subscribe();
    while let Ok(msg) = rx.recv().await {
        if socket.send(Message::Text(msg.into())).await.is_err() {
            break;
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
                "parameters": payload.args,
                "response": output.data,
                "duration_ms": rpc_duration,
                "status": "success",
                "error": null,
                "timestamp": now_iso,
                "queries": output.queries
            });

            // Store logs into ring buffer
            {
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
            let _ = state.hmr_tx.send(ws_event.to_string());

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
                "parameters": payload.args,
                "response": null,
                "duration_ms": rpc_duration,
                "status": "error",
                "error": err,
                "timestamp": now_iso,
                "queries": []
            });

            {
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
            let _ = state.hmr_tx.send(ws_event.to_string());

            eprintln!("  \x1b[1;31m✖ [RPC Error: {}::{}]\x1b[0m {}", mod_name, action_name, err);
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

async fn route_info_handler(
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let path = params.get("path").cloned().unwrap_or_else(|| "/".to_string());
    let dynamic_router = AppRouter::scan(&state.root_dir);

    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, "application/json; charset=utf-8".parse().unwrap());
    headers.insert(header::CACHE_CONTROL, "no-cache".parse().unwrap());

    if let Some((route, route_params)) = dynamic_router.match_route(&path) {
        let page_file = route.page_file.to_string_lossy().replace('\\', "/");
        let layout_files: Vec<String> = route
            .layout_files
            .iter()
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .collect();

        (
            StatusCode::OK,
            headers,
            Json(serde_json::json!({
                "found": true,
                "page_file": page_file,
                "layout_files": layout_files,
                "params": route_params
            })),
        )
            .into_response()
    } else {
        let err_msg = if dynamic_router.routes.is_empty() {
            "Không tìm thấy cấu trúc App Router (thư mục 'app/' hoặc 'src/app/'). Hãy đảm bảo lệnh 'com dev' được chạy đúng bên trong thư mục dự án."
        } else {
            "Không tìm thấy trang cho đường dẫn này (404 Not Found)."
        };
        (
            StatusCode::NOT_FOUND,
            headers,
            Json(serde_json::json!({
                "found": false,
                "error": err_msg
            })),
        )
            .into_response()
    }
}

async fn styles_handler(
    State(state): State<AppState>,
) -> impl IntoResponse {
    let css = crate::css_compiler::CssCompiler::compile_workspace(&state.root_dir);
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
    let custom_css = crate::css_compiler::CssCompiler::find_and_load_custom_css(&state.root_dir);
    (StatusCode::OK, headers, custom_css).into_response()
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

    // 2. Otherwise serve universal Zalo Mini App Native DOM Renderer
    let bootstrap_js = r###"// Com.AI.VN Zalo Mini App Native DOM Runtime
(function() {
  console.log("[Com.AI.VN] Initializing Zalo Mini App Native DOM Runtime...");

  // 1. Resolve Server Origin accurately from script tag URL
  var currentScript = document.currentScript || document.querySelector('script[src*="app.js"]');
  var serverOrigin = currentScript ? new URL(currentScript.src, window.location.href).origin : (window.location.origin && window.location.origin !== "null" && !window.location.origin.startsWith("file:") ? window.location.origin : "");
  var configuredBackend = "__COM_BACKEND_PLACEHOLDER__";
  var backendOrigin = (configuredBackend && !configuredBackend.startsWith("__") ? configuredBackend : (window.__COM_BACKEND_URL__ || serverOrigin));
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
    window.fetch = function(input, init) {
      if (typeof input === "string" && input.startsWith("/")) {
        input = backendOrigin + input;
      } else if (input && typeof input.url === "string" && input.url.startsWith("/")) {
        input = new Request(backendOrigin + input.url, input);
      }
      return originalFetch.call(this, input, init);
    };
  }

  // 4. Inject Mobile DevTools (Eruda) for live on-device debugging
  if (!window.__eruda_injected) {
    window.__eruda_injected = true;
    var erudaScript = document.createElement("script");
    erudaScript.src = "https://cdn.jsdelivr.net/npm/eruda";
    erudaScript.onload = function() {
      if (window.eruda) {
        window.eruda.init({ tool: ["console", "network", "elements", "resources", "info"] });
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

  // 6. Inject Tailwind Browser Engine
  if (!document.getElementById("_zmp_tailwind")) {
    var tw = document.createElement("script");
    tw.id = "_zmp_tailwind";
    tw.src = "https://cdn.jsdelivr.net/npm/@tailwindcss/browser@4";
    document.head.appendChild(tw);
  }

  // 7. Inject Import Map before module resolution
  if (!document.querySelector('script[type="importmap"]')) {
    var im = document.createElement("script");
    im.type = "importmap";
    im.textContent = JSON.stringify({
      imports: {
        "react": "https://esm.sh/react@18.3.1",
        "react/jsx-runtime": "https://esm.sh/react@18.3.1/jsx-runtime",
        "react-dom": "https://esm.sh/react-dom@18.3.1?external=react",
        "react-dom/client": "https://esm.sh/react-dom@18.3.1/client?external=react",
        "next/link": serverOrigin + "/_nata/shims/next/link",
        "next/image": serverOrigin + "/_nata/shims/next/image",
        "next/navigation": serverOrigin + "/_nata/shims/next/navigation",
        "next/router": serverOrigin + "/_nata/shims/next/router",
        "next/head": serverOrigin + "/_nata/shims/next/head",
        "next/headers": serverOrigin + "/_nata/shims/next/headers",
        "lucide-react": "https://esm.sh/lucide-react@0.460.0?external=react,react-dom",
        "framer-motion": "https://esm.sh/framer-motion@11.11.17?external=react,react-dom",
        "clsx": "https://esm.sh/clsx@2.1.1",
        "tailwind-merge": "https://esm.sh/tailwind-merge@2.5.4",
        "axios": "https://esm.sh/axios@1.7.7",
        "date-fns": "https://esm.sh/date-fns@4.1.0",
        "canvas-confetti": "https://esm.sh/canvas-confetti@1.9.4",
        "@tanstack/react-query": "https://esm.sh/@tanstack/react-query@5.59.0?external=react,react-dom",
        "@tanstack/react-table": "https://esm.sh/@tanstack/react-table@8.20.5?external=react,react-dom",
        "zustand": "https://esm.sh/zustand@5.0.0?external=react,react-dom",
        "jotai": "https://esm.sh/jotai@2.10.1?external=react,react-dom",
        "swr": "https://esm.sh/swr@2.2.5?external=react,react-dom",
        "zod": "https://esm.sh/zod@3.23.8",
        "react-hook-form": "https://esm.sh/react-hook-form@7.53.0?external=react,react-dom",
        "@hookform/resolvers": "https://esm.sh/@hookform/resolvers@3.9.0?external=react,react-dom",
        "@hookform/resolvers/zod": "https://esm.sh/@hookform/resolvers@3.9.0/zod?external=react,react-dom,zod",
        "sonner": "https://esm.sh/sonner@1.5.0?external=react,react-dom",
        "react-hot-toast": "https://esm.sh/react-hot-toast@2.4.1?external=react,react-dom",
        "next-themes": "https://esm.sh/next-themes@0.3.0?external=react,react-dom"
      }
    });
    document.head.appendChild(im);
  }

  // 8. Native DOM Mounting & Hydration
  async function bootApp() {
    var container = document.getElementById("app") || document.getElementById("root");
    if (!container) {
      container = document.createElement("div");
      container.id = "app";
      document.body.appendChild(container);
    }
    container.style.cssText = "min-height:100vh;width:100%;display:flex;flex-direction:column;box-sizing:border-box;";

    try {
      // Import React and ReactDOM Client
      var ReactMod = await import("https://esm.sh/react@18.3.1");
      var React = ReactMod.default || ReactMod;
      window.React = React;

      var ReactDOMClient = await import("https://esm.sh/react-dom@18.3.1/client?external=react");
      var ReactDOM = ReactDOMClient.default || ReactDOMClient;
      window.ReactDOM = ReactDOM;

      // Module loading cache
      var moduleCache = new Map();
      async function loadModule(specifier) {
        var clean = specifier.trim();
        if (clean.startsWith("/_bundle/")) clean = clean.slice(9);
        var normKey = clean.replace(/^\/+/, "").split("?")[0];
        if (moduleCache.has(normKey)) return moduleCache.get(normKey);

        var targetUrl = serverOrigin + "/_bundle/" + normKey;
        var mod = await import(targetUrl);
        moduleCache.set(normKey, mod);
        return mod;
      }

      function unwrapHtmlBody(node) {
        if (!node) return node;
        if (Array.isArray(node)) {
          var unwrapped = node.map(unwrapHtmlBody).filter(Boolean);
          if (unwrapped.length === 0) return null;
          if (unwrapped.length === 1) return unwrapped[0];
          return React.createElement(React.Fragment, null, ...unwrapped);
        }
        if (React.isValidElement(node)) {
          var type = node.type;
          var props = node.props || {};
          if (type === "html" || (typeof type === "string" && type.toLowerCase() === "html")) {
            return unwrapHtmlBody(props.children);
          }
          if (type === "body" || (typeof type === "string" && type.toLowerCase() === "body")) {
            return unwrapHtmlBody(props.children);
          }
          if (type === "head" || (typeof type === "string" && type.toLowerCase() === "head")) {
            return null;
          }
          return node;
        }
        return node;
      }

      function makeComponent(Comp) {
        if (!Comp) return () => null;
        var isAsync = Comp.constructor && (Comp.constructor.name === "AsyncFunction" || Comp[Symbol.toStringTag] === "AsyncFunction");
        return function DynamicComponentWrapper(props) {
          var safeProps = props || {};
          if (isAsync) {
            var [content, setContent] = React.useState(null);
            var [err, setErr] = React.useState(null);
            React.useEffect(() => {
              var active = true;
              Promise.resolve(Comp(safeProps)).then(res => {
                if (active) setContent(unwrapHtmlBody(res));
              }).catch(e => {
                if (active) setErr(e);
              });
              return () => { active = false; };
            }, [safeProps.children]);
            if (err) throw err;
            return content;
          }
          try {
            var res = Comp(safeProps);
            if (res instanceof Promise) {
              var [pContent, setPContent] = React.useState(null);
              var [pErr, setPErr] = React.useState(null);
              React.useEffect(() => {
                var pActive = true;
                res.then(r => { if (pActive) setPContent(unwrapHtmlBody(r)); }).catch(e => { if (pActive) setPErr(e); });
                return () => { pActive = false; };
              }, []);
              if (pErr) throw pErr;
              return pContent;
            }
            return unwrapHtmlBody(res);
          } catch(e) {
            throw e;
          }
        };
      }

      var currentRoot = null;
      async function renderNativeRoute(pageFile, layoutFiles, params) {
        window.__NATA_PARAMS__ = params || {};
        var pageMod = await loadModule(pageFile);
        var Page = pageMod.default || Object.values(pageMod).find(v => typeof v === "function");
        if (!Page) throw new Error("No React component found in " + pageFile);

        var RootComponent = makeComponent(Page);

        for (var i = (layoutFiles || []).length - 1; i >= 0; i--) {
          try {
            var layoutMod = await loadModule(layoutFiles[i]);
            var Layout = layoutMod.default || Object.values(layoutMod).find(v => typeof v === "function");
            if (Layout) {
              var Child = RootComponent;
              var WrappedLayout = makeComponent(Layout);
              RootComponent = (props) => React.createElement(WrappedLayout, { ...(props || {}), children: React.createElement(Child, props || {}) });
            }
          } catch(e) {
            console.warn("Could not wrap layout:", layoutFiles[i], e);
          }
        }

        if (!currentRoot) {
          container.innerHTML = "";
          currentRoot = ReactDOM.createRoot(container);
        }
        currentRoot.render(React.createElement(RootComponent, {}));
        dismissLoading();
      }

      // SPA Navigation function for Link & useRouter
      async function navigateTo(targetPath) {
        try {
          var cleanPath = targetPath.split("?")[0] || "/";
          var res = await fetch(serverOrigin + "/_nata/route_info?path=" + encodeURIComponent(cleanPath));
          var info = await res.json();
          if (info && info.found) {
            await renderNativeRoute(info.page_file, info.layout_files || [], info.params || {});
            window.scrollTo(0, 0);
          }
        } catch(e) {
          console.error("Navigation error:", e);
        }
      }
      window.__NATA_NAVIGATE__ = navigateTo;

      // Fetch initial route info (defaults to root /)
      var initialPath = "/";
      var routeRes = await fetch(serverOrigin + "/_nata/route_info?path=" + encodeURIComponent(initialPath));
      if (!routeRes.ok) {
        var errJson = await routeRes.json().catch(function() { return {}; });
        throw new Error(errJson.error || ("Không tìm thấy route: " + initialPath + " (HTTP " + routeRes.status + ")"));
      }
      var initialInfo = await routeRes.json();
      if (!initialInfo || !initialInfo.found) {
        throw new Error(initialInfo && initialInfo.error ? initialInfo.error : ("No page found for route: " + initialPath));
      }

      await renderNativeRoute(initialInfo.page_file, initialInfo.layout_files || [], initialInfo.params || {});
      console.log("[Com.AI.VN] Zalo Mini App Native DOM Rendered successfully!");
    } catch(err) {
      console.error("[Com.AI.VN] Native render error:", err);
      dismissLoading();
      var errMsg = (err && err.message) ? err.message : String(err);
      var errStack = (err && err.stack) ? err.stack : "";
      container.innerHTML = '<div style="padding:24px;font-family:sans-serif;color:#f87171;background:#0f172a;min-height:100vh;box-sizing:border-box;">' +
        '<h3 style="margin-top:0;font-size:18px;color:#f87171;">⚠️ Lỗi Khởi Chạy Giao Diện Zalo</h3>' +
        '<div style="font-size:14px;color:#fca5a5;font-weight:600;line-height:1.6;background:#1e293b;padding:12px;border-radius:8px;border-left:4px solid #ef4444;">' + errMsg + '</div>' +
        (errStack ? '<pre style="white-space:pre-wrap;word-break:break-all;font-size:11px;background:#020617;padding:12px;border-radius:8px;color:#94a3b8;margin-top:12px;">' + errStack + '</pre>' : '') +
        '<p style="font-size:12px;color:#64748b;margin-top:16px;">Bấm icon Eruda ở góc dưới màn hình để kiểm tra chi tiết Console và Network log.</p>' +
      '</div>';
    }
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", bootApp);
  } else {
    bootApp();
  }
})();
"###;
    let backend_url = std::env::var("COM_BACKEND_URL").unwrap_or_default();
    let rendered_js = bootstrap_js.replace("__COM_BACKEND_PLACEHOLDER__", &backend_url);
    (StatusCode::OK, headers, rendered_js).into_response()
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
        "next/link" | "link" => {
            r##"import React from "https://esm.sh/react@18.3.1";
export default function Link({ href, children, className, style, onClick, target, rel, replace, ...props }) {
  const handleClick = (e) => {
    if (onClick) onClick(e);
    if (!e.defaultPrevented && href && !target && !href.startsWith("http://") && !href.startsWith("https://") && !href.startsWith("mailto:") && !href.startsWith("tel:") && !href.startsWith("#")) {
      e.preventDefault();
      if (typeof window !== "undefined" && typeof window.__NATA_NAVIGATE__ === "function") {
        window.__NATA_NAVIGATE__(href, { replace });
      } else {
        if (replace) {
          window.location.replace(href);
        } else {
          window.location.href = href;
        }
      }
    }
  };
  return React.createElement("a", { href: href || "#", className, style, target, rel, onClick: handleClick, ...props }, children);
}
export { Link };
"##
        }
        "next/image" | "image" => {
            r##"import React from "https://esm.sh/react@18.3.1";
export default function Image({ src, alt, width, height, className, style, fill, priority, ...props }) {
  const imgSrc = (src && typeof src === "object") ? (src.src || src.default || "") : src;
  const imgStyle = fill
    ? { position: "absolute", height: "100%", width: "100%", left: 0, top: 0, right: 0, bottom: 0, objectFit: "cover", ...style }
    : style;
  return React.createElement("img", {
    src: imgSrc,
    alt: alt || "",
    width: fill ? undefined : width,
    height: fill ? undefined : height,
    className,
    style: imgStyle,
    loading: priority ? "eager" : "lazy",
    ...props
  });
}
export { Image };
"##
        }
        "next/navigation" | "navigation" => {
            r##"import React from "https://esm.sh/react@18.3.1";
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
    if (typeof window !== "undefined" && typeof window.__NATA_NAVIGATE__ === "function") {
      window.__NATA_NAVIGATE__(window.location.pathname + window.location.search, { replace: true });
    } else if (typeof window !== "undefined") {
      window.location.reload();
    }
  },
  prefetch: () => {}
});

export const usePathname = () => {
  const [pathname, setPathname] = React.useState(() => typeof window !== "undefined" ? window.location.pathname : "/");
  React.useEffect(() => {
    const handler = () => setPathname(window.location.pathname);
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
  const [searchParams, setSearchParams] = React.useState(() => typeof window !== "undefined" ? new URLSearchParams(window.location.search) : new URLSearchParams());
  React.useEffect(() => {
    const handler = () => setSearchParams(new URLSearchParams(window.location.search));
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
  const [params, setParams] = React.useState(() => (typeof window !== "undefined" && window.__NATA_PARAMS__) ? window.__NATA_PARAMS__ : {});
  React.useEffect(() => {
    const handler = () => setParams((typeof window !== "undefined" && window.__NATA_PARAMS__) ? window.__NATA_PARAMS__ : {});
    window.addEventListener("popstate", handler);
    window.addEventListener("_nata_navigate", handler);
    return () => {
      window.removeEventListener("popstate", handler);
      window.removeEventListener("_nata_navigate", handler);
    };
  }, []);
  return params;
};

export const redirect = (url) => {
  if (typeof window !== "undefined") {
    if (typeof window.__NATA_NAVIGATE__ === "function") {
      window.__NATA_NAVIGATE__(url, { replace: true });
    } else {
      window.location.href = url;
    }
  }
};

export const notFound = () => { throw new Error("404 Not Found"); };
export default { useRouter, usePathname, useSearchParams, useParams, redirect, notFound };
"##
        }
        "next/router" | "router" => {
            r##"import React from "https://esm.sh/react@18.3.1";
export const useRouter = () => {
  const [pathname, setPathname] = React.useState(() => typeof window !== "undefined" ? window.location.pathname : "/");
  const [search, setSearch] = React.useState(() => typeof window !== "undefined" ? window.location.search : "");
  React.useEffect(() => {
    const handler = () => {
      setPathname(window.location.pathname);
      setSearch(window.location.search);
    };
    window.addEventListener("popstate", handler);
    window.addEventListener("_nata_navigate", handler);
    return () => {
      window.removeEventListener("popstate", handler);
      window.removeEventListener("_nata_navigate", handler);
    };
  }, []);

  return {
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
    reload: () => { if (typeof window !== "undefined") window.location.reload(); },
    pathname,
    asPath: pathname + search,
    query: Object.fromEntries(new URLSearchParams(search)),
    events: { on: () => {}, off: () => {}, emit: () => {} }
  };
};
export default { useRouter };
"##
        }
        "next/head" | "head" => {
            r##"import React from "https://esm.sh/react@18.3.1";
export default function Head({ children }) {
  return React.createElement(React.Fragment, null, children);
}
export { Head };
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

async fn bundle_handler(
    State(state): State<AppState>,
    Path(path): Path<String>,
) -> impl IntoResponse {
    let clean_path = path.trim_start_matches('/');
    let tsconfig = crate::resolver::TsConfig::load_from_dir(&state.root_dir);

    match crate::resolver::PathResolver::resolve_file_with_config(&state.root_dir, clean_path, &tsconfig) {
        Some(resolved) => match tokio::fs::read_to_string(&resolved.absolute_path).await {
            Ok(content) => {
                let transformed = crate::resolver::PathResolver::transform_source_with_config(
                    &content,
                    &resolved.relative_path,
                    &tsconfig,
                );
                
                // Compile TypeScript / TSX directly via SWC Native Rust engine
                match crate::swc_compiler::SwcCompiler::compile(&transformed, &resolved.relative_path) {
                    Ok(compiled_js) => {
                        let mut headers = HeaderMap::new();
                        headers.insert(
                            header::CONTENT_TYPE,
                            "application/javascript; charset=utf-8".parse().unwrap(),
                        );
                        (StatusCode::OK, headers, compiled_js).into_response()
                    }
                    Err(err) => {
                        eprintln!("SWC Compilation Error in {}: {}", resolved.relative_path, err);
                        (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            format!("SWC Compilation Error in {}: {}", resolved.relative_path, err),
                        )
                            .into_response()
                    }
                }
            }
            Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Failed to read file").into_response(),
        },
        None => (StatusCode::NOT_FOUND, format!("Module not found: {}", clean_path)).into_response(),
    }
}

async fn app_router_fallback_handler(
    State(state): State<AppState>,
    req: axum::extract::Request,
) -> impl IntoResponse {
    let path = req.uri().path();

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
        || path.starts_with("/v")
        || path.starts_with("/node/")
        || path.starts_with("/npm/")
        || path.starts_with("/gh/")
        || (path.contains('@') && (path.ends_with(".mjs") || path.ends_with(".js") || path.ends_with(".map")))
    {
        let redirect_url = format!("https://esm.sh{}", path);
        return axum::response::Redirect::temporary(&redirect_url).into_response();
    }

    // Re-scan routes dynamically in dev mode so new layouts and pages are immediately active
    let dynamic_router = AppRouter::scan(&state.root_dir);
    if let Some((route, params)) = dynamic_router.match_route(path) {
        let custom_css = crate::css_compiler::CssCompiler::find_and_load_custom_css(&state.root_dir);

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
            params: serde_json::to_value(&params).unwrap_or_else(|_| serde_json::json!({})),
            search_params: serde_json::to_value(&query_map).unwrap_or_else(|_| serde_json::json!({})),
            cookies: raw_cookie_str,
            headers: Some(serde_json::to_value(&headers_map).unwrap_or_else(|_| serde_json::json!({}))),
        };

        let ssr_output = SsrEngine::render(&state.root_dir, ssr_req).await;

        // Check if SSR returned a redirect
        if let Some(redirect_to) = ssr_output.redirect_url {
            return axum::response::Redirect::temporary(&redirect_to).into_response();
        }

        let html = ClientTransformer::render_ssr_html_shell(
            "Com.AI.VN Application",
            &route.page_file,
            &route.layout_files,
            &custom_css,
            &ssr_output.html,
            &ssr_output.initial_state,
            &state.target,
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

    // Try serving static assets
    let static_candidate = state.root_dir.join(path.trim_start_matches('/'));
    if static_candidate.exists() && static_candidate.is_file() {
        if let Ok(bytes) = tokio::fs::read(static_candidate).await {
            return (StatusCode::OK, bytes).into_response();
        }
    }

    (StatusCode::NOT_FOUND, "Route not found in App Router").into_response()
}
