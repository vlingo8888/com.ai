//! The browser side of a production build.
//!
//! 1. `discover`: walks the react-server module graph from every file under `app/`; each
//!    `"use client"` module it reaches is a client reference, i.e. an entry of the browser bundle.
//! 2. `bundle`: `Bun.build` bundles those entries and the client router (code splitting, tree
//!    shaking, minification, content-hashed names). The bundler reads each module from the build's
//!    internal server, so it bundles exactly what `com dev` serves, compiled for production.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use axum::Router;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tower::ServiceExt;

use crate::resolver::Layer;
use crate::swc_compiler::module::{client_reference_id, NEXT_CLIENT_SHIMS};
use crate::swc_compiler::Directive;

const CODE_EXTENSIONS: [&str; 8] = ["js", "jsx", "ts", "tsx", "mjs", "cjs", "mts", "cts"];

/// Framework modules every page loads besides its client references
pub fn framework_entries() -> Vec<String> {
    let mut entries = vec![crate::rsc::ROUTER_URL.to_string(), "/_nata/rsc/boundaries.js".to_string()];
    entries.extend(NEXT_CLIENT_SHIMS.iter().map(|(name, _)| format!("/_nata/shims/{name}")));
    entries
}

#[derive(Default, Debug)]
pub struct ClientGraph {
    /// Client reference ids of the whole app
    pub entries: BTreeSet<String>,
    /// Client reference ids reachable from each file (memoized walk)
    reach: HashMap<PathBuf, BTreeSet<String>>,
    /// Every module the server layer loads (project and packages)
    pub server_files: BTreeSet<PathBuf>,
    pub errors: Vec<String>,
}

impl ClientGraph {
    /// Client references of the given route files (page, layouts, special files)
    pub fn references_of(&self, files: &[PathBuf]) -> BTreeSet<String> {
        files.iter().filter_map(|f| self.reach.get(f)).flatten().cloned().collect()
    }
}

fn is_code(file: &Path) -> bool {
    file.extension().and_then(|e| e.to_str()).is_some_and(|e| CODE_EXTENSIONS.contains(&e))
}

fn is_route_handler(file: &Path) -> bool {
    file.file_stem().is_some_and(|s| s == "route")
}

/// Code files under `app/` (or `src/app/`) except route handlers
pub fn app_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for dir in [root.join("app"), root.join("src/app")] {
        for entry in walkdir::WalkDir::new(&dir).into_iter().filter_map(Result::ok) {
            let path = entry.path();
            if entry.file_type().is_file() && is_code(path) && !is_route_handler(path) {
                files.push(crate::rpc::canonicalize(&path).unwrap_or_else(|_| path.to_path_buf()));
            }
        }
    }
    files.sort();
    files
}

pub fn discover(root: &Path) -> ClientGraph {
    let mut graph = ClientGraph::default();
    for file in app_files(root) {
        walk(root, &file, &mut graph);
    }
    graph
}

fn walk(root: &Path, file: &Path, graph: &mut ClientGraph) -> BTreeSet<String> {
    if let Some(found) = graph.reach.get(file) {
        return found.clone();
    }
    // Cycles: a module being walked contributes nothing more to its importers
    graph.reach.insert(file.to_path_buf(), BTreeSet::new());
    graph.server_files.insert(file.to_path_buf());
    if !is_code(file) {
        return BTreeSet::new();
    }
    let Ok(source) = std::fs::read_to_string(file) else { return BTreeSet::new() };
    let in_node_modules = file.components().any(|c| c.as_os_str() == "node_modules");
    let use_client = source.trim_start().starts_with("\"use client\"") || source.trim_start().starts_with("'use client'");
    // CommonJS packages are loaded as they are by the worker (no ES imports to follow)
    if in_node_modules && !use_client && !crate::module_server::is_esm_source(file, &source) {
        return BTreeSet::new();
    }

    let compiled = match crate::module_graph::compile(root, file, Layer::Rsc, &source) {
        Ok(compiled) => compiled,
        Err(err) => {
            graph.errors.push(err.to_string());
            return BTreeSet::new();
        }
    };
    let mut found = BTreeSet::new();
    if compiled.directive == Some(Directive::UseClient) {
        let package = crate::module_graph::for_root(root).read().unwrap_or_else(|e| e.into_inner()).package_specifier(file).map(String::from);
        let id = client_reference_id(root, file, package.as_deref());
        graph.entries.insert(id.clone());
        found.insert(id);
    } else {
        for import in &compiled.imports {
            found.extend(walk(root, import, graph));
        }
    }
    graph.reach.insert(file.to_path_buf(), found.clone());
    found
}

