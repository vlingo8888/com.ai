//! The project stylesheet compiled in the engine with the native Tailwind CSS v4 compiler
//! (`compiler-rs/tailwind`, a port of tailwindcss@4.3.3 and the `@tailwindcss/oxide` extractor), instead
//! of `@tailwindcss/node` in the Bun worker: no Tailwind packages to install, no worker round trip and
//! no Tailwind in the worker's memory.
//!
//! - Same inputs as the worker: the entry stylesheet with what it `@import`s, `@source` rules, and the
//!   project's files minus `node_modules/`, `.nata/`, `data/` and hidden folders.
//! - Candidates are cached per file (modification time + size). The watcher marks the project dirty on
//!   every change; until then the last stylesheet is returned as it is.
//! - What the port does not do falls back to the worker (`supports`): `@config`, `@plugin`s other than
//!   typography and forms, `@import` of packages. `COM_TAILWIND=js` always uses the worker.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::SystemTime;

use tailwind_rs::FileItem;

#[derive(Default)]
struct ProjectState {
    /// A file changed since `last` was built
    dirty: bool,
    /// Input stylesheet → built CSS
    last: Option<(String, String)>,
    /// Input stylesheet → why it needs the worker
    unsupported: Option<(String, String)>,
    /// Project-relative path → (modified, size, candidates)
    candidates: HashMap<String, (SystemTime, u64, Arc<Vec<String>>)>,
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
    map.entry(key(root)).or_insert_with(|| Arc::new(Mutex::new(ProjectState { dirty: true, ..Default::default() }))).clone()
}

/// Called by the watcher for every batch of changes: the next stylesheet request rescans
pub fn invalidate(root: &Path) {
    let map = states().lock().unwrap_or_else(|e| e.into_inner());
    if let Some(state) = map.get(&key(root)) {
        state.lock().unwrap_or_else(|e| e.into_inner()).dirty = true;
    }
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

/// Folders never scanned: dependencies, the engine's output, PGlite data and hidden folders
fn skipped_dir(name: &str) -> bool {
    name.starts_with('.') || matches!(name, "node_modules" | "data")
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

    // One walk: CSS files (for `@import`) and the paths and metadata of everything else
    let mut css_files = Vec::new();
    let mut others = Vec::new();
    let walker = walkdir::WalkDir::new(root).into_iter().filter_entry(|e| e.depth() == 0 || !(e.file_type().is_dir() && skipped_dir(&e.file_name().to_string_lossy())));
    for entry in walker.filter_map(Result::ok).filter(|e| e.file_type().is_file()) {
        let rel = relative(root, entry.path());
        if rel.ends_with(".css") {
            if let Ok(content) = std::fs::read_to_string(entry.path()) {
                css_files.push(FileItem { path: rel, content });
            }
        } else if tailwind_rs::scanner::is_scannable_file(&rel) {
            others.push((rel, entry));
        }
    }

    let entry_file = FileItem { path: entry.map(|e| relative(root, e)).unwrap_or_else(|| "app/globals.css".to_string()), content: input.to_string() };
    let resolved = tailwind_rs::css_resolve::resolve_css_entry(&entry_file, &css_files);
    if let Some(reason) = unsupported(&resolved.css) {
        state.unsupported = Some((input.to_string(), reason.clone()));
        state.dirty = false;
        return Outcome::Unsupported(reason);
    }

    // Candidates of each scanned file, from the cache when the file did not change
    let scanned: Vec<_> = others.into_iter().filter(|(rel, _)| tailwind_rs::scanner::is_scanned(rel, &resolved.sources)).collect();
    let mut fresh = HashMap::new();
    let mut to_read = Vec::new();
    for (rel, entry) in scanned {
        let Ok(meta) = entry.metadata() else { continue };
        let stamp = (meta.modified().unwrap_or(SystemTime::UNIX_EPOCH), meta.len());
        match state.candidates.get(&rel) {
            Some((modified, len, tokens)) if (*modified, *len) == stamp => {
                fresh.insert(rel, (stamp.0, stamp.1, tokens.clone()));
            }
            _ => to_read.push((rel, entry.into_path(), stamp)),
        }
    }
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
    fresh.extend(read);
    let tokens: Vec<String> = fresh.values().flat_map(|(_, _, t)| t.iter().cloned()).collect::<BTreeSet<_>>().into_iter().collect();
    state.candidates = fresh;

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
        invalidate(&root);
        let Outcome::Compiled(css) = compile(&root, Some(&entry), input, false) else { panic!() };
        assert!(css.contains(".grid"));
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
