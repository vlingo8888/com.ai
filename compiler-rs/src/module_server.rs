//! Serves server modules to the Bun worker (P1.3). The worker's `Bun.plugin` loads every `rsc:` and
//! `ssr:` specifier by asking the engine (`CALL load_module`), so the server runs code from the same
//! compiler as the browser: same resolver, same directives, same Server Action ids.
//!
//! - Project files and ES modules from packages are compiled with `compile_module` for the layer;
//!   their imports point at the current version of each module (`?v=`), so an edit is picked up by the
//!   next request without restarting the worker.
//! - CommonJS files of the react-server layer (React itself, most packages) cannot go through
//!   `Bun.plugin` source loading. The engine resolves their whole `require` graph with the
//!   react-server conditions and the worker evaluates it with a small CommonJS loader.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};

use regex::Regex;
use serde_json::{json, Value};

use crate::resolver::{oxc, Layer};

/// A worker CALL. Boxed: some calls use the worker themselves (fetching fonts), and the worker's
/// reader task handles calls, so the future type would otherwise refer to itself
pub fn handle_call(root: PathBuf, cmd: String, input: Value, body: Vec<u8>) -> std::pin::Pin<Box<dyn std::future::Future<Output = (Value, Vec<u8>)> + Send>> {
    Box::pin(async move { handle_call_inner(&root, &cmd, &input, body).await })
}

async fn handle_call_inner(root: &Path, cmd: &str, input: &Value, _body: Vec<u8>) -> (Value, Vec<u8>) {
    match cmd {
        "load_module" => {
            let load_blocking = |root: PathBuf, input: Value| async move {
                tokio::task::spawn_blocking(move || load(&root, &input))
                    .await
                    .unwrap_or_else(|e| (json!({ "kind": "error", "message": e.to_string() }), Vec::new(), Vec::new()))
            };
            let (reply, code, missing) = load_blocking(root.to_path_buf(), input.clone()).await;
            if missing.is_empty() {
                return (reply, code);
            }
            // `import { nanoid } from "nanoid"` in a server file, before `bun add nanoid`: install it,
            // like the browser's dependencies (deps.rs), then compile the file again
            let mut installed = false;
            for name in &missing {
                installed |= crate::deps::ensure_installed_for_worker(root, name).await.is_some();
            }
            if !installed {
                return (reply, code);
            }
            let (reply, code, _) = load_blocking(root.to_path_buf(), input.clone()).await;
            (reply, code)
        }
        "svg_to_png" => match crate::og_image::svg_to_png(input["svg"].as_str().unwrap_or("")) {
            Ok(png) => (json!({}), png),
            Err(error) => (json!({ "error": error }), Vec::new()),
        },
        "og_default_font" => match crate::og_image::default_font(root).await {
            Ok(font) => (json!({}), font),
            Err(error) => (json!({ "error": error }), Vec::new()),
        },
        "font_css" => {
            let (css, version) = crate::font_loader::stylesheet(root).await;
            (json!({ "version": version }), css.into_bytes())
        }
        "cache_get" | "cache_set" | "cache_invalidate" => (crate::cache_store::handle(root, cmd, input), Vec::new()),
        "client_reference" => match crate::rsc::client_reference_specifier(root, input["id"].as_str().unwrap_or("")) {
            Ok(specifier) => (json!({ "specifier": specifier }), Vec::new()),
            Err(error) => (json!({ "error": error }), Vec::new()),
        },
        "action_module" => {
            let id = input["id"].as_str().unwrap_or("");
            let graph = crate::module_graph::for_root(root);
            let graph = graph.read().unwrap_or_else(|e| e.into_inner());
            match graph.action(id) {
                Some((file, export)) => (json!({ "specifier": graph.specifier(file, Layer::Rsc), "export": export }), Vec::new()),
                None => (json!({ "specifier": null }), Vec::new()),
            }
        }
        _ => (json!({ "kind": "error", "message": format!("unknown engine call: {cmd}") }), Vec::new()),
    }
}

fn error_reply(err: &crate::swc_compiler::CompileError) -> (Value, Vec<u8>) {
    crate::term::error(&err.location(), Some(&err.message), Some(&err.frame));
    (json!({ "kind": "error", "message": err.message, "error": err }), Vec::new())
}

