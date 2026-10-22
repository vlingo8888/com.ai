//! Legacy `theme(dot.path)` resolution, ported from tailwindcss@4.3.3's compat
//! layer (`upgradeToFullPluginSupport` without user plugins/configs):
//! `compat/plugin-functions.ts` (createThemeFn, readFromCss, get, set),
//! `compat/config/resolve-config.ts` (mergeTheme), `compat/config/deep-merge.ts`,
//! `compat/config/create-compat-config.ts`, `compat/default-theme.ts` and
//! `keyPathToCssProperty` from `compat/apply-config-to-theme.ts`.
//!
//! JS values are modelled with [`V`]. `None` (`Option<V>`) stands for
//! `undefined`, [`V::Null`] for `null`.

use crate::tw::design_system::DesignSystem;
use crate::tw::theme::opts;
use crate::tw::utilities::with_alpha;
use crate::tw::utils::{js_num_to_string, segment, unescape};
use indexmap::IndexMap;
use regex::Regex;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

#[derive(Clone, Debug, PartialEq)]
enum V {
    Null,
    Str(String),
    Num(f64),
    Arr(Vec<V>),
    Obj(IndexMap<String, V>),
}

impl V {
    fn from_json(v: &serde_json::Value) -> V {
        match v {
            serde_json::Value::Null => V::Null,
            serde_json::Value::Bool(b) => V::Str(b.to_string()),
            serde_json::Value::Number(n) => V::Num(n.as_f64().unwrap_or(0.0)),
            serde_json::Value::String(s) => V::Str(s.clone()),
            serde_json::Value::Array(a) => V::Arr(a.iter().map(V::from_json).collect()),
            serde_json::Value::Object(o) => V::Obj(o.iter().map(|(k, v)| (k.clone(), V::from_json(v))).collect()),
        }
    }

    /// `value & ThemeOptions.X` with JS number coercion (non-numbers → 0).
    fn bits(&self) -> u8 {
        match self {
            V::Num(n) if n.is_finite() => *n as i64 as u8,
            _ => 0,
        }
    }

    /// JS `String(value)` as used by `Array.prototype.join`.
    fn join_str(&self) -> String {
        match self {
            V::Null => String::new(),
            V::Str(s) => s.clone(),
            V::Num(n) => js_num_to_string(*n),
            V::Arr(a) => a.iter().map(|v| v.join_str()).collect::<Vec<_>>().join(","),
            V::Obj(_) => "[object Object]".into(),
        }
    }
}

/// `obj?.[key]` for JS values.
fn index(obj: &V, key: &str) -> Option<V> {
    let canonical_index = || -> Option<usize> {
        let i: usize = key.parse().ok()?;
        (i.to_string() == key).then_some(i)
    };
    match obj {
        V::Obj(m) => m.get(key).cloned(),
        V::Arr(a) => canonical_index().and_then(|i| a.get(i).cloned()),
        V::Str(s) => canonical_index().and_then(|i| s.encode_utf16().nth(i)).map(|c| V::Str(String::from_utf16_lossy(&[c]))),
        _ => None,
    }
}

/// `{ ...value }` entries.
fn spread_into(target: &mut IndexMap<String, V>, value: Option<&V>) {
    match value {
        Some(V::Obj(m)) => {
            for (k, v) in m {
                target.insert(k.clone(), v.clone());
            }
        }
        Some(V::Arr(a)) => {
            for (i, v) in a.iter().enumerate() {
                target.insert(i.to_string(), v.clone());
            }
        }
        Some(V::Str(s)) => {
            for (i, c) in s.encode_utf16().enumerate() {
                target.insert(i.to_string(), V::Str(String::from_utf16_lossy(&[c])));
            }
        }
        _ => {}
    }
}

/* -------------------------------------------------------------------------- */
/* Default theme (generated from compat/default-theme.ts)                      */
/* -------------------------------------------------------------------------- */

