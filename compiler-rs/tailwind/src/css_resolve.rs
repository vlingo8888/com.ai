//! Project CSS → the custom CSS fed to the compiler, like a real build sees it.
//!
//! - Entry stylesheets are the CSS files no other project CSS file imports.
//! - `@import "./x.css" [layer(…)] [supports(…)] [media]` of another project
//!   file is inlined (wrapped in `@layer` / `@media` / `@supports` like
//!   Tailwind's `@import` handling); `@import "tw-animate-css"` is inlined
//!   from the embedded package.
//! - `@import "tailwindcss" …` is kept: the compilers move it into the base
//!   stylesheet's own import (see `tw::index::hoist_tailwind_imports`).
//! - `@tailwind …` (v3 directives) are dropped; the base stylesheet already
//!   has the utilities.
//! - `@source`, `@source not` and `source(none | "dir")` on the tailwindcss
//!   import decide which files are scanned ([`Sources`]).

use crate::types::FileItem;
use globset::{GlobBuilder, GlobMatcher};
use std::collections::HashMap;

const TW_ANIMATE: &str = include_str!("tw/assets/vendor/tw-animate-css/tw-animate.css");
const TW_ANIMATE_PREFIX: &str = include_str!("tw/assets/vendor/tw-animate-css/tw-animate-prefix.css");

/// CSS packages that are commonly imported by projects (npm `style` exports).
fn package_stylesheet(id: &str) -> Option<&'static str> {
    match id.trim_end_matches(".css") {
        "tw-animate-css" | "tw-animate-css/dist/tw-animate" => Some(TW_ANIMATE),
        "tw-animate-css/prefix" | "tw-animate-css/dist/tw-animate-prefix" => Some(TW_ANIMATE_PREFIX),
        _ => None,
    }
}

/// Which files are scanned for candidates.
#[derive(Debug, Clone)]
pub struct Sources {
    /// Automatic source detection (off with `source(none)`).
    pub auto: bool,
    /// Directory automatic detection is limited to (`source("dir")`), `""` = all.
    pub auto_base: String,
    /// `@source` rules in order; later rules win.
    pub rules: Vec<SourceRule>,
}

impl Default for Sources {
    fn default() -> Self {
        Self { auto: true, auto_base: String::new(), rules: vec![] }
    }
}

#[derive(Debug, Clone)]
pub struct SourceRule {
    pub negated: bool,
    /// Normalized path or glob relative to the project root.
    pub pattern: String,
    matcher: SourceMatcher,
}

#[derive(Debug, Clone)]
enum SourceMatcher {
    /// A file, or a directory (everything below it).
    Path(String),
    Glob(GlobMatcher),
}

impl SourceRule {
    fn new(base: &str, path: &str, negated: bool) -> Option<Self> {
        let pattern = normalize(&join(base, path));
        let matcher = if path.contains(['*', '?', '{', '[']) {
            let glob = GlobBuilder::new(&pattern).literal_separator(true).build().ok()?;
            SourceMatcher::Glob(glob.compile_matcher())
        } else {
            SourceMatcher::Path(pattern.clone())
        };
        Some(Self { negated, pattern, matcher })
    }

    /// Whether `path` (normalized) is matched; directories match their content.
    pub fn matches(&self, path: &str) -> bool {
        match &self.matcher {
            SourceMatcher::Path(p) => is_under(path, p),
            SourceMatcher::Glob(g) => g.is_match(path),
        }
    }

    /// Explicit globs select exactly what they match; a directory source
    /// still skips binary files and the like.
    pub fn is_glob(&self) -> bool {
        matches!(self.matcher, SourceMatcher::Glob(_))
    }
}

#[derive(Debug, Default)]
pub struct ResolvedCss {
    pub css: String,
    pub sources: Sources,
}

/// `a/b/../c/./d` → `a/c/d`, without leading `/` or `./`.
pub fn normalize(path: &str) -> String {
    let mut out: Vec<&str> = vec![];
    for seg in path.split(['/', '\\']) {
        match seg {
            "" | "." => {}
            ".." => {
                out.pop();
            }
            s => out.push(s),
        }
    }
    out.join("/")
}

