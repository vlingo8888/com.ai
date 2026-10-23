//! `com build`: a production build in `.nata/build/` (ROADMAP P3.1).
//!
//! 1. The app is served internally in production mode (`ServeMode::Build`, on 127.0.0.1): production
//!    React, no Fast Refresh, inlined `NODE_ENV=production`.
//! 2. Client references are discovered from the react-server graph and bundled with the router
//!    (`client.rs`).
//! 3. The stylesheet is compiled and minified once.
//! 4. Pages are prerendered when their render uses no request-time API: routes without dynamic
//!    segments, and the params of `generateStaticParams`. A `revalidate` makes them ISR pages.
//! 5. `manifest.json` records it all for `com start`, and the route table is printed.

pub mod client;
pub mod manifest;

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::time::Instant;

use futures_util::StreamExt;
use serde_json::Value;

use crate::router::types::RouteEntry;
use crate::server::{AppState, DevServer, ServeMode};
use manifest::{BuildManifest, PrerenderedPage, RouteKind, RouteSummary};

const PRERENDER_CONCURRENCY: usize = 8;
/// Routes prerendered at once (each renders up to `PRERENDER_CONCURRENCY` paths)
const ROUTE_CONCURRENCY: usize = 4;

pub async fn run(root: PathBuf, target: String) -> Result<(), String> {
    let started = Instant::now();
    crate::mode::check_production_bun()?;
    profile_mark("Starting the engine");
    let state = DevServer::prepare(root, target.clone()).await;
    let root = state.root_dir.clone();
    let out = manifest::build_dir(&root);
    // `static/` is kept: the client bundle is reused when its inputs did not change (client.rs)
    clean_except(&out, "static");
    std::fs::create_dir_all(&out).map_err(|e| format!("{}: {e}", out.display()))?;
    crate::next_config::ensure_loaded(&root).await;
    // React Server Components packages (installed on first use, like in development)
    crate::rsc::runtime_specifier(&root).await?;

    // Internal server the bundler reads modules from (and nothing else can reach)
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.map_err(|e| e.to_string())?;
    let base = format!("http://{}", listener.local_addr().map_err(|e| e.to_string())?);
    let app = DevServer::app(state.clone(), ServeMode::Build);
    let server = app.clone();
    tokio::spawn(async move {
        let _ = axum::serve(listener, server).await;
    });

    step("Compiling server components");
    let graph = client::discover(&root);
    if !graph.errors.is_empty() {
        return Err(format!("Failed to compile:\n\n{}", graph.errors.join("\n\n")));
    }

    step(&format!("Bundling client components ({} client references)", graph.entries.len()));
    let mut entries = client::framework_entries();
    entries.extend(graph.entries.iter().cloned());
    let bundle = client::bundle(&root, &app, &base, &entries).await?;
    let mut manifest = BuildManifest::default();
    for id in &entries {
        let url = bundle.entries.get(id).ok_or_else(|| format!("The bundle has no output for {id}"))?;
        manifest.client.insert(id.clone(), url.clone());
    }
    manifest.router = manifest.client.remove(crate::rsc::ROUTER_URL).unwrap_or_default();
    manifest.router_preload = bundle.closure(&manifest.router).into_iter().filter(|u| *u != manifest.router).collect();

    step("Compiling the stylesheet");
    match stylesheet(&root).await {
        crate::css_compiler::stylesheet::Stylesheet::Compiled(css) => {
            std::fs::write(out.join("styles.css"), &css).map_err(|e| e.to_string())?;
            manifest.styles = Some("styles.css".to_string());
        }
        crate::css_compiler::stylesheet::Stylesheet::Browser(_) => {
            crate::term::warn("Tailwind CSS is unavailable: pages are styled in the browser (install tailwindcss for a compiled stylesheet)");
        }
    }

    manifest.build_id = build_id(&manifest);
    let router = state.router.read().await;
    manifest.intercepting = router.routes.iter().any(|r| r.pattern.contains("(.") || r.page_file.to_string_lossy().contains("(."));
    // From here on documents use the bundles
    manifest::install(manifest.clone());

    step("Prerendering static pages");
    let routes: Vec<RouteEntry> = router.routes.clone();
    drop(router);
    let pages_dir = out.join("pages");
    std::fs::create_dir_all(&pages_dir).map_err(|e| e.to_string())?;
    let built: Vec<Result<(RouteSummary, Vec<(String, PrerenderedPage)>), String>> =
        futures_util::stream::iter(routes.iter().map(|route| build_route(&state, &root, route))).buffered(ROUTE_CONCURRENCY).collect().await;
    for result in built {
        let (summary, pages) = result?;
        manifest.routes.push(summary);
        manifest.pages.extend(pages);
    }
    for summary in manifest.routes.iter_mut() {
        let Some(route) = routes.iter().find(|r| r.pattern == summary.pattern && r.is_api == (summary.kind == RouteKind::Api)) else { continue };
        if route.is_api {
            continue;
        }
        let refs = graph.references_of(&route_files(&root, route));
        let mut urls = bundle.closure(&manifest.router);
        for id in refs {
            if let Some(url) = bundle.entries.get(&id) {
                urls.extend(bundle.closure(url));
            }
        }
        summary.first_load_js = bundle.gzip_size(&urls);
    }

    profile_mark("Writing the manifest");
    manifest::save(&root, &manifest)?;
    manifest::install(manifest.clone());
    profile_mark("");
    print_report(&manifest, started.elapsed());
    Ok(())
}

