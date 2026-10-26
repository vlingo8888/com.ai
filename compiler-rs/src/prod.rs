//! `com start`: serves a production build (ROADMAP P3.2).
//!
//! - Bundles (`/_nata/static/*`): from RAM, precompressed (brotli / gzip), `immutable`.
//! - Prerendered pages: documents and Flight payloads from RAM with ETag / `304`. ISR pages
//!   (`revalidate`) are served stale while one background render replaces them; `revalidatePath()` /
//!   `revalidateTag()` drop them, and the next request renders them again.
//! - Everything else (dynamic pages, route handlers, Server Actions) goes to the worker pool;
//!   responses are compressed on the fly.
//! - `/_nata/health`, graceful shutdown on SIGINT / SIGTERM (requests in flight finish).

use std::collections::{HashMap, HashSet, VecDeque};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock, RwLock};
use std::time::Instant;

use axum::body::{Body, Bytes};
use axum::extract::{Path as UrlPath, State};
use axum::http::{header, HeaderMap, HeaderValue, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use tower_http::compression::Predicate;

use crate::build::manifest::{self, BuildManifest, RouteKind};
use crate::router::types::RouteEntry;
use crate::server::{AppState, DevServer, ServeMode};

static STARTED: OnceLock<Instant> = OnceLock::new();

/// `com start` is serving (pages may come from the page cache)
pub fn active() -> bool {
    STARTED.get().is_some()
}

pub async fn run(root: PathBuf, port: u16, hostname: String, target: String) -> Result<(), String> {
    let root = crate::rpc::canonicalize(&root).unwrap_or(root);
    crate::mode::check_production_bun()?;
    let manifest = manifest::load(&root)?;
    let build_id = manifest.build_id.clone();
    manifest::install(manifest.clone());
    let _ = STARTED.set(Instant::now());
    load_pages(&root, &manifest);

    let state = DevServer::prepare(root.clone(), target).await;
    let app = DevServer::app(state, ServeMode::Start).layer(
        tower_http::compression::CompressionLayer::new().br(true).gzip(true).compress_when(
            // Pages and route handlers; bundles and cached pages are precompressed
            tower_http::compression::predicate::SizeAbove::new(1024)
                .and(tower_http::compression::predicate::NotForContentType::GRPC)
                .and(tower_http::compression::predicate::NotForContentType::IMAGES)
                .and(tower_http::compression::predicate::NotForContentType::const_new("text/event-stream")),
        ),
    );

    let listener = tokio::net::TcpListener::bind((hostname.as_str(), port)).await.map_err(|e| format!("Could not listen on {hostname}:{port}: {e}"))?;
    if std::env::var("COM_MANAGED").is_err() {
        println!("   - Local:        http://localhost:{port}");
        println!("   - Build:        {build_id}\n");
    }
    crate::term::ready();
    axum::serve(listener, app).with_graceful_shutdown(shutdown_signal()).await.map_err(|e| e.to_string())?;
    crate::worker_pool::shutdown_all().await;
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut s) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            s.recv().await;
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    crate::term::info("Shutting down: finishing requests in flight");
}

// ---------- precompressed bodies ----------

#[derive(Clone)]
pub struct Encoded {
    pub identity: Bytes,
    pub gzip: Bytes,
    pub br: Bytes,
    pub etag: String,
}

impl Encoded {
    /// From a body and its `.gz` / `.br` copies made by the build (an empty copy is not offered)
    fn precompressed(identity: Bytes, gzip: Bytes, br: Bytes) -> Self {
        let etag = format!("\"{}\"", &blake3::hash(&identity).to_hex()[..20]);
        Self { identity, gzip, br, etag }
    }

    /// What it costs to keep in memory
    fn size(&self) -> usize {
        self.identity.len() + self.gzip.len() + self.br.len()
    }

