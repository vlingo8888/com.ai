//! Spec-compliant module resolution through `oxc_resolver` (webpack `enhanced-resolve` semantics):
//! package `exports` / `imports` with conditions, `browser` / `module` / `main` fields and tsconfig
//! `paths` (with `extends`). One resolver per layer, all sharing the same file-system cache.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use oxc_resolver::{AliasValue, ResolveOptions, Resolver, TsconfigDiscovery};

use super::types::Layer;

const EXTENSIONS: [&str; 8] = [".tsx", ".ts", ".jsx", ".js", ".mjs", ".cjs", ".json", ".css"];

/// The resolvers of one project root
pub struct LayerResolvers {
    root: PathBuf,
    client: Resolver,
    ssr: Resolver,
    rsc: Resolver,
    /// `require()` inside CommonJS files of the react-server layer: same as `rsc` without `import`,
    /// so a package never hands an ES module to a CommonJS `require`
    rsc_require: Resolver,
    /// `require()` inside CommonJS files of the SSR layer
    ssr_require: Resolver,
}

/// A specifier resolved to a file on disk
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedModule {
    pub path: PathBuf,
    /// Whether the file belongs to an installed package (`node_modules`)
    pub in_node_modules: bool,
}

fn registry() -> &'static Mutex<HashMap<PathBuf, Arc<LayerResolvers>>> {
    static REGISTRY: OnceLock<Mutex<HashMap<PathBuf, Arc<LayerResolvers>>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The cached resolvers of `root`, built on first use
pub fn resolvers(root: &Path) -> Arc<LayerResolvers> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let mut map = registry().lock().unwrap();
    map.entry(root.clone())
        .or_insert_with(|| Arc::new(LayerResolvers::new(&root)))
        .clone()
}

/// Drops the cached resolvers (and their file-system cache) of `root`: called when `tsconfig.json`,
/// `package.json` or `node_modules` change
pub fn invalidate(root: &Path) {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    registry().lock().unwrap().remove(&root);
}

