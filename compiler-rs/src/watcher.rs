use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
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
    /// For `update`: what the changed module is to the router (see `client/router.js`)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub module: Option<HmrModule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HmrModule {
    /// URL the browser loads it from
    pub url: String,
    /// Loaded in the browser (as a client component or a module it imports)
    pub client: bool,
    /// Loaded by Server Components / SSR
    pub server: bool,
    /// Every export is a component: can be hot-swapped keeping state
    pub boundary: bool,
    /// Browser module version to import (`?v=`)
    pub version: u32,
}

impl ProjectWatcher {
    /// How the router should apply a change to `file`, when the module graph knows it
    pub fn hmr_module(root: &Path, file: &Path) -> Option<HmrModule> {
        use crate::resolver::Layer;
        let file = crate::rpc::canonicalize(&file).unwrap_or_else(|_| file.to_path_buf());
        let graph = crate::module_graph::for_root(root);
        let graph = graph.read().ok()?;
        let client = graph.node(&file, Layer::Client);
        // A "use client" module is only a reference in the react-server layer: editing it does not
        // change the server payload (its SSR copy only matters for the next full page load)
        let server = graph
            .node(&file, Layer::Rsc)
            .is_some_and(|n| n.directive != Some(crate::swc_compiler::Directive::UseClient));
        if client.is_none() && !server && graph.node(&file, Layer::Ssr).is_none() {
            return None;
        }
        Some(HmrModule {
            url: format!("/_bundle/{}", crate::swc_compiler::module::relative(root, &file)),
            client: client.is_some(),
            server,
            boundary: client.is_some_and(|n| crate::swc_compiler::module::is_refresh_boundary(&n.exports, n.directive)),
            version: graph.version(&file, Layer::Client),
        })
    }
}

pub struct ProjectWatcher;

impl ProjectWatcher {
    /// A save (one file, or a few: editors write a temporary file and rename it) is applied once
    /// changes have been quiet this long
    const QUIET_EDIT: Duration = Duration::from_millis(20);
    /// Bulk writes (plugin install, git checkout, codegen) must be quiet this long before a batch is
    /// applied: applying them mid-write restarted the worker, rescanned routes and reloaded the browser
    /// again and again
    const QUIET_BULK: Duration = Duration::from_millis(300);
    /// A batch with more files than this is bulk…
    const EDIT_FILES: usize = 3;
    /// …and so is one following BURST_BATCHES batches applied within BURST_WINDOW, when it brings a file
    /// they did not have (a tool writing files one by one, slowly enough for each to look like a save).
    /// Saving the same files again and again is still editing: it must not wait QUIET_BULK
    const BURST_BATCHES: usize = 3;
    const BURST_WINDOW: Duration = Duration::from_secs(1);
    /// …unless they keep coming: apply what we have after this
    const MAX_WAIT: Duration = Duration::from_secs(10);
    /// More files than this in one batch ⇒ a single full page reload
    const BULK_FILES: usize = 40;