fn dirname(path: &str) -> &str {
    match path.trim_start_matches("./").rfind('/') {
        Some(i) => &path.trim_start_matches("./")[..i],
        None => "",
    }
}

fn join(base: &str, rel: &str) -> String {
    if let Some(abs) = rel.strip_prefix('/') {
        return abs.to_string();
    }
    if base.is_empty() {
        rel.to_string()
    } else {
        format!("{base}/{rel}")
    }
}

pub(crate) fn is_under(path: &str, dir: &str) -> bool {
    dir.is_empty() || path == dir || (path.starts_with(dir) && path.as_bytes().get(dir.len()) == Some(&b'/'))
}

/// A top-level `@name prelude;` statement of a stylesheet.
struct Statement<'a> {
    start: usize,
    end: usize,
    name: &'a str,
    prelude: &'a str,
}

/// `@import`, `@source` and `@tailwind` statements outside of blocks,
/// comments and strings.
fn statements(css: &str) -> Vec<Statement<'_>> {
    let b = css.as_bytes();
    let mut out = vec![];
    let mut depth = 0usize;
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'/' if b.get(i + 1) == Some(&b'*') => i = skip_comment(b, i),
            b'"' | b'\'' => i = skip_string(b, i),
            b'{' => {
                depth += 1;
                i += 1;
            }
            b'}' => {
                depth = depth.saturating_sub(1);
                i += 1;
            }
            b'@' if depth == 0 => {
                let name_end = i + 1 + b[i + 1..].iter().take_while(|c| c.is_ascii_alphanumeric() || **c == b'-').count();
                let name = &css[i + 1..name_end];
                if !matches!(name, "import" | "source" | "tailwind") {
                    i = name_end;
                    continue;
                }
                match statement_end(b, name_end) {
                    Some(semi) => {
                        out.push(Statement { start: i, end: semi + 1, name, prelude: css[name_end..semi].trim() });
                        i = semi + 1;
                    }
                    None => i = name_end,
                }
            }
            _ => i += 1,
        }
    }
    out
}

fn skip_comment(b: &[u8], i: usize) -> usize {
    match b[i + 2..].windows(2).position(|w| w == b"*/") {
        Some(p) => i + 2 + p + 2,
        None => b.len(),
    }
}

fn skip_string(b: &[u8], i: usize) -> usize {
    let quote = b[i];
    let mut j = i + 1;
    while j < b.len() {
        match b[j] {
            b'\\' => j += 2,
            c if c == quote || c == b'\n' => return j + 1,
            _ => j += 1,
        }
    }
    b.len()
}

/// Position of the `;` ending a statement, or `None` when a block starts first.
fn statement_end(b: &[u8], mut i: usize) -> Option<usize> {
    let mut parens = 0usize;
    while i < b.len() {
        match b[i] {
            b'/' if b.get(i + 1) == Some(&b'*') => i = skip_comment(b, i),
            b'"' | b'\'' => i = skip_string(b, i),
            b'(' => {
                parens += 1;
                i += 1;
            }
            b')' => {
                parens = parens.saturating_sub(1);
                i += 1;
            }
            b';' if parens == 0 => return Some(i),
            b'{' | b'}' if parens == 0 => return None,
            _ => i += 1,
        }
    }
    None
}

/// Split at top-level whitespace (not inside parens or strings).
fn tokens(s: &str) -> Vec<&str> {
    let b = s.as_bytes();
    let mut out = vec![];
    let (mut start, mut parens, mut i) = (None, 0usize, 0);
    while i < b.len() {
        let c = b[i];
        if c.is_ascii_whitespace() && parens == 0 {
            if let Some(s0) = start.take() {
                out.push(&s[s0..i]);
            }
            i += 1;
            continue;
        }
        start.get_or_insert(i);
        match c {
            b'(' => parens += 1,
            b')' => parens = parens.saturating_sub(1),
            b'"' | b'\'' => {
                i = skip_string(b, i);
                continue;
            }
            _ => {}
        }
        i += 1;
    }
    if let Some(s0) = start {
        out.push(&s[s0..]);
    }
    out
}

