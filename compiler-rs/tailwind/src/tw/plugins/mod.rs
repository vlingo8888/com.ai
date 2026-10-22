//! `@plugin` support: the plugin API of `compat/plugin-api.ts` (`addBase`,
//! `addVariant`, `addUtilities` / `addComponents`, `objectToAst`) and native
//! ports of the plugins it can't run (JavaScript):
//!
//! - `@tailwindcss/typography` ([`typography`]): recorded plugin calls
//! - `@tailwindcss/forms` ([`forms`]): ported, reads the theme like the plugin
//!
//! Other plugins are dropped with a warning.

pub mod forms;
pub mod typography;

use crate::tw::ast::{at_rule, decl, rule, walk, AstNode, WalkAction};
use crate::tw::css_functions::substitute_functions;
use crate::tw::css_parser;
use crate::tw::design_system::DesignSystem;
use crate::tw::selector_parser::{self, SNode};
use crate::tw::utils::{escape, segment};
use crate::tw::variants::{compounds_for_selectors, substitute_at_slot};
use indexmap::IndexMap;
use std::rc::Rc;

/// A CSS-in-JS value, keys in insertion order.
#[derive(Clone, Debug, PartialEq)]
pub enum CssInJs {
    Str(String),
    Arr(Vec<CssInJs>),
    Obj(Vec<(String, CssInJs)>),
}

impl CssInJs {
    /// `{"o": [[k, v]…]}` / `{"a": […]}` / `"str"` (see `worker/gen-plugins.ts`).
    pub fn from_json(v: &serde_json::Value) -> CssInJs {
        match v {
            serde_json::Value::String(s) => CssInJs::Str(s.clone()),
            serde_json::Value::Object(o) if o.contains_key("o") => CssInJs::Obj(entries_from_json(&o["o"])),
            serde_json::Value::Object(o) if o.contains_key("a") => {
                CssInJs::Arr(o["a"].as_array().map(|a| a.iter().map(CssInJs::from_json).collect()).unwrap_or_default())
            }
            other => CssInJs::Str(other.to_string()),
        }
    }

    pub fn map_strings(&self, f: &impl Fn(&str) -> String) -> CssInJs {
        match self {
            CssInJs::Str(s) => CssInJs::Str(f(s)),
            CssInJs::Arr(a) => CssInJs::Arr(a.iter().map(|v| v.map_strings(f)).collect()),
            CssInJs::Obj(o) => CssInJs::Obj(o.iter().map(|(k, v)| (f(k), v.map_strings(f))).collect()),
        }
    }
}