pub(crate) fn is_esm_source(path: &Path, source: &str) -> bool {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    match ext {
        "mjs" | "mts" => true,
        "cjs" | "cts" | "json" => false,
        "ts" | "tsx" | "jsx" => true,
        _ => {
            if package_type_module(path) {
                return true;
            }
            // `.js` in a CommonJS package is usually CommonJS, but bundler-style `module` entries are
            // ES modules: decide from the syntax, parsing only when both styles seem present
            static ESM: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
            static CJS: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
            let esm = ESM
                .get_or_init(|| Regex::new(r"(?m)^(?:import\s*[\w{*'\x22]|export\s+(?:[\w{*]|default\b))").unwrap())
                .is_match(source);
            let cjs = CJS.get_or_init(|| Regex::new(r"\bmodule\.exports\b|\bexports\.[\w$]+\s*=|\brequire\(").unwrap()).is_match(source);
            match (esm, cjs) {
                (true, false) => true,
                (false, _) => false,
                (true, true) => has_module_syntax(source),
            }
        }
    }
}

fn has_module_syntax(source: &str) -> bool {
    use swc_core::common::{sync::Lrc, SourceMap};
    use swc_core::ecma::ast::{ModuleItem, Program};
    let cm: Lrc<SourceMap> = Default::default();
    match crate::swc_compiler::module::parse(&cm, source, "probe.js") {
        Ok(Program::Module(m)) => m.body.iter().any(|i| matches!(i, ModuleItem::ModuleDecl(_))),
        _ => false,
    }
}

/// `"type": "module"` in the nearest package.json
fn package_type_module(path: &Path) -> bool {
    let mut dir = path.parent();
    while let Some(d) = dir {
        let pkg = d.join("package.json");
        if let Ok(text) = std::fs::read_to_string(&pkg) {
            return serde_json::from_str::<Value>(&text).ok().is_some_and(|v| v["type"] == "module");
        }
        if d.file_name().is_some_and(|n| n == "node_modules") {
            break;
        }
        dir = d.parent();
    }
    false
}

pub fn load_module(root: &Path, input: &Value) -> (Value, Vec<u8>) {
    let (reply, code, _) = load(root, input);
    (reply, code)
}

/// `load_module`, with the packages the module imports that are not installed
fn load(root: &Path, input: &Value) -> (Value, Vec<u8>, Vec<String>) {
    let raw = input["path"].as_str().unwrap_or("");
    let Some(layer) = input["layer"].as_str().and_then(Layer::parse) else {
        return (json!({ "kind": "error", "message": "missing layer" }), Vec::new(), Vec::new());
    };
    let path_str = raw.split('?').next().unwrap_or(raw);

    if let Some(name) = path_str.strip_prefix("/__nata_client_shim/") {
        return match crate::swc_compiler::module::next_client_shim_stub(root, name) {
            Some(code) => (json!({ "kind": "esm" }), code.into_bytes(), Vec::new()),
            None => (json!({ "kind": "error", "message": format!("no client shim for {name}") }), Vec::new(), Vec::new()),
        };
    }

    let file = PathBuf::from(path_str);
    if crate::image_optimizer::is_image(&file) {
        return (json!({ "kind": "esm" }), crate::image_optimizer::static_import_module(root, &file).into_bytes(), Vec::new());
    }
    let source = match std::fs::read_to_string(&file) {
        Ok(s) => s,
        Err(e) => return (json!({ "kind": "error", "message": format!("Cannot read {}: {e}", file.display()) }), Vec::new(), Vec::new()),
    };

    let in_node_modules = file.components().any(|c| c.as_os_str() == "node_modules");
    if in_node_modules && !is_esm_source(&file, &source) {
        // A "use client" CommonJS file still becomes client references
        let use_client = source.trim_start().starts_with("\"use client\"") || source.trim_start().starts_with("'use client'");
        if layer == Layer::Ssr || !use_client {
            let (reply, code) = cjs_closure(root, &file, layer);
            return (reply, code, Vec::new());
        }
    }

    match crate::module_graph::compile(root, &file, layer, &source) {
        Ok(compiled) => {
            let graph = crate::module_graph::for_root(root);
            let code = graph.read().unwrap_or_else(|e| e.into_inner()).versioned_code(&compiled.code, layer, &compiled.imports);
            dump(root, &file, layer, &code);
            (json!({ "kind": "esm", "directive": compiled.directive }), code.into_bytes(), compiled.missing_packages)
        }
        Err(err) => {
            let (reply, code) = error_reply(&err);
            (reply, code, Vec::new())
        }
    }
}

