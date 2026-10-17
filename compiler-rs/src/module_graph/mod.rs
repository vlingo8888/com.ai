//! The module graph: every module compiled for any layer, with its imports, importers, directive,
//! exports and server actions. It is the single source of truth for
//! - invalidation: a changed file bumps its version and the version of every module importing it,
//!   so the worker loads fresh code through new specifiers (`rsc:/app/page.tsx?v=3`) instead of
//!   restarting, and the browser knows which modules to hot-update;
//! - the server action manifest (action id → module and export);
//! - the client reference ids of package files (`/_nata/deps/<specifier>`).

#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, RwLock};

use crate::compile_cache::{resolution_fingerprint, CompileCache};
use crate::resolver::Layer;
use crate::swc_compiler::{compile_module, CompileError, CompiledModule, Directive, ModuleCompileOptions};

pub type ModuleKey = (PathBuf, Layer);

#[derive(Debug, Clone)]
pub struct ModuleNode {
    pub file: PathBuf,
    pub layer: Layer,
    pub hash: [u8; 32],
    pub imports: Vec<PathBuf>,
    pub directive: Option<Directive>,
    pub exports: Vec<String>,
    pub actions: Vec<(String, String)>,
    pub version: u32,
}

#[derive(Default)]
pub struct ModuleGraph {
    nodes: HashMap<ModuleKey, ModuleNode>,
    importers: HashMap<ModuleKey, HashSet<PathBuf>>,
    /// Bumped versions of modules not compiled yet (or dropped), kept so they are never reused
    versions: HashMap<ModuleKey, u32>,
    actions: HashMap<String, (PathBuf, String)>,
    package_specifiers: HashMap<PathBuf, String>,
    /// How many times server modules were invalidated since the worker started
    pub server_invalidations: u64,
    /// `next/font` requests of every compiled module, by hash
    fonts: std::collections::BTreeMap<String, crate::swc_compiler::fonts::FontRequest>,
    /// Bumped when a new font request appears (the font stylesheet changes)
    pub fonts_version: u32,
}

impl ModuleGraph {
    pub fn version(&self, file: &Path, layer: Layer) -> u32 {
        self.versions.get(&(file.to_path_buf(), layer)).copied().unwrap_or(0)
    }

    pub fn node(&self, file: &Path, layer: Layer) -> Option<&ModuleNode> {
        self.nodes.get(&(file.to_path_buf(), layer))
    }

    pub fn contains(&self, file: &Path) -> bool {
        [Layer::Client, Layer::Ssr, Layer::Rsc].iter().any(|l| self.nodes.contains_key(&(file.to_path_buf(), *l)))
    }

    /// Records a compiled module and its edges
    pub fn upsert(&mut self, file: &Path, layer: Layer, hash: [u8; 32], compiled: &CompiledModule) {
        let key = (file.to_path_buf(), layer);
        if let Some(old) = self.nodes.get(&key) {
            for import in &old.imports {
                if let Some(set) = self.importers.get_mut(&(import.clone(), layer)) {
                    set.remove(file);
                }
            }
            for (id, _) in &old.actions {
                self.actions.remove(id);
            }
        }
        for import in &compiled.imports {
            self.importers.entry((import.clone(), layer)).or_default().insert(file.to_path_buf());
        }
        for (spec, target) in &compiled.package_imports {
            self.package_specifiers.entry(target.clone()).or_insert_with(|| spec.clone());
        }
        for (id, export) in &compiled.actions {
            self.actions.insert(id.clone(), (file.to_path_buf(), export.clone()));
        }
        for font in &compiled.fonts {
            if self.fonts.insert(font.hash.clone(), font.clone()).is_none() {
                self.fonts_version += 1;
            }
        }
        let version = self.version(file, layer);
        self.nodes.insert(
            key,
            ModuleNode {
                file: file.to_path_buf(),
                layer,
                hash,
                imports: compiled.imports.clone(),
                directive: compiled.directive,
                exports: compiled.exports.clone(),
                actions: compiled.actions.clone(),
                version,
            },
        );
    }