/// Output of `client_bundler.ts`, with the gzip sizes the engine adds
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BundleResult {
    /// Entry id → URL of its bundle
    pub entries: BTreeMap<String, String>,
    /// URL → URLs it imports statically
    pub imports: BTreeMap<String, Vec<String>>,
    /// URL → gzip size
    #[serde(default)]
    pub gzip_sizes: BTreeMap<String, u64>,
    /// Files written, relative to `static/`
    #[serde(default)]
    pub files: Vec<String>,
    /// Engine URLs of every module the bundle read
    #[serde(default)]
    pub inputs: Vec<String>,
    #[serde(default)]
    pub errors: Vec<String>,
}

impl BundleResult {
    /// The file and every chunk it imports statically (what loading it downloads)
    pub fn closure(&self, url: &str) -> BTreeSet<String> {
        let mut seen = BTreeSet::new();
        let mut stack = vec![url.to_string()];
        while let Some(u) = stack.pop() {
            if seen.insert(u.clone()) {
                stack.extend(self.imports.get(&u).into_iter().flatten().cloned());
            }
        }
        seen
    }

    pub fn gzip_size<'a>(&self, urls: impl IntoIterator<Item = &'a String>) -> u64 {
        urls.into_iter().map(|u| self.gzip_sizes.get(u).copied().unwrap_or(0)).sum()
    }
}

pub const BUNDLER_SOURCE: &str = include_str!("client_bundler.ts");

/// The previous build's bundle (`.nata/cache/client-bundle.json`): reused as it is while the bundler,
/// its configuration and every module it read are unchanged
#[derive(Serialize, Deserialize)]
struct CachedBundle {
    key: String,
    /// Engine URL → blake3 of what the build server served for it
    inputs: BTreeMap<String, String>,
    result: BundleResult,
}

fn cache_file(root: &Path) -> PathBuf {
    root.join(".nata/cache/client-bundle.json")
}

/// What `app` serves at `url`, the way the bundler reads it; `None` for a module that does not load
async fn served(app: &Router, url: &str) -> Option<Vec<u8>> {
    let request = axum::http::Request::get(url).body(axum::body::Body::empty()).ok()?;
    let response = app.clone().oneshot(request).await.ok()?;
    if !response.status().is_success() {
        return None;
    }
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.ok()?;
    Some(bytes.to_vec())
}

/// Hash of what `app` serves for each input, `None` when one of them does not load
async fn hash_inputs(app: &Router, inputs: &[String]) -> Option<BTreeMap<String, String>> {
    let hashes: Vec<Option<(String, String)>> = futures_util::stream::iter(inputs.iter().map(|url| async move {
        let body = served(app, url).await?;
        Some((url.clone(), blake3::hash(&body).to_hex().to_string()))
    }))
    .buffer_unordered(32)
    .collect()
    .await;
    hashes.into_iter().collect()
}

/// The cached bundle, when nothing it was built from changed and its files are all still there
async fn reusable(root: &Path, app: &Router, key: &str, out_dir: &Path) -> Option<BundleResult> {
    let cached: CachedBundle = serde_json::from_slice(&std::fs::read(cache_file(root)).ok()?).ok()?;
    if cached.key != key || !cached.result.files.iter().all(|f| out_dir.join(f).is_file() && out_dir.join(format!("{f}.br")).is_file()) {
        return None;
    }
    let urls: Vec<String> = cached.inputs.keys().cloned().collect();
    (hash_inputs(app, &urls).await? == cached.inputs).then_some(cached.result)
}