pub fn entries_from_json(v: &serde_json::Value) -> Vec<(String, CssInJs)> {
    v.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|pair| {
                    let pair = pair.as_array()?;
                    Some((pair.first()?.as_str()?.to_string(), CssInJs::from_json(pair.get(1)?)))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Shorthand for building CSS-in-JS in the native ports.
pub fn obj(entries: Vec<(&str, CssInJs)>) -> CssInJs {
    CssInJs::Obj(entries.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
}

pub fn s(value: impl Into<String>) -> CssInJs {
    CssInJs::Str(value.into())
}

/// `objectToAst` (camelCase properties become kebab-case, arrays repeat the
/// declaration, objects become nested rules).
pub fn object_to_ast(entries: &[(String, CssInJs)]) -> Vec<AstNode> {
    let mut ast = vec![];
    for (name, value) in entries {
        match value {
            CssInJs::Str(v) => {
                if name.starts_with("--") {
                    ast.push(decl(name.as_str(), v.as_str()));
                } else if v == "@slot" {
                    ast.push(rule(name.as_str(), vec![at_rule("@slot", "", vec![])]));
                } else {
                    ast.push(decl(kebab(name), v.as_str()));
                }
            }
            CssInJs::Arr(items) => {
                for item in items {
                    match item {
                        CssInJs::Str(v) => ast.push(decl(name.as_str(), v.as_str())),
                        CssInJs::Obj(o) => ast.push(rule(name.as_str(), object_to_ast(o))),
                        CssInJs::Arr(_) => ast.push(rule(name.as_str(), vec![])),
                    }
                }
            }
            CssInJs::Obj(o) => ast.push(rule(name.as_str(), object_to_ast(o))),
        }
    }
    ast
}

fn kebab(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    for c in name.chars() {
        if c.is_ascii_uppercase() {
            out.push('-');
        }
        out.push(c.to_ascii_lowercase());
    }
    out
}

/// A primitive `@plugin` option (`compat/apply-compat-hooks.ts`).
#[derive(Clone, Debug, PartialEq)]
pub enum OptionValue {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    List(Vec<OptionValue>),
}

impl OptionValue {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            OptionValue::Str(s) => Some(s),
            _ => None,
        }
    }
}

/// `@plugin "…" { key: value; }` options, parsed like upstream.
pub fn parse_options(nodes: &[AstNode]) -> Result<IndexMap<String, OptionValue>, String> {
    let mut options = IndexMap::new();
    for node in nodes {
        let AstNode::Decl { property, value, .. } = node else {
            return Err(format!(
                "Unexpected `@plugin` option:\n\n{}\n\n`@plugin` options must be a flat list of declarations.",
                crate::tw::ast::to_css(std::slice::from_ref(node))
            ));
        };
        let Some(value) = value else { continue };
        let parts: Vec<OptionValue> = segment(value, b',')
            .iter()
            .map(|p| {
                let p = p.trim();
                let b = p.as_bytes();
                if p == "null" {
                    Ok(OptionValue::Null)
                } else if p == "true" {
                    Ok(OptionValue::Bool(true))
                } else if p == "false" {
                    Ok(OptionValue::Bool(false))
                } else if let Some(n) = js_number(p) {
                    Ok(OptionValue::Num(n))
                } else if b.len() >= 2 && (b[0] == b'"' || b[0] == b'\'') && b[b.len() - 1] == b[0] {
                    Ok(OptionValue::Str(p[1..p.len() - 1].to_string()))
                } else if b.len() >= 2 && b[0] == b'{' && b[b.len() - 1] == b'}' {
                    Err(format!("Unexpected `@plugin` option: Value of declaration `{property}: {value};` is not supported.\n\nUsing an object as a plugin option is currently only supported in JavaScript configuration files."))
                } else {
                    Ok(OptionValue::Str(p.to_string()))
                }
            })
            .collect::<Result<_, _>>()?;
        options.insert(property.clone(), if parts.len() == 1 { parts.into_iter().next().unwrap() } else { OptionValue::List(parts) });
    }
    Ok(options)
}

/// `!Number.isNaN(Number(part))` for the inputs plugin options can have.
fn js_number(s: &str) -> Option<f64> {
    if s.is_empty() {
        return Some(0.0);
    }
    if s.chars().all(|c| c.is_ascii_digit() || matches!(c, '.' | '-' | '+' | 'e' | 'E')) {
        return s.parse::<f64>().ok();
    }
    None
}

/// One `@plugin` occurrence.
pub struct PluginRef {
    pub id: String,
    pub options: Option<IndexMap<String, OptionValue>>,
    pub reference: bool,
}

/// The plugin API given to native plugins (`buildPluginApi`).
pub struct PluginApi<'a> {
    pub ds: &'a mut DesignSystem,
    pub ast: &'a mut Vec<AstNode>,
    pub reference: bool,
}