/// Empties `dir`, except its entry `keep`
fn clean_except(dir: &Path, keep: &str) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        if entry.file_name() == keep {
            continue;
        }
        let path = entry.path();
        let _ = if path.is_dir() { std::fs::remove_dir_all(&path) } else { std::fs::remove_file(&path) };
    }
}

/// The project stylesheet, reused from the previous build when no file Tailwind may scan changed
/// (`.nata/cache/styles.css`): compiling it is a worker round trip and a scan of the whole project
async fn stylesheet(root: &Path) -> crate::css_compiler::stylesheet::Stylesheet {
    use crate::css_compiler::stylesheet::Stylesheet;
    let key = styles_key(root);
    let (css_file, key_file) = (root.join(".nata/cache/styles.css"), root.join(".nata/cache/styles.key"));
    if std::fs::read_to_string(&key_file).is_ok_and(|k| k == key) {
        if let Ok(css) = std::fs::read_to_string(&css_file) {
            return Stylesheet::Compiled(css);
        }
    }
    let sheet = crate::css_compiler::stylesheet::project_stylesheet(root).await;
    if let Stylesheet::Compiled(css) = &sheet {
        let _ = std::fs::create_dir_all(root.join(".nata/cache"));
        if std::fs::write(&css_file, css).is_ok() {
            let _ = std::fs::write(&key_file, &key);
        }
    }
    sheet
}

/// Fingerprint of every project file (path, size, modification time), outside dependencies, build
/// output and hidden folders. Tailwind scans the project for class names, and the lockfile stands for
/// the versions of Tailwind and its plugins.
fn styles_key(root: &Path) -> String {
    const SKIP: &[&str] = &["node_modules", "dist", "build", "out", "target", "coverage"];
    let mut hasher = blake3::Hasher::new();
    hasher.update(env!("CARGO_PKG_VERSION").as_bytes());
    let walker = walkdir::WalkDir::new(root).sort_by_file_name().into_iter().filter_entry(|e| {
        let name = e.file_name().to_string_lossy();
        e.depth() == 0 || !(name.starts_with('.') && e.file_type().is_dir() || e.file_type().is_dir() && SKIP.contains(&name.as_ref()))
    });
    for entry in walker.filter_map(Result::ok).filter(|e| e.file_type().is_file()) {
        let Ok(meta) = entry.metadata() else { continue };
        let modified = meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_nanos()).unwrap_or(0);
        hasher.update(entry.path().strip_prefix(root).unwrap_or(entry.path()).to_string_lossy().as_bytes());
        hasher.update(&meta.len().to_le_bytes());
        hasher.update(&modified.to_le_bytes());
    }
    hasher.finalize().to_hex().to_string()
}

