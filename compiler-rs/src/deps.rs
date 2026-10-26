//! Browser builds of npm dependencies, served from `/_nata/deps/<specifier>` (replaces esm.sh).
//!
//! - The package comes from the project's `node_modules`, so client and server run the same version
//!   and `com dev` works offline. A package that is not installed yet is added with `bun add`
//!   (Bun's global cache makes this fast and usually offline-capable).
//! - Each specifier is bundled once by the bundler process (deps_process.rs: Bun.build, or Rolldown
//!   with `COM_BUNDLER=rolldown`; CJS converted to ESM with named exports) into `.nata/deps/<name>@<version>/<subpath>.js`,
//!   a hard link to the machine-wide copy in `~/.com/cache/deps/` that other projects reuse (cleared
//!   with `com cache clean`); every other bare import inside it stays
//!   external and points back to `/_nata/deps/...`, so each package (React especially) exists once.
//! - If the package cannot be installed (offline, unknown name) the request falls back to esm.sh.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

/// `@scope/pkg/sub/path` -> (`@scope/pkg`, `sub/path`)
pub fn split_specifier(spec: &str) -> (String, String) {
    let spec = spec.trim_start_matches('/');
    let mut parts = spec.splitn(if spec.starts_with('@') { 3 } else { 2 }, '/');
    let name = if spec.starts_with('@') {
        let scope = parts.next().unwrap_or("");
        let pkg = parts.next().unwrap_or("");
        format!("{scope}/{pkg}")
    } else {
        parts.next().unwrap_or("").to_string()
    };
    let subpath = parts.next().unwrap_or("").to_string();
    (name, subpath)
}

/// npm package names: lowercase URL-safe characters, optional scope; rejects path tricks
pub fn is_valid_specifier(spec: &str) -> bool {
    let (name, subpath) = split_specifier(spec);
    let name_ok = !name.is_empty()
        && name.chars().all(|c| c.is_ascii_alphanumeric() || "@/._-".contains(c))
        && !name.starts_with('.')
        && name.matches('/').count() == usize::from(name.starts_with('@'));
    let subpath_ok = subpath.split('/').all(|seg| seg != ".." && seg != ".")
        && subpath.chars().all(|c| c.is_ascii_alphanumeric() || "/._-@".contains(c));
    name_ok && subpath_ok
}

pub fn installed_version(root: &Path, name: &str) -> Option<String> {
    let manifest = std::fs::read_to_string(crate::store::packages_dir(root).join(name).join("package.json")).ok()?;
    let json: serde_json::Value = serde_json::from_str(&manifest).ok()?;
    json.get("version")?.as_str().map(|v| v.to_string())
}

/// Bumped whenever the bundler's output changes, so bundles cached by an older engine are rebuilt
/// (v5: older engines bundled a fake zmp-sdk written into node_modules; v6: re-export-only ESM files
/// such as victory-vendor/d3-shape were wrapped as CommonJS and lost their exports; v7: `#name` subpath
/// imports such as vfile's `#minpath` were left external and fetched from `/_nata/deps/#name`; v8: a file
/// a package exports as another specifier is imported from it, so `use-intl` and `use-intl/react` share
/// one React context; v9: likewise for wildcard exports imported by subpath, so `konva` and the
/// `konva/lib/Core.js` of react-konva share one Konva object)
const BUNDLE_FORMAT: &str = "v9";

/// Where the bundled module for `spec` at `version` is cached
pub fn cache_path(root: &Path, spec: &str, version: &str) -> PathBuf {
    let (name, subpath) = split_specifier(spec);
    let file = if subpath.is_empty() { "index".to_string() } else { subpath.replace('/', "__") };
    root.join(".nata")
        .join("deps")
        .join(BUNDLE_FORMAT)
        .join(format!("{}@{}", name.replace('/', "__"), version))
        .join(format!("{file}.js"))
}

/// The machine-wide copy of a bundle (`~/.com/cache/deps/...`, see `com cache`): every project with the
/// same package version reuses it instead of bundling again. The bundler may add a fingerprint to the
/// name (bundles whose shape depends on the project, see `packageEntries` in deps_bundler.ts)
pub fn shared_cache_path(spec: &str, version: &str) -> PathBuf {
    let (name, subpath) = split_specifier(spec);
    let file = if subpath.is_empty() { "index".to_string() } else { subpath.replace('/', "__") };
    crate::framework_react::com_home()
        .join("cache")
        .join("deps")
        .join(BUNDLE_FORMAT)
        .join(format!("{}@{}", name.replace('/', "__"), version))
        .join(format!("{file}.js"))
}

