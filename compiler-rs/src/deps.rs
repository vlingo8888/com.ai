//! Browser builds of npm dependencies, served from `/_nata/deps/<specifier>` (replaces esm.sh).
//!
//! - The package comes from the project's `node_modules`, so client and server run the same version
//!   and `com dev` works offline. A package that is not installed yet is added with `bun add`
//!   (Bun's global cache makes this fast and usually offline-capable).
//! - Each specifier is bundled once by the worker (`Bun.build`, CJS converted to ESM with named
//!   exports) into `.nata/deps/<name>@<version>/<subpath>.js`; every other bare import inside it stays
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
    let manifest = std::fs::read_to_string(root.join("node_modules").join(name).join("package.json")).ok()?;
    let json: serde_json::Value = serde_json::from_str(&manifest).ok()?;
    json.get("version")?.as_str().map(|v| v.to_string())
}

/// Bumped whenever the bundler's output changes, so bundles cached by an older engine are rebuilt
const BUNDLE_FORMAT: &str = "v4";

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
    let mut cmd = tokio::process::Command::new(crate::rpc::find_bun_bin());
    cmd.arg("add").arg(name).current_dir(root).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::piped());
    crate::worker_pool::apply_runner_env(&mut cmd, root);
    let result = cmd.output().await;

    match (result, installed_version(root, name)) {
        (Ok(out), Some(version)) if out.status.success() => {
            crate::term::success(&format!(
                "Installed {name}@{version} in {}",
                crate::term::format_ms(started.elapsed().as_secs_f64() * 1000.0)
            ));
            // The worker may have cached a failed resolution of this package
            crate::worker_pool::restart(root).await;
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
    let mut cmd = tokio::process::Command::new(crate::rpc::find_bun_bin());
    cmd.arg("add").arg(&key).current_dir(root).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::piped());
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
    let mut cmd = tokio::process::Command::new(crate::rpc::find_bun_bin());
    cmd.arg("add").arg("-d").args(&missing).current_dir(root).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::piped());
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
    if matches!(std::env::var("COM_DEPS_PROD").as_deref(), Ok("1") | Ok("true")) {
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
    let out_file = cache_path(root, spec, &if node_env == "production" { format!("{version}-prod") } else { version.clone() });
    if let Ok(code) = std::fs::read_to_string(&out_file) {
        return DepResult::Module { etag: content_etag(&code), code };
    }

    let started = std::time::Instant::now();
    let mut input = serde_json::json!({ "spec": spec, "outFile": out_file, "nodeEnv": node_env });
    if let Some(dir) = framework_dir.as_ref().and_then(|d| d.parent()) {
        input["resolveDir"] = serde_json::json!(dir);
    }
    // A one-off process: bundling happens once per package version, and Bun.build's memory would
    // otherwise stay in the long-lived worker
    let script = format!(
        "import {{ bundleDependency }} from {};\nawait bundleDependency({});\n",
        serde_json::to_string(&crate::rpc::clean_path(root.join(".nata/deps_bundler.ts"))).unwrap_or_default(),
        input
    );
    match crate::worker_pool::spawn_once(root, &script).await {
        Ok(output) => match std::fs::read_to_string(&out_file) {
            Ok(code) => {
                crate::term::success(&format!(
                    "Prebundled {spec}@{version} in {}",
                    crate::term::format_ms(started.elapsed().as_secs_f64() * 1000.0)
                ));
                DepResult::Module { etag: content_etag(&code), code }
            }
            Err(_) => DepResult::Error(format!("Could not bundle {spec}:\n{}", output.stderr.trim())),
        },
        Err(e) => DepResult::Fallback({
            crate::term::warn(&format!("No JavaScript runtime to bundle {spec} ({e}), loading it from esm.sh"));
            esm_sh_url(spec)
        }),
    }
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
    fn cache_path_is_versioned_per_subpath() {
        let root = Path::new("/p");
        assert_eq!(cache_path(root, "react", "19.1.0"), Path::new("/p/.nata/deps/v4/react@19.1.0/index.js"));
        assert_eq!(
            cache_path(root, "@hookform/resolvers/zod", "3.9.0"),
            Path::new("/p/.nata/deps/v4/@hookform__resolvers@3.9.0/zod.js")
        );
    }
}