enum Entry {
    Value(V),
    /// `({ theme }) => …`: literal entries followed by `...theme(ref)` spreads,
    /// or a direct `theme(ref)` return.
    Fn { refs: Vec<String>, direct: bool, literal: IndexMap<String, V> },
}

static DEFAULT_THEME: LazyLock<IndexMap<String, Entry>> = LazyLock::new(|| {
    let json: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(include_str!("assets/compat_default_theme.json")).expect("valid default theme json");
    json.iter()
        .map(|(k, v)| {
            let entry = match v.get("$fn") {
                Some(f) => Entry::Fn {
                    refs: f["refs"].as_array().map(|a| a.iter().filter_map(|r| r.as_str().map(String::from)).collect()).unwrap_or_default(),
                    direct: f["direct"].as_bool().unwrap_or(false),
                    literal: match V::from_json(&f["literal"]) {
                        V::Obj(m) => m,
                        _ => IndexMap::new(),
                    },
                },
                None => Entry::Value(V::from_json(v)),
            };
            (k.clone(), entry)
        })
        .collect()
});

/// Keys of `createCompatConfig(...).theme.extend` and the CSS theme
/// namespace they spread in (`None` for the transition defaults).
const EXTEND_KEYS: &[(&str, &str)] = &[
    ("fontSize", "text"),
    ("boxShadow", "shadow"),
    ("animation", "animate"),
    ("aspectRatio", "aspect"),
    ("borderRadius", "radius"),
    ("screens", "breakpoint"),
    ("letterSpacing", "tracking"),
    ("lineHeight", "leading"),
    ("transitionDuration", "--default-transition-duration"),
    ("transitionTimingFunction", "--default-transition-timing-function"),
    ("maxWidth", "container"),
];

/* -------------------------------------------------------------------------- */
/* Key paths                                                                   */
/* -------------------------------------------------------------------------- */

fn to_key_path(path: &str) -> Vec<String> {
    let mut keypath = vec![];
    for part in segment(path, b'.') {
        if !part.contains('[') {
            keypath.push(part.to_string());
            continue;
        }
        let mut current = 0;
        loop {
            let Some(l) = part[current..].find('[').map(|i| i + current) else { break };
            let Some(r) = part[l..].find(']').map(|i| i + l) else { break };
            if l > current {
                keypath.push(part[current..l].to_string());
            }
            keypath.push(part[l + 1..r].to_string());
            current = r + 1;
        }
        if current < part.len() {
            keypath.push(part[current..].to_string());
        }
    }
    keypath
}

fn key_path_to_css_property(path: &[String]) -> Option<String> {
    let special = match path[0].as_str() {
        "borderWidth" => Some("border-width"),
        "outlineWidth" => Some("outline-width"),
        "ringColor" => Some("ring-color"),
        "ringWidth" => Some("ring-width"),
        "transitionDuration" => Some("transition-duration"),
        "transitionTimingFunction" => Some("transition-timing-function"),
        _ => None,
    };
    if let Some(s) = special {
        if path.get(1).map(|s| s.as_str()) == Some("DEFAULT") {
            return Some(format!("default-{}", s));
        }
    }
    if path[0] == "container" {
        return None;
    }
    static VALID: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[a-zA-Z0-9\-_%/.]+$").unwrap());
    if path.iter().any(|p| !VALID.is_match(p)) {
        return None;
    }

    let mut path: Vec<String> = path.to_vec();
    let ns = match path[0].as_str() {
        "animation" => Some("animate"),
        "aspectRatio" => Some("aspect"),
        "borderRadius" => Some("radius"),
        "boxShadow" => Some("shadow"),
        "colors" => Some("color"),
        "containers" => Some("container"),
        "fontFamily" => Some("font"),
        "fontSize" => Some("text"),
        "letterSpacing" => Some("tracking"),
        "lineHeight" => Some("leading"),
        "maxWidth" => Some("container"),
        "screens" => Some("breakpoint"),
        "transitionTimingFunction" => Some("ease"),
        _ => None,
    };
    if let Some(ns) = ns {
        path[0] = ns.to_string();
    }

    static CAMEL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"([a-z])([A-Z])").unwrap());
    let len = path.len();
    let parts: Vec<String> = path
        .iter()
        .enumerate()
        .map(|(idx, p)| if p == "1" && idx != len - 1 { String::new() } else { p.clone() })
        .enumerate()
        .map(|(idx, part)| {
            let part = part.replace('.', "_");
            if idx == 0 || part.starts_with('-') || part == "lineHeight" {
                CAMEL
                    .replace_all(&part, |c: &regex::Captures| format!("{}-{}", &c[1], c[2].to_lowercase()))
                    .into_owned()
            } else {
                part
            }
        })
        .enumerate()
        .filter(|(idx, part)| part != "DEFAULT" || *idx != len - 1)
        .map(|(_, p)| p)
        .collect();
    Some(parts.join("-"))
}