    pub fn new(body: Bytes, quality: u32) -> Self {
        let etag = format!("\"{}\"", &blake3::hash(&body).to_hex()[..20]);
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::new(quality.min(9)));
        let _ = gz.write_all(&body);
        let gzip = Bytes::from(gz.finish().unwrap_or_default());
        let mut br = Vec::new();
        {
            let mut writer = brotli::CompressorWriter::new(&mut br, 4096, quality.min(11), 22);
            let _ = writer.write_all(&body);
        }
        Self { identity: body, gzip, br: Bytes::from(br), etag }
    }

    /// The best encoding the client accepts
    fn pick(&self, req: &HeaderMap) -> (Option<&'static str>, Bytes) {
        let accept = req.get(header::ACCEPT_ENCODING).and_then(|v| v.to_str().ok()).unwrap_or("");
        let accepts = |name: &str| accept.split(',').any(|p| p.split(';').next().is_some_and(|n| n.trim() == name));
        if accepts("br") && !self.br.is_empty() {
            (Some("br"), self.br.clone())
        } else if accepts("gzip") && !self.gzip.is_empty() {
            (Some("gzip"), self.gzip.clone())
        } else {
            (None, self.identity.clone())
        }
    }

    pub fn respond(&self, req: &HeaderMap, status: StatusCode, headers: &[(String, String)]) -> Response {
        let not_modified = req.get(header::IF_NONE_MATCH).and_then(|v| v.to_str().ok()).is_some_and(|v| v.split(',').any(|t| t.trim() == self.etag));
        let mut builder = Response::builder().header(header::ETAG, &self.etag).header(header::VARY, "Accept-Encoding");
        for (k, v) in headers {
            builder = builder.header(k.as_str(), v.as_str());
        }
        if not_modified && status == StatusCode::OK {
            return builder.status(StatusCode::NOT_MODIFIED).body(Body::empty()).unwrap_or_default();
        }
        let (encoding, body) = self.pick(req);
        if let Some(encoding) = encoding {
            builder = builder.header(header::CONTENT_ENCODING, encoding);
        }
        builder.status(status).header(header::CONTENT_LENGTH, body.len()).body(Body::from(body)).unwrap_or_default()
    }
}

// ---------- static bundles ----------

fn static_files() -> &'static RwLock<HashMap<String, Arc<Encoded>>> {
    static F: OnceLock<RwLock<HashMap<String, Arc<Encoded>>>> = OnceLock::new();
    F.get_or_init(Default::default)
}

/// `file.<ext>` (a copy precompressed by the build), empty when missing
fn read_sibling(file: &Path, ext: &str) -> Bytes {
    std::fs::read(format!("{}.{ext}", file.display())).map(Bytes::from).unwrap_or_default()
}

/// A memory budget in bytes, from an environment variable in MB
fn cache_limit(var: &str, default_mb: usize) -> usize {
    std::env::var(var).ok().and_then(|v| v.parse::<usize>().ok()).unwrap_or(default_mb) * 1024 * 1024
}

