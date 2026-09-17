use lightningcss::stylesheet::{ParserOptions, PrinterOptions, StyleSheet};
use std::{
    collections::HashSet,
    fs,
    path::Path,
};

use super::{rules::RuleGenerator, scanner::ClassScanner};

/// High-performance Native Server CSS Engine
pub struct CssCompiler;

impl CssCompiler {
    /// Scans project directories for custom CSS (globals.css/global.css) and recursively inlines imports
    pub fn find_and_load_custom_css<P: AsRef<Path>>(root_dir: P) -> String {
        let root = root_dir.as_ref();
        let candidates = [
            root.join("app").join("globals.css"),
            root.join("app").join("global.css"),
            root.join("src").join("app").join("globals.css"),
            root.join("src").join("app").join("global.css"),
            root.join("src").join("globals.css"),
            root.join("src").join("global.css"),
            root.join("styles").join("globals.css"),
            root.join("styles").join("global.css"),
        ];

        for c in &candidates {
            if c.exists() {
                if let Ok(content) = fs::read_to_string(c) {
                    let parent = c.parent().unwrap_or(root);
                    return Self::inline_css_imports(&content, parent, root);
                }
            }
        }
        String::new()
    }

    /// Ensures global.css <-> globals.css reciprocal compatibility in app/, src/app/, and styles/
    pub fn sync_css_aliases<P: AsRef<Path>>(root_dir: P) {
        let root = root_dir.as_ref();
        let dirs = [
            root.join("app"),
            root.join("src").join("app"),
            root.join("src"),
            root.join("styles"),
        ];

        for dir in &dirs {
            if dir.exists() {
                let global_css = dir.join("global.css");
                let globals_css = dir.join("globals.css");
                if global_css.exists() && !globals_css.exists() {
                    let _ = std::fs::copy(&global_css, &globals_css);
                } else if globals_css.exists() && !global_css.exists() {
                    let _ = std::fs::copy(&globals_css, &global_css);
                } else if global_css.exists() && globals_css.exists() {
                    if let Ok(content_globals) = std::fs::read_to_string(&globals_css) {
                        if content_globals.trim().starts_with("@import \"./global.css\"")
                            || content_globals.trim().starts_with("@import './global.css'")
                        {
                            let _ = std::fs::copy(&global_css, &globals_css);
                        }
                    }
                    if let Ok(content_global) = std::fs::read_to_string(&global_css) {
                        if content_global.trim().starts_with("@import \"./globals.css\"")
                            || content_global.trim().starts_with("@import './globals.css'")
                        {
                            let _ = std::fs::copy(&globals_css, &global_css);
                        }
                    }
                }
            }
        }
    }

    /// Compiles complete CSS for a workspace root directory
    pub fn compile_workspace<P: AsRef<Path>>(root_dir: P) -> String {
        let root = root_dir.as_ref();

        // 1. Read and inline custom globals.css / global.css
        let custom_css = Self::find_and_load_custom_css(root);

        // 2. Scan all project files for active class tokens
        let scanned_classes = ClassScanner::scan_directory(root);

        // 3. Synthesize preflight + utility rules + custom CSS
        Self::compile_from_classes(&scanned_classes, &custom_css)
    }

    /// Inlines relative CSS @import statements (e.g., @import "./global.css"; or @import "@/styles/theme.css";)
    pub fn inline_css_imports(css: &str, current_dir: &Path, root_dir: &Path) -> String {
        let mut result = Vec::new();
        for line in css.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("@import") {
                let after_import = trimmed.trim_start_matches("@import").trim();
                let quote_char = after_import.chars().next();
                if quote_char == Some('"') || quote_char == Some('\'') {
                    let q = quote_char.unwrap();
                    if let Some(end_idx) = after_import[1..].find(q) {
                        let import_path = &after_import[1..=end_idx];
                        if import_path.starts_with('.') || import_path.starts_with('/') || import_path.starts_with('@') || import_path.starts_with('~') {
                            let target_path = if import_path.starts_with("@/") {
                                root_dir.join(&import_path[2..])
                            } else if import_path.starts_with("~/") {
                                root_dir.join(&import_path[2..])
                            } else if import_path.starts_with('/') {
                                root_dir.join(&import_path[1..])
                            } else {
                                current_dir.join(import_path)
                            };

                            if target_path.exists() {
                                if let Ok(imported_content) = std::fs::read_to_string(&target_path) {
                                    let target_dir = target_path.parent().unwrap_or(root_dir);
                                    let inlined = Self::inline_css_imports(&imported_content, target_dir, root_dir);
                                    result.push(inlined);
                                    continue;
                                }
                            }
                            result.push(format!("/* inlined import: {} */", import_path));
                            continue;
                        }
                    }
                }
            }
            result.push(line.to_string());
        }
        result.join("\n")
    }

    /// Compiles CSS from a set of classes and optional custom CSS string
    pub fn compile_from_classes(classes: &HashSet<String>, custom_css: &str) -> String {
        let mut raw_css = String::new();

        // Add preflight reset
        raw_css.push_str(RuleGenerator::generate_preflight());
        raw_css.push('\n');

        // Add generated utility rules
        raw_css.push_str(&RuleGenerator::generate_rules(classes));
        raw_css.push('\n');

        // Clean and add custom CSS (strip @import "tailwindcss" if present)
        let clean_custom = custom_css
            .lines()
            .filter(|line| !line.trim().starts_with("@import \"tailwindcss\"") && !line.trim().starts_with("@import 'tailwindcss'"))
            .collect::<Vec<_>>()
            .join("\n");
        raw_css.push_str(&clean_custom);

        // Run through LightningCSS for nesting expansion, vendor prefixing, and minification
        Self::minify_and_optimize(&raw_css).unwrap_or(raw_css)
    }

    /// Uses LightningCSS to parse, vendor-prefix, and minify CSS
    pub fn minify_and_optimize(css_input: &str) -> Result<String, String> {
        let stylesheet = match StyleSheet::parse(css_input, ParserOptions::default()) {
            Ok(s) => s,
            Err(err) => return Err(format!("LightningCSS Parse Error: {:?}", err)),
        };

        let printer_options = PrinterOptions {
            minify: true,
            ..Default::default()
        };

        match stylesheet.to_css(printer_options) {
            Ok(res) => Ok(res.code),
            Err(err) => Err(format!("LightningCSS Codegen Error: {:?}", err)),
        }
    }
}