/* -------------------------------------------------------------------------- */
/* Resolver                                                                    */
/* -------------------------------------------------------------------------- */

struct Resolver<'a> {
    ds: &'a DesignSystem,
    memo: RefCell<HashMap<String, Option<V>>>,
    in_progress: RefCell<HashSet<String>>,
}

impl<'a> Resolver<'a> {
    /// The resolved value of `config.theme[key]`.
    fn root(&self, key: &str) -> Option<V> {
        if let Some(v) = self.memo.borrow().get(key) {
            return v.clone();
        }
        if !self.in_progress.borrow_mut().insert(key.to_string()) {
            return None;
        }
        let value = self.compute_root(key);
        self.in_progress.borrow_mut().remove(key);
        self.memo.borrow_mut().insert(key.to_string(), value.clone());
        value
    }

    fn eval_entry(&self, key: &str) -> Option<V> {
        if key == "colors" {
            // createCompatConfig: `colors: ({ theme }) => theme('color', {})`
            return Some(self.theme("color", Some(V::Obj(IndexMap::new()))).unwrap_or(V::Null));
        }
        match DEFAULT_THEME.get(key)? {
            Entry::Value(v) => Some(v.clone()),
            Entry::Fn { refs, direct, literal } => {
                if *direct {
                    return Some(self.theme(&refs[0], None).unwrap_or(V::Null));
                }
                let mut obj = literal.clone();
                for r in refs {
                    spread_into(&mut obj, self.theme(r, None).as_ref());
                }
                Some(V::Obj(obj))
            }
        }
    }

    fn compute_root(&self, key: &str) -> Option<V> {
        let Some(&(_, ext)) = EXTEND_KEYS.iter().find(|(k, _)| *k == key) else {
            return self.eval_entry(key);
        };

        // `extend` value from createCompatConfig
        let extension = if let Some(var) = ext.strip_prefix("--").map(|_| ext) {
            let mut m = IndexMap::new();
            m.insert("DEFAULT".to_string(), self.ds.theme.get(&[var]).map(V::Str).unwrap_or(V::Null));
            V::Obj(m)
        } else {
            let mut m = IndexMap::new();
            spread_into(&mut m, self.theme(ext, Some(V::Obj(IndexMap::new()))).as_ref());
            V::Obj(m)
        };

        let values: Vec<V> = vec![self.eval_entry(key).unwrap_or(V::Null), extension];
        let mut target = IndexMap::new();
        deep_merge(&mut target, &values, true);
        Some(V::Obj(target))
    }