/// esm.sh URL used when a package cannot be installed locally
pub fn esm_sh_url(spec: &str) -> String {
    format!("https://esm.sh/{spec}?external=react,react-dom")
}

fn install_locks() -> &'static Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>> {
    static LOCKS: OnceLock<Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>> = OnceLock::new();
    LOCKS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn failed_installs() -> &'static Mutex<HashSet<String>> {
    static FAILED: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    FAILED.get_or_init(|| Mutex::new(HashSet::new()))
}

/// Installs `name` with `bun add` unless it is already present; returns the installed version.
/// Concurrent requests for the same package wait for a single install; a failed install is not
/// retried during this process (the esm.sh fallback is used instead).
pub async fn ensure_installed(root: &Path, name: &str) -> Option<String> {
    install(root, name, true).await
}

/// `ensure_installed` for a module the worker is waiting on (module_server.rs): the worker is not
/// restarted, since it is in the middle of loading that module and has cached no failure yet
pub async fn ensure_installed_for_worker(root: &Path, name: &str) -> Option<String> {
    install(root, name, false).await
}

async fn install(root: &Path, name: &str, restart_worker: bool) -> Option<String> {
    if let Some(version) = installed_version(root, name) {
        return Some(version);
    }
    if failed_installs().lock().map(|f| f.contains(name)).unwrap_or(false) {
        return None;
    }
    let lock = install_locks().lock().unwrap_or_else(|e| e.into_inner()).entry(name.to_string()).or_default().clone();
    let _guard = lock.lock().await;
    if let Some(version) = installed_version(root, name) {
        return Some(version);
    }

    crate::term::event(&format!("Installing {name}..."));
    let started = std::time::Instant::now();
    let Some(mut cmd) = add_command(root, &[name], false) else {
        crate::term::warn(&format!("Could not install {name} into the shared store (run `com pkg add {name}`), loading it from esm.sh instead"));
        if let Ok(mut failed) = failed_installs().lock() {
            failed.insert(name.to_string());
        }
        return None;
    };
    cmd.current_dir(root).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::piped());
    crate::worker_pool::apply_runner_env(&mut cmd, root);
    let result = cmd.output().await;

    match (result, installed_version(root, name)) {
        (Ok(out), Some(version)) if out.status.success() => {
            crate::term::success(&format!(
                "Installed {name}@{version} in {}",
                crate::term::format_ms(started.elapsed().as_secs_f64() * 1000.0)
            ));
            // The resolvers (and the worker) may have cached a failed resolution of this package
            crate::resolver::oxc::invalidate(root);
            if restart_worker {
                crate::worker_pool::restart(root).await;
            }
            Some(version)
        }
        (result, _) => {
            let detail = match result {
                Ok(out) => String::from_utf8_lossy(&out.stderr).trim().to_string(),
                Err(e) => e.to_string(),
            };
            crate::term::warn(&format!("Could not install {name}, loading it from esm.sh instead: {detail}"));
            if let Ok(mut failed) = failed_installs().lock() {
                failed.insert(name.to_string());
            }
            None
        }
    }
}

/// `bun add` (`node_modules`), or `com pkg add` for a project on the shared store; `None` when the
/// project is on the store and the CLI is unknown
fn add_command(root: &Path, specs: &[&str], dev: bool) -> Option<tokio::process::Command> {
    if crate::store::uses_store(root) {
        return crate::store::add_command(root, specs, dev);
    }
    let mut cmd = crate::rpc::bun_command();
    cmd.arg("add");
    if dev {
        cmd.arg("-d");
    }
    cmd.args(specs);
    Some(cmd)
}

/// Installs `name@version` unless exactly that version is present (packages that must match another
/// package's version, like `react-server-dom-webpack` and `react`)
pub async fn ensure_version(root: &Path, name: &str, version: &str) -> bool {
    if installed_version(root, name).as_deref() == Some(version) {
        return true;
    }
    let key = format!("{name}@{version}");
    if failed_installs().lock().map(|f| f.contains(&key)).unwrap_or(false) {
        return false;
    }
    let lock = install_locks().lock().unwrap_or_else(|e| e.into_inner()).entry(key.clone()).or_default().clone();
    let _guard = lock.lock().await;
    if installed_version(root, name).as_deref() == Some(version) {
        return true;
    }
    crate::term::event(&format!("Installing {key}..."));
    let started = std::time::Instant::now();
    let Some(mut cmd) = add_command(root, &[&key], false) else { return false };
    cmd.current_dir(root).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::piped());
    crate::worker_pool::apply_runner_env(&mut cmd, root);
    let ok = matches!(cmd.output().await, Ok(out) if out.status.success()) && installed_version(root, name).as_deref() == Some(version);
    if ok {
        crate::term::success(&format!("Installed {key} in {}", crate::term::format_ms(started.elapsed().as_secs_f64() * 1000.0)));
        crate::worker_pool::restart(root).await;
    } else {
        crate::term::warn(&format!("Could not install {key}"));
        if let Ok(mut failed) = failed_installs().lock() {
            failed.insert(key);
        }
    }
    ok
}

