//! The framework's own React for the App Router, like Next.js (which ships React canary inside
//! `next` and ignores the project's `react` for `app/`): projects declaring React 18 (most official
//! Next.js examples) run on React 19 without changing their dependencies.
//!
//! React 19 is installed once per machine into `~/.com/react/<version>/node_modules` (react,
//! react-dom, react-server-dom-webpack, scheduler, react-refresh) and searched before the project's
//! `node_modules` by every layer: the compiler's resolver, the worker's module loader (server
//! layers are loaded through the engine) and the browser dependency bundler. Projects already on
//! React 19 use their own copy.

use std::path::{Path, PathBuf};

pub const VERSION: &str = "19.3.0";
const REFRESH_VERSION: &str = "0.19.0";

/// The packages that come from the framework's React when it is used
pub const PACKAGES: [&str; 5] = ["react", "react-dom", "react-server-dom-webpack", "scheduler", "react-refresh"];

/// `~/.com` (or `COM_HOME`): what the framework installs once per machine
pub fn com_home() -> PathBuf {
    if let Ok(dir) = std::env::var("COM_HOME") {
        return PathBuf::from(dir);
    }
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    home.join(".com")
}

/// `~/.com/react/<version>`
pub fn install_dir() -> PathBuf {
    com_home().join("react").join(VERSION)
}

pub fn modules_dir() -> PathBuf {
    install_dir().join("node_modules")
}

fn installed() -> bool {
    PACKAGES.iter().all(|p| modules_dir().join(p).join("package.json").is_file())
}

/// Whether this project runs on the framework's React. Like Next.js, the App Router does not use the
/// project's React unless the project brings the whole set: React 19 with its own
/// `react-server-dom-webpack` (projects made by `com create` pin it). Next.js apps declare only
/// `react` and `react-dom`, often 18.
pub fn needed(root: &Path) -> bool {
    crate::module_graph::rsc_enabled_by_env()
        && !(crate::module_graph::react_major(root).is_some_and(|major| major >= 19) && declares(root, "react-server-dom-webpack"))
}

fn declares(root: &Path, name: &str) -> bool {
    let Some(manifest) = std::fs::read_to_string(root.join("package.json")).ok().and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok()) else {
        return false;
    };
    ["dependencies", "devDependencies"].iter().any(|k| manifest[*k].get(name).is_some())
}

/// The framework React `node_modules` to search first for this project, when it is used and installed
pub fn active_modules_dir(root: &Path) -> Option<PathBuf> {
    (needed(root) && installed()).then(modules_dir)
}

/// Installs the framework's React if this project needs it; `false` when it is needed but could not
/// be installed (offline on first use)
pub async fn ensure(root: &Path) -> bool {
    if !needed(root) || installed() {
        return true;
    }
    let dir = install_dir();
    if std::fs::create_dir_all(&dir).is_err() {
        return false;
    }
    let manifest = serde_json::json!({
        "name": "com-framework-react",
        "private": true,
        "dependencies": {
            "react": VERSION,
            "react-dom": VERSION,
            "react-server-dom-webpack": VERSION,
            "react-refresh": REFRESH_VERSION,
            "scheduler": "*",
        },
    });
    if std::fs::write(dir.join("package.json"), manifest.to_string()).is_err() {
        return false;
    }
    crate::term::event(&format!("Installing the framework's React {VERSION} for the App Router (once per machine)..."));
    let started = std::time::Instant::now();
    let mut cmd = tokio::process::Command::new(crate::rpc::find_bun_bin());
    // Without peers: react-server-dom-webpack declares webpack, which only its webpack plugin uses
    cmd.args(["install", "--omit", "peer"])
        .current_dir(&dir)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped());
    let ok = matches!(cmd.output().await, Ok(out) if out.status.success()) && installed();
    if ok {
        crate::term::success(&format!("Installed React {VERSION} in {}", crate::term::format_ms(started.elapsed().as_secs_f64() * 1000.0)));
        crate::resolver::oxc::invalidate(root);
    }
    ok
}

/// Version of a package as this project's App Router sees it
pub fn package_version(root: &Path, name: &str) -> Option<String> {
    if PACKAGES.contains(&name) && active_modules_dir(root).is_some() {
        let manifest = std::fs::read_to_string(modules_dir().join(name).join("package.json")).ok()?;
        return serde_json::from_str::<serde_json::Value>(&manifest).ok()?["version"].as_str().map(String::from);
    }
    crate::deps::installed_version(root, name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(name: &str, manifest: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("nata_fw_react_{name}_{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("package.json"), manifest).unwrap();
        root
    }

    #[test]
    fn next_apps_use_the_framework_react() {
        // Official Next.js examples: React 18 (or 19) without a Flight runtime of their own
        assert!(needed(&project("r18", r#"{"dependencies":{"next":"latest","react":"^18.2.0","react-dom":"^18.2.0"}}"#)));
        assert!(needed(&project("r19", r#"{"dependencies":{"next":"latest","react":"^19","react-dom":"^19"}}"#)));
        assert!(needed(&project("none", r#"{"dependencies":{}}"#)));
        // React 18 with a stray react-server-dom-webpack still cannot render Server Components
        assert!(needed(&project("r18rsdw", r#"{"dependencies":{"react":"18.3.1","react-server-dom-webpack":"19.3.0"}}"#)));
    }

    #[test]
    fn projects_with_their_own_react_19_set_keep_it() {
        let root = project("own", r#"{"dependencies":{"react":"19.3.0","react-dom":"19.3.0","react-server-dom-webpack":"19.3.0"}}"#);
        assert!(!needed(&root));
        assert!(active_modules_dir(&root).is_none());
    }
}
