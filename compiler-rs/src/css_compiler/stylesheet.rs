//! The project stylesheet, compiled on the server with Tailwind CSS v4 (`@tailwindcss/node` + Oxide
//! class scanning, run in the worker) instead of shipping `@tailwindcss/browser` to every client.
//!
//! - CSS that already has `@import "tailwindcss"` is compiled exactly as written (like Next.js).
//! - Older projects that relied on the theme the shell used to inject get that theme prepended
//!   (`legacy_theme.css`), so they keep rendering the same.
//! - When Tailwind cannot be installed, pages fall back to the browser build (`Stylesheet::Browser`).
//!
//! Also serves component CSS: `import "./x.css"` and CSS Modules (`import s from "./x.module.css"`)
//! resolve to `/_nata/css/<path>` (see `css_module_js`).

use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

pub const LEGACY_THEME_CSS: &str = include_str!("legacy_theme.css");
const TAILWIND_PACKAGES: &[&str] = &["tailwindcss", "@tailwindcss/node", "@tailwindcss/oxide"];

pub enum Stylesheet {
    /// Final CSS, ready for a plain `<style>`
    Compiled(String),
    /// Tailwind unavailable: raw project CSS for `<style type="text/tailwindcss">` + the browser JIT
    Browser(String),
}

impl Stylesheet {
    /// Tags for the HTML `<head>`; `#_nata_custom_css` is what CSS hot reload replaces
    pub fn head_tags(&self) -> String {
        match self {
            Stylesheet::Compiled(_) => {
                format!(r#"<style id="_nata_custom_css">{}</style>"#, self.style_content().replace("</style", "<\\/style"))
            }
            Stylesheet::Browser(_) => format!(
                "<script src=\"https://cdn.jsdelivr.net/npm/@tailwindcss/browser@4\" defer></script>\n  <style id=\"_nata_custom_css\" type=\"text/tailwindcss\">\n{}\n  </style>",
                self.style_content().replace("</style", "<\\/style")
            ),
        }
    }

    pub fn css(&self) -> &str {
        match self {
            Stylesheet::Compiled(css) | Stylesheet::Browser(css) => css,
        }
    }

    /// Text of `#_nata_custom_css`, also what CSS hot reload fetches from `/_nata/styles.css`
    pub fn style_content(&self) -> String {
        match self {
            Stylesheet::Compiled(css) => css.clone(),
            Stylesheet::Browser(css) => format!("{LEGACY_THEME_CSS}\n{css}"),
        }
    }

    pub fn is_compiled(&self) -> bool {
        matches!(self, Stylesheet::Compiled(_))
    }
}

/// The global stylesheet entry, in the same lookup order the engine has always used
pub fn find_entry(root: &Path) -> Option<PathBuf> {
    [
        "app/globals.css",
        "app/global.css",
        "src/app/globals.css",
        "src/app/global.css",
        "src/globals.css",
        "src/global.css",
        "styles/globals.css",
        "styles/global.css",
    ]
    .iter()
    .map(|rel| root.join(rel))
    .find(|p| p.is_file())
}

fn uses_tailwind_import(css: &str) -> bool {
    css.lines().any(|l| {
        let l = l.trim();
        l.starts_with("@import") && (l.contains("\"tailwindcss") || l.contains("'tailwindcss"))
    })
}

/// Tailwind input: the project CSS as-is when it imports Tailwind, otherwise Tailwind + the legacy theme
pub fn compose_input(user_css: &str) -> String {
    if uses_tailwind_import(user_css) {
        user_css.to_string()
    } else {
        format!("@import \"tailwindcss\";\n{LEGACY_THEME_CSS}\n{user_css}")
    }
}

fn tailwind_unavailable() -> &'static Mutex<bool> {
    static UNAVAILABLE: OnceLock<Mutex<bool>> = OnceLock::new();
    UNAVAILABLE.get_or_init(|| Mutex::new(false))
}

async fn compile_in_worker(root: &Path, entry: &Option<PathBuf>, input: &str) -> Result<String, String> {
    let payload = serde_json::json!({ "entry": entry, "css": input });
    match crate::worker_pool::call(root, "css", payload, Vec::new()).await {
        Ok(reply) => match reply.error() {
            Some(err) => Err(err.to_string()),
            None => reply.output()["css"].as_str().map(|s| s.to_string()).ok_or_else(|| "no css in reply".to_string()),
        },
        Err(e) => Err(e.to_string()),
    }
}