/// Installs build-time packages (e.g. Tailwind) as devDependencies, all in one `bun add -d`.
/// Attempted at most once per process; returns whether all of them are installed afterwards.
pub async fn ensure_dev_packages(root: &Path, names: &[&str]) -> bool {
    let missing: Vec<&str> = names.iter().copied().filter(|n| installed_version(root, n).is_none()).collect();
    if missing.is_empty() {
        return true;
    }
    let key = missing.join(" ");
    if failed_installs().lock().map(|f| f.contains(&key)).unwrap_or(false) {
        return false;
    }
    let lock = install_locks().lock().unwrap_or_else(|e| e.into_inner()).entry(key.clone()).or_default().clone();
    let _guard = lock.lock().await;
    if names.iter().all(|n| installed_version(root, n).is_some()) {
        return true;
    }

    crate::term::event(&format!("Installing {}...", missing.join(", ")));
    let started = std::time::Instant::now();
    let Some(mut cmd) = add_command(root, &missing, true) else { return false };
    cmd.current_dir(root).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::piped());
    crate::worker_pool::apply_runner_env(&mut cmd, root);
    let ok = matches!(cmd.output().await, Ok(out) if out.status.success()) && names.iter().all(|n| installed_version(root, n).is_some());
    if ok {
        crate::term::success(&format!("Installed {} in {}", missing.join(", "), crate::term::format_ms(started.elapsed().as_secs_f64() * 1000.0)));
        crate::worker_pool::restart(root).await;
    } else if let Ok(mut failed) = failed_installs().lock() {
        failed.insert(key);
    }
    ok
}

/// ETag from the bundle's content, not the package version: when the bundler changes, browsers must
/// not get `304 Not Modified` for a module they cached from an older (possibly broken) build
pub fn content_etag(code: &str) -> String {
    format!("\"{}\"", &blake3::hash(code.as_bytes()).to_hex()[..20])
}

/// `COM_DEPS_PROD=1` serves the production builds of dependencies (React without its development
/// checks, minified): what a production build ships. For measuring client JavaScript size only
/// (`scripts/bench.ts`): the dev server still renders with React's development builds, whose Flight
/// payload the production client does not read, so pages do not hydrate in this mode. `com build`
/// (ROADMAP P3) builds both sides for production.
pub fn deps_node_env() -> &'static str {
    if crate::mode::is_production() || matches!(std::env::var("COM_DEPS_PROD").as_deref(), Ok("1") | Ok("true")) {
        "production"
    } else {
        "development"
    }
}

pub enum DepResult {
    /// Bundled module source plus an ETag for conditional requests
    Module { code: String, etag: String },
    /// Package unavailable locally: redirect the browser here
    Fallback(String),
    Error(String),
}

/// Produces the browser module for `spec`, building and caching it on first request
pub async fn resolve(root: &Path, spec: &str) -> DepResult {
    if !is_valid_specifier(spec) {
        return DepResult::Error(format!("Invalid package specifier: {spec}"));
    }
    let (name, _) = split_specifier(spec);
    // React itself may come from the framework (projects declaring React 18, see framework_react.rs)
    let framework_dir = crate::framework_react::PACKAGES
        .contains(&name.as_str())
        .then(|| crate::framework_react::active_modules_dir(root))
        .flatten();
    let version = match &framework_dir {
        Some(_) => crate::framework_react::package_version(root, &name),
        None => ensure_installed(root, &name).await,
    };
    let Some(version) = version else {
        return DepResult::Fallback(esm_sh_url(spec));
    };
    let node_env = deps_node_env();
    // Production builds (smaller, no dev warnings) are cached apart from development ones
    let cache_version = if node_env == "production" { format!("{version}-prod") } else { version.clone() };
    let out_file = cache_path(root, spec, &cache_version);
    if let Ok(code) = std::fs::read_to_string(&out_file) {
        return DepResult::Module { etag: content_etag(&code), code };
    }

    let started = std::time::Instant::now();
    let started_at = std::time::SystemTime::now();
    let shared_file = shared_cache_path(spec, &cache_version);
    let mut input = serde_json::json!({ "spec": spec, "outFile": out_file, "sharedFile": shared_file, "nodeEnv": node_env });
    if let Some(dir) = framework_dir.as_ref().and_then(|d| d.parent()) {
        input["resolveDir"] = serde_json::json!(dir);
    } else if crate::store::uses_store(root) {
        // Bun finds the store links as `node_modules` of this folder, with package `exports` (a
        // NODE_PATH lookup only reads `main`)
        input["resolveDir"] = serde_json::json!(crate::store::resolve_dir(root));
    }
    if let Some(module) = crate::deps_process::rolldown_module().await {
        input["rolldown"] = serde_json::json!(module);
    }
    let errors = match bundle(root, &input).await {
        Ok(errors) => errors,
        Err(e) => {
            crate::term::warn(&format!("No JavaScript runtime to bundle {spec} ({e}), loading it from esm.sh"));
            return DepResult::Fallback(esm_sh_url(spec));
        }
    };
    match std::fs::read_to_string(&out_file) {
        Ok(code) => {
            // A link to the machine-wide copy keeps that copy's time: built earlier, by another project
            let reused = std::fs::metadata(&out_file).and_then(|m| m.modified()).is_ok_and(|t| t < started_at);
            if !reused {
                crate::term::success(&format!(
                    "Prebundled {spec}@{version} in {}",
                    crate::term::format_ms(started.elapsed().as_secs_f64() * 1000.0)
                ));
            }
            DepResult::Module { etag: content_etag(&code), code }
        }
        Err(_) => DepResult::Error(format!("Could not bundle {spec}:\n{}", errors.trim())),
    }
}