impl PluginApi<'_> {
    /// `addBase`: a `@layer base` block at the end of the stylesheet.
    pub fn add_base(&mut self, css: &[(String, CssInJs)]) -> Result<(), String> {
        if self.reference {
            return Ok(());
        }
        let mut nodes = object_to_ast(css);
        substitute_functions(&mut nodes, self.ds)?;
        self.ast.push(at_rule("@layer", "base", nodes));
        Ok(())
    }

    /// `addVariant(name, selector | selectors)`.
    pub fn add_variant(&mut self, name: &str, selectors: &[String]) {
        if selectors.iter().any(|s| s.contains(":merge(")) {
            return;
        }
        let compounds = compounds_for_selectors(&selectors.iter().map(String::as_str).collect::<Vec<_>>());
        let selectors: Rc<Vec<String>> = Rc::new(selectors.to_vec());
        self.ds.variants.static_(
            name,
            Rc::new(move |r, _, _| {
                let nodes = std::mem::take(r.nodes_mut().unwrap());
                *r.nodes_mut().unwrap() = parse_variant_value(&selectors, nodes);
                true
            }),
            Some(compounds),
        );
    }

    /// `addUtilities` / `addComponents`: one static utility per class.
    pub fn add_utilities(&mut self, entries: &[(String, CssInJs)]) -> Result<(), String> {
        let mut utils: IndexMap<String, Vec<AstNode>> = IndexMap::new();

        for (name, css) in entries {
            for selector in segment(name, b',') {
                let selector = selector.trim();
                let CssInJs::Obj(css) = css else { continue };
                if selector.starts_with("@keyframes ") {
                    if !self.reference {
                        self.ast.push(rule(selector, object_to_ast(css)));
                    }
                    continue;
                }

                let mut selector_ast = selector_parser::parse(selector);
                let mut found = false;
                let mut classes: Vec<(String, String)> = vec![];
                collect_utility_classes(&mut selector_ast, &mut |ast, class| {
                    // Swap the class for `&` to get the rule's selector
                    let selector = selector_parser::to_css(ast);
                    classes.push((class.to_string(), selector));
                });
                for (class, selector) in classes {
                    let contents =
                        if selector == "&" { object_to_ast(css) } else { vec![rule(selector, object_to_ast(css))] };
                    utils.entry(class).or_default().extend(contents);
                    found = true;
                }
                if !found {
                    return Err(format!("`addUtilities({{ '{selector}' : … }})` defines an invalid utility selector. Utilities must be a single class name and start with a lowercase letter, eg. `.scrollbar-none`."));
                }
            }
        }

        for (class_name, mut ast) in utils {
            // Prefix all class selectors with the configured theme prefix
            if let Some(prefix) = self.ds.theme.prefix.clone() {
                walk(&mut ast, &mut |node, _| {
                    if let AstNode::Rule { selector, .. } = node {
                        let mut sel = selector_parser::parse(selector);
                        map_class_selectors(&mut sel, &mut |value| format!(".{prefix}\\:{}", &value[1..]));
                        *selector = selector_parser::to_css(&sel);
                    }
                    WalkAction::Continue
                });
            }

            let ast = Rc::new(ast);
            let name = class_name.clone();
            self.ds.utilities.static_(&class_name, move |candidate, _| {
                let mut cloned: Vec<AstNode> = (*ast).clone();
                replace_nested_class_name_references(&mut cloned, &name, &candidate.raw);
                Some(cloned)
            });
        }
        Ok(())
    }
}

const fn is_valid_utility_byte(b: u8, first: bool) -> bool {
    if first {
        b.is_ascii_lowercase() || b == b'@'
    } else {
        b.is_ascii_alphanumeric() || matches!(b, b'/' | b'%' | b'.' | b'_' | b'-')
    }
}

/// `IS_VALID_UTILITY_NAME = /^[a-z@][a-zA-Z0-9/%._-]*$/`
fn is_valid_utility_name(name: &str) -> bool {
    let b = name.as_bytes();
    !b.is_empty() && b.iter().enumerate().all(|(i, c)| is_valid_utility_byte(*c, i == 0))
}

