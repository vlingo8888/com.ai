//! Port of `optimizeAst` from `tailwindcss/src/ast.ts`.

use crate::tw::ast::{AstNode, Ctx};
use crate::tw::design_system::DesignSystem;
use crate::tw::theme::{opts, Theme};
use crate::tw::value_parser::{self, ValueNode};
use std::collections::{HashMap, HashSet};

pub mod polyfills {
    pub const NONE: u8 = 0;
    pub const AT_PROPERTY: u8 = 1 << 0;
    pub const COLOR_MIX: u8 = 1 << 1;
    pub const ALL: u8 = AT_PROPERTY | COLOR_MIX;
}

/// Intermediate node with bookkeeping flags.
enum ONode {
    Rule { selector: String, nodes: Vec<ONode> },
    AtRule { name: String, params: String, nodes: Vec<ONode>, theme_keyframes: bool },
    Decl { property: String, value: String, important: bool, theme_var: bool, color_mix: bool },
    Comment(String),
}

struct State<'a> {
    ds: &'a DesignSystem,
    polyfills: u8,
    at_roots: Vec<ONode>,
    seen_at_properties: HashSet<String>,
    used_keyframe_names: HashSet<String>,
    property_fallbacks_root: Vec<(String, String)>,
    property_fallbacks_universal: Vec<(String, String)>,
    variable_dependencies: HashMap<String, Vec<String>>,
}

fn extract_keyframe_names(value: &str) -> Vec<String> {
    value
        .split(|c: char| c.is_whitespace() || c == ',')
        .map(|s| s.to_string())
        .collect()
}

fn transform(node: AstNode, parent: &mut Vec<ONode>, context: Ctx, depth: usize, st: &mut State) {
    match node {
        AstNode::Decl { property, value, important } => {
            if property == "--tw-sort" {
                return;
            }
            let Some(value) = value else { return };

            let mut theme_var = false;
            if context.theme && property.starts_with("--") {
                if value == "initial" {
                    return;
                }
                if !context.keyframes {
                    theme_var = true;
                }
            }

            if value.contains("var(") {
                if context.theme && property.starts_with("--") {
                    for variable in st.ds.used_variables(&value).iter() {
                        st.variable_dependencies.entry(variable.clone()).or_default().push(property.clone());
                    }
                } else {
                    st.ds.track_used_variables(&value);
                }
            }

            if property == "animation" {
                for name in extract_keyframe_names(&value) {
                    st.used_keyframe_names.insert(name);
                }
            }

            let color_mix = st.polyfills & polyfills::COLOR_MIX != 0
                && value.contains("color-mix(")
                && !context.supports_color_mix
                && !context.keyframes;

            parent.push(ONode::Decl { property, value, important, theme_var, color_mix });
        }

        AstNode::Rule { selector, nodes: children } => {
            let mut nodes: Vec<ONode> = Vec::new();
            for child in children {
                transform(child, &mut nodes, context, depth + 1, st);
            }

            if !nodes.is_empty() {
                parent.push(ONode::Rule { selector, nodes });
            }
        }

        AstNode::AtRule { name, params, nodes: children } if name == "@property" && depth == 0 => {
            if st.seen_at_properties.contains(&params) {
                return;
            }

            if st.polyfills & polyfills::AT_PROPERTY != 0 {
                let mut initial_value: Option<String> = None;
                let mut inherits = false;
                for prop in &children {
                    if let AstNode::Decl { property, value, .. } = prop {
                        if property == "initial-value" {
                            initial_value = value.clone();
                        } else if property == "inherits" {
                            inherits = value.as_deref() == Some("true");
                        }
                    }
                }
                let fallback = (params.clone(), initial_value.unwrap_or_else(|| "initial".to_string()));
                if inherits {
                    st.property_fallbacks_root.push(fallback);
                } else {
                    st.property_fallbacks_universal.push(fallback);
                }
            }

            st.seen_at_properties.insert(params.clone());

            let mut nodes = Vec::new();
            for child in children {
                transform(child, &mut nodes, context, depth + 1, st);
            }
            parent.push(ONode::AtRule { name, params, nodes, theme_keyframes: false });
        }

        AstNode::AtRule { name, params, nodes: children } => {
            let mut ctx = context;
            if name == "@keyframes" {
                ctx.keyframes = true;
            } else if name == "@supports" && params.contains("color-mix(") {
                ctx.supports_color_mix = true;
            }

            let mut nodes = Vec::new();
            for child in children {
                transform(child, &mut nodes, ctx, depth + 1, st);
            }

            let theme_keyframes = name == "@keyframes" && ctx.theme;

            if !nodes.is_empty()
                || matches!(name.as_str(), "@layer" | "@charset" | "@custom-media" | "@namespace" | "@import" | "@apply")
            {
                parent.push(ONode::AtRule { name, params, nodes, theme_keyframes });
            }
        }

        AstNode::AtRoot(children) => {
            for child in children {
                let mut new_parent = Vec::new();
                transform(child, &mut new_parent, context, 0, st);
                st.at_roots.extend(new_parent);
            }
        }

        AstNode::Context { ctx, nodes } => {
            if ctx.reference {
                return;
            }
            let merged = context.merge(ctx);
            for child in nodes {
                transform(child, parent, merged, depth, st);
            }
        }

        AstNode::Comment(v) => parent.push(ONode::Comment(v)),
    }
}

