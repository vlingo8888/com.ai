//! The project stylesheet compiled in the engine with the native Tailwind CSS v4 compiler
//! (`compiler-rs/tailwind`, a port of tailwindcss@4.3.3 and the `@tailwindcss/oxide` extractor), instead
//! of `@tailwindcss/node` in the Bun worker: no Tailwind packages to install, no worker round trip and
//! no Tailwind in the worker's memory.
//!
//! - Same inputs as the worker: the entry stylesheet with what it `@import`s, `@source` rules, and the
//!   project's files minus `node_modules/`, `.nata/`, `data/` and hidden folders.
//! - Candidates are cached per file (modification time + size), and kept in `.nata/cache/tailwind.json`
//!   so the first page after a restart only reads the files that changed. The watcher marks the
//!   project dirty when a file Tailwind reads changes; until then the last stylesheet is returned as
//!   it is. The list of files is kept too: the tree is walked again only when files come or go.
//! - What the port does not do falls back to the worker (`supports`): `@config`, `@plugin`s other than
//!   typography and forms, `@import` of packages. `COM_TAILWIND=js` always uses the worker.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::SystemTime;

use tailwind_rs::FileItem;

type Candidates = HashMap<String, (SystemTime, u64, Arc<Vec<String>>)>;

#[derive(Default)]
struct ProjectState {
    /// A file changed since `last` was built
    dirty: bool,
    /// Input stylesheet → built CSS
    last: Option<(String, String)>,
    /// Input stylesheet → why it needs the worker
    unsupported: Option<(String, String)>,
    /// Project-relative path → (modified, size, candidates)
    candidates: Candidates,
    /// The project's files from the last walk; `None` when files were added or removed since
    listing: Option<Listing>,
    /// CSS files by project-relative path → (modified, size, contents)
    css: HashMap<String, (SystemTime, u64, String)>,
}

#[derive(Clone, Default)]
struct Listing {
    css: Vec<(String, PathBuf)>,
    others: Vec<(String, PathBuf)>,
}

fn states() -> &'static Mutex<HashMap<PathBuf, Arc<Mutex<ProjectState>>>> {
    static STATES: OnceLock<Mutex<HashMap<PathBuf, Arc<Mutex<ProjectState>>>>> = OnceLock::new();
    STATES.get_or_init(Default::default)
}

/// The watcher and the server may spell the root differently (symlinks such as macOS `/var`)
fn key(root: &Path) -> PathBuf {
    crate::rpc::canonicalize(&root).unwrap_or_else(|_| root.to_path_buf())
}

fn state(root: &Path) -> Arc<Mutex<ProjectState>> {
    let mut map = states().lock().unwrap_or_else(|e| e.into_inner());
    map.entry(key(root))
        .or_insert_with(|| Arc::new(Mutex::new(ProjectState { dirty: true, candidates: load_candidates(root), ..Default::default() })))
        .clone()
}

/// Called by the watcher with each batch of changed paths: the next stylesheet request rebuilds when
/// one of them is a file Tailwind reads, and walks the tree again when one was added or removed
pub fn invalidate(root: &Path, files: &[PathBuf]) {
    let map = states().lock().unwrap_or_else(|e| e.into_inner());
    let Some(state) = map.get(&key(root)) else { return };
    let mut state = state.lock().unwrap_or_else(|e| e.into_inner());
    let canonical = key(root);
    for file in files {
        // As the server spelled the root when it listed the files, or canonical (the watcher)
        let rel = if file.starts_with(root) { relative(root, file) } else { relative(&canonical, file) };
        // A folder (created, removed or renamed): its files changed too
        let folder = file.extension().is_none();
        if !folder && !rel.ends_with(".css") && !tailwind_rs::scanner::is_scannable_file(&rel) {
            continue;
        }
        state.dirty = true;
        let known = state.listing.as_ref().is_some_and(|l| l.css.iter().chain(&l.others).any(|(r, _)| *r == rel));
        if folder || !known || !file.is_file() {
            state.listing = None;
        }
    }
}

/// `.nata/cache/tailwind.json`: the candidates of the last build, for the next `com dev`
fn cache_file(root: &Path) -> PathBuf {
    root.join(".nata/cache/tailwind.json")
}

#[derive(serde::Serialize, serde::Deserialize)]
struct CachedCandidates {
    /// Extraction may change with the engine: another engine's cache is not used
    engine: String,
    files: HashMap<String, (SystemTime, u64, Vec<String>)>,
}