/// Prints `message` once per project and message
fn warn_once(root: &Path, message: &str) {
    static SEEN: OnceLock<Mutex<std::collections::HashSet<String>>> = OnceLock::new();
    let key = format!("{}\0{message}", root.display());
    if SEEN.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner()).insert(key) {
        crate::term::warn(message);
    }
}

/// The compiled project stylesheet (or the browser fallback)
pub async fn project_stylesheet(root: &Path) -> Stylesheet {
    // `com start`: compiled (and minified) by `com build`, Tailwind is not needed at runtime
    if let Some(css) = crate::build::manifest::styles(root) {
        return Stylesheet::Compiled(css.to_string());
    }
    let entry = find_entry(root);
    let legacy_css = || super::CssCompiler::find_and_load_custom_css(root);
    let user_css = entry.as_ref().and_then(|p| std::fs::read_to_string(p).ok()).unwrap_or_default();
    let input = compose_input(&user_css);

    // Native Tailwind in the engine; the worker only for what it does not support
    if super::tailwind_native::enabled() {
        let (root_buf, entry_buf, input_buf) = (root.to_path_buf(), entry.clone(), input.clone());
        let minify = crate::mode::is_production();
        let outcome = tokio::task::spawn_blocking(move || super::tailwind_native::compile(&root_buf, entry_buf.as_deref(), &input_buf, minify)).await;
        match outcome {
            Ok(super::tailwind_native::Outcome::Compiled(css)) => return Stylesheet::Compiled(css),
            Ok(super::tailwind_native::Outcome::Failed(err)) => {
                crate::term::error("Could not compile the stylesheet", Some(&err), None);
                return Stylesheet::Browser(legacy_css());
            }
            Ok(super::tailwind_native::Outcome::Unsupported(reason)) => warn_once(root, &format!("{reason} needs the JavaScript Tailwind compiler: compiling the stylesheet in the worker")),
            Err(_) => {}
        }
    }

    if !crate::worker_pool::enabled() || *tailwind_unavailable().lock().unwrap_or_else(|e| e.into_inner()) {
        return Stylesheet::Browser(legacy_css());
    }
    match compile_in_worker(root, &entry, &input).await {
        Ok(css) => Stylesheet::Compiled(css),
        Err(err) if err.contains("TAILWIND_MISSING") => {
            if crate::deps::ensure_dev_packages(root, TAILWIND_PACKAGES).await {
                if let Ok(css) = compile_in_worker(root, &entry, &input).await {
                    return Stylesheet::Compiled(css);
                }
            }
            *tailwind_unavailable().lock().unwrap_or_else(|e| e.into_inner()) = true;
            crate::term::warn("Tailwind CSS is unavailable, styling pages in the browser instead");
            Stylesheet::Browser(legacy_css())
        }
        Err(err) => {
            // A CSS syntax error: report it and keep the page usable with the raw CSS
            crate::term::error("Could not compile the stylesheet", Some(&err), None);
            Stylesheet::Browser(legacy_css())
        }
    }
}

/// JavaScript module for a CSS file imported by a component. Plain CSS is injected once into
/// `<head>`; CSS Modules also export their local -> hashed class name map. The global Tailwind entry is
/// already in the page, so importing it (e.g. `import "./globals.css"` in a layout) is a no-op.
pub fn css_module_js(root: &Path, rel_path: &str) -> Result<String, String> {
    let path = root.join(rel_path);
    let canonical = path.canonicalize().map_err(|_| format!("CSS file not found: {rel_path}"))?;
    let root_canonical = root.canonicalize().map_err(|e| e.to_string())?;
    if !canonical.starts_with(&root_canonical) {
        return Err(format!("CSS file outside the project: {rel_path}"));
    }
    let source = std::fs::read_to_string(&canonical).map_err(|e| e.to_string())?;
    let is_entry = find_entry(root).and_then(|e| e.canonicalize().ok()).as_deref() == Some(canonical.as_path());
    if is_entry || uses_tailwind_import(&source) {
        return Ok("export default {};\n".to_string());
    }

    let is_module = rel_path.ends_with(".module.css");
    let (css, exports) = transform_css(&source, rel_path, is_module)?;
    let id = serde_json::to_string(rel_path).unwrap_or_default();
    Ok(format!(
        "const __id = {id};\nconst __css = {css};\nif (typeof document !== \"undefined\") {{\n  let el = document.querySelector(`style[data-nata-css=\"${{__id}}\"]`);\n  if (!el) {{ el = document.createElement(\"style\"); el.setAttribute(\"data-nata-css\", __id); document.head.appendChild(el); }}\n  el.textContent = __css;\n}}\nexport default {exports};\n",
        css = serde_json::to_string(&css).unwrap_or_default(),
        exports = serde_json::to_string(&exports).unwrap_or_else(|_| "{}".to_string()),
    ))
}