fn unquote(s: &str) -> Option<&str> {
    let b = s.as_bytes();
    (b.len() >= 2 && (b[0] == b'"' || b[0] == b'\'') && b[b.len() - 1] == b[0]).then(|| &s[1..s.len() - 1])
}

struct Import<'a> {
    uri: &'a str,
    layer: Option<&'a str>,
    supports: Option<&'a str>,
    media: Option<String>,
}

/// `"uri" [layer | layer(name)] [supports(…)] [media…]`, like Tailwind's
/// `parseImportParams` (`url(…)` imports are left alone).
fn parse_import(prelude: &str) -> Option<Import<'_>> {
    let toks = tokens(prelude);
    let (first, rest) = toks.split_first()?;
    let uri = unquote(first)?;
    let (mut layer, mut supports, mut media) = (None, None, None);
    for (i, t) in rest.iter().enumerate() {
        if layer.is_none() && supports.is_none() && (*t == "layer" || (t.starts_with("layer(") && t.ends_with(')'))) {
            layer = Some(if *t == "layer" { "" } else { &t[6..t.len() - 1] });
        } else if supports.is_none() && t.starts_with("supports(") && t.ends_with(')') {
            supports = Some(&t[9..t.len() - 1]);
        } else {
            media = Some(rest[i..].join(" "));
            break;
        }
    }
    Some(Import { uri, layer, supports, media })
}

fn wrap(css: String, import: &Import<'_>) -> String {
    let mut out = css;
    if let Some(layer) = import.layer {
        out = if layer.is_empty() { format!("@layer {{\n{out}\n}}") } else { format!("@layer {layer} {{\n{out}\n}}") };
    }
    if let Some(media) = &import.media {
        out = format!("@media {media} {{\n{out}\n}}");
    }
    if let Some(supports) = import.supports {
        let s = supports.trim();
        let s = if s.starts_with('(') { s.to_string() } else { format!("({s})") };
        out = format!("@supports {s} {{\n{out}\n}}");
    }
    out
}

struct Resolver<'a> {
    files: HashMap<String, &'a FileItem>,
    sources: Sources,
}

impl<'a> Resolver<'a> {
    /// The project CSS file an `@import` of `from` refers to.
    fn find(&self, from: &str, uri: &str) -> Option<&'a FileItem> {
        let mut candidates = vec![];
        if uri.starts_with("./") || uri.starts_with("../") || uri.starts_with('/') {
            candidates.push(normalize(&join(dirname(from), uri)));
        } else if let Some(rest) = uri.strip_prefix("@/").or_else(|| uri.strip_prefix("~/")) {
            // The usual `@` → `src` alias, or the project root
            candidates.push(normalize(&format!("src/{rest}")));
            candidates.push(normalize(rest));
        } else {
            // A bare name is a relative URL in plain CSS
            candidates.push(normalize(&join(dirname(from), uri)));
        }
        for c in candidates {
            if let Some(f) = self.files.get(&c).or_else(|| self.files.get(&format!("{c}.css"))) {
                return Some(f);
            }
        }
        None
    }

    fn inline(&mut self, file: &FileItem, stack: &mut Vec<String>) -> String {
        let path = normalize(&file.path);
        let base = dirname(&file.path).to_string();
        let css = file.content.as_str();
        let mut out = String::with_capacity(css.len());
        let mut last = 0;

        for st in statements(css) {
            let replacement: Option<String> = match st.name {
                "tailwind" => Some(String::new()),
                "source" => {
                    self.add_source(&base, st.prelude);
                    None
                }
                "import" => match parse_import(st.prelude) {
                    None => None,
                    Some(import) if import.uri == "tailwindcss" || import.uri.starts_with("tailwindcss/") => {
                        self.import_source_option(&base, st.prelude);
                        None
                    }
                    Some(import) if import.uri.starts_with("http:") || import.uri.starts_with("https:") || import.uri.starts_with("data:") => None,
                    Some(import) => {
                        if let Some(pkg) = package_stylesheet(import.uri) {
                            Some(wrap(pkg.to_string(), &import))
                        } else if let Some(target) = self.find(&path, import.uri) {
                            let target_path = normalize(&target.path);
                            if stack.contains(&target_path) {
                                tracing::warn!("tailwind: circular @import of {target_path} in {path}");
                                Some(String::new())
                            } else {
                                stack.push(target_path);
                                let inner = self.inline(target, stack);
                                stack.pop();
                                Some(wrap(inner, &import))
                            }
                        } else {
                            tracing::warn!("tailwind: cannot resolve @import \"{}\" in {path}; left as-is", import.uri);
                            None
                        }
                    }
                },
                _ => None,
            };
            if let Some(r) = replacement {
                out.push_str(&css[last..st.start]);
                out.push_str(&r);
                last = st.end;
            }
        }
        out.push_str(&css[last..]);
        out
    }

    /// `@source "path"`, `@source not "path"` (`inline(…)` is the compiler's).
    fn add_source(&mut self, base: &str, prelude: &str) {
        let (negated, rest) = match prelude.strip_prefix("not") {
            Some(r) if r.starts_with(char::is_whitespace) => (true, r.trim()),
            _ => (false, prelude),
        };
        if let Some(path) = unquote(rest) {
            if let Some(rule) = SourceRule::new(base, path, negated) {
                self.sources.rules.push(rule);
            }
        }
    }

    /// `source(none)` / `source("dir")` on `@import "tailwindcss"`.
    fn import_source_option(&mut self, base: &str, prelude: &str) {
        for t in tokens(prelude) {
            if let Some(arg) = t.strip_prefix("source(").and_then(|t| t.strip_suffix(')')) {
                if arg.trim() == "none" {
                    self.sources.auto = false;
                } else if let Some(dir) = unquote(arg.trim()) {
                    self.sources.auto_base = normalize(&join(base, dir));
                }
            }
        }
    }
}