fn load_candidates(root: &Path) -> Candidates {
    let Ok(text) = std::fs::read(cache_file(root)) else { return HashMap::new() };
    match serde_json::from_slice::<CachedCandidates>(&text) {
        Ok(cached) if cached.engine == crate::compile_cache::engine_fingerprint() => {
            cached.files.into_iter().map(|(rel, (modified, len, tokens))| (rel, (modified, len, Arc::new(tokens)))).collect()
        }
        _ => HashMap::new(),
    }
}

fn save_candidates(root: &Path, candidates: &Candidates) {
    let cached = CachedCandidates {
        engine: crate::compile_cache::engine_fingerprint().to_string(),
        files: candidates.iter().map(|(rel, (modified, len, tokens))| (rel.clone(), (*modified, *len, tokens.to_vec()))).collect(),
    };
    let file = cache_file(root);
    let _ = std::fs::create_dir_all(file.parent().unwrap_or(root));
    if let Ok(json) = serde_json::to_vec(&cached) {
        let tmp = file.with_extension("json.tmp");
        if std::fs::write(&tmp, json).is_ok() {
            let _ = std::fs::rename(&tmp, &file);
        }
    }
}

/// One walk of the project: CSS files (for `@import`) and the files Tailwind may scan
fn walk(root: &Path) -> Listing {
    let mut listing = Listing::default();
    let walker = walkdir::WalkDir::new(root)
        .into_iter()
        .filter_entry(|e| e.depth() == 0 || !(e.file_type().is_dir() && skipped_dir(&e.file_name().to_string_lossy(), e.depth())));
    for entry in walker.filter_map(Result::ok).filter(|e| e.file_type().is_file()) {
        let rel = relative(root, entry.path());
        if rel.ends_with(".css") {
            listing.css.push((rel, entry.into_path()));
        } else if tailwind_rs::scanner::is_scannable_file(&rel) {
            listing.others.push((rel, entry.into_path()));
        }
    }
    listing
}

pub fn enabled() -> bool {
    !matches!(std::env::var("COM_TAILWIND").as_deref(), Ok("js") | Ok("worker"))
}

const NATIVE_PLUGINS: &[&str] = &["@tailwindcss/typography", "@tailwindcss/forms"];