fn is_variable_used(
    variable: &str,
    theme: &Theme,
    deps: &HashMap<String, Vec<String>>,
    seen: &mut HashSet<String>,
) -> bool {
    if !seen.insert(variable.to_string()) {
        return true;
    }
    let options = theme.get_options(variable);
    if options & (opts::STATIC | opts::USED) != 0 {
        return true;
    }
    if let Some(dependencies) = deps.get(variable) {
        for d in dependencies {
            if is_variable_used(d, theme, deps, seen) {
                return true;
            }
        }
    }
    false
}

#[derive(PartialEq, Clone, Copy)]
enum Owner {
    Root,
    AtRule,
    Rule,
}

/// Removes unused theme variables. Returns `true` when the owner of `nodes`
/// should be removed from its own parent (upstream's removal chain).
fn prune_theme_vars(nodes: &mut Vec<ONode>, owner: Owner, st: &mut State) -> bool {
    let mut chain = false;

    // Recurse first so that nested structures are handled.
    let mut i = 0;
    while i < nodes.len() {
        let remove = match &mut nodes[i] {
            ONode::Rule { nodes: children, .. } => {
                let emptied = prune_theme_vars(children, Owner::Rule, st);
                emptied && owner != Owner::Rule
            }
            ONode::AtRule { nodes: children, .. } => {
                let signal = prune_theme_vars(children, Owner::AtRule, st);
                signal && owner != Owner::Rule
            }
            _ => false,
        };
        if remove {
            nodes.remove(i);
            chain = true;
        } else {
            i += 1;
        }
    }

    // Remove unused theme variable declarations directly in this list.
    let animate_prefix = st.ds.theme.prefix_key("--animate-");
    let mut seen = HashSet::new();
    let mut removed_any = false;
    let mut keep = Vec::with_capacity(nodes.len());
    for node in nodes.iter() {
        let k = match node {
            ONode::Decl { property, value, theme_var: true, .. } => {
                seen.clear();
                if is_variable_used(property, &st.ds.theme, &st.variable_dependencies, &mut seen) {
                    if property.starts_with(&animate_prefix) {
                        for name in extract_keyframe_names(value) {
                            st.used_keyframe_names.insert(name);
                        }
                    }
                    true
                } else {
                    false
                }
            }
            _ => true,
        };
        removed_any |= !k;
        keep.push(k);
    }
    if removed_any {
        let mut i = 0;
        nodes.retain(|_| {
            let k = keep[i];
            i += 1;
            k
        });
        if nodes.is_empty() {
            // Only a style rule starts a removal chain.
            return owner == Owner::Rule || chain;
        }
    }

    chain
}

fn needs_color_mix_polyfill(value: &str, theme: &Theme) -> Option<String> {
    let mut ast = value_parser::parse(value);
    let mut requires_polyfill = false;
    polyfill_color_mix(&mut ast, theme, &mut requires_polyfill);
    if !requires_polyfill {
        return None;
    }
    Some(value_parser::to_css(&ast))
}