    /// A file changed: bumps its version in every layer and, in the server layers, the version of
    /// every module importing it (transitively). Returns the affected modules, the changed file first.
    pub fn invalidate(&mut self, file: &Path) -> Vec<ModuleKey> {
        let mut affected = Vec::new();
        for layer in [Layer::Client, Layer::Ssr, Layer::Rsc] {
            let start = (file.to_path_buf(), layer);
            if !self.nodes.contains_key(&start) && !self.importers.contains_key(&start) {
                continue;
            }
            let mut seen = HashSet::new();
            let mut queue = VecDeque::from([file.to_path_buf()]);
            while let Some(current) = queue.pop_front() {
                if !seen.insert(current.clone()) {
                    continue;
                }
                let key = (current.clone(), layer);
                let version = self.versions.entry(key.clone()).or_insert(0);
                *version += 1;
                if let Some(node) = self.nodes.get_mut(&key) {
                    node.version = *version;
                }
                affected.push(key.clone());
                // Browser modules are fetched by stable URLs; only server modules need their
                // importers to request the new version
                if layer != Layer::Client {
                    if let Some(importers) = self.importers.get(&key) {
                        queue.extend(importers.iter().cloned());
                    }
                }
            }
            if layer != Layer::Client {
                self.server_invalidations += 1;
            }
        }
        affected
    }

    /// Forgets every server module (the worker restarted: Bun's module registry is empty again)
    pub fn reset_server_layers(&mut self) {
        self.nodes.retain(|(_, layer), _| *layer == Layer::Client);
        self.importers.retain(|(_, layer), _| *layer == Layer::Client);
        self.server_invalidations = 0;
    }

    /// Every `next/font` request seen so far
    pub fn fonts(&self) -> Vec<crate::swc_compiler::fonts::FontRequest> {
        self.fonts.values().cloned().collect()
    }

    /// The module and export a server action id refers to
    pub fn action(&self, id: &str) -> Option<&(PathBuf, String)> {
        self.actions.get(id)
    }

    pub fn package_specifier(&self, file: &Path) -> Option<&str> {
        self.package_specifiers.get(file).map(|s| s.as_str())
    }

    /// The namespaced, versioned specifier the worker imports a server module by
    pub fn specifier(&self, file: &Path, layer: Layer) -> String {
        let base = crate::swc_compiler::module::server_specifier(layer, file);
        match self.version(file, layer) {
            0 => base,
            v => format!("{base}?v={v}"),
        }
    }

    /// Points the imports of compiled server code at the current version of each module
    pub fn versioned_code(&self, code: &str, layer: Layer, imports: &[PathBuf]) -> String {
        let mut out = code.to_string();
        for import in imports {
            let version = self.version(import, layer);
            if version == 0 {
                continue;
            }
            let base = crate::swc_compiler::module::server_specifier(layer, import);
            out = out.replace(&format!("\"{base}\""), &format!("\"{base}?v={version}\""));
        }
        out
    }
}

fn registry() -> &'static Mutex<HashMap<PathBuf, Arc<RwLock<ModuleGraph>>>> {
    static GRAPHS: OnceLock<Mutex<HashMap<PathBuf, Arc<RwLock<ModuleGraph>>>>> = OnceLock::new();
    GRAPHS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The module graph of a project root
pub fn for_root(root: &Path) -> Arc<RwLock<ModuleGraph>> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    registry().lock().unwrap().entry(root).or_default().clone()
}

/// Whether React Server Components are on (`COM_RSC=0` keeps the client-rendered pipeline)
pub fn rsc_enabled() -> bool {
    rsc_enabled_by_env() && !RSC_UNSUPPORTED.load(std::sync::atomic::Ordering::Relaxed)
}

/// `COM_RSC=0` turns React Server Components off
pub fn rsc_enabled_by_env() -> bool {
    !matches!(std::env::var("COM_RSC").as_deref(), Ok("0") | Ok("false") | Ok("off"))
}

/// Falls back to the client-rendered pipeline (React 19 unavailable)
pub fn disable_rsc() {
    RSC_UNSUPPORTED.store(true, std::sync::atomic::Ordering::Relaxed);
}

