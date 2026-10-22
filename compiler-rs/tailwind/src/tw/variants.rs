//! Port of `tailwindcss/src/variants.ts`.

use crate::tw::ast::{at_root, at_rule, decl, rule, style_rule, walk, AstNode, WalkAction};
use crate::tw::candidate::{Variant, VariantValue};
use crate::tw::design_system::DesignSystem;
use crate::tw::utils::{compare_breakpoints, is_positive_integer, segment};
use indexmap::IndexMap;
use regex::Regex;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::LazyLock;

pub static IS_VALID_VARIANT_NAME: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^@?[a-z0-9][a-zA-Z0-9_-]*$").unwrap());

pub fn is_valid_variant_name(name: &str) -> bool {
    IS_VALID_VARIANT_NAME.is_match(name) && !name.ends_with('_') && !name.ends_with('-')
}

pub mod compounds {
    pub const NEVER: u8 = 0;
    pub const AT_RULES: u8 = 1 << 0;
    pub const STYLE_RULES: u8 = 1 << 1;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VariantKind {
    Static,
    Functional,
    Compound,
}

/// Returns `false` when the variant cannot be applied (upstream returns `null`).
pub type VariantFn = Rc<dyn Fn(&mut AstNode, &Variant, &DesignSystem) -> bool>;
pub type CompareFn = Rc<dyn Fn(&Variant, &Variant, &DesignSystem) -> Ordering>;

#[derive(Clone)]
pub struct VariantDef {
    pub kind: VariantKind,
    pub order: u32,
    pub apply: VariantFn,
    pub compounds_with: u8,
    pub compounds: u8,
}

#[derive(Clone, Default)]
pub struct Variants {
    pub compare_fns: HashMap<u32, CompareFn>,
    pub variants: IndexMap<String, VariantDef>,
    group_order: Option<u32>,
    last_order: u32,
}

impl Variants {
    pub fn static_(&mut self, name: &str, apply: VariantFn, compounds: Option<u8>) {
        self.set(name, VariantKind::Static, apply, compounds::NEVER, compounds.unwrap_or(compounds::STYLE_RULES));
    }

    pub fn functional(&mut self, name: &str, apply: VariantFn, compounds: Option<u8>) {
        self.set(name, VariantKind::Functional, apply, compounds::NEVER, compounds.unwrap_or(compounds::STYLE_RULES));
    }

    pub fn compound(&mut self, name: &str, compounds_with: u8, apply: VariantFn) {
        self.set(name, VariantKind::Compound, apply, compounds_with, compounds::STYLE_RULES);
    }

    pub fn from_ast(&mut self, name: &str, ast: Vec<AstNode>) {
        let mut selectors: Vec<String> = vec![];
        let mut uses_at_variant = false;
        let mut tmp = ast.clone();
        walk(&mut tmp, &mut |node, _| {
            match node {
                AstNode::Rule { selector, .. } => selectors.push(selector.clone()),
                AstNode::AtRule { name, .. } if name == "@variant" => uses_at_variant = true,
                AstNode::AtRule { name, params, .. } if name != "@slot" => selectors.push(format!("{} {}", name, params)),
                _ => {}
            }
            WalkAction::Continue
        });

        let compounds = compounds_for_selectors(&selectors.iter().map(|s| s.as_str()).collect::<Vec<_>>());
        let ast = Rc::new(ast);
        self.static_(
            name,
            Rc::new(move |r, _, ds| {
                let mut body: Vec<AstNode> = (*ast).clone();
                if uses_at_variant {
                    let _ = substitute_at_variant(&mut body, ds);
                }
                let nodes = std::mem::take(r.nodes_mut().unwrap());
                substitute_at_slot(&mut body, &nodes);
                *r.nodes_mut().unwrap() = body;
                true
            }),
            Some(compounds),
        );
    }

    pub fn group(&mut self, f: impl FnOnce(&mut Variants), compare: Option<CompareFn>) {
        let order = self.next_order();
        self.group_order = Some(order);
        if let Some(c) = compare {
            self.compare_fns.insert(order, c);
        }
        f(self);
        self.group_order = None;
    }

    pub fn has(&self, name: &str) -> bool {
        self.variants.contains_key(name)
    }

    pub fn get(&self, name: &str) -> Option<&VariantDef> {
        self.variants.get(name)
    }

