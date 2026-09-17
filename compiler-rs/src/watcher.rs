use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::mpsc::channel,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::sync::broadcast;

/// HMR Message sent over WebSocket
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HmrMessage {
    #[serde(rename = "type")]
    pub event_type: String,
    pub file: String,
    pub timestamp: u128,
}

pub struct ProjectWatcher;

impl ProjectWatcher {
    /// Starts the native background file watcher for the project root and symlinked node_modules
    pub fn start<P: AsRef<Path>>(
        root_dir: P,
        hmr_tx: broadcast::Sender<String>,
    ) -> tokio::task::JoinHandle<()> {
        let root = root_dir.as_ref().to_path_buf();
        let canon_root = root.canonicalize().unwrap_or_else(|_| root.clone());

        tokio::spawn(async move {
            let (tx, rx) = channel();

            let mut watcher: RecommendedWatcher = match RecommendedWatcher::new(tx, notify::Config::default()) {
                Ok(w) => w,
                Err(e) => {
                    eprintln!("  \x1b[31m✖ [HMR Watcher Error]\x1b[0m Failed to initialize file watcher: {}", e);
                    return;
                }
            };

            // 1. Watch root directory recursively
            if let Err(e) = watcher.watch(&canon_root, RecursiveMode::Recursive) {
                eprintln!("  \x1b[31m✖ [HMR Watcher Error]\x1b[0m Could not watch root directory {:?}: {}", canon_root, e);
            }

            // 2. Discover and watch symlinks in node_modules (e.g. bun link, npm link)
            let symlinks = Self::discover_node_modules_symlinks(&canon_root);
            for symlink_target in &symlinks {
                if let Err(e) = watcher.watch(symlink_target, RecursiveMode::Recursive) {
                    eprintln!("  \x1b[33m▲ [HMR Watcher Warning]\x1b[0m Could not watch symlink target {:?}: {}", symlink_target, e);
                } else {
                    println!("  \x1b[32m✔ [HMR Watcher]\x1b[0m Watching linked package: \x1b[36m{}\x1b[0m", symlink_target.display());
                }
            }

            // 3. Debounced Event Loop
            let mut last_event_time = Instant::now() - Duration::from_secs(1);
            let mut pending_files: HashSet<PathBuf> = HashSet::new();

            loop {
                // Poll from sync channel non-blockingly with tokio sleep
                while let Ok(event_res) = rx.try_recv() {
                    if let Ok(event) = event_res {
                        if Self::is_relevant_event(&event) {
                            for path in event.paths {
                                if !Self::should_ignore_path(&path, &canon_root) {
                                    pending_files.insert(path);
                                }
                            }
                        }
                    }
                }

                // If we have pending changes and debounce time has elapsed (80ms)
                if !pending_files.is_empty() && last_event_time.elapsed() >= Duration::from_millis(80) {
                    let now_ms = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis();

                    let files_to_notify: Vec<PathBuf> = pending_files.drain().collect();

                    for path in files_to_notify {
                        let rel_path = path
                            .strip_prefix(&canon_root)
                            .unwrap_or(&path)
                            .to_string_lossy()
                            .replace('\\', "/");

                        let ext = path
                            .extension()
                            .and_then(|s| s.to_str())
                            .unwrap_or("")
                            .to_lowercase();

                        let is_css = ext == "css";

                        let event_type = if is_css { "css" } else { "reload" };

                        let hmr_payload = HmrMessage {
                            event_type: event_type.to_string(),
                            file: rel_path.clone(),
                            timestamp: now_ms,
                        };

                        if let Ok(json_str) = serde_json::to_string(&hmr_payload) {
                            let _ = hmr_tx.send(json_str);

                            if is_css {
                                println!(
                                    "  \x1b[1;35m⚡ [HMR:CSS]\x1b[0m \x1b[1;36m{}\x1b[0m (hot updated)",
                                    rel_path
                                );
                            } else {
                                println!(
                                    "  \x1b[1;32m⚡ [HMR:RELOAD]\x1b[0m \x1b[1;36m{}\x1b[0m (notified client)",
                                    rel_path
                                );
                            }
                        }
                    }

                    last_event_time = Instant::now();
                }

                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
    }

    /// Checks if a notify Event is relevant (creation, modification, deletion)
    pub fn is_relevant_event(event: &Event) -> bool {
        match event.kind {
            EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_) => true,
            _ => false,
        }
    }

    /// Determines if a path should be ignored by the watcher
    pub fn should_ignore_path(path: &Path, root: &Path) -> bool {
        let path_str = path.to_string_lossy();

        // 1. Always ignore system/temp artifacts
        if path_str.contains(".DS_Store")
            || path_str.ends_with('~')
            || path_str.ends_with(".tmp")
            || path_str.ends_with(".swp")
            || path_str.contains(".git/")
            || path_str.contains(".git\\")
            || path_str.contains(".next/")
            || path_str.contains(".next\\")
            || path_str.contains(".nata/")
            || path_str.contains(".nata\\")
            || path_str.contains("target/")
            || path_str.contains("target\\")
            || path_str.contains(".turbo/")
            || path_str.contains(".turbo\\")
        {
            return true;
        }

        // 2. Ignore non-symlinked node_modules files
        if path_str.contains("node_modules/") || path_str.contains("node_modules\\") {
            // If the path itself is inside node_modules but is not part of a watched symlink
            if let Ok(rel) = path.strip_prefix(root) {
                let rel_str = rel.to_string_lossy();
                if rel_str.starts_with("node_modules/") || rel_str.starts_with("node_modules\\") {
                    return true;
                }
            }
        }

        // 3. Ignore root config files and package lockfiles to avoid HMR loops
        let file_name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
        if file_name == "tsconfig.json"
            || file_name == "package.json"
            || file_name == "package-lock.json"
            || file_name == "bun.lockb"
            || file_name == "yarn.lock"
            || file_name == "pnpm-lock.yaml"
        {
            return true;
        }

        if file_name.starts_with(".env") {
            return false;
        }

        if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
            let lower = ext.to_lowercase();
            let is_allowed = matches!(
                lower.as_str(),
                "tsx"
                    | "ts"
                    | "jsx"
                    | "js"
                    | "mjs"
                    | "cjs"
                    | "css"
                    | "scss"
                    | "sass"
                    | "json"
                    | "html"
                    | "svg"
                    | "png"
                    | "jpg"
                    | "jpeg"
                    | "webp"
            );
            !is_allowed
        } else {
            true
        }
    }

    /// Discovers symlinked packages inside node_modules/
    pub fn discover_node_modules_symlinks(root: &Path) -> Vec<PathBuf> {
        let mut symlinks = Vec::new();
        let nm_dir = root.join("node_modules");

        if !nm_dir.exists() || !nm_dir.is_dir() {
            return symlinks;
        }

        if let Ok(entries) = std::fs::read_dir(&nm_dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                let file_name = entry.file_name().to_string_lossy().to_string();

                // Check scoped packages (e.g. node_modules/@nata/...)
                if file_name.starts_with('@') && path.is_dir() {
                    if let Ok(scoped_entries) = std::fs::read_dir(&path) {
                        for scoped_entry in scoped_entries.filter_map(|e| e.ok()) {
                            let scoped_path = scoped_entry.path();
                            if let Ok(meta) = std::fs::symlink_metadata(&scoped_path) {
                                if meta.file_type().is_symlink() {
                                    if let Ok(canonical) = scoped_path.canonicalize() {
                                        symlinks.push(canonical);
                                    }
                                }
                            }
                        }
                    }
                } else {
                    if let Ok(meta) = std::fs::symlink_metadata(&path) {
                        if meta.file_type().is_symlink() {
                            if let Ok(canonical) = path.canonicalize() {
                                symlinks.push(canonical);
                            }
                        }
                    }
                }
            }
        }

        symlinks
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_should_ignore_git_and_temp() {
        let root = Path::new("/workspace");
        assert!(ProjectWatcher::should_ignore_path(
            Path::new("/workspace/.git/HEAD"),
            root
        ));
        assert!(ProjectWatcher::should_ignore_path(
            Path::new("/workspace/.next/cache.json"),
            root
        ));
        assert!(ProjectWatcher::should_ignore_path(
            Path::new("/workspace/.nata/temp.ts"),
            root
        ));
        assert!(ProjectWatcher::should_ignore_path(
            Path::new("/workspace/app/.DS_Store"),
            root
        ));
    }

    #[test]
    fn test_allow_valid_source_files() {
        let root = Path::new("/workspace");
        assert!(!ProjectWatcher::should_ignore_path(
            Path::new("/workspace/app/page.tsx"),
            root
        ));
        assert!(!ProjectWatcher::should_ignore_path(
            Path::new("/workspace/styles/globals.css"),
            root
        ));
        assert!(!ProjectWatcher::should_ignore_path(
            Path::new("/workspace/.env.local"),
            root
        ));
    }
}