/// Walk class selectors like `addUtilities` (not inside `:not()` /
/// `:nth-child()` / `:nth-last-child()`), calling `f` with the selector where
/// that class is replaced by `&`.
fn collect_utility_classes(ast: &mut Vec<SNode>, f: &mut impl FnMut(&[SNode], &str)) {
    // Paths to the class selectors, then replace one at a time.
    let mut paths: Vec<Vec<usize>> = vec![];
    fn find(nodes: &[SNode], path: &mut Vec<usize>, out: &mut Vec<Vec<usize>>) {
        for (i, n) in nodes.iter().enumerate() {
            path.push(i);
            match n {
                SNode::Selector(v) if v.starts_with('.') && is_valid_utility_name(&v[1..]) => out.push(path.clone()),
                SNode::Function(name, _) if matches!(name.as_str(), ":not" | ":nth-child" | ":nth-last-child") => {}
                SNode::Function(_, children) | SNode::List(children) | SNode::Complex(children) | SNode::Compound(children) => {
                    find(children, path, out)
                }
                _ => {}
            }
            path.pop();
        }
    }
    find(ast, &mut vec![], &mut paths);

    fn at<'a>(nodes: &'a mut [SNode], path: &[usize]) -> &'a mut SNode {
        let node = &mut nodes[path[0]];
        if path.len() == 1 {
            return node;
        }
        match node {
            SNode::Function(_, c) | SNode::List(c) | SNode::Complex(c) | SNode::Compound(c) => at(c, &path[1..]),
            _ => unreachable!(),
        }
    }

    for path in paths {
        let node = at(ast, &path);
        let SNode::Selector(value) = node else { continue };
        let original = std::mem::replace(value, "&".to_string());
        f(ast, &original[1..]);
        if let SNode::Selector(v) = at(ast, &path) {
            *v = original;
        }
    }
}

fn map_class_selectors(nodes: &mut [SNode], f: &mut impl FnMut(&str) -> String) {
    for n in nodes {
        match n {
            SNode::Selector(v) if v.starts_with('.') => *v = f(v),
            SNode::Function(_, c) | SNode::List(c) | SNode::Complex(c) | SNode::Compound(c) => map_class_selectors(c, f),
            _ => {}
        }
    }
}

/// Nested rules using the utility's class get the candidate's class instead.
/// Like upstream, every rule selector is re-serialized (normalizing it).
fn replace_nested_class_name_references(ast: &mut Vec<AstNode>, utility_name: &str, raw: &str) {
    let target = format!(".{utility_name}");
    let replacement = format!(".{}", escape(raw));
    walk(ast, &mut |node, _| {
        if let AstNode::Rule { selector, .. } = node {
            let mut sel = selector_parser::parse(selector);
            map_class_selectors(&mut sel, &mut |v| if v == target { replacement.clone() } else { v.to_string() });
            *selector = selector_parser::to_css(&sel);
        }
        WalkAction::Continue
    });
}

/// `parseVariantValue`: a selector wraps the nodes; a CSS block gets them in
/// its `@slot`.
fn parse_variant_value(selectors: &[String], nodes: Vec<AstNode>) -> Vec<AstNode> {
    selectors
        .iter()
        .flat_map(|r| {
            if r.trim_end().ends_with('}') {
                let updated = r.replacen('}', "{@slot}}", 1);
                let mut ast = css_parser::parse(&updated).unwrap_or_default();
                substitute_at_slot(&mut ast, &nodes);
                ast
            } else {
                vec![rule(r.as_str(), nodes.clone())]
            }
        })
        .collect()
}

/// Run the `@plugin`s in order (upstream: `applyCompatibilityHooks`).
pub fn run_plugins(plugins: &[PluginRef], ds: &mut DesignSystem, ast: &mut Vec<AstNode>) -> Result<(), String> {
    for plugin in plugins {
        let mut api = PluginApi { ds, ast, reference: plugin.reference };
        let options = plugin.options.as_ref();
        match plugin.id.as_str() {
            "@tailwindcss/typography" => typography::register(&mut api, options)?,
            "@tailwindcss/forms" => forms::register(&mut api, options)?,
            other => tracing::warn!("tailwind: `@plugin \"{other}\"` is a JavaScript plugin; skipped by the native compiler"),
        }
    }
    Ok(())
}