/// Resolve the project's CSS files into the custom CSS for the compiler and
/// the scan configuration.
pub fn resolve_css(files: &[FileItem]) -> ResolvedCss {
    let css_files: Vec<&FileItem> = files.iter().filter(|f| f.path.ends_with(".css")).collect();
    let mut resolver = Resolver {
        files: css_files.iter().map(|f| (normalize(&f.path), *f)).collect(),
        sources: Sources::default(),
    };

    // Entries: CSS files no other project CSS file imports.
    let mut imported = std::collections::HashSet::new();
    for f in &css_files {
        let path = normalize(&f.path);
        for st in statements(&f.content) {
            if st.name == "import" {
                if let Some(target) = parse_import(st.prelude).and_then(|i| resolver.find(&path, i.uri)) {
                    let target = normalize(&target.path);
                    if target != path {
                        imported.insert(target);
                    }
                }
            }
        }
    }
    let mut entries: Vec<&FileItem> = css_files.iter().copied().filter(|f| !imported.contains(&normalize(&f.path))).collect();
    if entries.is_empty() {
        // Only import cycles: treat every file as an entry.
        entries = css_files.clone();
    }

    let mut css = String::new();
    for f in entries {
        css.push_str("\n/* File: ");
        css.push_str(&f.path);
        css.push_str(" */\n");
        let mut stack = vec![normalize(&f.path)];
        css.push_str(&resolver.inline(f, &mut stack));
        if !css.ends_with('\n') {
            css.push('\n');
        }
    }
    ResolvedCss { css, sources: resolver.sources }
}