impl LayerResolvers {
    fn new(root: &Path) -> Self {
        let has_tsconfig = root.join("tsconfig.json").is_file() || root.join("jsconfig.json").is_file();
        let path_str = |p: PathBuf| p.to_string_lossy().to_string();
        // Next.js-style `@/` and `~/` aliases for projects without a tsconfig (and as a fallback when
        // the tsconfig does not declare them)
        let default_aliases = vec![
            (
                "@".to_string(),
                vec![AliasValue::Path(path_str(root.join("src"))), AliasValue::Path(path_str(root.to_path_buf()))],
            ),
            (
                "~".to_string(),
                vec![AliasValue::Path(path_str(root.to_path_buf())), AliasValue::Path(path_str(root.join("src")))],
            ),
        ];

        // next-intl reads the project's request config through this alias (its Next.js plugin sets it)
        let intl_config = ["i18n/request.ts", "src/i18n/request.ts", "i18n/request.js", "src/i18n/request.js", "i18n.ts", "src/i18n.ts"]
            .iter()
            .map(|p| root.join(p))
            .find(|p| p.is_file());
        let mut alias = intl_config
            .map(|p| vec![("next-intl/config$".to_string(), vec![AliasValue::Path(path_str(p))])])
            .unwrap_or_default();
        // The framework's next/* modules (written by next_shims.rs); browser code never resolves
        // `next/*` (it imports /_nata/shims/next/*)
        let next_dir = root.join(".nata/next");
        if next_dir.join("headers.ts").is_file() {
            for (name, file) in crate::next_shims::SERVER_MODULES {
                alias.push((format!("next/{name}$"), vec![AliasValue::Path(path_str(next_dir.join(file)))]));
            }
        }
        let base = ResolveOptions {
            alias,
            cwd: Some(root.to_path_buf()),
            tsconfig: has_tsconfig.then_some(TsconfigDiscovery::Auto),
            extensions: EXTENSIONS.iter().map(|e| e.to_string()).collect(),
            // `import "./x.js"` in TypeScript sources refers to `./x.ts`
            extension_alias: vec![
                (".js".into(), vec![".ts".into(), ".tsx".into(), ".js".into()]),
                (".jsx".into(), vec![".tsx".into(), ".jsx".into()]),
                (".mjs".into(), vec![".mts".into(), ".mjs".into()]),
            ],
            fallback: default_aliases,
            main_files: vec!["index".into()],
            // The framework's React first when the project's is too old (see framework_react.rs)
            modules: crate::framework_react::active_modules_dir(root)
                .map(|dir| vec![dir.to_string_lossy().to_string(), "node_modules".into()])
                .unwrap_or_else(|| vec!["node_modules".into()]),
            builtin_modules: true,
            symlinks: true,
            ..ResolveOptions::default()
        };

        let with = |conditions: &[&str], main_fields: &[&str], alias_fields: bool| ResolveOptions {
            condition_names: conditions.iter().map(|c| c.to_string()).collect(),
            main_fields: main_fields.iter().map(|f| f.to_string()).collect(),
            alias_fields: if alias_fields { vec![vec!["browser".into()]] } else { vec![] },
            ..base.clone()
        };

        let client = Resolver::new(with(&["browser", "import", "module", "default"], &["browser", "module", "main"], true));
        let ssr = client.clone_with_options(with(&["node", "import", "require", "default"], &["module", "main"], false));
        let rsc = client.clone_with_options(with(
            &["react-server", "node", "import", "require", "default"],
            &["module", "main"],
            false,
        ));
        let rsc_require =
            client.clone_with_options(with(&["react-server", "node", "require", "default"], &["main"], false));
        let ssr_require = client.clone_with_options(with(&["node", "require", "default"], &["main"], false));

        Self { root: root.to_path_buf(), client, ssr, rsc, rsc_require, ssr_require }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn for_layer(&self, layer: Layer) -> &Resolver {
        match layer {
            Layer::Client => &self.client,
            Layer::Ssr => &self.ssr,
            Layer::Rsc => &self.rsc,
        }
    }

    /// Resolves `specifier` imported by the file `importer` (which need not exist: a directory's
    /// `package.json` path works for resolving from that directory). Resolving from the file, not its
    /// directory, applies the tsconfig that includes it.
    pub fn resolve(&self, layer: Layer, importer: &Path, specifier: &str) -> Option<ResolvedModule> {
        Self::finish(self.for_layer(layer).resolve_file(importer, specifier).ok()?.into_path_buf())
    }

    /// Resolves a CommonJS `require(specifier)` of the react-server layer
    pub fn resolve_rsc_require(&self, importer: &Path, specifier: &str) -> Option<ResolvedModule> {
        Self::finish(self.rsc_require.resolve_file(importer, specifier).ok()?.into_path_buf())
    }

    /// Resolves a CommonJS `require(specifier)` of a server layer
    pub fn resolve_require(&self, layer: Layer, importer: &Path, specifier: &str) -> Option<ResolvedModule> {
        let resolver = if layer == Layer::Rsc { &self.rsc_require } else { &self.ssr_require };
        Self::finish(resolver.resolve_file(importer, specifier).ok()?.into_path_buf())
    }

    /// Resolves `specifier` from the project root
    pub fn resolve_from_root(&self, layer: Layer, specifier: &str) -> Option<ResolvedModule> {
        self.resolve(layer, &self.root.join("package.json"), specifier)
    }

    fn finish(path: PathBuf) -> Option<ResolvedModule> {
        let in_node_modules = path.components().any(|c| c.as_os_str() == "node_modules");
        Some(ResolvedModule { path, in_node_modules })
    }
}

/// Node.js built-in modules (and Bun's) that are never resolved to files
pub fn is_builtin(specifier: &str) -> bool {
    if specifier.starts_with("node:") || specifier.starts_with("bun:") || specifier == "bun" {
        return true;
    }
    const BUILTINS: [&str; 40] = [
        "assert", "async_hooks", "buffer", "child_process", "cluster", "console", "constants", "crypto",
        "dgram", "diagnostics_channel", "dns", "domain", "events", "fs", "fs/promises", "http", "http2",
        "https", "inspector", "module", "net", "os", "path", "perf_hooks", "process", "punycode",
        "querystring", "readline", "repl", "stream", "stream/web", "string_decoder", "timers", "tls", "tty",
        "url", "util", "v8", "vm", "zlib",
    ];
    let base = specifier.split('/').next().unwrap_or(specifier);
    BUILTINS.contains(&specifier) || (BUILTINS.contains(&base) && !specifier.starts_with('@'))
}

/// The package name of a bare specifier (`@scope/pkg/sub` → `@scope/pkg`)
pub fn package_name(specifier: &str) -> &str {
    let mut parts = specifier.splitn(3, '/');
    let first = parts.next().unwrap_or("");
    if first.starts_with('@') {
        match parts.next() {
            Some(second) => &specifier[..first.len() + 1 + second.len()],
            None => first,
        }
    } else {
        first
    }
}

/// Whether `specifier` names a package rather than a path
pub fn is_bare(specifier: &str) -> bool {
    !(specifier.starts_with('.') || specifier.starts_with('/') || specifier.contains("://"))
}