    pub fn kind(&self, name: &str) -> Option<VariantKind> {
        self.variants.get(name).map(|v| v.kind)
    }

    pub fn compounds_with(&self, parent: &str, child: &Variant) -> bool {
        let Some(parent_info) = self.variants.get(parent) else { return false };
        let child_compounds = match child {
            Variant::Arbitrary { selector, .. } => compounds_for_selectors(&[selector.as_str()]),
            other => match self.variants.get(other.root().unwrap()) {
                Some(info) => info.compounds,
                None => return false,
            },
        };
        if parent_info.kind != VariantKind::Compound {
            return false;
        }
        if child_compounds == compounds::NEVER {
            return false;
        }
        if parent_info.compounds_with == compounds::NEVER {
            return false;
        }
        if parent_info.compounds_with & child_compounds == 0 {
            return false;
        }
        true
    }

    pub fn compounds_with_name(&self, parent: &str, child: &str) -> bool {
        let (Some(parent_info), Some(child_info)) = (self.variants.get(parent), self.variants.get(child)) else {
            return false;
        };
        parent_info.kind == VariantKind::Compound
            && child_info.compounds != compounds::NEVER
            && parent_info.compounds_with != compounds::NEVER
            && parent_info.compounds_with & child_info.compounds != 0
    }

    pub fn compare(&self, a: &Variant, z: &Variant, ds: &DesignSystem) -> Ordering {
        if std::ptr::eq(a, z) || a == z {
            return Ordering::Equal;
        }

        match (a, z) {
            (Variant::Arbitrary { selector: sa, .. }, Variant::Arbitrary { selector: sz, .. }) => {
                return if sa < sz { Ordering::Less } else { Ordering::Greater };
            }
            (Variant::Arbitrary { .. }, _) => return Ordering::Greater,
            (_, Variant::Arbitrary { .. }) => return Ordering::Less,
            _ => {}
        }

        let a_order = self.variants.get(a.root().unwrap()).map(|v| v.order).unwrap_or(0);
        let z_order = self.variants.get(z.root().unwrap()).map(|v| v.order).unwrap_or(0);

        if a_order != z_order {
            return a_order.cmp(&z_order);
        }

        if let (
            Variant::Compound { variant: av, modifier: am, .. },
            Variant::Compound { variant: zv, modifier: zm, .. },
        ) = (a, z)
        {
            let order = self.compare(av, zv, ds);
            if order != Ordering::Equal {
                return order;
            }
            return match (am, zm) {
                (Some(am), Some(zm)) => {
                    if am.value() < zm.value() {
                        Ordering::Less
                    } else {
                        Ordering::Greater
                    }
                }
                (Some(_), None) => Ordering::Greater,
                (None, Some(_)) => Ordering::Less,
                (None, None) => Ordering::Equal,
            };
        }

        if let Some(f) = self.compare_fns.get(&a_order) {
            return f(a, z, ds);
        }

        let (ar, zr) = (a.root().unwrap(), z.root().unwrap());
        if ar != zr {
            return if ar < zr { Ordering::Less } else { Ordering::Greater };
        }

        let a_value = match a {
            Variant::Functional { value, .. } => value.as_ref(),
            _ => None,
        };
        let z_value = match z {
            Variant::Functional { value, .. } => value.as_ref(),
            _ => None,
        };

        let Some(av) = a_value else { return Ordering::Less };
        let Some(zv) = z_value else { return Ordering::Greater };

        match (av, zv) {
            (VariantValue::Arbitrary(_), VariantValue::Named(_)) => return Ordering::Greater,
            (VariantValue::Named(_), VariantValue::Arbitrary(_)) => return Ordering::Less,
            _ => {}
        }

        if av.value() < zv.value() {
            Ordering::Less
        } else {
            Ordering::Greater
        }
    }

    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.variants.keys()
    }

    fn set(&mut self, name: &str, kind: VariantKind, apply: VariantFn, compounds_with: u8, compounds: u8) {
        if let Some(existing) = self.variants.get_mut(name) {
            existing.kind = kind;
            existing.apply = apply;
            existing.compounds = compounds;
        } else {
            self.last_order = self.next_order();
            let order = self.last_order;
            self.variants.insert(name.to_string(), VariantDef { kind, order, apply, compounds_with, compounds });
        }
    }