/// Bundles `entries` into `<build>/static`, reading modules from the build server (`app`, listening at
/// `base` for the bundler process). Reuses the previous bundle when its inputs are unchanged.
pub async fn bundle(root: &Path, app: &Router, base: &str, entries: &[String]) -> Result<BundleResult, String> {
    let out_dir = crate::build::manifest::build_dir(root).join("static");
    let import_map = crate::bundler::ClientTransformer::import_map("")["imports"].clone();
    let key = {
        let mut hasher = blake3::Hasher::new();
        hasher.update(env!("CARGO_PKG_VERSION").as_bytes());
        hasher.update(BUNDLER_SOURCE.as_bytes());
        hasher.update(serde_json::json!([entries, import_map, crate::build::manifest::STATIC_URL]).to_string().as_bytes());
        hasher.finalize().to_hex().to_string()
    };
    if let Some(result) = reusable(root, app, &key, &out_dir).await {
        return Ok(result);
    }

    let nata = root.join(".nata");
    let script = nata.join("client_bundler.ts");
    std::fs::write(&script, BUNDLER_SOURCE).map_err(|e| e.to_string())?;
    let config = serde_json::json!({
        "base": base,
        "entries": entries,
        "outdir": out_dir,
        "publicPath": crate::build::manifest::STATIC_URL,
        "importMap": import_map,
    });
    let config_file = nata.join("client_bundler.json");
    std::fs::write(&config_file, config.to_string()).map_err(|e| e.to_string())?;

    let output = crate::rpc::bun_command()
        .arg(&script)
        .arg(&config_file)
        .current_dir(root)
        .env("NODE_ENV", "production")
        .envs(crate::store::env(root))
        .output()
        .await
        .map_err(|e| format!("Could not run Bun: {e}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let result_line = stdout.lines().rev().find(|l| l.starts_with("__NATA_BUNDLE__")).map(|l| &l["__NATA_BUNDLE__".len()..]);
    let Some(json) = result_line else {
        return Err(format!("The client bundler failed:\n{}{}", stdout, String::from_utf8_lossy(&output.stderr)));
    };
    let mut result: BundleResult = serde_json::from_str(json).map_err(|e| format!("client bundler output: {e}"))?;
    if !result.errors.is_empty() {
        return Err(result.errors.join("\n"));
    }

    let out = out_dir.clone();
    let files = result.files.clone();
    let sizes = tokio::task::spawn_blocking(move || precompress(&nata.join("cache/compressed"), &out, &files)).await.map_err(|e| e.to_string())??;
    let public = crate::build::manifest::STATIC_URL;
    result.gzip_sizes = sizes.into_iter().map(|(file, size)| (format!("{public}{file}"), size)).collect();

    // Remember what the bundle was built from. A module that no longer loads (it is served from the
    // bundler's own process, never again) just means the next build bundles again.
    if let Some(inputs) = hash_inputs(app, &result.inputs).await {
        let cached = CachedBundle { key, inputs, result: result.clone() };
        let file = cache_file(root);
        let _ = std::fs::create_dir_all(file.parent().unwrap_or(root));
        let _ = std::fs::write(&file, serde_json::to_vec(&cached).unwrap_or_default());
    } else {
        let _ = std::fs::remove_file(cache_file(root));
    }
    Ok(result)
}

/// Writes `.gz` and `.br` copies next to each file (served by `com start` to clients that accept
/// them), on every core, and returns each file's gzip size. Compression at the highest levels is
/// most of a build's time, so results are cached by content: the React chunks rarely change.
fn precompress(cache: &Path, dir: &Path, files: &[String]) -> Result<Vec<(String, u64)>, String> {
    let _ = std::fs::create_dir_all(cache);
    let next = std::sync::atomic::AtomicUsize::new(0);
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).min(files.len().max(1));
    let results: Vec<Result<Vec<(String, u64)>, String>> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..threads)
            .map(|_| {
                scope.spawn(|| {
                    let mut sizes = Vec::new();
                    loop {
                        let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some(file) = files.get(i) else { break };
                        let path = dir.join(file);
                        let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
                        let hash = blake3::hash(&bytes).to_hex();
                        let mut gzip_len = 0;
                        for (ext, encode) in [("gz", gzip as fn(&[u8]) -> Vec<u8>), ("br", brotli_max)] {
                            let cached = cache.join(format!("{hash}.{ext}"));
                            let encoded = match std::fs::read(&cached) {
                                Ok(encoded) => encoded,
                                Err(_) => {
                                    let encoded = encode(&bytes);
                                    let _ = std::fs::write(&cached, &encoded);
                                    encoded
                                }
                            };
                            if ext == "gz" {
                                gzip_len = encoded.len() as u64;
                            }
                            std::fs::write(path.with_file_name(format!("{}.{ext}", path.file_name().unwrap_or_default().to_string_lossy())), &encoded)
                                .map_err(|e| e.to_string())?;
                        }
                        sizes.push((file.clone(), gzip_len));
                    }
                    Ok(sizes)
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap_or_else(|_| Err("compression thread panicked".to_string()))).collect()
    });
    let mut sizes = Vec::new();
    for r in results {
        sizes.extend(r?);
    }
    // Keep only this build's copies: the cache stays the size of one build
    let used: std::collections::HashSet<String> = files
        .iter()
        .filter_map(|f| std::fs::read(dir.join(f)).ok())
        .map(|bytes| blake3::hash(&bytes).to_hex().to_string())
        .collect();
    for entry in std::fs::read_dir(cache).into_iter().flatten().flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.split('.').next().is_some_and(|hash| used.contains(hash)) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
    Ok(sizes)
}

fn gzip(bytes: &[u8]) -> Vec<u8> {
    use std::io::Write;
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
    let _ = gz.write_all(bytes);
    gz.finish().unwrap_or_default()
}

fn brotli_max(bytes: &[u8]) -> Vec<u8> {
    use std::io::Write;
    // A window just big enough for the file: same output, a fraction of the memory per thread
    let window = (usize::BITS - bytes.len().max(1).leading_zeros()).clamp(16, 22);
    let mut out = Vec::new();
    {
        let mut writer = brotli::CompressorWriter::new(&mut out, 4096, 11, window);
        let _ = writer.write_all(bytes);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("nata_build_{name}_{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn brotli_round_trips_with_a_window_sized_to_the_file() {
        use std::io::Read;
        for input in [b"".to_vec(), b"hello".repeat(3), (0..200_000u32).map(|i| (i % 251) as u8).collect()] {
            let mut out = Vec::new();
            brotli::Decompressor::new(&brotli_max(&input)[..], 4096).read_to_end(&mut out).unwrap();
            assert_eq!(out, input);
        }
    }

    #[test]
    fn precompress_writes_copies_and_reuses_them_by_content() {
        use std::io::Read;
        let dir = temp_dir("precompress");
        let (out, cache) = (dir.join("static"), dir.join("cache"));
        std::fs::create_dir_all(out.join("chunks")).unwrap();
        let code = "export const answer = 42;\n".repeat(50);
        std::fs::write(out.join("a.js"), &code).unwrap();
        std::fs::write(out.join("chunks/b.js"), "export {};").unwrap();
        let files = vec!["a.js".to_string(), "chunks/b.js".to_string()];

        let mut sizes = precompress(&cache, &out, &files).unwrap();
        sizes.sort();
        let gz = std::fs::read(out.join("a.js.gz")).unwrap();
        assert_eq!(sizes[0], ("a.js".to_string(), gz.len() as u64));
        let mut text = String::new();
        flate2::read::GzDecoder::new(&gz[..]).read_to_string(&mut text).unwrap();
        assert_eq!(text, code);
        assert!(out.join("chunks/b.js.br").is_file());

        // Same content in a new build: the copies come from the cache, not from compressing again
        let hash = blake3::hash(code.as_bytes()).to_hex();
        std::fs::write(cache.join(format!("{hash}.br")), b"cached").unwrap();
        std::fs::remove_file(out.join("a.js.br")).unwrap();
        precompress(&cache, &out, &files).unwrap();
        assert_eq!(std::fs::read(out.join("a.js.br")).unwrap(), b"cached");

        // Copies of files the build no longer has are dropped
        std::fs::write(cache.join("0000.br"), b"old").unwrap();
        precompress(&cache, &out, &files[..1]).unwrap();
        let mut left: Vec<String> = std::fs::read_dir(&cache).unwrap().flatten().map(|e| e.file_name().to_string_lossy().to_string()).collect();
        left.sort();
        assert_eq!(left, vec![format!("{hash}.br"), format!("{hash}.gz")]);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn bundle_closure_follows_static_imports() {
        let result = BundleResult {
            entries: BTreeMap::new(),
            imports: [("/a.js".to_string(), vec!["/c.js".to_string()]), ("/c.js".to_string(), vec!["/d.js".to_string()])].into_iter().collect(),
            gzip_sizes: [("/a.js", 10), ("/c.js", 20), ("/d.js", 5), ("/b.js", 100)].into_iter().map(|(k, v)| (k.to_string(), v)).collect(),
            ..Default::default()
        };
        let closure = result.closure("/a.js");
        assert_eq!(closure.iter().map(String::as_str).collect::<Vec<_>>(), vec!["/a.js", "/c.js", "/d.js"]);
        assert_eq!(result.gzip_size(&closure), 35);
    }

    #[test]
    fn framework_entries_include_router_boundaries_and_next_shims() {
        let entries = framework_entries();
        assert_eq!(entries[0], "/_nata/router.js");
        assert!(entries.contains(&"/_nata/rsc/boundaries.js".to_string()));
        assert!(entries.contains(&"/_nata/shims/next/link".to_string()));
    }
}