/// Set when the project's React cannot run Server Components (React 18 and older)
static RSC_UNSUPPORTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The major version of the project's React: the installed one, else the `package.json` range
/// (`^18.3.1` → 18); `None` when unknown (not declared, `latest`, a tag)
pub fn react_major(root: &Path) -> Option<u64> {
    let major = |v: &str| {
        let digits: String = v.trim_start_matches(|c: char| !c.is_ascii_digit()).chars().take_while(|c| c.is_ascii_digit()).collect();
        digits.parse::<u64>().ok()
    };
    if let Some(installed) = crate::deps::installed_version(root, "react") {
        return major(&installed);
    }
    let manifest: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(root.join("package.json")).ok()?).ok()?;
    ["dependencies", "devDependencies", "peerDependencies"]
        .iter()
        .find_map(|k| manifest[*k]["react"].as_str())
        .and_then(major)
}

/// React Server Components need React 19. A project on an older React runs on the framework's own
/// React 19 (like Next.js); only when that cannot be installed does it keep the client-rendered
/// pipeline (like `COM_RSC=0`). Returns the project's version in that case.
pub async fn check_react_support(root: &Path) -> Option<u64> {
    let major = react_major(root).filter(|major| *major < 19)?;
    if !rsc_enabled_by_env() || crate::framework_react::ensure(root).await {
        return None;
    }
    disable_rsc();
    Some(major)
}

/// The versioned `rsc:` specifier of a server module (route handler, RPC module) when React Server
/// Components are on, so it is loaded through the engine; `None` keeps Bun's own loading
pub fn server_module_specifier(root: &Path, file: impl AsRef<Path>) -> Option<String> {
    if !rsc_enabled() {
        return None;
    }
    let file = file.as_ref();
    let file = file.canonicalize().unwrap_or_else(|_| file.to_path_buf());
    Some(for_root(root).read().unwrap_or_else(|e| e.into_inner()).specifier(&file, Layer::Rsc))
}

/// Compiles `file` for `layer` (through the persistent cache) and records it in the graph.
/// The returned code still has unversioned server specifiers; see `ModuleGraph::versioned_code`.
pub fn compile(root: &Path, file: &Path, layer: Layer, source: &str) -> Result<CompiledModule, CompileError> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let graph = for_root(&root);
    let package_specifier = graph.read().unwrap().package_specifier(file).map(|s| s.to_string());
    let rel = crate::swc_compiler::module::relative(&root, file);
    let hash = *blake3::hash(source.as_bytes()).as_bytes();

    let cache = CompileCache::for_root(&root);
    let refresh = layer == Layer::Client && !rel.contains("node_modules/") && refresh_enabled() && crate::framework_react::package_version(&root, "react-refresh").is_some();
    // Browser modules use the production JSX runtime with production dependencies
    let development = layer != Layer::Client || crate::deps::deps_node_env() == "development";
    let cache_key = CompileCache::key(
        &format!("{}#{}#{}#{}#{}", rel, layer.as_str(), package_specifier.as_deref().unwrap_or(""), refresh, development),
        source,
        &resolution_fingerprint(&root),
    );
    let cached = cache
        .as_ref()
        .and_then(|c| c.get(&cache_key))
        .and_then(|json| serde_json::from_str::<CompiledModule>(&json).ok());
    let compiled = match cached {
        Some(compiled) => compiled,
        None => {
            let compiled = compile_module(
                source,
                &ModuleCompileOptions {
                    root: &root,
                    file,
                    layer,
                    development,
                    refresh,
                    package_specifier: package_specifier.as_deref(),
                },
            )?;
            if let (Some(cache), Ok(json)) = (&cache, serde_json::to_string(&compiled)) {
                cache.put(&cache_key, &json);
            }
            compiled
        }
    };
    graph.write().unwrap().upsert(file, layer, hash, &compiled);
    Ok(compiled)
}

/// React Fast Refresh for browser modules (`COM_FAST_REFRESH=0` turns it off; never with the
/// production builds of dependencies, which React Refresh refuses to run with)
pub fn refresh_enabled() -> bool {
    !matches!(std::env::var("COM_FAST_REFRESH").as_deref(), Ok("0") | Ok("false") | Ok("off"))
        && crate::deps::deps_node_env() == "development"
}