/// Like [`resolve_css`], with a single entry stylesheet (`app/globals.css` of a Next.js app): the
/// other CSS files are only inlined where the entry imports them. Added for com.ai.vn, where CSS that
/// components import is served on its own.
pub fn resolve_css_entry(entry: &FileItem, files: &[FileItem]) -> ResolvedCss {
    let mut resolver = Resolver {
        files: files.iter().filter(|f| f.path.ends_with(".css")).map(|f| (normalize(&f.path), f)).collect(),
        sources: Sources::default(),
    };
    let mut stack = vec![normalize(&entry.path)];
    let css = resolver.inline(entry, &mut stack);
    ResolvedCss { css, sources: resolver.sources }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_one_entry_and_what_it_imports() {
        let files = vec![
            file("app/theme.css", "@theme { --color-brand: #123456; }"),
            file("app/components/card.module.css", ".card { color: red; }"),
        ];
        let entry = file("app/globals.css", "@import \"tailwindcss\";\n@import \"./theme.css\";\n@source \"../lib\";");
        let r = resolve_css_entry(&entry, &files);
        assert!(r.css.contains("--color-brand"), "{}", r.css);
        assert!(!r.css.contains(".card"));
        assert!(r.sources.rules[0].matches("lib/a.tsx"));
    }

    fn file(path: &str, content: &str) -> FileItem {
        FileItem { path: path.into(), content: content.into() }
    }

    #[test]
    fn inlines_project_imports_and_packages() {
        let files = vec![
            file("src/index.css", "@import \"tailwindcss\" prefix(tw);\n@import \"tw-animate-css\";\n@import \"./theme.css\" layer(theme);\n@import \"@/components/btn.css\";\n@tailwind utilities;\n.a { color: red; }"),
            file("src/theme.css", "@theme { --color-brand: #123456; }"),
            file("src/components/btn.css", "@import \"../theme.css\";\n.btn { @apply p-2; }"),
        ];
        let r = resolve_css(&files);
        assert!(r.css.contains("@import \"tailwindcss\" prefix(tw);"), "{}", r.css);
        assert!(!r.css.contains("@import \"tw-animate-css\""));
        assert!(r.css.contains("@utility fade-in"));
        assert!(r.css.contains("@layer theme {\n@theme { --color-brand: #123456; }\n}"));
        assert!(r.css.contains(".btn { @apply p-2; }"));
        assert!(!r.css.contains("@tailwind"));
        // Imported files are not entries
        assert_eq!(r.css.matches("/* File:").count(), 1);
        assert!(!r.css.contains("/* File: src/theme.css"));
    }

    #[test]
    fn keeps_unresolved_imports_and_breaks_cycles() {
        let files = vec![
            file("a.css", "@import \"./b.css\";\n@import \"https://fonts.googleapis.com/css2?family=Inter\";\n@import \"missing.css\";\n.a{}"),
            file("b.css", "@import \"./a.css\";\n.b{}"),
        ];
        let r = resolve_css(&files);
        assert!(r.css.contains("https://fonts.googleapis.com"));
        assert!(r.css.contains("@import \"missing.css\";"));
        assert!(r.css.contains(".a{}") && r.css.contains(".b{}"));
    }

    #[test]
    fn wraps_like_tailwind() {
        let files = vec![file("a.css", "@import \"./b.css\" layer(x) supports(display: grid) screen and (min-width: 40rem);"), file("b.css", ".b{}")];
        let r = resolve_css(&files);
        assert!(r.css.contains("@supports (display: grid) {\n@media screen and (min-width: 40rem) {\n@layer x {\n.b{}\n}\n}\n}"), "{}", r.css);
    }

    #[test]
    fn collects_sources() {
        let files = vec![file(
            "app/styles/main.css",
            "@import \"tailwindcss\" source(none);\n@source \"../components\";\n@source \"../../lib/**/*.{ts,tsx}\";\n@source not \"../components/legacy\";\n@source inline(\"p-4\");",
        )];
        let s = resolve_css(&files).sources;
        assert!(!s.auto);
        assert_eq!(s.rules.len(), 3);
        assert!(s.rules[0].matches("app/components/Button.tsx"));
        assert!(s.rules[1].matches("lib/ui/card.tsx") && !s.rules[1].matches("lib/ui/card.css"));
        assert!(s.rules[2].negated && s.rules[2].matches("app/components/legacy/Old.tsx"));
    }

    #[test]
    fn ignores_statements_in_comments_strings_and_blocks() {
        let files = vec![file("a.css", "/* @import \"./b.css\"; */ .x { content: \"@import './b.css';\"; } @media print { @import \"./b.css\"; } .y{}"), file("b.css", ".b{}")];
        let r = resolve_css(&files);
        // Nothing was inlined into a.css, so b.css is an entry of its own
        assert_eq!(r.css.matches(".b{}").count(), 1, "{}", r.css);
        assert!(r.css.contains("@media print { @import \"./b.css\"; }"));
        assert!(r.css.contains("/* @import \"./b.css\"; */"));
    }
}
