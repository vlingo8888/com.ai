//! Port of `tailwindcss/src/theme.ts`.

use crate::tw::ast::AstNode;
use crate::tw::utils::{escape, unescape};
use indexmap::IndexMap;
use std::cell::Cell;

pub mod opts {
    pub const NONE: u8 = 0;
    pub const INLINE: u8 = 1 << 0;
    pub const REFERENCE: u8 = 1 << 1;
    pub const DEFAULT: u8 = 1 << 2;
    pub const STATIC: u8 = 1 << 3;
    pub const USED: u8 = 1 << 4;
}

#[derive(Debug, Clone)]
pub struct ThemeValue {
    pub value: String,
    pub options: Cell<u8>,
}

#[derive(Debug, Clone, Default)]
pub struct Theme {
    pub prefix: Option<String>,
    values: IndexMap<String, ThemeValue>,
    keyframes: Vec<AstNode>,
}

fn ignored_keys(namespace: &str) -> &'static [&'static str] {
    match namespace {
        "--font" => &["--font-weight", "--font-size"],
        "--inset" => &["--inset-shadow", "--inset-ring"],
        "--text" => &[
            "--text-color",
            "--text-decoration-color",
            "--text-decoration-thickness",
            "--text-indent",
            "--text-shadow",
            "--text-underline-offset",
        ],
        "--grid-column" => &["--grid-column-start", "--grid-column-end"],
        "--grid-row" => &["--grid-row-start", "--grid-row-end"],
        _ => &[],
    }
}

fn is_ignored_theme_key(theme_key: &str, namespace: &str) -> bool {
    ignored_keys(namespace).iter().any(|ignored| {
        theme_key == *ignored
            || (theme_key.starts_with(ignored) && theme_key.as_bytes().get(ignored.len()) == Some(&b'-'))
    })
}

impl Theme {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn size(&self) -> usize {
        self.values.len()
    }

    pub fn add(&mut self, key: &str, value: &str, options: u8) -> Result<(), String> {
        if key.ends_with("-*") {
            if value != "initial" {
                return Err(format!("Invalid theme value `{}` for namespace `{}`", value, key));
            }
            if key == "--*" {
                self.values.clear();
            } else {
                self.clear_namespace(&key[..key.len() - 2], opts::NONE);
            }
        }

        if options & opts::DEFAULT != 0 {
            if let Some(existing) = self.values.get(key) {
                if existing.options.get() & opts::DEFAULT == 0 {
                    return Ok(());
                }
            }
        }

        if value == "initial" {
            self.values.shift_remove(key);
        } else if let Some(existing) = self.values.get_mut(key) {
            existing.value = value.to_string();
            existing.options.set(options);
        } else {
            self.values.insert(key.to_string(), ThemeValue { value: value.to_string(), options: Cell::new(options) });
        }
        Ok(())
    }

    pub fn keys_in_namespaces(&self, theme_keys: &[&str]) -> Vec<String> {
        let mut keys = vec![];
        for namespace in theme_keys {
            let prefix = format!("{}-", namespace);
            for key in self.values.keys() {
                if !key.starts_with(&prefix) {
                    continue;
                }
                if key[2..].contains("--") {
                    continue;
                }
                if is_ignored_theme_key(key, namespace) {
                    continue;
                }
                keys.push(key[prefix.len()..].to_string());
            }
        }
        keys
    }

    pub fn get(&self, theme_keys: &[&str]) -> Option<String> {
        for key in theme_keys {
            if let Some(v) = self.values.get(*key) {
                return Some(v.value.clone());
            }
        }
        None
    }

    pub fn has_default(&self, key: &str) -> bool {
        self.get_options(key) & opts::DEFAULT == opts::DEFAULT
    }

    pub fn get_options(&self, key: &str) -> u8 {
        if self.prefix.is_none() && !key.contains('\\') {
            return self.values.get(key).map(|v| v.options.get()).unwrap_or(opts::NONE);
        }
        let key = unescape(&self.unprefix_key(key));
        self.values.get(&key).map(|v| v.options.get()).unwrap_or(opts::NONE)
    }

    pub fn entries(&self) -> Vec<(String, &ThemeValue)> {
        self.values.iter().map(|(k, v)| (self.prefix_key(k), v)).collect()
    }

    pub fn prefix_key(&self, key: &str) -> String {
        match &self.prefix {
            None => key.to_string(),
            Some(p) => format!("--{}-{}", p, &key[2..]),
        }
    }

    fn unprefix_key(&self, key: &str) -> String {
        match &self.prefix {
            None => key.to_string(),
            Some(p) => {
                let skip = 3 + p.len();
                if key.len() >= skip {
                    format!("--{}", &key[skip..])
                } else {
                    key.to_string()
                }
            }
        }
    }

