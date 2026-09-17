use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::Path,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TsConfigRaw {
    #[serde(rename = "compilerOptions")]
    compiler_options: Option<CompilerOptionsRaw>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CompilerOptionsRaw {
    #[serde(rename = "baseUrl")]
    base_url: Option<String>,
    paths: Option<HashMap<String, Vec<String>>>,
}

/// Strongly typed parser & resolver for `tsconfig.json` / `jsconfig.json` path mappings
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TsConfig {
    pub base_url: String,
    pub paths: HashMap<String, Vec<String>>,
    pub is_custom: bool,
}

impl Default for TsConfig {
    fn default() -> Self {
        Self::default_fallback()
    }
}

impl TsConfig {
    /// Default Next.js style path alias fallback when no tsconfig.json is present
    pub fn default_fallback() -> Self {
        let mut paths = HashMap::new();
        paths.insert("@/*".to_string(), vec!["src/*".to_string(), "*".to_string()]);
        paths.insert("~/*".to_string(), vec!["*".to_string(), "src/*".to_string()]);

        Self {
            base_url: ".".to_string(),
            paths,
            is_custom: false,
        }
    }

    /// Loads and parses `tsconfig.json` or `jsconfig.json` from a workspace root directory
    pub fn load_from_dir<P: AsRef<Path>>(root_dir: P) -> Self {
        let root = root_dir.as_ref();
        let tsconfig_cand = root.join("tsconfig.json");
        if tsconfig_cand.exists() {
            if let Ok(content) = std::fs::read_to_string(&tsconfig_cand) {
                return Self::parse(&content);
            }
        }

        let jsconfig_cand = root.join("jsconfig.json");
        if jsconfig_cand.exists() {
            if let Ok(content) = std::fs::read_to_string(&jsconfig_cand) {
                return Self::parse(&content);
            }
        }

        Self::default_fallback()
    }

    /// Parses tsconfig JSON string, supporting JSON with comments (JSONC)
    pub fn parse(json_str: &str) -> Self {
        let clean_json = Self::strip_comments(json_str);

        match serde_json::from_str::<TsConfigRaw>(&clean_json) {
            Ok(parsed) => {
                let (base_url, mut paths) = if let Some(opts) = parsed.compiler_options {
                    (
                        opts.base_url.unwrap_or_else(|| ".".to_string()),
                        opts.paths.unwrap_or_default(),
                    )
                } else {
                    (".".to_string(), HashMap::new())
                };

                // Normalize target paths (strip leading `./`)
                for targets in paths.values_mut() {
                    for target in targets.iter_mut() {
                        if target.starts_with("./") {
                            *target = target[2..].to_string();
                        }
                    }
                }

                // If user didn't specify @/*, provide standard default
                if !paths.contains_key("@/*") && !paths.contains_key("@") {
                    paths.insert("@/*".to_string(), vec!["src/*".to_string(), "*".to_string()]);
                }
                if !paths.contains_key("~/*") && !paths.contains_key("~") {
                    paths.insert("~/*".to_string(), vec!["*".to_string(), "src/*".to_string()]);
                }

                Self {
                    base_url,
                    paths,
                    is_custom: true,
                }
            }
            Err(_) => Self::default_fallback(),
        }
    }

    /// Strips single-line `//` and multi-line `/* */` comments from JSONC
    pub fn strip_comments(source: &str) -> String {
        let mut result = String::with_capacity(source.len());
        let mut chars = source.chars().peekable();
        let mut in_string = false;
        let mut escape = false;

        while let Some(ch) = chars.next() {
            if in_string {
                result.push(ch);
                if escape {
                    escape = false;
                } else if ch == '\\' {
                    escape = true;
                } else if ch == '"' {
                    in_string = false;
                }
            } else if ch == '"' {
                in_string = true;
                result.push(ch);
            } else if ch == '/' && chars.peek() == Some(&'/') {
                // Single line comment: skip until newline
                chars.next();
                for c in chars.by_ref() {
                    if c == '\n' {
                        result.push('\n');
                        break;
                    }
                }
            } else if ch == '/' && chars.peek() == Some(&'*') {
                // Multi line comment: skip until */
                chars.next();
                while let Some(c) = chars.next() {
                    if c == '*' && chars.peek() == Some(&'/') {
                        chars.next();
                        break;
                    }
                }
            } else {
                result.push(ch);
            }
        }

        result
    }

    /// Resolves an imported specifier against the configured `paths` mappings.
    /// Returns a list of candidate relative paths without leading `./`
    pub fn resolve_path_alias(&self, specifier: &str) -> Option<Vec<String>> {
        let trimmed = specifier.trim();

        // 1. Exact match against non-wildcard aliases (e.g. "@utils": ["lib/utils"])
        if let Some(targets) = self.paths.get(trimmed) {
            return Some(targets.clone());
        }

        // 2. Wildcard pattern matches (e.g. "@/*", "@components/*", "~/*")
        for (pattern, targets) in &self.paths {
            if let Some((prefix, suffix)) = pattern.split_once('*') {
                if trimmed.starts_with(prefix) && trimmed.ends_with(suffix) {
                    let match_start = prefix.len();
                    let match_end = trimmed.len() - suffix.len();
                    if match_start <= match_end {
                        let wildcard_content = &trimmed[match_start..match_end];
                        let resolved_list: Vec<String> = targets
                            .iter()
                            .map(|target| target.replace('*', wildcard_content))
                            .collect();

                        if !resolved_list.is_empty() {
                            return Some(resolved_list);
                        }
                    }
                }
            }
        }

        None
    }

    /// Resolves a single primary candidate path for an alias
    pub fn resolve_primary_alias(&self, specifier: &str) -> Option<String> {
        self.resolve_path_alias(specifier)
            .and_then(|list| list.into_iter().next())
    }
}
