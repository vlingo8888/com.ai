//! The dependency bundler process: one long-lived Bun process per project (`serveBundler` in
//! deps_bundler.ts) bundles every `/_nata/deps/*` package, concurrently, instead of a new process per
//! package (~18ms each to start, ~30ms more to load Rolldown's native module).
//!
//! Requests are JSON lines on its stdin, replies JSON lines on its stdout, matched by id. The process
//! exits by itself after 30s without work, and the next request starts a new one.
//!
//! `COM_DEPS_PROCESS=0` goes back to a one-off process per package.
//!
//! `COM_BUNDLER=rolldown` bundles with Rolldown, installed once per machine into
//! `~/.com/rolldown/<version>` (like the framework's React); Bun.build otherwise, or when Rolldown
//! cannot be installed.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::oneshot;

pub const ROLLDOWN_VERSION: &str = "1.2.13";

/// A bundle can take long for a big package on a busy machine, never this long
const REQUEST_TIMEOUT: Duration = Duration::from_secs(120);

pub enum BundleError {
    /// No bundler process could take the request: bundle in a one-off process instead
    Unavailable(String),
    /// The bundler ran and failed (the package does not bundle)
    Failed(String),
}

type Pending = Arc<Mutex<HashMap<u64, oneshot::Sender<Result<(), String>>>>>;

struct BundlerProcess {
    stdin: tokio::sync::Mutex<ChildStdin>,
    pending: Pending,
    next_id: AtomicU64,
    alive: Arc<AtomicBool>,
    // Killed when the process entry is replaced or the engine exits
    _child: Child,
}

fn processes() -> &'static tokio::sync::Mutex<HashMap<PathBuf, Arc<BundlerProcess>>> {
    static PROCESSES: OnceLock<tokio::sync::Mutex<HashMap<PathBuf, Arc<BundlerProcess>>>> = OnceLock::new();
    PROCESSES.get_or_init(|| tokio::sync::Mutex::new(HashMap::new()))
}

/// Bundles one package (`input` is `bundleDependency`'s argument) in the project's bundler process
pub async fn bundle(root: &Path, input: &serde_json::Value) -> Result<(), BundleError> {
    let process = {
        let mut map = processes().lock().await;
        match map.get(root).filter(|p| p.alive.load(Ordering::SeqCst)) {
            Some(process) => process.clone(),
            None => {
                let process = Arc::new(spawn(root).await.map_err(BundleError::Unavailable)?);
                map.insert(root.to_path_buf(), process.clone());
                process
            }
        }
    };

    let id = process.next_id.fetch_add(1, Ordering::SeqCst);
    let (tx, rx) = oneshot::channel();
    process.pending.lock().unwrap_or_else(|e| e.into_inner()).insert(id, tx);
    let line = serde_json::json!({ "id": id, "input": input }).to_string() + "\n";
    let written = {
        let mut stdin = process.stdin.lock().await;
        stdin.write_all(line.as_bytes()).await.and(stdin.flush().await)
    };
    if let Err(e) = written {
        process.pending.lock().unwrap_or_else(|e| e.into_inner()).remove(&id);
        process.alive.store(false, Ordering::SeqCst);
        return Err(BundleError::Unavailable(format!("bundler process unavailable: {e}")));
    }
    match tokio::time::timeout(REQUEST_TIMEOUT, rx).await {
        Ok(Ok(Ok(()))) => Ok(()),
        Ok(Ok(Err(message))) => Err(BundleError::Failed(message)),
        // The process exited (idle timeout racing this request, or a crash) before answering
        Ok(Err(_)) => Err(BundleError::Unavailable("bundler process exited".into())),
        Err(_) => {
            process.pending.lock().unwrap_or_else(|e| e.into_inner()).remove(&id);
            Err(BundleError::Unavailable("bundler process timed out".into()))
        }
    }
}