fn step(message: &str) {
    profile_mark(message);
    if std::env::var_os("COM_BUILD_QUIET").is_none() {
        println!("   \x1b[2m○\x1b[0m {message}");
    }
}

/// `COM_BUILD_PROFILE=1`: prints how long each build step took
fn profile_mark(next: &str) {
    static LAST: std::sync::Mutex<Option<(Instant, String)>> = std::sync::Mutex::new(None);
    if std::env::var_os("COM_BUILD_PROFILE").is_none() {
        return;
    }
    let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((at, name)) = last.as_ref() {
        eprintln!("   [profile] {name}: {}ms", at.elapsed().as_millis());
    }
    *last = Some((Instant::now(), next.to_string()));
}

fn build_id(manifest: &BuildManifest) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(manifest.router.as_bytes());
    for (k, v) in &manifest.client {
        hasher.update(k.as_bytes());
        hasher.update(v.as_bytes());
    }
    hasher.update(format!("{:?}", std::time::SystemTime::now()).as_bytes());
    hasher.finalize().to_hex()[..16].to_string()
}

/// Every file whose client references the route's first load needs
fn route_files(root: &Path, route: &RouteEntry) -> Vec<PathBuf> {
    let mut files = vec![route.page_file.clone()];
    files.extend(route.layout_files.iter().cloned());
    for seg in &route.segments_files {
        files.extend([&seg.template, &seg.error, &seg.loading, &seg.not_found].into_iter().flatten().cloned());
    }
    files.extend(route.global_error_file.iter().cloned());
    files.into_iter().map(|f| {
        let f = if f.is_absolute() { f } else { root.join(f) };
        f.canonicalize().unwrap_or(f)
    }).collect()
}

/// `/blog/[slug]` + `{slug: "a"}` → `/blog/a`; catch-alls take arrays; `None` when a param is missing
pub fn fill_pattern(pattern: &str, params: &serde_json::Map<String, Value>) -> Option<String> {
    let mut out = String::new();
    for segment in pattern.split('/').filter(|s| !s.is_empty()) {
        let (name, catch_all, optional) = if let Some(n) = segment.strip_prefix("[[...").and_then(|s| s.strip_suffix("]]")) {
            (Some(n), true, true)
        } else if let Some(n) = segment.strip_prefix("[...").and_then(|s| s.strip_suffix(']')) {
            (Some(n), true, false)
        } else if let Some(n) = segment.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            (Some(n), false, false)
        } else {
            (None, false, false)
        };
        let Some(name) = name else {
            out.push('/');
            out.push_str(segment);
            continue;
        };
        let parts: Vec<String> = match params.get(name) {
            Some(Value::Array(list)) if catch_all => list.iter().map(value_text).collect(),
            Some(v @ (Value::String(_) | Value::Number(_))) => vec![value_text(v)],
            None | Some(Value::Null) if optional => Vec::new(),
            _ => return None,
        };
        if parts.is_empty() && !optional {
            return None;
        }
        for part in parts {
            out.push('/');
            out.push_str(&encode_segment(&part));
        }
    }
    Some(if out.is_empty() { "/".to_string() } else { out })
}

fn value_text(v: &Value) -> String {
    v.as_str().map(String::from).unwrap_or_else(|| v.to_string())
}