/// Writes a prerendered page with `.gz` and `.br` copies, so `com start` loads them instead of
/// compressing every page again at each boot
pub fn write_precompressed(file: &Path, body: &[u8]) -> Result<(), String> {
    let encoded = Encoded::new(Bytes::copy_from_slice(body), PRERENDER_QUALITY);
    std::fs::write(file, body).map_err(|e| e.to_string())?;
    for (ext, copy) in [("gz", &encoded.gzip), ("br", &encoded.br)] {
        std::fs::write(format!("{}.{ext}", file.display()), copy).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn content_type(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or("") {
        "js" | "mjs" => "application/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" | "map" => "application/json",
        "wasm" => "application/wasm",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "ttf" => "font/ttf",
        _ => "application/octet-stream",
    }
}

/// A file of the build's `static/` directory, `None` for paths outside it
fn load_static(root: &Path, path: &str) -> Option<Arc<Encoded>> {
    if path.split('/').any(|p| p == ".." || p.is_empty()) || path.ends_with(".br") || path.ends_with(".gz") {
        return None;
    }
    if let Some(hit) = static_files().read().unwrap_or_else(|e| e.into_inner()).get(path) {
        return Some(hit.clone());
    }
    let file = manifest::build_dir(root).join("static").join(path);
    let identity = Bytes::from(std::fs::read(&file).ok()?);
    let encoded = Arc::new(Encoded::precompressed(identity, read_sibling(&file, "gz"), read_sibling(&file, "br")));
    // Up to `COM_STATIC_CACHE_MB` (default 32) in memory; past it files are read from disk, where the
    // OS page cache keeps the hot ones anyway
    let mut files = static_files().write().unwrap_or_else(|e| e.into_inner());
    let used: usize = files.values().map(|f| f.size()).sum();
    if used + encoded.size() <= cache_limit("COM_STATIC_CACHE_MB", 32) {
        files.insert(path.to_string(), encoded.clone());
    }
    Some(encoded)
}

pub async fn static_handler(State(state): State<AppState>, UrlPath(path): UrlPath<String>, headers: HeaderMap) -> Response {
    let path = path.trim_start_matches('/');
    match load_static(&state.root_dir, path) {
        Some(file) => file.respond(
            &headers,
            StatusCode::OK,
            &[
                (header::CONTENT_TYPE.to_string(), content_type(path).to_string()),
                // Names carry a content hash: cache forever
                (header::CACHE_CONTROL.to_string(), "public, max-age=31536000, immutable".to_string()),
            ],
        ),
        None => (StatusCode::NOT_FOUND, "Not found").into_response(),
    }
}

pub async fn health_handler() -> Response {
    let build = manifest::current().map(|m| m.build_id.clone()).unwrap_or_default();
    let uptime = STARTED.get().map(|s| s.elapsed().as_secs()).unwrap_or(0);
    let mut res = axum::Json(serde_json::json!({ "status": "ok", "buildId": build, "uptime": uptime })).into_response();
    res.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    res
}

// ---------- page cache ----------

pub struct Variant {
    pub body: Encoded,
    pub headers: Vec<(String, String)>,
}

pub struct CachedPage {
    pub status: StatusCode,
    pub html: Variant,
    pub flight: Option<Variant>,
    pub rendered: Instant,
    pub revalidate: Option<u64>,
    pub tags: Vec<String>,
    regenerating: AtomicBool,
}

impl CachedPage {
    fn is_stale(&self) -> bool {
        self.revalidate.is_some_and(|r| self.rendered.elapsed().as_secs() >= r)
    }
}

impl CachedPage {
    fn size(&self) -> usize {
        self.html.body.size() + self.flight.as_ref().map_or(0, |f| f.body.size())
    }
}

#[derive(Default)]
struct PageCache {
    pages: HashMap<String, Arc<CachedPage>>,
    /// Paths in the order they were cached and the bytes they hold: pages rendered at runtime (params
    /// outside generateStaticParams) would otherwise grow the cache without limit
    order: VecDeque<String>,
    bytes: usize,
    /// Routes found dynamic at runtime (a page of a static route used a request-time API)
    dynamic_routes: HashSet<String>,
}

fn cache() -> &'static RwLock<PageCache> {
    static C: OnceLock<RwLock<PageCache>> = OnceLock::new();
    C.get_or_init(Default::default)
}

/// Compression level of pages rendered while serving
const RUNTIME_QUALITY: u32 = 5;
/// Compression level of prerendered pages (done once, by `com build`)
const PRERENDER_QUALITY: u32 = 9;

impl PageCache {
    fn insert(&mut self, path: String, page: Arc<CachedPage>) {
        self.bytes += page.size();
        match self.pages.insert(path.clone(), page) {
            Some(old) => self.bytes -= old.size(),
            None => self.order.push_back(path),
        }
        // Over `COM_PAGE_CACHE_MB` (default 64): drop the oldest, rendered again on their next request
        let limit = cache_limit("COM_PAGE_CACHE_MB", 64);
        while self.bytes > limit && self.pages.len() > 1 {
            let Some(oldest) = self.order.pop_front() else { break };
            if let Some(old) = self.pages.remove(&oldest) {
                self.bytes -= old.size();
            }
        }
    }

    fn retain(&mut self, keep: impl Fn(&CachedPage) -> bool) {
        self.pages.retain(|_, page| keep(page));
        let pages = &self.pages;
        self.order.retain(|p| pages.contains_key(p));
        self.bytes = self.pages.values().map(|p| p.size()).sum();
    }
}

fn variant(body: Vec<u8>, headers: Vec<(String, String)>, quality: u32) -> Variant {
    let headers = headers.into_iter().filter(|(k, _)| !k.eq_ignore_ascii_case("content-length") && !k.eq_ignore_ascii_case("cache-control")).collect();
    Variant { body: Encoded::new(Bytes::from(body), quality), headers }
}

fn load_pages(root: &Path, manifest: &BuildManifest) {
    let dir = manifest::build_dir(root);
    let mut cache = cache().write().unwrap_or_else(|e| e.into_inner());
    for (path, page) in &manifest.pages {
        let Some(html) = load_variant(&dir.join(&page.html), vec![("content-type".into(), "text/html; charset=utf-8".into())]) else { continue };
        let flight_headers = vec![("content-type".into(), "text/x-component".into()), ("vary".into(), "RSC, Nata-Router-State".into())];
        let flight = page.flight.as_ref().and_then(|f| load_variant(&dir.join(f), flight_headers));
        let entry = CachedPage {
            status: StatusCode::from_u16(page.status).unwrap_or(StatusCode::OK),
            html,
            flight,
            rendered: Instant::now(),
            revalidate: page.revalidate,
            tags: page.tags.clone(),
            regenerating: AtomicBool::new(false),
        };
        cache.insert(path.clone(), Arc::new(entry));
    }
}

/// A prerendered file with the build's compressed copies; builds older than those are compressed here
fn load_variant(file: &Path, headers: Vec<(String, String)>) -> Option<Variant> {
    let body = std::fs::read(file).ok()?;
    let (gzip, br) = (read_sibling(file, "gz"), read_sibling(file, "br"));
    if gzip.is_empty() || br.is_empty() {
        return Some(variant(body, headers, PRERENDER_QUALITY));
    }
    Some(Variant { body: Encoded::precompressed(Bytes::from(body), gzip, br), headers })
}

/// `revalidatePath()` / `revalidateTag()`: drops the pages carrying one of `tags`
pub fn invalidate_tags(tags: &[String]) {
    if !active() {
        return;
    }
    let mut cache = cache().write().unwrap_or_else(|e| e.into_inner());
    cache.retain(|page| !page.tags.iter().any(|t| tags.iter().any(|p| crate::cache_store::tag_matches(p, t))));
}

fn cache_control(page: &CachedPage) -> String {
    match page.revalidate {
        Some(r) => format!("s-maxage={r}, stale-while-revalidate"),
        None => "s-maxage=31536000".to_string(),
    }
}

fn respond(page: &CachedPage, flight: bool, req: &HeaderMap, cache_state: &str) -> Option<Response> {
    let variant = if flight { page.flight.as_ref()? } else { &page.html };
    let mut headers = variant.headers.clone();
    headers.push(("cache-control".into(), cache_control(page)));
    headers.push(("x-nata-cache".into(), cache_state.into()));
    Some(variant.body.respond(req, page.status, &headers))
}

async fn render_page(state: &AppState, path: &str, route: &RouteEntry) -> Result<Option<Arc<CachedPage>>, String> {
    match crate::build::prerender(state, path, route).await? {
        Ok((html, flight)) => {
            let page = Arc::new(CachedPage {
                status: StatusCode::from_u16(html.status).unwrap_or(StatusCode::OK),
                revalidate: match (html.revalidate, flight.revalidate) {
                    (Some(a), Some(b)) => Some(a.min(b)),
                    (a, b) => a.or(b),
                },
                tags: html.tags.iter().chain(flight.tags.iter()).cloned().collect::<HashSet<_>>().into_iter().collect(),
                html: variant(html.body, html.headers, RUNTIME_QUALITY),
                flight: Some(variant(flight.body, flight.headers, RUNTIME_QUALITY)),
                rendered: Instant::now(),
                regenerating: AtomicBool::new(false),
            });
            cache().write().unwrap_or_else(|e| e.into_inner()).insert(path.to_string(), page.clone());
            Ok(Some(page))
        }
        Err(reason) => {
            crate::term::warn(&format!("{} is dynamic at runtime ({reason}): rendering it for every request", route.pattern));
            cache().write().unwrap_or_else(|e| e.into_inner()).dynamic_routes.insert(route.pattern.clone());
            Ok(None)
        }
    }
}

/// A page request served from the page cache, or `None` to render it like a dynamic page
pub async fn serve_page(state: &AppState, method: &Method, headers: &HeaderMap, path: &str, route: &RouteEntry) -> Option<Response> {
    if !active() || !matches!(*method, Method::GET | Method::HEAD) || headers.contains_key("next-action") {
        return None;
    }
    let manifest = manifest::current()?;
    let flight = headers.get("rsc").is_some_and(|v| v == "1");
    if flight && manifest.intercepting {
        return None;
    }
    // Draft Mode renders fresh content
    if headers.get(header::COOKIE).and_then(|v| v.to_str().ok()).is_some_and(|c| c.contains("__prerender_bypass=")) {
        return None;
    }
    let kind = manifest.routes.iter().find(|r| r.pattern == route.pattern && r.kind != RouteKind::Api).map(|r| r.kind)?;
    if kind == RouteKind::Dynamic || cache().read().unwrap_or_else(|e| e.into_inner()).dynamic_routes.contains(&route.pattern) {
        return None;
    }

    let hit = cache().read().unwrap_or_else(|e| e.into_inner()).pages.get(path).cloned();
    if let Some(page) = hit {
        if page.is_stale() {
            // Stale-while-revalidate: one background render at a time
            if !page.regenerating.swap(true, Ordering::SeqCst) {
                let (state, path, route, page_ref) = (state.clone(), path.to_string(), route.clone(), page.clone());
                tokio::spawn(async move {
                    if let Err(err) = render_page(&state, &path, &route).await {
                        crate::term::error(&format!("Could not regenerate {path}"), Some(&err), None);
                        page_ref.regenerating.store(false, Ordering::SeqCst);
                    }
                });
            }
            return respond(&page, flight, headers, "STALE");
        }
        return respond(&page, flight, headers, "HIT");
    }

    // Not cached yet (a param outside generateStaticParams) or revalidated: render it now
    match render_page(state, path, route).await {
        Ok(Some(page)) => respond(&page, flight, headers, "MISS"),
        Ok(None) => None,
        Err(err) => {
            crate::term::error(&format!("Error rendering {path}"), Some(&err), None);
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_brotli_then_gzip_then_identity() {
        let body = Encoded::new(Bytes::from("hello ".repeat(200)), 5);
        let mut headers = HeaderMap::new();
        assert_eq!(body.pick(&headers).0, None);
        headers.insert(header::ACCEPT_ENCODING, HeaderValue::from_static("gzip, deflate"));
        assert_eq!(body.pick(&headers).0, Some("gzip"));
        headers.insert(header::ACCEPT_ENCODING, HeaderValue::from_static("gzip, br;q=1"));
        assert_eq!(body.pick(&headers).0, Some("br"));
        assert!(body.br.len() < body.identity.len() && body.gzip.len() < body.identity.len());
    }

    #[test]
    fn answers_304_for_a_matching_etag() {
        let body = Encoded::new(Bytes::from_static(b"page"), 5);
        let mut headers = HeaderMap::new();
        headers.insert(header::IF_NONE_MATCH, HeaderValue::from_str(&body.etag).unwrap());
        assert_eq!(body.respond(&headers, StatusCode::OK, &[]).status(), StatusCode::NOT_MODIFIED);
        assert_eq!(body.respond(&HeaderMap::new(), StatusCode::OK, &[]).status(), StatusCode::OK);
    }

    #[test]
    fn static_paths_cannot_leave_the_build() {
        let root = std::env::temp_dir();
        assert!(load_static(&root, "../secret.js").is_none());
        assert!(load_static(&root, "a//b.js").is_none());
        assert!(load_static(&root, "app.js.br").is_none());
    }

    #[test]
    fn isr_pages_go_stale_after_revalidate() {
        let page = CachedPage {
            status: StatusCode::OK,
            html: variant(b"x".to_vec(), Vec::new(), 1),
            flight: None,
            rendered: Instant::now() - std::time::Duration::from_secs(10),
            revalidate: Some(5),
            tags: Vec::new(),
            regenerating: AtomicBool::new(false),
        };
        assert!(page.is_stale());
        assert_eq!(cache_control(&page), "s-maxage=5, stale-while-revalidate");
    }
}