async fn spawn(root: &Path) -> Result<BundlerProcess, String> {
    crate::worker_pool::write_runtime_files(root).map_err(|e| e.to_string())?;
    // The bundler (~50MB) exits after `COM_DEPS_IDLE` seconds without work (default 5): a cold page asks
    // for its packages within a second or two, and a later request starts it again
    let idle_ms = std::env::var("COM_DEPS_IDLE").ok().and_then(|v| v.parse::<u64>().ok()).unwrap_or(5) * 1000;
    let script = format!(
        "import {{ serveBundler }} from {};\nawait serveBundler({{ idleMs: {idle_ms} }});\n",
        serde_json::to_string(&crate::rpc::clean_path(root.join(".nata/deps_bundler.ts"))).unwrap_or_default()
    );
    let mut cmd = Command::new(crate::rpc::find_bun_bin());
    cmd.args(crate::store::bun_args(root))
        .arg("-e")
        .arg(script)
        .current_dir(root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        // Errors come back in the replies; packages imported to list their exports may print
        .stderr(Stdio::null())
        .kill_on_drop(true);
    crate::worker_pool::apply_runner_env(&mut cmd, root);
    let mut child = cmd.spawn().map_err(|e| format!("could not start the bundler process: {e}"))?;
    let stdin = child.stdin.take().ok_or("bundler process has no stdin")?;
    let stdout = child.stdout.take().ok_or("bundler process has no stdout")?;

    let pending: Pending = Arc::new(Mutex::new(HashMap::new()));
    let alive = Arc::new(AtomicBool::new(true));
    {
        let pending = pending.clone();
        let alive = alive.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let Ok(reply) = serde_json::from_str::<serde_json::Value>(&line) else { continue };
                let Some(id) = reply["id"].as_u64() else { continue };
                let Some(tx) = pending.lock().unwrap_or_else(|e| e.into_inner()).remove(&id) else { continue };
                let result = if reply["ok"].as_bool() == Some(true) {
                    Ok(())
                } else {
                    Err(reply["error"].as_str().unwrap_or("bundling failed").to_string())
                };
                let _ = tx.send(result);
            }
            // Exited: requests still waiting are dropped, their callers fall back to a one-off process
            alive.store(false, Ordering::SeqCst);
            pending.lock().unwrap_or_else(|e| e.into_inner()).clear();
        });
    }
    Ok(BundlerProcess { stdin: tokio::sync::Mutex::new(stdin), pending, next_id: AtomicU64::new(0), alive, _child: child })
}

/// Rolldown's module when `COM_BUNDLER=rolldown`, installing it on first use; `None` means Bun.build
pub async fn rolldown_module() -> Option<PathBuf> {
    if !matches!(std::env::var("COM_BUNDLER").as_deref(), Ok("rolldown")) {
        return None;
    }
    static INSTALLED: tokio::sync::OnceCell<Option<PathBuf>> = tokio::sync::OnceCell::const_new();
    INSTALLED.get_or_init(install_rolldown).await.clone()
}

fn rolldown_dir() -> PathBuf {
    crate::framework_react::com_home().join("rolldown").join(ROLLDOWN_VERSION)
}

async fn install_rolldown() -> Option<PathBuf> {
    let dir = rolldown_dir();
    let module = dir.join("node_modules/rolldown/dist/index.mjs");
    if module.is_file() {
        return Some(module);
    }
    std::fs::create_dir_all(&dir).ok()?;
    let manifest = serde_json::json!({ "name": "com-framework-rolldown", "private": true, "dependencies": { "rolldown": ROLLDOWN_VERSION } });
    std::fs::write(dir.join("package.json"), manifest.to_string()).ok()?;
    crate::term::event(&format!("Installing Rolldown {ROLLDOWN_VERSION} for COM_BUNDLER=rolldown (once per machine)..."));
    let started = std::time::Instant::now();
    let mut cmd = Command::new(crate::rpc::find_bun_bin());
    cmd.arg("install").current_dir(&dir).stdout(Stdio::null()).stderr(Stdio::piped());
    let ok = matches!(cmd.output().await, Ok(out) if out.status.success()) && module.is_file();
    if ok {
        crate::term::success(&format!("Installed Rolldown {ROLLDOWN_VERSION} in {}", crate::term::format_ms(started.elapsed().as_secs_f64() * 1000.0)));
        Some(module)
    } else {
        crate::term::warn("Could not install Rolldown, bundling dependencies with Bun.build");
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project() -> PathBuf {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let root = std::env::temp_dir().join(format!("nata_deps_process_{nanos}"));
        for (name, source) in [("a", "export const a = 1;\n"), ("b", "import { a } from \"a\";\nexport const b = a + 1;\n")] {
            let dir = root.join("node_modules").join(name);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("package.json"), format!(r#"{{"name":"{name}","type":"module","exports":"./index.js"}}"#)).unwrap();
            std::fs::write(dir.join("index.js"), source).unwrap();
        }
        std::fs::write(root.join("package.json"), r#"{"name":"app"}"#).unwrap();
        root.canonicalize().unwrap()
    }

    #[tokio::test]
    async fn bundles_packages_in_one_long_lived_process() {
        let root = project();
        for spec in ["a", "b"] {
            let out = root.join(format!("out/{spec}.js"));
            let input = serde_json::json!({ "spec": spec, "outFile": out, "resolveDir": root });
            assert!(bundle(&root, &input).await.is_ok(), "{spec}");
            let code = std::fs::read_to_string(&out).unwrap();
            assert!(code.contains("export"), "{code}");
        }
        // `b` keeps `a` external, pointing at the dev server
        assert!(std::fs::read_to_string(root.join("out/b.js")).unwrap().contains("/_nata/deps/a"));
        // Both went to the same process
        let process = processes().lock().await.get(&root).cloned().unwrap();
        assert_eq!(process.next_id.load(Ordering::SeqCst), 2);

        // A package that does not bundle is a failure of that request, not of the process
        let input = serde_json::json!({ "spec": "missing-package", "outFile": root.join("out/m.js"), "resolveDir": root });
        assert!(matches!(bundle(&root, &input).await, Err(BundleError::Failed(_))));
        assert!(process.alive.load(Ordering::SeqCst));
        let _ = std::fs::remove_dir_all(&root);
    }
}