fn polyfill_color_mix(ast: &mut Vec<ValueNode>, theme: &Theme, requires_polyfill: &mut bool) {
    let mut i = 0;
    while i < ast.len() {
        let mut replacement: Option<ValueNode> = None;
        if let ValueNode::Function { value, nodes } = &mut ast[i] {
            if value == "color-mix" {
                let mut contains_unresolvable = false;
                let mut contains_currentcolor = false;
                inline_vars(nodes, theme, requires_polyfill, &mut contains_unresolvable, &mut contains_currentcolor);

                if contains_unresolvable || contains_currentcolor {
                    let sep = nodes.iter().position(|n| matches!(n, ValueNode::Separator(s) if s.trim().contains(',')));
                    if let Some(sep) = sep {
                        if let Some(first_color) = nodes.get(sep + 1) {
                            replacement = Some(first_color.clone());
                        }
                    }
                } else if *requires_polyfill {
                    if let Some(ValueNode::Word(cs)) = nodes.get_mut(2) {
                        if matches!(cs.as_str(), "oklab" | "oklch" | "lab" | "lch") {
                            *cs = "srgb".to_string();
                        }
                    }
                }
            } else {
                polyfill_color_mix(nodes, theme, requires_polyfill);
            }
        }
        if let Some(r) = replacement {
            // Upstream re-visits the replacement (it may be a nested color-mix).
            ast[i] = r;
            continue;
        }
        i += 1;
    }
}

fn inline_vars(
    nodes: &mut Vec<ValueNode>,
    theme: &Theme,
    requires_polyfill: &mut bool,
    unresolvable: &mut bool,
    currentcolor: &mut bool,
) {
    let mut i = 0;
    while i < nodes.len() {
        if let ValueNode::Word(w) = &nodes[i] {
            if w.to_ascii_lowercase() == "currentcolor" {
                *currentcolor = true;
                *requires_polyfill = true;
                i += 1;
                continue;
            }
        }

        let is_var = matches!(&nodes[i], ValueNode::Function { value, .. } if value == "var");
        if is_var {
            let mut var_node = Some(nodes[i].clone());
            let mut inlined: Option<String> = None;
            let mut seen = HashSet::new();
            let mut bail = false;
            while let Some(ValueNode::Function { value, nodes: children }) = &var_node {
                if value != "var" {
                    break;
                }
                let Some(ValueNode::Word(name)) = children.first() else {
                    bail = true;
                    break;
                };
                if !seen.insert(name.clone()) {
                    *unresolvable = true;
                    bail = true;
                    break;
                }
                *requires_polyfill = true;
                inlined = theme.resolve_value(None, &[name.as_str()]);
                let Some(v) = &inlined else {
                    *unresolvable = true;
                    bail = true;
                    break;
                };
                if v.to_ascii_lowercase() == "currentcolor" {
                    *currentcolor = true;
                    bail = true;
                    break;
                }
                if v.starts_with("var(") {
                    var_node = value_parser::parse(v).into_iter().next();
                } else {
                    var_node = None;
                }
            }
            if !bail {
                if let Some(v) = inlined {
                    nodes[i] = ValueNode::Word(v);
                }
            }
            i += 1;
            continue;
        }

        if let ValueNode::Function { nodes: children, .. } = &mut nodes[i] {
            inline_vars(children, theme, requires_polyfill, unresolvable, currentcolor);
        }
        i += 1;
    }
}

fn apply_color_mix_polyfills(nodes: &mut Vec<ONode>, theme: &Theme) {
    let mut i = 0;
    while i < nodes.len() {
        match &mut nodes[i] {
            ONode::Rule { nodes: children, .. } | ONode::AtRule { nodes: children, .. } => {
                apply_color_mix_polyfills(children, theme);
                i += 1;
            }
            ONode::Decl { property, value, important, color_mix: true, .. } => {
                match needs_color_mix_polyfill(value, theme) {
                    Some(fallback_value) => {
                        let fallback = ONode::Decl {
                            property: property.clone(),
                            value: fallback_value,
                            important: *important,
                            theme_var: false,
                            color_mix: false,
                        };
                        let original = ONode::Decl {
                            property: property.clone(),
                            value: value.clone(),
                            important: *important,
                            theme_var: false,
                            color_mix: false,
                        };
                        let supports = ONode::AtRule {
                            name: "@supports".into(),
                            params: "(color: color-mix(in lab, red, red))".into(),
                            nodes: vec![original],
                            theme_keyframes: false,
                        };
                        nodes.splice(i..i + 1, [fallback, supports]);
                        i += 2;
                    }
                    None => i += 1,
                }
            }
            _ => i += 1,
        }
    }
}