    fn next_order(&self) -> u32 {
        self.group_order.unwrap_or(self.last_order + 1)
    }
}

pub fn compounds_for_selectors(selectors: &[&str]) -> u8 {
    let mut c = compounds::NEVER;
    for sel in selectors {
        if sel.starts_with('@') {
            if !sel.starts_with("@media") && !sel.starts_with("@supports") && !sel.starts_with("@container") {
                return compounds::NEVER;
            }
            c |= compounds::AT_RULES;
            continue;
        }
        if sel.contains("::") {
            return compounds::NEVER;
        }
        c |= compounds::STYLE_RULES;
    }
    c
}

fn static_variant(variants: &mut Variants, name: &str, selectors: &[&str], compounds: Option<u8>) {
    let compounds = compounds.unwrap_or_else(|| compounds_for_selectors(selectors));
    let selectors: Vec<String> = selectors.iter().map(|s| s.to_string()).collect();
    variants.static_(
        name,
        Rc::new(move |r, _, _| {
            let nodes = std::mem::take(r.nodes_mut().unwrap());
            *r.nodes_mut().unwrap() = selectors.iter().map(|s| rule(s.as_str(), nodes.clone())).collect();
            true
        }),
        Some(compounds),
    );
}

fn negate_conditions(rule_name: &str, conditions: Vec<&str>) -> Vec<String> {
    conditions
        .into_iter()
        .map(|condition| {
            if rule_name == "@container" {
                use crate::tw::value_parser::{self as vp, ValueNode};
                let mut ast = vp::parse(condition.trim());
                let is_fn = |n: Option<&ValueNode>| matches!(n, Some(ValueNode::Function { .. }));
                let word = |n: Option<&ValueNode>| match n {
                    Some(ValueNode::Word(w)) => Some(w.clone()),
                    _ => None,
                };

                // @container {query}
                if is_fn(ast.first()) {
                    return format!("not {}", condition);
                }
                // @container not {query}
                if ast.len() >= 3 && word(ast.first()).as_deref() == Some("not") && is_fn(ast.get(2)) {
                    ast.drain(0..2);
                    return vp::to_css(&ast);
                }
                // @container {name} not {query}
                if ast.len() >= 5
                    && word(ast.first()).is_some()
                    && word(ast.get(2)).as_deref() == Some("not")
                    && is_fn(ast.get(4))
                {
                    ast.drain(2..4);
                    return vp::to_css(&ast);
                }
                // @container {name} {query}
                if ast.len() >= 3 && word(ast.first()).is_some_and(|w| w != "not") && is_fn(ast.get(2)) {
                    ast.splice(1..1, [ValueNode::Separator(" ".into()), ValueNode::Word("not".into())]);
                    return vp::to_css(&ast);
                }
                return format!("not {}", condition);
            }

            let condition = condition.trim();
            let parts = segment(condition, b' ');
            if parts[0] == "not" {
                return parts[1..].join(" ");
            }
            format!("not {}", condition)
        })
        .collect()
}

fn negate_at_rule(name: &str, params: &str) -> Option<AstNode> {
    for rule_name in ["@media", "@supports", "@container"] {
        if rule_name != name {
            continue;
        }
        let conditions = segment(params, b',');
        if conditions.len() > 1 {
            return None;
        }
        let conditions = negate_conditions(name, conditions);
        return Some(at_rule(name, conditions.join(", "), vec![]));
    }
    None
}

fn negate_selector(selector: &str) -> Option<String> {
    if selector.contains("::") {
        return None;
    }
    let selectors: Vec<String> = segment(selector, b',').iter().map(|s| s.replace('&', "*")).collect();
    Some(format!("&:not({})", selectors.join(", ")))
}

/// Snapshot of a node on a path (for the `not` variant).
#[derive(Clone)]
enum PathItem {
    Rule(String),
    AtRule(String, String),
    Other,
}

fn collect_leaf_paths(node: &AstNode, path: &mut Vec<PathItem>, out: &mut Vec<Vec<PathItem>>) {
    let item = match node {
        AstNode::Rule { selector, .. } => PathItem::Rule(selector.clone()),
        AstNode::AtRule { name, params, .. } => PathItem::AtRule(name.clone(), params.clone()),
        _ => PathItem::Other,
    };
    match node {
        AstNode::Rule { nodes, .. } | AstNode::AtRule { nodes, .. } => {
            path.push(item);
            if nodes.is_empty() {
                out.push(path.clone());
            } else {
                for child in nodes {
                    collect_leaf_paths(child, path, out);
                }
            }
            path.pop();
        }
        AstNode::Context { nodes, .. } | AstNode::AtRoot(nodes) => {
            path.push(item);
            for child in nodes {
                collect_leaf_paths(child, path, out);
            }
            path.pop();
        }
        _ => {}
    }
}