    pub fn clear_namespace(&mut self, namespace: &str, clear_options: u8) {
        let ignored = ignored_keys(namespace);
        let keys: Vec<String> = self.values.keys().cloned().collect();
        'outer: for key in keys {
            if key.starts_with(namespace) {
                if clear_options != opts::NONE {
                    let options = self.get_options(&key);
                    if options & clear_options != clear_options {
                        continue;
                    }
                }
                for ignored_ns in ignored {
                    if key.starts_with(ignored_ns) {
                        continue 'outer;
                    }
                }
                self.values.shift_remove(&key);
            }
        }
    }

    fn resolve_key(&self, candidate_value: Option<&str>, theme_keys: &[&str]) -> Option<String> {
        for namespace in theme_keys {
            let mut theme_key = match candidate_value {
                Some(v) => format!("{}-{}", namespace, v),
                None => namespace.to_string(),
            };
            if !self.values.contains_key(&theme_key) {
                match candidate_value {
                    Some(v) if v.contains('.') => {
                        theme_key = format!("{}-{}", namespace, v.replace('.', "_"));
                        if !self.values.contains_key(&theme_key) {
                            continue;
                        }
                    }
                    _ => continue,
                }
            }
            if is_ignored_theme_key(&theme_key, namespace) {
                continue;
            }
            return Some(theme_key);
        }
        None
    }

    fn var(&self, theme_key: &str) -> Option<String> {
        let value = self.values.get(theme_key)?;
        let fallback = if value.options.get() & opts::REFERENCE != 0 { Some(&value.value) } else { None };
        Some(match fallback {
            Some(f) => format!("var({}, {})", escape(&self.prefix_key(theme_key)), f),
            None => format!("var({})", escape(&self.prefix_key(theme_key))),
        })
    }

    pub fn mark_used_variable(&self, theme_key: &str) -> bool {
        let owned;
        let key = if self.prefix.is_none() && !theme_key.contains('\\') {
            theme_key
        } else {
            owned = unescape(&self.unprefix_key(theme_key));
            owned.as_str()
        };
        let Some(value) = self.values.get(key) else {
            return false;
        };
        let was_used = value.options.get() & opts::USED != 0;
        value.options.set(value.options.get() | opts::USED);
        !was_used
    }

    /// Options of every value, in insertion order.
    pub fn snapshot_options(&self) -> Vec<u8> {
        self.values.values().map(|v| v.options.get()).collect()
    }

    /// Restore options captured by `snapshot_options` (values must not have
    /// been added or removed in between).
    pub fn restore_options(&self, snapshot: &[u8]) {
        for (v, o) in self.values.values().zip(snapshot) {
            v.options.set(*o);
        }
    }

    pub fn clear_used(&self, theme_key: &str) {
        let key = unescape(&self.unprefix_key(theme_key));
        if let Some(value) = self.values.get(&key) {
            value.options.set(value.options.get() & !opts::USED);
        }
    }

    pub fn resolve(&self, candidate_value: Option<&str>, theme_keys: &[&str]) -> Option<String> {
        self.resolve_opts(candidate_value, theme_keys, opts::NONE)
    }

    pub fn resolve_opts(&self, candidate_value: Option<&str>, theme_keys: &[&str], options: u8) -> Option<String> {
        let theme_key = self.resolve_key(candidate_value, theme_keys)?;
        let value = self.values.get(&theme_key)?;
        if (options | value.options.get()) & opts::INLINE != 0 {
            return Some(value.value.clone());
        }
        self.var(&theme_key)
    }

    pub fn resolve_value(&self, candidate_value: Option<&str>, theme_keys: &[&str]) -> Option<String> {
        let theme_key = self.resolve_key(candidate_value, theme_keys)?;
        self.values.get(&theme_key).map(|v| v.value.clone())
    }

    pub fn resolve_with(
        &self,
        candidate_value: &str,
        theme_keys: &[&str],
        nested_keys: &[&str],
    ) -> Option<(String, Vec<(String, String)>)> {
        let theme_key = self.resolve_key(Some(candidate_value), theme_keys)?;
        let mut extra = vec![];
        for name in nested_keys {
            let nested_key = format!("{}{}", theme_key, name);
            let Some(nested) = self.values.get(&nested_key) else { continue };
            if nested.options.get() & opts::INLINE != 0 {
                extra.push((name.to_string(), nested.value.clone()));
            } else {
                extra.push((name.to_string(), self.var(&nested_key)?));
            }
        }
        let value = self.values.get(&theme_key)?;
        if value.options.get() & opts::INLINE != 0 {
            return Some((value.value.clone(), extra));
        }
        Some((self.var(&theme_key)?, extra))
    }

    /// Returns `(key, value)` pairs. `None` key means the namespace itself.
    pub fn namespace(&self, namespace: &str) -> Vec<(Option<String>, String)> {
        let mut out: IndexMap<Option<String>, String> = IndexMap::new();
        let prefix = format!("{}-", namespace);
        let double_prefix = format!("{}-", prefix);
        for (key, value) in &self.values {
            if key == namespace {
                out.insert(None, value.value.clone());
            } else if key.starts_with(&double_prefix) {
                out.insert(Some(key[namespace.len()..].to_string()), value.value.clone());
            } else if key.starts_with(&prefix) {
                out.insert(Some(key[prefix.len()..].to_string()), value.value.clone());
            }
        }
        out.into_iter().collect()
    }

    pub fn add_keyframes(&mut self, value: AstNode) {
        if !self.keyframes.contains(&value) {
            self.keyframes.push(value);
        }
    }

    pub fn get_keyframes(&self) -> &[AstNode] {
        &self.keyframes
    }
}