    /// `createThemeFn(...)(path, defaultValue)`
    fn theme(&self, path: &str, default: Option<V>) -> Option<V> {
        let mut path = path.to_string();
        let mut modifier: Option<String> = None;
        if let Some(i) = path.rfind('/') {
            modifier = Some(path[i + 1..].trim().to_string());
            path = path[..i].trim().to_string();
        }

        let keypath = to_key_path(&path);
        let (css_value, options) = self.read_from_css(&keypath);
        let mut config_value = self.get(keypath).unwrap_or(V::Null);
        if let V::Str(s) = &config_value {
            config_value = V::Str(s.replacen("<alpha-value>", "1", 1));
        }

        let resolved: Option<V> = if let V::Str(_) = css_value {
            if matches!(options, V::Num(_)) && options.bits() & opts::DEFAULT != 0 {
                Some(if config_value != V::Null { config_value } else { css_value })
            } else {
                Some(css_value)
            }
        } else if let V::Obj(config) = &config_value {
            let mut copy = config.clone();
            let mut css = css_value.clone();
            if css == V::Null {
                if let Some(V::Obj(css_values)) = config.get("__CSS_VALUES__") {
                    let mut local = IndexMap::new();
                    for key in css_values.keys() {
                        if let Some(v) = config.get(key) {
                            local.insert(key.clone(), v.clone());
                        }
                        copy.shift_remove(key);
                    }
                    css = V::Obj(local);
                }
            }
            let entries: Vec<(String, V)> = match &css {
                V::Obj(m) => m.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
                V::Arr(a) => a.iter().enumerate().map(|(i, v)| (i.to_string(), v.clone())).collect(),
                _ => vec![],
            };
            for (key, value) in entries {
                if key == "__CSS_VALUES__" {
                    continue;
                }
                let is_default = match config.get("__CSS_VALUES__") {
                    Some(cv) => index(cv, &key).map(|o| o.bits() & opts::DEFAULT != 0).unwrap_or(false),
                    None => false,
                };
                if is_default && get_in(&V::Obj(copy.clone()), key.split('-').map(String::from).collect()).is_some() {
                    continue;
                }
                copy.insert(unescape(&key), value);
            }
            Some(V::Obj(copy))
        } else if let (V::Arr(css), V::Arr(o), V::Arr(cfg)) = (&css_value, &options, &config_value) {
            let mut base = css.first().cloned().unwrap_or(V::Null);
            let mut extra = match css.get(1) {
                Some(V::Obj(m)) => m.clone(),
                _ => IndexMap::new(),
            };
            if o.first().map(|v| v.bits()).unwrap_or(0) & opts::DEFAULT != 0 {
                if let Some(v) = cfg.first().filter(|v| **v != V::Null) {
                    base = v.clone();
                }
            }
            let extra_options = o.get(1).cloned().unwrap_or(V::Null);
            let keys: Vec<String> = extra.keys().cloned().collect();
            for key in keys {
                if index(&extra_options, &key).map(|v| v.bits()).unwrap_or(0) & opts::DEFAULT != 0 {
                    // `configValue[1][key]` throws when `configValue[1]` is undefined
                    let cfg_extra = cfg.get(1)?;
                    if let Some(v) = index(cfg_extra, &key).filter(|v| *v != V::Null) {
                        extra.insert(key, v);
                    }
                }
            }
            Some(V::Arr(vec![base, V::Obj(extra)]))
        } else if css_value != V::Null {
            Some(css_value)
        } else {
            Some(config_value)
        };

        let mut resolved = resolved;
        if let (Some(m), Some(V::Str(s))) = (&modifier, &resolved) {
            if !m.is_empty() {
                resolved = Some(V::Str(with_alpha(s, m)));
            }
        }

        match resolved {
            None | Some(V::Null) => default,
            v => v,
        }
    }

    /// `get(configTheme(), keypath)` (the key path is mutated as upstream does).
    fn get(&self, mut path: Vec<String>) -> Option<V> {
        if path.is_empty() {
            return None;
        }
        let mut obj: Option<V> = None;
        let mut i = 0;
        while i < path.len() {
            let key = path[i].clone();
            let child = if i == 0 { self.root(&key) } else { obj.as_ref().and_then(|o| index(o, &key)) };
            match child {
                None => {
                    if i + 1 >= path.len() {
                        return None;
                    }
                    path[i + 1] = format!("{}-{}", key, path[i + 1]);
                }
                Some(c) => {
                    if matches!(obj, Some(V::Str(_))) {
                        return None;
                    }
                    obj = Some(c);
                }
            }
            i += 1;
        }
        obj
    }