/// Rewrites selectors of style rules, failing when style rules are nested.
fn rewrite_style_rules(node: &mut AstNode, in_rule: bool, did_apply: &mut bool, f: &dyn Fn(&str) -> String) -> bool {
    match node {
        AstNode::Rule { selector, nodes } => {
            if in_rule {
                *did_apply = false;
                return false;
            }
            *selector = f(selector);
            *did_apply = true;
            for child in nodes.iter_mut() {
                if !rewrite_style_rules(child, true, did_apply, f) {
                    return false;
                }
            }
            true
        }
        AstNode::AtRule { nodes, .. } | AstNode::Context { nodes, .. } | AstNode::AtRoot(nodes) => {
            for child in nodes.iter_mut() {
                if !rewrite_style_rules(child, in_rule, did_apply, f) {
                    return false;
                }
            }
            true
        }
        _ => true,
    }
}

fn quote_attribute_value(input: &str) -> String {
    if input.contains('=') {
        let parts = segment(input, b'=');
        let attribute = parts[0];
        let value = parts[1..].join("=");
        let value = value.trim();
        if value.starts_with('\'') || value.starts_with('"') {
            return input.to_string();
        }
        let vb = value.as_bytes();
        if vb.len() > 1 {
            let trailing = vb[vb.len() - 1];
            if vb[vb.len() - 2] == b' ' && matches!(trailing, b'i' | b'I' | b's' | b'S') {
                return format!("{}=\"{}\" {}", attribute, &value[..value.len() - 2], trailing as char);
            }
        }
        return format!("{}=\"{}\"", attribute, value);
    }
    input.to_string()
}

fn wrap(r: &mut AstNode, f: impl FnOnce(Vec<AstNode>) -> AstNode) {
    let nodes = std::mem::take(r.nodes_mut().unwrap());
    *r.nodes_mut().unwrap() = vec![f(nodes)];
}

fn resolved_breakpoint(variant: &Variant, ds: &DesignSystem) -> Option<String> {
    match variant {
        Variant::Static { root } => ds.theme.resolve_value(Some(root), &["--breakpoint"]),
        Variant::Functional { value, modifier, .. } => {
            let value = value.as_ref()?;
            if modifier.is_some() {
                return None;
            }
            let v = match value {
                VariantValue::Arbitrary(v) => Some(v.clone()),
                VariantValue::Named(v) => ds.theme.resolve_value(Some(v), &["--breakpoint"]),
            }?;
            if v.is_empty() || v.contains("var(") {
                return None;
            }
            Some(v)
        }
        _ => None,
    }
}

fn resolved_width(variant: &Variant, ds: &DesignSystem) -> Option<String> {
    match variant {
        Variant::Functional { value, .. } => {
            let value = value.as_ref()?;
            let v = match value {
                VariantValue::Arbitrary(v) => Some(v.clone()),
                VariantValue::Named(v) => ds.theme.resolve_value(Some(v), &["--container"]),
            }?;
            if v.is_empty() || v.contains("var(") {
                return None;
            }
            Some(v)
        }
        _ => None,
    }
}

fn compare_breakpoint_variants(
    a: &Variant,
    z: &Variant,
    asc: bool,
    lookup: &dyn Fn(&Variant) -> Option<String>,
) -> Ordering {
    if a == z {
        return Ordering::Equal;
    }
    let Some(av) = lookup(a) else {
        return if asc { Ordering::Less } else { Ordering::Greater };
    };
    let Some(zv) = lookup(z) else {
        return if asc { Ordering::Greater } else { Ordering::Less };
    };
    compare_breakpoints(&av, &zv, asc)
}