/// `COM_DUMP_MODULES=1`: writes what the worker loads to `.nata/modules/`, for debugging the compiler
fn dump(root: &Path, file: &Path, layer: Layer, code: &str) {
    if std::env::var_os("COM_DUMP_MODULES").is_none() {
        return;
    }
    let name = file.to_string_lossy().replace(['/', '\\', ':'], "_");
    let dir = root.join(".nata/modules");
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(dir.join(format!("{}.{}.js", name.trim_start_matches('_'), layer.as_str())), code);
}

fn require_regex() -> &'static Regex {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"\brequire\(\s*(?:"([^"]+)"|'([^']+)')\s*\)"#).unwrap())
}

/// The CommonJS files reachable from `entry` through `require()` (with the conditions of `layer`),
/// with how each file's `require` specifiers resolve. The sources go in the binary body, one after the
/// other (`offset` / `length` in bytes): inside the JSON meta, megabytes of React were escaped here
/// and parsed again by the worker.
fn cjs_closure(root: &Path, entry: &Path, layer: Layer) -> (Value, Vec<u8>) {
    let resolvers = oxc::resolvers(root);
    // The worker's NODE_ENV (see `apply_runner_env`) decides which build is dead code
    let unused_build = if crate::mode::is_production() { ".development." } else { ".production." };
    let mut files = serde_json::Map::new();
    let mut seen = HashSet::new();
    let mut queue = VecDeque::from([entry.to_path_buf()]);
    let mut body = Vec::new();
    // Sources already read to tell CommonJS from ESM, taken back when their file is visited
    let mut read: HashMap<PathBuf, String> = HashMap::new();
    while let Some(file) = queue.pop_front() {
        if !seen.insert(file.clone()) {
            continue;
        }
        let Some(source) = read.remove(&file).or_else(|| std::fs::read_to_string(&file).ok()) else { continue };
        let (offset, length) = (body.len(), source.len());
        body.extend_from_slice(source.as_bytes());
        let mut requires: HashMap<String, Value> = HashMap::new();
        if file.extension().is_some_and(|e| e == "json") {
            files.insert(crate::rpc::clean_path(&file), json!({ "json": true, "offset": offset, "length": length }));
            continue;
        }
        for caps in require_regex().captures_iter(&source) {
            let spec = caps.get(1).or_else(|| caps.get(2)).unwrap().as_str();
            if requires.contains_key(spec) {
                continue;
            }
            // `if (process.env.NODE_ENV === "production") require("./x.production.js") else ...`:
            // the other build is never loaded, so do not ship (and keep in memory) its source
            if spec.contains(unused_build) {
                requires.insert(spec.to_string(), json!({ "missing": spec }));
                continue;
            }
            if oxc::is_builtin(spec) {
                requires.insert(spec.to_string(), json!({ "builtin": spec }));
                continue;
            }
            match resolvers.resolve_require(layer, &file, spec) {
                Some(resolved) => {
                    let target_source = std::fs::read_to_string(&resolved.path).unwrap_or_default();
                    if resolved.path.extension().is_some_and(|e| e == "json") || !is_esm_source(&resolved.path, &target_source) {
                        requires.insert(spec.to_string(), json!({ "file": crate::rpc::clean_path(&resolved.path) }));
                        if !seen.contains(&resolved.path) {
                            read.insert(resolved.path.clone(), target_source);
                        }
                        queue.push_back(resolved.path);
                    } else {
                        // `require()` of an ES module: loaded through the layer like an import
                        requires.insert(spec.to_string(), json!({ "esm": crate::swc_compiler::module::server_specifier(layer, &resolved.path) }));
                    }
                }
                None => {
                    requires.insert(spec.to_string(), json!({ "missing": spec }));
                }
            }
        }
        files.insert(crate::rpc::clean_path(&file), json!({ "offset": offset, "length": length, "requires": requires }));
    }
    (json!({ "kind": "cjs", "layer": layer.as_str(), "entry": crate::rpc::clean_path(entry), "files": files }), body)
}