    fn read_from_css(&self, path: &[String]) -> (V, V) {
        let theme = &self.ds.theme;
        if path.len() == 1 && path[0].starts_with("--") {
            return (
                theme.get(&[path[0].as_str()]).map(V::Str).unwrap_or(V::Null),
                V::Num(theme.get_options(&path[0]) as f64),
            );
        }

        let theme_key = key_path_to_css_property(path).unwrap_or_else(|| "null".to_string());
        let ns = theme.namespace(&format!("--{}", theme_key));
        if ns.is_empty() {
            return (V::Null, V::Num(0.0));
        }

        let mut map: IndexMap<Option<String>, V> = IndexMap::new();
        let mut options: IndexMap<Option<String>, V> = IndexMap::new();
        let mut nested: IndexMap<Option<String>, IndexMap<String, (String, u8)>> = IndexMap::new();

        static DASH_LOWER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"-([a-z])").unwrap());

        for (key, value) in ns {
            match &key {
                Some(k) if k.contains("--") => {
                    let idx = k.find("--").unwrap();
                    let main = &k[..idx];
                    let nested_key = DASH_LOWER
                        .replace_all(&k[idx + 2..], |c: &regex::Captures| c[1].to_uppercase())
                        .into_owned();
                    let opt = theme.get_options(&format!("--{}{}", theme_key, k));
                    nested
                        .entry(if main.is_empty() { None } else { Some(main.to_string()) })
                        .or_default()
                        .insert(nested_key, (value, opt));
                }
                _ => {
                    let opt_key = match &key {
                        None => format!("--{}", theme_key),
                        Some(k) => format!("--{}-{}", theme_key, k),
                    };
                    map.insert(key.clone(), V::Str(value));
                    options.insert(key, V::Num(theme.get_options(&opt_key) as f64));
                }
            }
        }

        let base_options = theme.get_options(&format!("--{}", theme_key)) as f64;
        for (key, extra) in nested {
            let Some(V::Str(value)) = map.get(&key).cloned() else { continue };
            let extra_obj: IndexMap<String, V> = extra.iter().map(|(k, (v, _))| (k.clone(), V::Str(v.clone()))).collect();
            let extra_opts: IndexMap<String, V> = extra.iter().map(|(k, (_, o))| (k.clone(), V::Num(*o as f64))).collect();
            map.insert(key.clone(), V::Arr(vec![V::Str(value), V::Obj(extra_obj)]));
            options.insert(key, V::Arr(vec![V::Num(base_options), V::Obj(extra_opts)]));
        }

        let mut obj: IndexMap<String, V> = IndexMap::new();
        for (k, v) in map {
            obj.insert(k.unwrap_or_else(|| "DEFAULT".into()), v);
        }
        let mut options_obj: IndexMap<String, V> = IndexMap::new();
        for (k, v) in options {
            options_obj.insert(k.unwrap_or_else(|| "DEFAULT".into()), v);
        }

        if path.last().map(|s| s.as_str()) == Some("DEFAULT") {
            return (
                obj.get("DEFAULT").cloned().unwrap_or(V::Null),
                options_obj.get("DEFAULT").cloned().unwrap_or(V::Num(0.0)),
            );
        }

        if obj.contains_key("DEFAULT") && obj.len() == 1 {
            return (obj["DEFAULT"].clone(), options_obj.get("DEFAULT").cloned().unwrap_or(V::Num(0.0)));
        }

        obj.insert("__CSS_VALUES__".into(), V::Obj(options_obj.clone()));
        (V::Obj(obj), V::Obj(options_obj))
    }
}