    /// Starts the native background file watcher for the project root and symlinked node_modules
    pub fn start<P: AsRef<Path>>(
        root_dir: P,
        hmr_tx: broadcast::Sender<String>,
        router: std::sync::Arc<tokio::sync::RwLock<crate::router::AppRouter>>,
    ) -> tokio::task::JoinHandle<()> {
        let root = root_dir.as_ref().to_path_buf();
        let canon_root = crate::rpc::canonicalize(&root).unwrap_or_else(|_| root.clone());

        tokio::spawn(async move {
            // Events wake this task up: nothing runs while no file changes
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<notify::Result<Event>>();
            let handler = move |event: notify::Result<Event>| {
                let _ = tx.send(event);
            };

            let mut watcher: RecommendedWatcher = match RecommendedWatcher::new(handler, notify::Config::default()) {
                Ok(w) => w,
                Err(e) => {
                    crate::term::error("Failed to start the file watcher (hot reload disabled)", Some(&e.to_string()), None);
                    return;
                }
            };

            // 1. Watch the project: the root itself (new files and folders), and each folder in it
            // recursively, except those whose events are ignored anyway
            Self::watch_project(&mut watcher, &canon_root);

            // 2. Discover and watch symlinks in node_modules (e.g. bun link, npm link)
            let symlinks = Self::discover_node_modules_symlinks(&canon_root);
            for symlink_target in &symlinks {
                if let Err(e) = watcher.watch(symlink_target, RecursiveMode::Recursive) {
                    crate::term::warn(&format!("Could not watch linked package {}: {}", symlink_target.display(), e));
                } else {
                    crate::term::event(&format!("Watching linked package {}", symlink_target.display()));
                }
            }

            // 3. Debounced event loop: a batch is applied once changes have been quiet for QUIET_EDIT (a
            // save) or QUIET_BULK (many files), or after MAX_WAIT when they keep coming
            let mut last_event_time = Instant::now();
            let mut pending_files: HashSet<PathBuf> = HashSet::new();
            // When the current batch of changes started, for the ` ✓ Compiled … in Xms` line
            let mut first_change: Option<Instant> = None;
            let mut recent_batches: std::collections::VecDeque<(Instant, Vec<PathBuf>)> = std::collections::VecDeque::new();

            loop {
                let event = if pending_files.is_empty() {
                    match rx.recv().await {
                        Some(event) => Some(event),
                        None => return,
                    }
                } else {
                    recent_batches.retain(|(at, _)| at.elapsed() < Self::BURST_WINDOW);
                    let bulk = Self::is_bulk(&pending_files, &recent_batches);
                    let quiet = if bulk { Self::QUIET_BULK } else { Self::QUIET_EDIT };
                    let deadline = (last_event_time + quiet).min(first_change.unwrap_or(last_event_time) + Self::MAX_WAIT);
                    match tokio::time::timeout_at(deadline.into(), rx.recv()).await {
                        Ok(Some(event)) => Some(event),
                        Ok(None) => return,
                        Err(_) => None,
                    }
                };
                if let Some(event) = event {
                    // A watcher error is not a change: keep waiting
                    let Ok(event) = event else { continue };
                    // A folder created in the root (the root is only watched for its own entries)
                    if matches!(event.kind, EventKind::Create(_)) {
                        for path in event.paths.iter().filter(|p| p.parent() == Some(canon_root.as_path())) {
                            Self::watch_folder(&mut watcher, path);
                        }
                    }
                    if Self::is_relevant_event(&event) {
                        for path in event.paths {
                            if !Self::should_ignore_path(&path, &canon_root) {
                                pending_files.insert(path);
                                first_change.get_or_insert_with(Instant::now);
                                last_event_time = Instant::now();
                            }
                        }
                    }
                    continue;
                }

                if !pending_files.is_empty() {
                    let now_ms = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis();

                    let files_to_notify: Vec<PathBuf> = pending_files.drain().collect();
                    recent_batches.push_back((Instant::now(), files_to_notify.clone()));
                    // New class names, CSS edits: the next stylesheet request rescans (changed files only)
                    crate::css_compiler::tailwind_native::invalidate(&canon_root, &files_to_notify);
                    let changed_at = first_change.take().unwrap_or_else(Instant::now);

                    // Server code: with React Server Components the worker loads modules through the
                    // engine, so a changed file just gets a new version (its importers too) and the next
                    // request imports it fresh. The worker restarts only for environment/config changes,
                    // or to drop accumulated old module versions.
                    if Self::apply_server_changes(&canon_root, &files_to_notify) {
                        crate::worker_pool::restart(&canon_root).await;
                    }
                    if files_to_notify.iter().any(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("next.config."))) {
                        crate::next_config::load(&canon_root).await;
                    }

                    let should_rescan_router = {
                        let r_guard = router.read().await;
                        Self::routes_changed(&canon_root, &r_guard, &files_to_notify)
                    };
                    if should_rescan_router {
                        let new_router = crate::router::AppRouter::scan(&canon_root);
                        let mut r_guard = router.write().await;
                        *r_guard = new_router;
                    }

                    // A big batch (plugin install, git checkout, codegen): one reload of the page instead of one
                    // message per file, which made the browser refetch or reload dozens of times concurrently
                    if files_to_notify.len() > Self::BULK_FILES {
                        let msg = HmrMessage { event_type: "full_reload".to_string(), file: String::new(), timestamp: now_ms, module: None };
                        if let Ok(json_str) = serde_json::to_string(&msg) {
                            let _ = hmr_tx.send(json_str);
                        }
                        crate::term::success(&format!(
                            "Reloaded after {} file changes in {}",
                            files_to_notify.len(),
                            crate::term::format_ms(changed_at.elapsed().as_secs_f64() * 1000.0)
                        ));
                        continue;
                    }

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

                        let module = if is_css || !crate::module_graph::rsc_enabled() { None } else { Self::hmr_module(&canon_root, &path) };
                        let event_type = if is_css { "css" } else if module.is_some() { "update" } else { "reload" };

                        let hmr_payload = HmrMessage {
                            event_type: event_type.to_string(),
                            file: rel_path.clone(),
                            timestamp: now_ms,
                            module,
                        };

                        if let Ok(json_str) = serde_json::to_string(&hmr_payload) {
                            let _ = hmr_tx.send(json_str);

                            if is_css {
                                crate::term::success(&format!("Updated styles {}", rel_path));
                            } else if matches!(ext.as_str(), "ts" | "tsx" | "js" | "jsx" | "mjs" | "mdx") {
                                crate::term::success(&format!("Compiled {} in {}", rel_path, crate::term::format_ms(changed_at.elapsed().as_secs_f64() * 1000.0)));
                            }
                        }
                    }

                }
            }
        })
    }

    /// Whether the pending changes are bulk writes (see `EDIT_FILES` and `BURST_BATCHES`), given the
    /// batches applied within `BURST_WINDOW`
    fn is_bulk(pending: &HashSet<PathBuf>, recent: &std::collections::VecDeque<(Instant, Vec<PathBuf>)>) -> bool {
        pending.len() > Self::EDIT_FILES
            || (recent.len() >= Self::BURST_BATCHES && pending.iter().any(|p| !recent.iter().any(|(_, files)| files.contains(p))))
    }

    /// Updates the module graph for changed files; returns whether the worker must restart
    pub fn apply_server_changes(root: &Path, files: &[PathBuf]) -> bool {
        let rsc = crate::module_graph::rsc_enabled();
        let graph = crate::module_graph::for_root(root);
        let mut restart = false;
        for path in files {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
            if name.starts_with(".env") || name.starts_with("next.config.") || matches!(name, "package.json" | "tsconfig.json" | "jsconfig.json") {
                // Resolution may change (paths, installed packages)
                crate::resolver::oxc::invalidate(root);
                restart = true;
                continue;
            }
            if !matches!(ext.as_str(), "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs" | "mts" | "cts" | "json") {
                continue;
            }
            if !rsc {
                // Client-rendered pipeline: Bun imports server files itself
                restart = true;
                continue;
            }
            // Loaded by Bun itself (lifecycle), or run once per worker (instrumentation register())
            if name.starts_with("lifecycle.") || name.starts_with("instrumentation.") {
                restart = true;
                continue;
            }
            let file = crate::rpc::canonicalize(&path).unwrap_or_else(|_| path.clone());
            let mut graph = graph.write().unwrap_or_else(|e| e.into_inner());
            if !graph.contains(&file) {
                continue;
            }
            // Saved without changes (or touched): nothing to reload
            let hash = std::fs::read(&file).map(|b| *blake3::hash(&b).as_bytes()).ok();
            let unchanged = hash.is_some_and(|h| {
                [crate::resolver::Layer::Client, crate::resolver::Layer::Ssr, crate::resolver::Layer::Rsc]
                    .iter()
                    .filter_map(|l| graph.node(&file, *l))
                    .all(|n| n.hash == h)
            });
            if !unchanged {
                graph.invalidate(&file);
                crate::cache_store::for_root(root).lock().unwrap_or_else(|e| e.into_inner()).invalidate_code();
            }
        }
        let recycle_after = std::env::var("COM_WORKER_RECYCLE").ok().and_then(|v| v.parse::<u64>().ok()).unwrap_or(300);
        if graph.read().map(|g| g.server_invalidations >= recycle_after).unwrap_or(false) {
            restart = true;
        }
        restart
    }

    /// Whether a change adds, removes or renames a route file (page, layout, route handler, ...):
    /// editing an existing one does not change the route table
    pub fn routes_changed(root: &Path, router: &crate::router::AppRouter, files: &[PathBuf]) -> bool {
        const SPECIAL: [&str; 11] = ["page", "layout", "template", "loading", "error", "not-found", "forbidden", "unauthorized", "route", "default", "global-error"];
        let mut known = HashSet::new();
        for route in &router.routes {
            known.insert(route.page_file.to_string_lossy().replace('\\', "/"));
            if let Some(f) = &route.global_error_file {
                known.insert(f.to_string_lossy().replace('\\', "/"));
            }
            for seg in &route.segments_files {
                for f in [&seg.layout, &seg.template, &seg.error, &seg.loading, &seg.not_found, &seg.forbidden, &seg.unauthorized].into_iter().flatten() {
                    known.insert(f.to_string_lossy().replace('\\', "/"));
                }
            }
        }
        files.iter().any(|path| {
            let rel = path.strip_prefix(root).unwrap_or(path).to_string_lossy().replace('\\', "/");
            let in_app = rel == "app" || rel == "src/app" || rel.starts_with("app/") || rel.starts_with("src/app/");
            if !in_app {
                return false;
            }
            match path.extension() {
                // A folder was added, removed or renamed
                None => true,
                Some(_) => {
                    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                    SPECIAL.contains(&stem) && (path.exists() != known.contains(&rel))
                }
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
            || path_str.contains(".turbo/")
            || path_str.contains(".turbo\\")
        {
            return true;
        }

        // 1b. Build output and the local PGlite database at the root: a write to them (every query that
        // changes data) must not trigger a reload. Only the root level, so `app/target/page.tsx` still counts
        if let Ok(rel) = path.strip_prefix(root) {
            let mut parts = rel.components().map(|c| c.as_os_str().to_string_lossy());
            let first = parts.next().unwrap_or_default();
            if Self::UNWATCHED.contains(&first.as_ref()) || (first == "data" && parts.next().is_some_and(|p| p == "app.db")) {
                return true;
            }
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

    /// Folders of the root that are never watched (their events are ignored, see `should_ignore_path`).
    /// inotify and kqueue walk a watched tree and follow links: watching `node_modules` (or the shared
    /// store's links) would register every installed package, for every project.
    /// `dist`, `out`, `coverage` hold build and test output, `.data` the default PGlite database.
    const UNWATCHED: &'static [&'static str] =
        &["node_modules", ".git", ".next", ".nata", "target", ".turbo", "dist", "out", "coverage", ".data", ".vercel"];

    fn watch_project(watcher: &mut RecommendedWatcher, root: &Path) {
        if let Err(e) = watcher.watch(root, RecursiveMode::NonRecursive) {
            crate::term::error(&format!("Could not watch {}", root.display()), Some(&e.to_string()), None);
        }
        for entry in std::fs::read_dir(root).into_iter().flatten().flatten() {
            Self::watch_folder(watcher, &entry.path());
        }
    }

    /// Watches a folder of the root recursively, unless it is one of `UNWATCHED`
    fn watch_folder(watcher: &mut RecommendedWatcher, path: &Path) {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if Self::UNWATCHED.contains(&name) || !path.is_dir() {
            return;
        }
        if let Err(e) = watcher.watch(path, RecursiveMode::Recursive) {
            crate::term::warn(&format!("Could not watch {}: {e}", path.display()));
        }
    }

    /// Discovers symlinked packages inside node_modules/
    pub fn discover_node_modules_symlinks(root: &Path) -> Vec<PathBuf> {
        // On the shared store every package is a link into it: watching them would watch the store
        if crate::store::uses_store(root) {
            return Vec::new();
        }
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
                                    if let Ok(canonical) = crate::rpc::canonicalize(&scoped_path) {
                                        symlinks.push(canonical);
                                    }
                                }
                            }
                        }
                    }
                } else {
                    if let Ok(meta) = std::fs::symlink_metadata(&path) {
                        if meta.file_type().is_symlink() {
                            if let Ok(canonical) = crate::rpc::canonicalize(&path) {
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

    fn project(name: &str, files: &[(&str, &str)]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("nata_watch_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for (rel, body) in files {
            let p = dir.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, body).unwrap();
        }
        crate::rpc::canonicalize(&dir).unwrap()
    }

    #[test]
    fn test_route_table_rescans_only_when_route_files_come_or_go() {
        let root = project("routes", &[("app/page.tsx", "x"), ("app/layout.tsx", "x"), ("app/ui.tsx", "x")]);
        let router = crate::router::AppRouter::scan(&root);
        // Editing an existing page or a plain component: same routes
        assert!(!ProjectWatcher::routes_changed(&root, &router, &[root.join("app/page.tsx"), root.join("app/ui.tsx")]));
        // A new page
        std::fs::create_dir_all(root.join("app/about")).unwrap();
        std::fs::write(root.join("app/about/page.tsx"), "x").unwrap();
        assert!(ProjectWatcher::routes_changed(&root, &router, &[root.join("app/about/page.tsx")]));
        // A deleted layout
        std::fs::remove_file(root.join("app/layout.tsx")).unwrap();
        assert!(ProjectWatcher::routes_changed(&root, &router, &[root.join("app/layout.tsx")]));
    }

    #[test]
    fn test_code_changes_invalidate_modules_instead_of_restarting() {
        let root = project("graph", &[("lib/a.ts", "export const a = 1;"), ("package.json", "{}")]);
        let file = root.join("lib/a.ts");
        crate::module_graph::compile(&root, &file, crate::resolver::Layer::Rsc, "export const a = 1;").unwrap();
        let graph = crate::module_graph::for_root(&root);
        // Saved without changes: nothing happens
        assert!(!ProjectWatcher::apply_server_changes(&root, &[file.clone()]));
        assert_eq!(graph.read().unwrap().version(&file, crate::resolver::Layer::Rsc), 0);
        std::fs::write(&file, "export const a = 2;").unwrap();
        assert!(!ProjectWatcher::apply_server_changes(&root, &[file.clone()]));
        assert_eq!(graph.read().unwrap().version(&file, crate::resolver::Layer::Rsc), 1);
        // Config changes still restart the worker
        assert!(ProjectWatcher::apply_server_changes(&root, &[root.join("package.json")]));
    }

    #[test]
    fn test_saving_the_same_file_quickly_is_not_bulk() {
        let file = |name: &str| PathBuf::from(format!("/workspace/{name}"));
        let batch = |files: &[&str]| (Instant::now(), files.iter().map(|f| file(f)).collect::<Vec<_>>());
        let pending = |files: &[&str]| files.iter().map(|f| file(f)).collect::<HashSet<_>>();
        let saves: std::collections::VecDeque<_> = [batch(&["app/page.tsx"]), batch(&["app/page.tsx"]), batch(&["ui/card.tsx"])].into();
        // Saving the files being edited again (several times a second) still applies in QUIET_EDIT
        assert!(!ProjectWatcher::is_bulk(&pending(&["app/page.tsx"]), &saves));
        assert!(!ProjectWatcher::is_bulk(&pending(&["ui/card.tsx", "app/page.tsx"]), &saves));
        // A tool writing new files one by one is bulk
        assert!(ProjectWatcher::is_bulk(&pending(&["lib/new.ts"]), &saves));
        // So is a big batch, and a new file after only a couple of saves is not
        assert!(ProjectWatcher::is_bulk(&pending(&["a.ts", "b.ts", "c.ts", "d.ts"]), &Default::default()));
        assert!(!ProjectWatcher::is_bulk(&pending(&["lib/new.ts"]), &saves.iter().take(2).cloned().collect()));
    }

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
    fn test_should_ignore_build_output_and_local_database() {
        let root = Path::new("/workspace");
        assert!(ProjectWatcher::should_ignore_path(Path::new("/workspace/dist/index.js"), root));
        assert!(ProjectWatcher::should_ignore_path(Path::new("/workspace/target/debug/x.json"), root));
        assert!(ProjectWatcher::should_ignore_path(Path::new("/workspace/.data/pglite/pg_control.json"), root));
        assert!(ProjectWatcher::should_ignore_path(Path::new("/workspace/data/app.db/base/1/1259.json"), root));
        // Only at the root: routes and data files with these names stay watched
        assert!(!ProjectWatcher::should_ignore_path(Path::new("/workspace/app/target/page.tsx"), root));
        assert!(!ProjectWatcher::should_ignore_path(Path::new("/workspace/data/products.ts"), root));
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