/// Runs `bundleDependency(input)`: in the project's bundler process (retried once, it may have just
/// exited after being idle), else in a one-off process. `Ok` holds the bundler's errors, if any;
/// `Err` means no JavaScript runtime could run it.
async fn bundle(root: &Path, input: &serde_json::Value) -> Result<String, String> {
    // `COM_DEPS_PROCESS=0`: a one-off process per package, as before the bundler process
    let attempts = if matches!(std::env::var("COM_DEPS_PROCESS").as_deref(), Ok("0") | Ok("false")) { 0 } else { 2 };
    for _ in 0..attempts {
        match crate::deps_process::bundle(root, input).await {
            Ok(()) => return Ok(String::new()),
            Err(crate::deps_process::BundleError::Failed(message)) => return Ok(message),
            Err(crate::deps_process::BundleError::Unavailable(_)) => {}
        }
    }
    let script = format!(
        "import {{ bundleDependency }} from {};\nawait bundleDependency({});\n",
        serde_json::to_string(&crate::rpc::clean_path(root.join(".nata/deps_bundler.ts"))).unwrap_or_default(),
        input
    );
    crate::worker_pool::spawn_once(root, &script).await.map(|output| output.stderr)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_scoped_and_plain_specifiers() {
        assert_eq!(split_specifier("react"), ("react".into(), "".into()));
        assert_eq!(split_specifier("react-dom/client"), ("react-dom".into(), "client".into()));
        assert_eq!(split_specifier("@hookform/resolvers/zod"), ("@hookform/resolvers".into(), "zod".into()));
        assert_eq!(split_specifier("@tanstack/react-query"), ("@tanstack/react-query".into(), "".into()));
    }

    #[test]
    fn rejects_paths_that_are_not_package_specifiers() {
        assert!(is_valid_specifier("react/jsx-runtime"));
        assert!(is_valid_specifier("@scope/pkg/sub"));
        assert!(!is_valid_specifier("../etc/passwd"));
        assert!(!is_valid_specifier("react/../../x"));
        assert!(!is_valid_specifier("a/b c"));
        assert!(!is_valid_specifier(""));
    }

    #[test]
    fn etag_changes_with_bundle_content() {
        assert_eq!(content_etag("export default 1"), content_etag("export default 1"));
        assert_ne!(content_etag("export default 1"), content_etag("export default 2"));
    }

    #[test]
    fn shared_cache_is_per_machine_and_versioned_like_the_project_cache() {
        let file = shared_cache_path("@hookform/resolvers/zod", "3.9.0");
        let home = crate::framework_react::com_home();
        assert_eq!(file, home.join("cache/deps").join(BUNDLE_FORMAT).join("@hookform__resolvers@3.9.0/zod.js"));
    }

    #[test]
    fn cache_path_is_versioned_per_subpath() {
        let root = Path::new("/p");
        assert_eq!(cache_path(root, "react", "19.1.0"), Path::new("/p/.nata/deps/v9/react@19.1.0/index.js"));
        assert_eq!(
            cache_path(root, "@hookform/resolvers/zod", "3.9.0"),
            Path::new("/p/.nata/deps/v9/@hookform__resolvers@3.9.0/zod.js")
        );
    }
}