fn encode_segment(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~!$&'()*+,;=:@".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Where a prerendered path is stored under `pages/`
pub fn page_file_name(path: &str) -> String {
    let trimmed = path.trim_matches('/');
    if trimmed.is_empty() {
        "index".to_string()
    } else {
        // Params come from generateStaticParams: never let one climb out of `pages/`
        crate::router::types::percent_decode(trimmed)
            .replace(['\\', ':', '*', '?', '"', '<', '>', '|'], "_")
            .split('/')
            .map(|part| if part.is_empty() || part == "." || part == ".." { "_" } else { part })
            .collect::<Vec<_>>()
            .join("/")
    }
}

async fn static_paths(root: &Path, route: &RouteEntry) -> Result<Option<Vec<String>>, String> {
    if route.param_names.is_empty() {
        return Ok(Some(vec![fill_pattern(&route.pattern, &Default::default()).unwrap_or_else(|| "/".into())]));
    }
    let runtime = crate::rsc::runtime_specifier(root).await?;
    let specifiers: Vec<Option<String>> = route
        .layout_files
        .iter()
        .chain(std::iter::once(&route.page_file))
        .map(|f| crate::module_graph::server_module_specifier(root, root.join(f)))
        .collect();
    let reply = crate::worker_pool::call(root, "static_params", serde_json::json!({ "runtime": runtime, "specifiers": specifiers }), Vec::new())
        .await
        .map_err(|e| e.to_string())?;
    crate::ssr::worker::print_logs(reply.stdout(), reply.stderr());
    if let Some(err) = reply.error() {
        return Err(format!("generateStaticParams of {} failed: {err}", route.pattern));
    }
    let Some(list) = reply.output().as_array() else { return Ok(None) };
    let mut paths = Vec::new();
    for params in list {
        let Some(obj) = params.as_object() else { continue };
        match fill_pattern(&route.pattern, obj) {
            Some(path) => paths.push(path),
            None => crate::term::warn(&format!("generateStaticParams of {} returned {params} without every param", route.pattern)),
        }
    }
    Ok(Some(paths))
}

fn page_request<'a>(path: &'a str, route: &'a RouteEntry, params: HashMap<String, String>, mode: crate::rsc::Mode) -> crate::rsc::PageRequest<'a> {
    crate::rsc::PageRequest {
        pathname: path,
        query: "",
        route: Some(route),
        params,
        cookies: None,
        headers: [("host".to_string(), "localhost".to_string())].into_iter().collect(),
        router_state: Vec::new(),
        mode,
        not_found_segments: Vec::new(),
        navigated_from: None,
    }
}

/// Renders `path` as a document and as a Flight payload; `Err` for a render error, `Ok(Err(reason))`
/// when the page is dynamic
pub async fn prerender(state: &AppState, path: &str, route: &RouteEntry) -> Result<Result<(crate::rsc::Rendered, crate::rsc::Rendered), String>, String> {
    let root = &state.root_dir;
    let params = {
        let router = state.router.read().await;
        router.match_route(path).map(|(_, p)| p).unwrap_or_default()
    };
    let stylesheet = crate::css_compiler::stylesheet::project_head_tags(root).await;
    let html = crate::rsc::render_collect(root, &state.target, &stylesheet, page_request(path, route, params.clone(), crate::rsc::Mode::Html)).await?;
    if !html.is_static() {
        return Ok(Err(html.dynamic.unwrap_or_else(|| format!("status {}", html.status))));
    }
    let flight = crate::rsc::render_collect(root, &state.target, &stylesheet, page_request(path, route, params, crate::rsc::Mode::Flight)).await?;
    if let Some(reason) = flight.dynamic.clone() {
        return Ok(Err(reason));
    }
    Ok(Ok((html, flight)))
}

/// The route's summary and its prerendered pages
async fn build_route(state: &AppState, root: &Path, route: &RouteEntry) -> Result<(RouteSummary, Vec<(String, PrerenderedPage)>), String> {
    let mut summary = RouteSummary { pattern: route.pattern.clone(), kind: RouteKind::Dynamic, reason: None, paths: Vec::new(), first_load_js: 0 };
    let mut pages = Vec::new();
    if route.is_api {
        summary.kind = RouteKind::Api;
        return Ok((summary, pages));
    }
    let Some(paths) = static_paths(root, route).await? else {
        summary.reason = Some("dynamic segment without generateStaticParams".to_string());
        return Ok((summary, pages));
    };

    let rendered: Vec<(String, Result<Result<(crate::rsc::Rendered, crate::rsc::Rendered), String>, String>)> =
        futures_util::stream::iter(paths.into_iter().map(|path| async move {
            let result = prerender(state, &path, route).await;
            (path, result)
        }))
        .buffer_unordered(PRERENDER_CONCURRENCY)
        .collect()
        .await;

    let pages_dir = manifest::build_dir(root).join("pages");
    let mut min_revalidate: Option<u64> = None;
    let mut written = BTreeSet::new();
    for (path, result) in rendered {
        let (html, flight) = match result {
            Err(err) => return Err(format!("Error prerendering {path}:\n{err}")),
            Ok(Err(reason)) => {
                // One dynamic path makes the route dynamic
                summary.reason = Some(reason);
                summary.paths.clear();
                return Ok((summary, Vec::new()));
            }
            Ok(Ok(pair)) => pair,
        };
        let name = page_file_name(&path);
        let html_file = format!("pages/{name}.html");
        let flight_file = format!("pages/{name}.rsc");
        if let Some(parent) = pages_dir.join(format!("{name}.html")).parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        for (file, body) in [(&html_file, &html.body), (&flight_file, &flight.body)] {
            crate::prod::write_precompressed(&manifest::build_dir(root).join(file), body)?;
        }
        let revalidate = match (html.revalidate, flight.revalidate) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
        min_revalidate = match (min_revalidate, revalidate) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
        let tags: BTreeSet<String> = html.tags.iter().chain(flight.tags.iter()).cloned().collect();
        pages.push((path.clone(), PrerenderedPage { status: html.status, html: html_file, flight: Some(flight_file), revalidate, tags: tags.into_iter().collect() }));
        written.insert(path);
    }
    summary.kind = if min_revalidate.is_some() { RouteKind::Isr } else { RouteKind::Static };
    summary.paths = written.into_iter().collect();
    Ok((summary, pages))
}

fn format_size(bytes: u64) -> String {
    if bytes >= 1024 {
        format!("{:.1} kB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} B")
    }
}

fn print_report(manifest: &BuildManifest, elapsed: std::time::Duration) {
    if std::env::var_os("COM_BUILD_QUIET").is_some() {
        return;
    }
    let width = manifest.routes.iter().map(|r| r.pattern.len()).max().unwrap_or(5).max(11) + 2;
    println!("\n   \x1b[1mRoute (app)\x1b[0m{}\x1b[1mFirst Load JS\x1b[0m", " ".repeat(width + 2 - "Route (app)".len()));
    let count = manifest.routes.len();
    for (i, route) in manifest.routes.iter().enumerate() {
        let branch = if count == 1 { "─" } else if i == 0 { "┌" } else if i + 1 == count { "└" } else { "├" };
        let size = if route.kind == RouteKind::Api { String::new() } else { format_size(route.first_load_js) };
        println!("   {branch} {} {:<width$} {size}", route.kind.symbol(), route.pattern);
        let shown = if route.pattern.contains('[') { route.paths.iter().take(3).collect::<Vec<_>>() } else { Vec::new() };
        for path in &shown {
            println!("   {}   ├ {path}", if i + 1 == count { " " } else { "│" });
        }
        if route.pattern.contains('[') && route.paths.len() > 3 {
            println!("   {}   └ [+{} more paths]", if i + 1 == count { " " } else { "│" }, route.paths.len() - 3);
        }
    }
    let shared = manifest.router_preload.len() + 1;
    println!("\n   + First Load JS shared by all: router and React ({shared} files)");
    println!("\n   ○  (Static)   prerendered as static content");
    println!("   ◐  (ISR)      prerendered, regenerated in the background after revalidate");
    println!("   ƒ  (Dynamic)  server-rendered on demand\n");
    println!("   \x1b[32m✓\x1b[0m Compiled in {:.1}s → {}\n", elapsed.as_secs_f64(), manifest::BUILD_DIR);
    // The CLI handed this process over to the engine (`execve`): print what it used to print after
    if std::env::var_os("COM_BUILD_NEXT_STEPS").is_some() {
        println!("   \x1b[1mNext steps:\x1b[0m\n   \x1b[2m-\x1b[0m \x1b[36mcom start\x1b[0m \x1b[2m# Serve the production build\x1b[0m\n");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(v: Value) -> serde_json::Map<String, Value> {
        v.as_object().unwrap().clone()
    }

    #[test]
    fn fills_dynamic_catch_all_and_optional_segments() {
        assert_eq!(fill_pattern("/", &params(serde_json::json!({}))).as_deref(), Some("/"));
        assert_eq!(fill_pattern("/blog/[slug]", &params(serde_json::json!({ "slug": "hello world" }))).as_deref(), Some("/blog/hello%20world"));
        assert_eq!(fill_pattern("/docs/[...path]", &params(serde_json::json!({ "path": ["a", "b"] }))).as_deref(), Some("/docs/a/b"));
        assert_eq!(fill_pattern("/shop/[[...slug]]", &params(serde_json::json!({}))).as_deref(), Some("/shop"));
        assert_eq!(fill_pattern("/[lang]/post/[id]", &params(serde_json::json!({ "lang": "vi", "id": 7 }))).as_deref(), Some("/vi/post/7"));
        assert_eq!(fill_pattern("/blog/[slug]", &params(serde_json::json!({}))), None);
        assert_eq!(fill_pattern("/docs/[...path]", &params(serde_json::json!({ "path": [] }))), None);
    }

    #[test]
    fn clean_keeps_only_the_named_entry() {
        let dir = std::env::temp_dir().join(format!("nata_build_clean_{}", std::process::id()));
        std::fs::create_dir_all(dir.join("static")).unwrap();
        std::fs::create_dir_all(dir.join("pages")).unwrap();
        std::fs::write(dir.join("static/a.js"), "").unwrap();
        std::fs::write(dir.join("manifest.json"), "{}").unwrap();
        clean_except(&dir, "static");
        let left: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.file_name()).collect();
        assert_eq!(left, vec![std::ffi::OsString::from("static")]);
        assert!(dir.join("static/a.js").is_file());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn styles_key_follows_project_files_but_not_dependencies_or_build_output() {
        let dir = std::env::temp_dir().join(format!("nata_build_styles_{}", std::process::id()));
        std::fs::create_dir_all(dir.join("app")).unwrap();
        std::fs::create_dir_all(dir.join("node_modules/x")).unwrap();
        std::fs::create_dir_all(dir.join(".nata/build")).unwrap();
        std::fs::write(dir.join("app/page.tsx"), "<p className=\"a\" />").unwrap();
        let key = styles_key(&dir);
        std::fs::write(dir.join("node_modules/x/index.js"), "x").unwrap();
        std::fs::write(dir.join(".nata/build/styles.css"), "x").unwrap();
        assert_eq!(styles_key(&dir), key);
        std::fs::write(dir.join("app/page.tsx"), "<p className=\"a b\" />").unwrap();
        assert_ne!(styles_key(&dir), key);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn page_files_are_safe_relative_names() {
        assert_eq!(page_file_name("/"), "index");
        assert_eq!(page_file_name("/blog/a"), "blog/a");
        assert_eq!(page_file_name("/blog/hello%20world"), "blog/hello world");
        assert_eq!(page_file_name("/a/b:c"), "a/b_c");
        assert_eq!(page_file_name("/blog/%2E%2E/%2E%2E/x"), "blog/_/_/x");
        assert_eq!(page_file_name("/blog/..%2F..%2Fetc"), "blog/_/_/etc");
    }
}