/// Why the native compiler cannot build `css` (the entry with its imports inlined), if it cannot
fn unsupported(css: &str) -> Option<String> {
    static PLUGIN: OnceLock<regex::Regex> = OnceLock::new();
    static IMPORT: OnceLock<regex::Regex> = OnceLock::new();
    static CONFIG: OnceLock<regex::Regex> = OnceLock::new();
    let css = strip_comments(css);
    if CONFIG.get_or_init(|| regex::Regex::new(r"(?m)^\s*@config\b").unwrap()).is_match(&css) {
        return Some("@config".to_string());
    }
    let plugin = PLUGIN.get_or_init(|| regex::Regex::new(r#"@plugin\s+["']([^"']+)["']"#).unwrap());
    if let Some(name) = plugin.captures_iter(&css).map(|c| c[1].to_string()).find(|n| !NATIVE_PLUGINS.contains(&n.as_str())) {
        return Some(format!("@plugin \"{name}\""));
    }
    // Imports left after inlining: URLs stay in the output; a package stylesheet needs the worker
    let import = IMPORT.get_or_init(|| regex::Regex::new(r#"@import\s+["']([^"']+)["']"#).unwrap());
    import
        .captures_iter(&css)
        .map(|c| c[1].to_string())
        .find(|uri| !(uri.starts_with("http:") || uri.starts_with("https:") || uri.starts_with("//") || uri == "tailwindcss" || uri.starts_with("tailwindcss/")))
        .map(|uri| format!("@import \"{uri}\""))
}

fn strip_comments(css: &str) -> String {
    let mut out = String::with_capacity(css.len());
    let mut rest = css;
    while let Some(start) = rest.find("/*") {
        out.push_str(&rest[..start]);
        rest = rest[start + 2..].find("*/").map_or("", |end| &rest[start + 2 + end + 2..]);
    }
    out.push_str(rest);
    out
}

/// Folders never scanned: dependencies, the engine's output, PGlite data, hidden folders, and build
/// or test output at the root (what the watcher does not watch either)
fn skipped_dir(name: &str, depth: usize) -> bool {
    name.starts_with('.') || matches!(name, "node_modules" | "data") || (depth == 1 && matches!(name, "dist" | "out" | "target" | "coverage"))
}

/// Project-relative path with `/`
fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root).unwrap_or(path).to_string_lossy().replace('\\', "/")
}

pub enum Outcome {
    Compiled(String),
    /// A syntax error or an invalid `@apply` in the project CSS
    Failed(String),
    /// Needs the worker (see `unsupported`)
    Unsupported(String),
}

/// Builds the project stylesheet from `input` (the entry's CSS as `compose_input` made it)
pub fn compile(root: &Path, entry: Option<&Path>, input: &str, minify: bool) -> Outcome {
    let state = state(root);
    let mut state = state.lock().unwrap_or_else(|e| e.into_inner());
    if !state.dirty {
        if let Some((_, css)) = state.last.as_ref().filter(|(last, _)| last == input) {
            return Outcome::Compiled(css.clone());
        }
        if let Some((_, reason)) = state.unsupported.as_ref().filter(|(last, _)| last == input) {
            return Outcome::Unsupported(reason.clone());
        }
    }

    let listing = match state.listing.take() {
        Some(listing) => listing,
        None => walk(root),
    };
    // CSS files (for `@import`), read again only when they changed
    let mut css_files = Vec::with_capacity(listing.css.len());
    let mut css_cache = HashMap::with_capacity(listing.css.len());
    for (rel, path) in &listing.css {
        let Ok(meta) = std::fs::metadata(path) else { continue };
        let stamp = (meta.modified().unwrap_or(SystemTime::UNIX_EPOCH), meta.len());
        let content = match state.css.remove(rel) {
            Some((modified, len, content)) if (modified, len) == stamp => content,
            _ => match std::fs::read_to_string(path) {
                Ok(content) => content,
                Err(_) => continue,
            },
        };
        css_files.push(FileItem { path: rel.clone(), content: content.clone() });
        css_cache.insert(rel.clone(), (stamp.0, stamp.1, content));
    }
    state.css = css_cache;

    let entry_file = FileItem { path: entry.map(|e| relative(root, e)).unwrap_or_else(|| "app/globals.css".to_string()), content: input.to_string() };
    let resolved = tailwind_rs::css_resolve::resolve_css_entry(&entry_file, &css_files);
    if let Some(reason) = unsupported(&resolved.css) {
        state.listing = Some(listing);
        state.unsupported = Some((input.to_string(), reason.clone()));
        state.dirty = false;
        return Outcome::Unsupported(reason);
    }

    // Candidates of each scanned file, from the cache when the file did not change
    let mut fresh = HashMap::new();
    let mut to_read = Vec::new();
    for (rel, path) in listing.others.iter().filter(|(rel, _)| tailwind_rs::scanner::is_scanned(rel, &resolved.sources)) {
        let Ok(meta) = std::fs::metadata(path) else { continue };
        let stamp = (meta.modified().unwrap_or(SystemTime::UNIX_EPOCH), meta.len());
        match state.candidates.get(rel) {
            Some((modified, len, tokens)) if (*modified, *len) == stamp => {
                fresh.insert(rel.clone(), (stamp.0, stamp.1, tokens.clone()));
            }
            _ => to_read.push((rel.clone(), path.clone(), stamp)),
        }
    }
    state.listing = Some(listing);
    use rayon::prelude::*;
    let read: Vec<_> = to_read
        .into_par_iter()
        .filter_map(|(rel, path, stamp)| {
            let bytes = std::fs::read(&path).ok()?;
            let text = String::from_utf8_lossy(&bytes);
            let tokens = tailwind_rs::scanner::extract_file_candidates(&rel, &text);
            Some((rel, (stamp.0, stamp.1, Arc::new(tokens))))
        })
        .collect();
    let changed = !read.is_empty() || fresh.len() + read.len() != state.candidates.len();
    fresh.extend(read);
    let tokens: Vec<String> = fresh.values().flat_map(|(_, _, t)| t.iter().cloned()).collect::<BTreeSet<_>>().into_iter().collect();
    state.candidates = fresh;
    if changed {
        save_candidates(root, &state.candidates);
    }

    match tailwind_rs::engine::compile_stylesheet(&resolved.css, &tokens, minify) {
        Ok(css) => {
            state.last = Some((input.to_string(), css.clone()));
            state.unsupported = None;
            state.dirty = false;
            Outcome::Compiled(css)
        }
        Err(err) => Outcome::Failed(err),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(name: &str) -> PathBuf {
        let nanos = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("nata_tw_native_{name}_{nanos}"));
        std::fs::create_dir_all(dir.join("app")).unwrap();
        std::fs::create_dir_all(dir.join("node_modules/pkg")).unwrap();
        std::fs::create_dir_all(dir.join(".nata/deps")).unwrap();
        dir
    }

    #[test]
    fn builds_from_project_files_and_follows_changes() {
        let root = project("build");
        std::fs::write(root.join("app/page.tsx"), r#"export default () => <p className="text-red-500 flex">hi</p>"#).unwrap();
        std::fs::write(root.join("app/theme.css"), "@theme { --color-brand: #123456; }").unwrap();
        std::fs::write(root.join("node_modules/pkg/index.js"), r#"x = "bg-blue-900""#).unwrap();
        std::fs::write(root.join(".nata/deps/react.js"), r#"x = "bg-green-900""#).unwrap();
        let input = "@import \"tailwindcss\";\n@import \"./theme.css\";\n.btn { @apply bg-brand; }";
        let entry = root.join("app/globals.css");

        let Outcome::Compiled(css) = compile(&root, Some(&entry), input, false) else { panic!("not compiled") };
        assert!(css.contains(".text-red-500") && css.contains(".flex"), "{css}");
        assert!(css.contains("#123456"));
        assert!(!css.contains("bg-blue-900") && !css.contains("bg-green-900"));

        // Unchanged until the watcher says otherwise
        std::fs::write(root.join("app/card.tsx"), r#"<div className="grid" />"#).unwrap();
        let Outcome::Compiled(same) = compile(&root, Some(&entry), input, false) else { panic!() };
        assert_eq!(same, css);
        invalidate(&root, &[root.join("app/card.tsx")]);
        let Outcome::Compiled(css) = compile(&root, Some(&entry), input, false) else { panic!() };
        assert!(css.contains(".grid"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn rebuilds_only_for_files_tailwind_reads_and_keeps_candidates_across_restarts() {
        let root = project("incremental");
        std::fs::write(root.join("app/page.tsx"), r#"<p className="text-red-500" />"#).unwrap();
        let input = "@import \"tailwindcss\";";
        let Outcome::Compiled(css) = compile(&root, None, input, false) else { panic!("not compiled") };
        assert!(css.contains(".text-red-500"));

        // An image or a lockfile changing: the stylesheet stays as it is, nothing is read
        std::fs::write(root.join("app/logo.png"), [0u8; 4]).unwrap();
        invalidate(&root, &[root.join("app/logo.png")]);
        assert!(!state(&root).lock().unwrap().dirty);

        // Editing a known file reuses the file list; a new file walks the tree again
        std::fs::write(root.join("app/page.tsx"), r#"<p className="text-blue-500" />"#).unwrap();
        invalidate(&root, &[root.join("app/page.tsx")]);
        assert!(state(&root).lock().unwrap().listing.is_some());
        let Outcome::Compiled(css) = compile(&root, None, input, false) else { panic!() };
        assert!(css.contains(".text-blue-500") && !css.contains(".text-red-500"));
        std::fs::write(root.join("app/new.tsx"), r#"<p className="underline" />"#).unwrap();
        invalidate(&root, &[root.join("app/new.tsx")]);
        assert!(state(&root).lock().unwrap().listing.is_none());
        let Outcome::Compiled(css) = compile(&root, None, input, false) else { panic!() };
        assert!(css.contains(".underline"));

        // Build output at the root is not scanned
        std::fs::create_dir_all(root.join("dist")).unwrap();
        std::fs::write(root.join("dist/index.js"), r#"x = "bg-pink-700""#).unwrap();
        invalidate(&root, &[root.join("dist")]);
        let Outcome::Compiled(css) = compile(&root, None, input, false) else { panic!() };
        assert!(!css.contains("bg-pink-700"));

        // A restarted engine starts from the saved candidates
        let saved = load_candidates(&root);
        assert!(saved.get("app/new.tsx").is_some_and(|(_, _, tokens)| tokens.iter().any(|t| t == "underline")));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn reports_css_errors_and_leaves_unsupported_input_to_the_worker() {
        let root = project("errors");
        assert!(matches!(compile(&root, None, "@import \"tailwindcss\";\n.x { @apply nope; }", false), Outcome::Failed(_)));
        assert!(matches!(compile(&root, None, "@import \"tailwindcss\";\n@plugin \"daisyui\";", false), Outcome::Unsupported(_)));
        assert!(matches!(compile(&root, None, "@import \"tailwindcss\";\n@config \"./tailwind.config.js\";", false), Outcome::Unsupported(_)));
        assert!(matches!(compile(&root, None, "@import \"tailwindcss\";\n@import \"some-ui/styles.css\";", false), Outcome::Unsupported(_)));
        let ok = "@import \"tailwindcss\";\n@import \"https://fonts.googleapis.com/css2?family=Inter\";\n@plugin \"@tailwindcss/typography\";\n/* @plugin \"daisyui\"; */";
        assert!(matches!(compile(&root, None, ok, true), Outcome::Compiled(_)));
        let _ = std::fs::remove_dir_all(root);
    }
}