pub fn create_variants(theme: &crate::tw::theme::Theme) -> Variants {
    let mut v = Variants::default();
    use compounds::*;

    static_variant(&mut v, "*", &[":is(& > *)"], Some(NEVER));
    static_variant(&mut v, "**", &[":is(& *)"], Some(NEVER));

    v.compound(
        "not",
        STYLE_RULES | AT_RULES,
        Rc::new(|rule_node, variant, _| {
            let Variant::Compound { variant: sub, modifier, .. } = variant else { return false };
            if let Variant::Arbitrary { relative: true, .. } = sub.as_ref() {
                return false;
            }
            if modifier.is_some() {
                return false;
            }

            let mut leaves = vec![];
            collect_leaf_paths(rule_node, &mut vec![], &mut leaves);

            let mut did_apply = false;
            let mut root_override: Option<AstNode> = None;

            for mut path in leaves {
                if root_override.is_some() {
                    path[0] = PathItem::Rule("&".into());
                }
                let mut at_rules = vec![];
                let mut style_rules = vec![];
                for item in &path {
                    match item {
                        PathItem::Rule(s) => style_rules.push(s.clone()),
                        PathItem::AtRule(n, p) => at_rules.push((n.clone(), p.clone())),
                        PathItem::Other => {}
                    }
                }
                if at_rules.len() > 1 || style_rules.len() > 1 {
                    break;
                }
                let mut rules = vec![];
                let mut failed = false;
                for s in &style_rules {
                    match negate_selector(s) {
                        Some(sel) => rules.push(style_rule(sel, vec![])),
                        None => {
                            failed = true;
                            break;
                        }
                    }
                }
                if !failed {
                    for (n, p) in &at_rules {
                        match negate_at_rule(n, p) {
                            Some(r) => rules.push(r),
                            None => {
                                failed = true;
                                break;
                            }
                        }
                    }
                }
                if failed {
                    did_apply = false;
                    break;
                }
                root_override = Some(style_rule("&", rules));
                did_apply = true;
            }

            if let Some(new_node) = root_override {
                *rule_node = new_node;
            }

            if let AstNode::Rule { selector, nodes } = rule_node {
                if selector == "&" && nodes.len() == 1 {
                    let only = nodes.remove(0);
                    *rule_node = only;
                }
            }

            did_apply
        }),
    );

    for (name, combinator) in [("group", " *"), ("peer", " ~ *")] {
        let combinator = combinator.to_string();
        let name_owned = name.to_string();
        v.compound(
            name,
            STYLE_RULES,
            Rc::new(move |rule_node, variant, ds| {
                let Variant::Compound { variant: sub, modifier, .. } = variant else { return false };
                if let Variant::Arbitrary { relative: true, .. } = sub.as_ref() {
                    return false;
                }
                let prefix = match &ds.theme.prefix {
                    Some(p) => format!("{}\\:", p),
                    None => String::new(),
                };
                let variant_selector = match modifier {
                    Some(m) => format!(":where(.{}{}\\/{})", prefix, name_owned, m.value()),
                    None => format!(":where(.{}{})", prefix, name_owned),
                };
                let mut did_apply = false;
                let comb = combinator.clone();
                rewrite_style_rules(rule_node, false, &mut did_apply, &|sel| {
                    let mut selector = sel.replace('&', &variant_selector);
                    if segment(&selector, b',').len() > 1 {
                        selector = format!(":is({})", selector);
                    }
                    format!("&:is({}{})", selector, comb)
                });
                did_apply
            }),
        );
    }

    static_variant(&mut v, "first-letter", &["&::first-letter"], None);
    static_variant(&mut v, "first-line", &["&::first-line"], None);
    static_variant(
        &mut v,
        "marker",
        &["& *::marker", "&::marker", "& *::-webkit-details-marker", "&::-webkit-details-marker"],
        None,
    );
    static_variant(&mut v, "selection", &["& *::selection", "&::selection"], None);
    static_variant(&mut v, "file", &["&::file-selector-button"], None);
    static_variant(&mut v, "placeholder", &["&::placeholder"], None);
    static_variant(&mut v, "backdrop", &["&::backdrop"], None);
    static_variant(&mut v, "details-content", &["&::details-content"], None);

    fn content_properties() -> AstNode {
        at_root(vec![at_rule(
            "@property",
            "--tw-content",
            vec![decl("syntax", "\"*\""), decl("initial-value", "\"\""), decl("inherits", "false")],
        )])
    }

    for (name, sel) in [("before", "&::before"), ("after", "&::after")] {
        v.static_(
            name,
            Rc::new(move |r, _, _| {
                wrap(r, |nodes| {
                    let mut children = vec![content_properties(), decl("content", "var(--tw-content)")];
                    children.extend(nodes);
                    style_rule(sel, children)
                });
                true
            }),
            Some(NEVER),
        );
    }

    for (name, sel) in [
        ("first", "&:first-child"),
        ("last", "&:last-child"),
        ("only", "&:only-child"),
        ("odd", "&:nth-child(odd)"),
        ("even", "&:nth-child(even)"),
        ("first-of-type", "&:first-of-type"),
        ("last-of-type", "&:last-of-type"),
        ("only-of-type", "&:only-of-type"),
        ("visited", "&:visited"),
        ("target", "&:target"),
        ("open", "&:is([open], :popover-open, :open)"),
        ("default", "&:default"),
        ("checked", "&:checked"),
        ("indeterminate", "&:indeterminate"),
        ("placeholder-shown", "&:placeholder-shown"),
        ("autofill", "&:autofill"),
        ("optional", "&:optional"),
        ("required", "&:required"),
        ("valid", "&:valid"),
        ("invalid", "&:invalid"),
        ("user-valid", "&:user-valid"),
        ("user-invalid", "&:user-invalid"),
        ("in-range", "&:in-range"),
        ("out-of-range", "&:out-of-range"),
        ("read-only", "&:read-only"),
        ("empty", "&:empty"),
        ("focus-within", "&:focus-within"),
    ] {
        static_variant(&mut v, name, &[sel], None);
    }

    v.static_(
        "hover",
        Rc::new(|r, _, _| {
            wrap(r, |nodes| style_rule("&:hover", vec![at_rule("@media", "(hover: hover)", nodes)]));
            true
        }),
        None,
    );

    for (name, sel) in [
        ("focus", "&:focus"),
        ("focus-visible", "&:focus-visible"),
        ("active", "&:active"),
        ("enabled", "&:enabled"),
        ("disabled", "&:disabled"),
        ("inert", "&:is([inert], [inert] *)"),
    ] {
        static_variant(&mut v, name, &[sel], None);
    }

    v.compound(
        "in",
        STYLE_RULES,
        Rc::new(|rule_node, variant, _| {
            let Variant::Compound { modifier, .. } = variant else { return false };
            if modifier.is_some() {
                return false;
            }
            let mut did_apply = false;
            rewrite_style_rules(rule_node, false, &mut did_apply, &|sel| {
                format!(":where({}) &", sel.replace('&', "*"))
            });
            did_apply
        }),
    );

    v.compound(
        "has",
        STYLE_RULES,
        Rc::new(|rule_node, variant, _| {
            let Variant::Compound { modifier, .. } = variant else { return false };
            if modifier.is_some() {
                return false;
            }
            let mut did_apply = false;
            rewrite_style_rules(rule_node, false, &mut did_apply, &|sel| format!("&:has({})", sel.replace('&', "*")));
            did_apply
        }),
    );

    v.functional(
        "aria",
        Rc::new(|r, variant, _| {
            let Variant::Functional { value: Some(value), modifier: None, .. } = variant else { return false };
            let sel = match value {
                VariantValue::Arbitrary(v) => format!("[aria-{}]", quote_attribute_value(v)),
                VariantValue::Named(v) => format!("[aria-{}=\"true\"]", v),
            };
            if !crate::tw::attribute_selector::is_valid(&sel) {
                return false;
            }
            wrap(r, |nodes| style_rule(format!("&{sel}"), nodes));
            true
        }),
        None,
    );

    v.functional(
        "data",
        Rc::new(|r, variant, _| {
            let Variant::Functional { value: Some(value), modifier: None, .. } = variant else { return false };
            let sel = format!("[data-{}]", quote_attribute_value(value.value()));
            if !crate::tw::attribute_selector::is_valid(&sel) {
                return false;
            }
            wrap(r, |nodes| style_rule(format!("&{sel}"), nodes));
            true
        }),
        None,
    );

    for (name, pseudo) in [
        ("nth", "nth-child"),
        ("nth-last", "nth-last-child"),
        ("nth-of-type", "nth-of-type"),
        ("nth-last-of-type", "nth-last-of-type"),
    ] {
        v.functional(
            name,
            Rc::new(move |r, variant, _| {
                let Variant::Functional { value: Some(value), modifier: None, .. } = variant else { return false };
                if let VariantValue::Named(n) = value {
                    if !is_positive_integer(n) {
                        return false;
                    }
                }
                let sel = format!("&:{}({})", pseudo, value.value());
                wrap(r, |nodes| style_rule(sel, nodes));
                true
            }),
            None,
        );
    }

    v.functional(
        "supports",
        Rc::new(|r, variant, _| {
            let Variant::Functional { value: Some(value), modifier: None, .. } = variant else { return false };
            let mut value = value.value().to_string();

            static FN_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[\w-]*\s*\(").unwrap());
            static KW_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b(and|or|not)\b").unwrap());

            if FN_RE.is_match(&value) {
                let query = KW_RE.replace_all(&value, " $1 ").into_owned();
                wrap(r, |nodes| at_rule("@supports", query, nodes));
                return true;
            }

            if !value.contains(':') {
                value = format!("{}: var(--tw)", value);
            }
            if !(value.starts_with('(') && value.ends_with(')')) {
                value = format!("({})", value);
            }
            wrap(r, |nodes| at_rule("@supports", value, nodes));
            true
        }),
        Some(AT_RULES),
    );

    static_variant(&mut v, "motion-safe", &["@media (prefers-reduced-motion: no-preference)"], None);
    static_variant(&mut v, "motion-reduce", &["@media (prefers-reduced-motion: reduce)"], None);
    static_variant(&mut v, "contrast-more", &["@media (prefers-contrast: more)"], None);
    static_variant(&mut v, "contrast-less", &["@media (prefers-contrast: less)"], None);

    // Breakpoints
    {
        v.group(
            |v| {
                v.functional(
                    "max",
                    Rc::new(|r, variant, ds| {
                        if let Variant::Functional { modifier: Some(_), .. } = variant {
                            return false;
                        }
                        let Some(value) = resolved_breakpoint(variant, ds) else { return false };
                        wrap(r, |nodes| at_rule("@media", format!("(width < {})", value), nodes));
                        true
                    }),
                    Some(AT_RULES),
                );
            },
            Some(Rc::new(|a, z, ds| compare_breakpoint_variants(a, z, false, &|v| resolved_breakpoint(v, ds)))),
        );

        let breakpoints = theme.namespace("--breakpoint");
        v.group(
            |v| {
                for (key, value) in breakpoints {
                    let Some(key) = key else { continue };
                    v.static_(
                        &key,
                        Rc::new(move |r, _, _| {
                            let value = value.clone();
                            wrap(r, |nodes| at_rule("@media", format!("(width >= {})", value), nodes));
                            true
                        }),
                        Some(AT_RULES),
                    );
                }
                v.functional(
                    "min",
                    Rc::new(|r, variant, ds| {
                        if let Variant::Functional { modifier: Some(_), .. } = variant {
                            return false;
                        }
                        let Some(value) = resolved_breakpoint(variant, ds) else { return false };
                        wrap(r, |nodes| at_rule("@media", format!("(width >= {})", value), nodes));
                        true
                    }),
                    Some(AT_RULES),
                );
            },
            Some(Rc::new(|a, z, ds| compare_breakpoint_variants(a, z, true, &|v| resolved_breakpoint(v, ds)))),
        );
    }

    // Container queries
    {
        v.group(
            |v| {
                v.functional(
                    "@max",
                    Rc::new(|r, variant, ds| {
                        let Some(value) = resolved_width(variant, ds) else { return false };
                        let Variant::Functional { modifier, .. } = variant else { return false };
                        let params = match modifier {
                            Some(m) => format!("{} (width < {})", m.value(), value),
                            None => format!("(width < {})", value),
                        };
                        wrap(r, |nodes| at_rule("@container", params, nodes));
                        true
                    }),
                    Some(AT_RULES),
                );
            },
            Some(Rc::new(|a, z, ds| compare_breakpoint_variants(a, z, false, &|v| resolved_width(v, ds)))),
        );

        v.group(
            |v| {
                for name in ["@", "@min"] {
                    v.functional(
                        name,
                        Rc::new(|r, variant, ds| {
                            let Some(value) = resolved_width(variant, ds) else { return false };
                            let Variant::Functional { modifier, .. } = variant else { return false };
                            let params = match modifier {
                                Some(m) => format!("{} (width >= {})", m.value(), value),
                                None => format!("(width >= {})", value),
                            };
                            wrap(r, |nodes| at_rule("@container", params, nodes));
                            true
                        }),
                        Some(AT_RULES),
                    );
                }
            },
            Some(Rc::new(|a, z, ds| compare_breakpoint_variants(a, z, true, &|v| resolved_width(v, ds)))),
        );
    }

    static_variant(&mut v, "portrait", &["@media (orientation: portrait)"], None);
    static_variant(&mut v, "landscape", &["@media (orientation: landscape)"], None);
    static_variant(&mut v, "ltr", &["&:where(:dir(ltr), [dir=\"ltr\"], [dir=\"ltr\"] *)"], None);
    static_variant(&mut v, "rtl", &["&:where(:dir(rtl), [dir=\"rtl\"], [dir=\"rtl\"] *)"], None);
    static_variant(&mut v, "dark", &["@media (prefers-color-scheme: dark)"], None);
    static_variant(&mut v, "starting", &["@starting-style"], None);
    static_variant(&mut v, "print", &["@media print"], None);
    static_variant(&mut v, "forced-colors", &["@media (forced-colors: active)"], None);
    static_variant(&mut v, "inverted-colors", &["@media (inverted-colors: inverted)"], None);
    static_variant(&mut v, "pointer-none", &["@media (pointer: none)"], None);
    static_variant(&mut v, "pointer-coarse", &["@media (pointer: coarse)"], None);
    static_variant(&mut v, "pointer-fine", &["@media (pointer: fine)"], None);
    static_variant(&mut v, "any-pointer-none", &["@media (any-pointer: none)"], None);
    static_variant(&mut v, "any-pointer-coarse", &["@media (any-pointer: coarse)"], None);
    static_variant(&mut v, "any-pointer-fine", &["@media (any-pointer: fine)"], None);
    static_variant(&mut v, "noscript", &["@media (scripting: none)"], None);

    v
}

pub fn substitute_at_slot(ast: &mut Vec<AstNode>, nodes: &[AstNode]) {
    walk(ast, &mut |node, _| match node {
        AstNode::AtRule { name, .. } if name == "@slot" => WalkAction::ReplaceSkip(nodes.to_vec()),
        AstNode::AtRule { name, params, nodes: children } if name == "@keyframes" || name == "@property" => {
            let inner = at_rule(name.clone(), params.clone(), std::mem::take(children));
            *node = at_root(vec![inner]);
            WalkAction::Skip
        }
        _ => WalkAction::Continue,
    });
}

/// Replace `@variant` at-rules. Returns `Err` for unknown variants.
pub fn substitute_at_variant(ast: &mut Vec<AstNode>, ds: &DesignSystem) -> Result<bool, String> {
    let mut found = false;
    let mut error: Option<String> = None;
    walk(ast, &mut |variant_node, _| {
        let AstNode::AtRule { name, params, nodes } = variant_node else { return WalkAction::Continue };
        if name != "@variant" {
            return WalkAction::Continue;
        }
        // `@variant hover, focus` (compound) and `@variant hover:focus` (stacked)
        let body = std::mem::take(nodes);
        let compound_variants: Vec<String> = segment(params, b',').into_iter().map(String::from).collect();
        let mut out = vec![];
        for compound_variant in &compound_variants {
            let mut node = style_rule("&", body.clone());
            let stacked = segment(compound_variant, b':');
            for variant in stacked.iter().rev() {
                let variant = variant.trim();
                if variant.is_empty() {
                    error = Some("Cannot use `@variant` with empty variant".into());
                    return WalkAction::Stop;
                }
                let Some(variant_ast) = ds.parse_variant(variant) else {
                    error = Some(format!("Cannot use `@variant` with unknown variant: {}", variant));
                    return WalkAction::Stop;
                };
                if !crate::tw::compile::apply_variant(&mut node, &variant_ast, ds, 0) {
                    error = Some(format!("Cannot use `@variant` with variant: {}", variant));
                    return WalkAction::Stop;
                }
            }
            match node {
                AstNode::Rule { selector, nodes } if selector == "&" => out.extend(nodes),
                node => out.push(node),
            }
        }
        found = true;
        WalkAction::Replace(out)
    });
    match error {
        Some(e) => Err(e),
        None => Ok(found),
    }
}
