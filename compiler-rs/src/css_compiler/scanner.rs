use regex::Regex;
use std::{
    collections::HashSet,
    fs,
    path::Path,
};
use walkdir::WalkDir;

/// Fast scanner for extracting CSS utility classes from JSX, TSX, JS, TS, and HTML files
pub struct ClassScanner;

impl ClassScanner {
    /// Extracts all class name tokens from a source code string
    pub fn scan_source(source: &str) -> HashSet<String> {
        let mut classes = HashSet::new();

        // Regex 1: Match `className="..."`, `className='...'`, `class="..."`, `class='...'`
        let class_attr_regex = Regex::new(
            r#"(?:className|class)\s*=\s*["']([^"']+)["']"#,
        )
        .unwrap();

        for caps in class_attr_regex.captures_iter(source) {
            if let Some(matched) = caps.get(1) {
                Self::tokenize_classes(matched.as_str(), &mut classes);
            }
        }

        // Regex 2: Match template literals: `className={`...`}` or `clsx(`...`)`
        let template_regex = Regex::new(
            r#"`([^`]+)`"#,
        )
        .unwrap();

        for caps in template_regex.captures_iter(source) {
            if let Some(matched) = caps.get(1) {
                Self::tokenize_classes(matched.as_str(), &mut classes);
            }
        }

        // Regex 3: Match quoted string literals in clsx/cn/cva/twMerge calls: cn("...", '...')
        let fn_call_regex = Regex::new(
            r#"(?:cn|clsx|cva|twMerge|classNames)\s*\(([^)]+)\)"#,
        )
        .unwrap();

        let str_in_fn_regex = Regex::new(r#"["']([^"']+)["']"#).unwrap();

        for caps in fn_call_regex.captures_iter(source) {
            if let Some(args_str) = caps.get(1) {
                for str_caps in str_in_fn_regex.captures_iter(args_str.as_str()) {
                    if let Some(str_matched) = str_caps.get(1) {
                        Self::tokenize_classes(str_matched.as_str(), &mut classes);
                    }
                }
            }
        }

        classes
    }

    /// Recursively scans an entire directory for all candidate classes in source files
    pub fn scan_directory<P: AsRef<Path>>(root_dir: P) -> HashSet<String> {
        let mut all_classes = HashSet::new();
        let root = root_dir.as_ref();

        if !root.exists() {
            return all_classes;
        }

        for entry in WalkDir::new(root)
            .into_iter()
            .filter_entry(|e| !Self::should_ignore(e.path()))
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if path.is_file() && Self::is_scannable_extension(path) {
                if let Ok(content) = fs::read_to_string(path) {
                    let file_classes = Self::scan_source(&content);
                    all_classes.extend(file_classes);
                }
            }
        }

        all_classes
    }

    fn tokenize_classes(raw: &str, set: &mut HashSet<String>) {
        for token in raw.split_whitespace() {
            let clean = token
                .trim_matches(|c: char| {
                    c == '\''
                        || c == '"'
                        || c == '`'
                        || c == '{'
                        || c == '}'
                        || c == '('
                        || c == ')'
                        || c == ','
                        || c == ';'
                        || c == '?'
                        || c == ':'
                })
                .trim();

            if !clean.is_empty()
                && !clean.starts_with('$')
                && !clean.starts_with('{')
                && !clean.ends_with('}')
                && !clean.contains("${")
                && clean != "true"
                && clean != "false"
                && clean != "null"
                && clean != "undefined"
                && clean.len() >= 2
            {
                set.insert(clean.to_string());
            }
        }
    }

    fn is_scannable_extension(path: &Path) -> bool {
        match path.extension().and_then(|s| s.to_str()) {
            Some("tsx") | Some("ts") | Some("jsx") | Some("js") | Some("html") | Some("vue") | Some("svelte") => true,
            _ => false,
        }
    }

    fn should_ignore(path: &Path) -> bool {
        let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
        if name.starts_with('.') && name != "." && name != ".." {
            return true;
        }
        matches!(
            name,
            "node_modules" | "target" | "dist" | "build" | ".git" | ".next" | ".turbo" | "out" | "vendor"
        )
    }
}