/// Suffix for CSS Modules class names: FNV-1a of the project-relative path, as 8 hex digits. The SSR
/// worker computes the same value (`worker.ts`), so server markup and client CSS agree.
pub fn css_module_suffix(rel_path: &str) -> String {
    let mut hash: u32 = 0x811c9dc5;
    for byte in rel_path.as_bytes() {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    format!("{hash:08x}")
}

/// Lightning CSS: nesting, vendor prefixes, minification, and CSS Modules class names
/// (`<local>_<suffix>`, see `css_module_suffix`)
pub fn transform_css(source: &str, filename: &str, css_modules: bool) -> Result<(String, serde_json::Map<String, serde_json::Value>), String> {
    use lightningcss::stylesheet::{ParserOptions, PrinterOptions, StyleSheet};
    let pattern_source = format!("[local]_{}", css_module_suffix(filename));
    let modules_config = if css_modules {
        let pattern = lightningcss::css_modules::Pattern::parse(&pattern_source).map_err(|e| format!("{filename}: {e:?}"))?;
        Some(lightningcss::css_modules::Config { pattern, ..Default::default() })
    } else {
        None
    };
    let options = ParserOptions {
        filename: filename.to_string(),
        css_modules: modules_config,
        ..ParserOptions::default()
    };
    let sheet = StyleSheet::parse(source, options).map_err(|e| format!("{filename}: {e}"))?;
    let result = sheet.to_css(PrinterOptions { minify: true, ..PrinterOptions::default() }).map_err(|e| format!("{filename}: {e}"))?;
    let mut exports = serde_json::Map::new();
    for (local, export) in result.exports.unwrap_or_default() {
        let mut names = vec![export.name.clone()];
        for composed in &export.composes {
            if let lightningcss::css_modules::CssModuleReference::Local { name } = composed {
                names.push(name.clone());
            }
        }
        exports.insert(local, serde_json::Value::String(names.join(" ")));
    }
    Ok((result.code, exports))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tailwind_projects_are_compiled_as_written() {
        let css = "@import \"tailwindcss\";\nbody { color: red; }\n";
        assert_eq!(compose_input(css), css);
        let legacy = compose_input(".card { padding: 4px; }");
        assert!(legacy.starts_with("@import \"tailwindcss\";"));
        assert!(legacy.contains("@theme inline"));
        assert!(legacy.ends_with(".card { padding: 4px; }"));
    }

    #[test]
    fn css_modules_hash_classes_and_export_the_map() {
        let (css, exports) = transform_css(".title { color: red; }\n.big { composes: title; font-size: 2em; }", "app/x.module.css", true).unwrap();
        let title = exports["title"].as_str().unwrap();
        assert_eq!(title, format!("title_{}", css_module_suffix("app/x.module.css")));
        assert!(css.contains(&format!(".{title}")));
        // `composes` lists both class names
        assert!(exports["big"].as_str().unwrap().contains(title));
    }

    #[test]
    fn module_suffix_matches_the_worker_fnv1a() {
        // Same vectors as worker.ts `fnv1a`: FNV-1a 32-bit of the UTF-8 path
        assert_eq!(css_module_suffix(""), "811c9dc5");
        assert_eq!(css_module_suffix("a"), "e40c292c");
        assert_eq!(css_module_suffix("app/about/card.module.css").len(), 8);
    }

    #[test]
    fn plain_css_is_minified_without_renaming() {
        let (css, exports) = transform_css(".a {\n  color: red;\n}\n", "x.css", false).unwrap();
        assert_eq!(css, ".a{color:red}");
        assert!(exports.is_empty());
    }
}