fn into_ast(nodes: Vec<ONode>) -> Vec<AstNode> {
    nodes
        .into_iter()
        .map(|n| match n {
            ONode::Rule { selector, nodes } => AstNode::Rule { selector, nodes: into_ast(nodes) },
            ONode::AtRule { name, params, nodes, .. } => AstNode::AtRule { name, params, nodes: into_ast(nodes) },
            ONode::Decl { property, value, important, .. } => AstNode::Decl { property, value: Some(value), important },
            ONode::Comment(v) => AstNode::Comment(v),
        })
        .collect()
}

pub fn optimize_ast(ast: Vec<AstNode>, ds: &DesignSystem, polyfills: u8) -> Vec<AstNode> {
    let mut st = State {
        ds,
        polyfills,
        at_roots: vec![],
        seen_at_properties: HashSet::new(),
        used_keyframe_names: HashSet::new(),
        property_fallbacks_root: vec![],
        property_fallbacks_universal: vec![],
        variable_dependencies: HashMap::new(),
    };

    let mut new_ast: Vec<ONode> = Vec::new();
    for node in ast {
        transform(node, &mut new_ast, Ctx::default(), 0, &mut st);
    }

    // Remove unused theme variables
    prune_theme_vars(&mut new_ast, Owner::Root, &mut st);

    // Remove unused keyframes (only the ones defined in `@theme`)
    let used = std::mem::take(&mut st.used_keyframe_names);
    let mut at_roots = std::mem::take(&mut st.at_roots);
    at_roots.retain(|n| match n {
        ONode::AtRule { params, theme_keyframes: true, .. } => used.contains(params),
        _ => true,
    });

    new_ast.extend(at_roots);

    if polyfills & polyfills::COLOR_MIX != 0 {
        apply_color_mix_polyfills(&mut new_ast, &ds.theme);
    }

    let mut out = into_ast(new_ast);

    if polyfills & polyfills::AT_PROPERTY != 0 {
        let mut fallback_ast = vec![];
        if !st.property_fallbacks_root.is_empty() {
            fallback_ast.push(AstNode::Rule {
                selector: ":root, :host".into(),
                nodes: st.property_fallbacks_root.iter().map(|(p, v)| crate::tw::ast::decl(p.as_str(), v.as_str())).collect(),
            });
        }
        if !st.property_fallbacks_universal.is_empty() {
            fallback_ast.push(AstNode::Rule {
                selector: "*, ::before, ::after, ::backdrop".into(),
                nodes: st
                    .property_fallbacks_universal
                    .iter()
                    .map(|(p, v)| crate::tw::ast::decl(p.as_str(), v.as_str()))
                    .collect(),
            });
        }

        if !fallback_ast.is_empty() {
            let first_valid = out.iter().position(|n| match n {
                AstNode::Comment(_) => false,
                AstNode::AtRule { name, .. } => name != "@charset" && name != "@import",
                _ => true,
            });
            let layer_stmt = AstNode::AtRule { name: "@layer".into(), params: "properties".into(), nodes: vec![] };
            out.insert(first_valid.unwrap_or(out.len()), layer_stmt);

            out.push(AstNode::AtRule {
                name: "@layer".into(),
                params: "properties".into(),
                nodes: vec![AstNode::AtRule {
                    name: "@supports".into(),
                    params: "((-webkit-hyphens: none) and (not (margin-trim: inline))) or ((-moz-orient: inline) and (not (color:rgb(from red r g b))))".into(),
                    nodes: fallback_ast,
                }],
            });
        }
    }

    crate::tw::nesting::handle_nesting(out)
}