/// `get()` on a plain value (no root resolution).
fn get_in(start: &V, mut path: Vec<String>) -> Option<V> {
    let mut obj = start.clone();
    let mut i = 0;
    while i < path.len() {
        let key = path[i].clone();
        match index(&obj, &key) {
            None => {
                if i + 1 >= path.len() {
                    return None;
                }
                path[i + 1] = format!("{}-{}", key, path[i + 1]);
            }
            Some(c) => {
                if matches!(obj, V::Str(_)) {
                    return None;
                }
                obj = c;
            }
        }
        i += 1;
    }
    Some(obj)
}

/// `mergeThemeExtension` (`None` = default behavior).
fn merge_theme_extension(theme_value: Option<&V>, extension: &V) -> Option<V> {
    if let Some(V::Arr(a)) = theme_value {
        if matches!(a.first(), Some(V::Obj(_))) {
            let mut out = a.clone();
            match extension {
                V::Arr(e) => out.extend(e.iter().cloned()),
                other => out.push(other.clone()),
            }
            return Some(V::Arr(out));
        }
    }
    if let V::Arr(e) = extension {
        if matches!(e.first(), Some(V::Obj(_))) && matches!(theme_value, Some(V::Obj(_))) {
            let mut out = vec![theme_value.unwrap().clone()];
            out.extend(e.iter().cloned());
            return Some(V::Arr(out));
        }
        return Some(extension.clone());
    }
    None
}

/// `deepMerge(target, sources, mergeThemeExtension)`
fn deep_merge(target: &mut IndexMap<String, V>, sources: &[V], with_extension: bool) {
    for source in sources {
        let V::Obj(src) = source else {
            if let V::Arr(a) = source {
                // Reflect.ownKeys on arrays: indices (+ `length`, which is
                // not enumerable in practice for our data).
                for (i, v) in a.iter().enumerate() {
                    target.insert(i.to_string(), v.clone());
                }
            }
            continue;
        };
        for (k, v) in src {
            let current = target.get(k).cloned();
            let merged = if with_extension { merge_theme_extension(current.as_ref(), v) } else { None };
            if let Some(m) = merged {
                target.insert(k.clone(), m);
            } else if let (Some(V::Obj(_)), V::Obj(_)) = (&current, v) {
                let mut inner = IndexMap::new();
                deep_merge(&mut inner, &[current.unwrap(), v.clone()], with_extension);
                target.insert(k.clone(), V::Obj(inner));
            } else {
                target.insert(k.clone(), v.clone());
            }
        }
    }
}

/// The plugin API's `theme(path)` when it resolves to a string (what native
/// plugins read, e.g. `theme('colors.gray.500')`).
pub fn plugin_theme_string(path: &str, ds: &DesignSystem) -> Option<String> {
    let resolver = Resolver { ds, memo: RefCell::new(HashMap::new()), in_progress: RefCell::new(HashSet::new()) };
    match resolver.theme(path, None)? {
        V::Str(s) => Some(s),
        _ => None,
    }
}

/// Resolve a legacy `theme(…)` path (not starting with `--`).
pub fn resolve_legacy_theme_value(path: &str, ds: &DesignSystem) -> Option<String> {
    let resolver = Resolver { ds, memo: RefCell::new(HashMap::new()), in_progress: RefCell::new(HashSet::new()) };
    match resolver.theme(path, None)? {
        V::Arr(a) if a.len() == 2 => match &a[0] {
            V::Str(s) => Some(s.clone()),
            _ => None,
        },
        V::Arr(a) => Some(a.iter().map(|v| v.join_str()).collect::<Vec<_>>().join(", ")),
        V::Obj(m) => match m.get("DEFAULT") {
            Some(V::Str(s)) => Some(s.clone()),
            _ => None,
        },
        V::Str(s) => Some(s),
        V::Num(_) | V::Null => None,
    }
}
