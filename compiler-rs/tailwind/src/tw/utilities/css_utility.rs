//! Port of `createCssUtility` (`@utility` at-rules) from utilities.ts.

use super::*;
use crate::tw::ast::{walk, WalkAction};
use crate::tw::utils::{infer_data_type, is_positive_integer, is_valid_spacing_multiplier, unescape, DataType};
use crate::tw::value_parser::{self, ValueNode, WalkResult};
use regex::Regex;
use std::cell::RefCell;
use std::sync::LazyLock;

const BARE_VALUE_DATA_TYPES: &[&str] = &["number", "integer", "ratio", "percentage"];

/// `node` is the `@utility` at-rule. It is shared because `@apply` inside the
/// body is substituted *after* registration (upstream reads `node.nodes`
/// lazily when the utility is compiled), so the compile closures must borrow
/// it at call time.
///
/// Returns a registration closure, or `None` if the utility name is invalid.
pub fn create_css_utility(node: Rc<RefCell<AstNode>>) -> Option<Box<dyn FnOnce(&mut Utilities)>> {
    let name = match &*node.borrow() {
        AstNode::AtRule { params, .. } => unescape(params),
        _ => return None,
    };

    if is_valid_functional_utility_name(&name) {
        return Some(Box::new(move |u: &mut Utilities| {
            // Pre-process the AST: normalize `--value(…)` / `--modifier(…)` args.
            if let Some(nodes) = node.borrow_mut().nodes_mut() {
                walk(nodes, &mut |child, _| {
                    if let AstNode::Decl { value: Some(value), .. } = child {
                        if !value.is_empty() && (value.contains("--value(") || value.contains("--modifier(")) {
                            let mut ast = value_parser::parse(value);
                            value_parser::walk_mut(&mut ast, &mut |fun| {
                                if let ValueNode::Function { value: fname, nodes } = fun {
                                    if fname == "--value" || fname == "--modifier" {
                                        *nodes = value_parser::parse(&normalize_args(&value_parser::to_css(nodes)));
                                    }
                                }
                                WalkResult::Continue
                            });
                            *value = value_parser::to_css(&ast);
                        }
                    }
                    WalkAction::Continue
                });
            }

            let root = name[..name.len() - 2].to_string();
            let node = node.clone();
            u.push_raw(
                &root,
                Utility {
                    kind: UtilityKind::Functional,
                    compile: Rc::new(move |candidate, ds| compile_functional(&node, candidate, ds)),
                    types: vec![],
                },
            );
        }));
    }

    if is_valid_static_utility_name(&name) {
        return Some(Box::new(move |u: &mut Utilities| {
            u.static_(&name, move |_, _| Some(node.borrow().nodes().cloned().unwrap_or_default()));
        }));
    }

    None
}

fn normalize_args(args: &str) -> String {
    static ESCAPED_STAR: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\\\*").unwrap());
    static SPLIT_KEYS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"--(.*?)\s--(.*?)").unwrap());
    static WS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+").unwrap());
    static MULTI_STAR: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(-\*){2,}").unwrap());

    segment(args, b',')
        .into_iter()
        .map(|arg| {
            let arg = ESCAPED_STAR.replace_all(arg, "*");
            let arg = SPLIT_KEYS.replace_all(&arg, "--${1}-*--${2}");
            let arg = WS.replace_all(&arg, "");
            let mut arg = MULTI_STAR.replace_all(&arg, "-*").into_owned();
            if arg.starts_with("--") && !arg.contains('(') && !arg.contains("-*") {
                arg.push_str("-*");
            }
            arg
        })
        .collect::<Vec<_>>()
        .join(",")
}

#[derive(Default)]
struct State {
    used_value_fn: bool,
    resolved_value_fn: bool,
    used_modifier_fn: bool,
    resolved_modifier_fn: bool,
    resolved_ratio_value: bool,
    /// Paths (indices in the final tree) of declarations resolved by a
    /// non-ratio `--value(…)`.
    resolved_declarations: Vec<Vec<usize>>,
}

fn compile_functional(node: &Rc<RefCell<AstNode>>, candidate: &Candidate, ds: &DesignSystem) -> UResult {
    // A missing value resolves through `--default(…)`.
    let value = candidate.value().map(|value| match value {
        UtilityValue::Named { value, fraction } => Target::Named { value, fraction: Some(fraction.as_deref()) },
        UtilityValue::Arbitrary { value, data_type } => Target::Arbitrary { value, data_type: data_type.as_deref() },
    });
    let modifier = candidate.modifier.as_ref().map(|m| match m {
        Modifier::Named(v) => Target::Named { value: v, fraction: None },
        Modifier::Arbitrary(v) => Target::Arbitrary { value: v, data_type: None },
    });

    let nodes = node.borrow().nodes().cloned().unwrap_or_default();
    let mut state = State::default();
    let mut path = vec![];
    // The `@utility` at-rule itself is the parent of its top-level nodes.
    let mut nodes = process(nodes, true, value.as_ref(), modifier.as_ref(), ds, &mut state, &mut path);

    // Functional CSS utilities require a resolved `--value(…)`.
    if !state.used_value_fn || !state.resolved_value_fn {
        return UResult::Abort;
    }
    if state.used_modifier_fn && !state.resolved_modifier_fn && modifier.is_some() {
        return UResult::Abort;
    }
    if state.resolved_ratio_value && state.resolved_modifier_fn {
        return UResult::Abort;
    }
    if modifier.is_some() && !state.resolved_ratio_value && !state.resolved_modifier_fn {
        return UResult::Abort;
    }

    if state.resolved_ratio_value {
        let mut paths = state.resolved_declarations;
        paths.sort_by(|a, b| b.cmp(a));
        for p in paths {
            remove_at(&mut nodes, &p);
        }
    }

    UResult::Nodes(nodes)
}

fn remove_at(nodes: &mut Vec<AstNode>, path: &[usize]) {
    if path.len() == 1 {
        if path[0] < nodes.len() {
            nodes.remove(path[0]);
        }
        return;
    }
    if let Some(children) = nodes.get_mut(path[0]).and_then(|n| n.nodes_mut()) {
        remove_at(children, &path[1..]);
    }
}

fn process(
    nodes: Vec<AstNode>,
    parent_is_rule: bool,
    value: Option<&Target>,
    modifier: Option<&Target>,
    ds: &DesignSystem,
    state: &mut State,
    path: &mut Vec<usize>,
) -> Vec<AstNode> {
    let mut out = Vec::with_capacity(nodes.len());
    for node in nodes {
        match node {
            AstNode::Decl { property, value: Some(v), important } if parent_is_rule && !v.is_empty() => {
                let mut should_remove = false;
                let mut non_ratio_resolved = false;
                let mut ast = value_parser::parse(&v);
                value_parser::walk_mut(&mut ast, &mut |vn| {
                    let ValueNode::Function { value: fname, .. } = &*vn else { return WalkResult::Continue };
                    if fname == "--value" {
                        state.used_value_fn = true;
                        if let Some(resolved) = resolve_value_function(value, vn, ds) {
                            state.resolved_value_fn = true;
                            if resolved.ratio {
                                state.resolved_ratio_value = true;
                            } else {
                                non_ratio_resolved = true;
                            }
                            return WalkResult::Replace(resolved.nodes);
                        }
                        should_remove = true;
                        return WalkResult::Stop;
                    } else if fname == "--modifier" {
                        state.used_modifier_fn = true;
                        if let Some(resolved) = resolve_value_function(modifier, vn, ds) {
                            state.resolved_modifier_fn = true;
                            return WalkResult::Replace(resolved.nodes);
                        }
                        should_remove = true;
                        return WalkResult::Stop;
                    }
                    WalkResult::Continue
                });
                if should_remove {
                    continue;
                }
                if non_ratio_resolved {
                    path.push(out.len());
                    state.resolved_declarations.push(path.clone());
                    path.pop();
                }
                out.push(AstNode::Decl { property, value: Some(value_parser::to_css(&ast)), important });
            }
            mut other => {
                let is_rule = other.is_rule_or_at_rule();
                if let Some(children) = other.nodes_mut() {
                    path.push(out.len());
                    let taken = std::mem::take(children);
                    *children = process(taken, is_rule, value, modifier, ds, state, path);
                    path.pop();
                }
                out.push(other);
            }
        }
    }
    out
}

/// A candidate value or modifier (upstream passes either to `resolveValueFunction`).
enum Target<'a> {
    /// `fraction` is `Some(..)` when the object has a `fraction` key (utility
    /// values), `None` for named modifiers.
    Named { value: &'a str, fraction: Option<Option<&'a str>> },
    Arbitrary { value: &'a str, data_type: Option<&'a str> },
}

struct Resolved {
    nodes: Vec<ValueNode>,
    ratio: bool,
}

fn resolved(nodes: Vec<ValueNode>) -> Option<Resolved> {
    Some(Resolved { nodes, ratio: false })
}

fn js_positive_integer_number(s: &str) -> bool {
    match crate::tw::utils::js_number(s) {
        Some(n) => n.is_finite() && n.fract() == 0.0 && n >= 0.0,
        None => false,
    }
}

fn resolve_value_function(value: Option<&Target>, fun: &ValueNode, ds: &DesignSystem) -> Option<Resolved> {
    let ValueNode::Function { nodes: args, .. } = fun else { return None };

    // No value provided: use `--default(…)` if present
    let Some(value) = value else {
        return args.iter().find_map(|arg| match arg {
            ValueNode::Function { value, nodes } if value == "--default" => resolved(nodes.clone()),
            _ => None,
        });
    };

    for arg in args {
        let ValueNode::Word(arg) = arg else { continue };
        let ab = arg.as_bytes();

        match value {
            Target::Named { value: v, fraction } => {
                // Literal value, e.g.: `--modifier('closest-side')`
                if (ab[0] == b'\'' || ab[0] == b'"') && ab[ab.len() - 1] == ab[0] && ab.len() >= 2 && &arg[1..arg.len() - 1] == *v {
                    return resolved(value_parser::parse(v));
                }

                // Theme value, e.g.: `--value(--color)`
                if arg.starts_with("--") {
                    if let Some(theme_key) = arg.strip_suffix("-*") {
                        if let Some(r) = ds.theme.resolve(Some(v), &[theme_key]) {
                            if !r.is_empty() {
                                return resolved(value_parser::parse(&r));
                            }
                        }
                    } else {
                        let mut nested: Vec<&str> = arg.split("-*").collect();
                        if nested.len() <= 1 {
                            continue;
                        }
                        let first = nested.remove(0);
                        if let Some((_, options)) = ds.theme.resolve_with(v, &[first], &nested) {
                            let last = nested.last().copied().unwrap_or("");
                            if let Some((_, r)) = options.iter().find(|(k, _)| k == last) {
                                if !r.is_empty() {
                                    return resolved(value_parser::parse(r));
                                }
                            }
                        }
                    }
                    continue;
                }

                // Bare value, e.g.: `--value(integer)`
                if !BARE_VALUE_DATA_TYPES.contains(&arg.as_str()) {
                    continue;
                }
                let r: &str = match fraction {
                    Some(f) if arg == "ratio" => match f {
                        Some(f) => f,
                        None => continue,
                    },
                    _ => v,
                };
                if r.is_empty() {
                    continue;
                }
                let Some(ty) = DataType::from_str(arg).and_then(|t| infer_data_type(r, &[t])) else { continue };

                if ty == DataType::Ratio {
                    let parts = segment(r, b'/');
                    let lhs = parts[0];
                    let rhs = parts.get(1).copied();
                    if !js_positive_integer_number(lhs) || !rhs.is_some_and(js_positive_integer_number) {
                        continue;
                    }
                } else if ty == DataType::Number && !is_valid_spacing_multiplier(r) {
                    continue;
                } else if ty == DataType::Percentage && !is_positive_integer(&r[..r.len() - 1]) {
                    continue;
                }

                if ty == DataType::Ratio {
                    let parts = segment(r, b'/');
                    let rhs = parts.get(1).copied().unwrap_or("");
                    return Some(Resolved {
                        nodes: value_parser::parse(&format!("{} / {}", parts[0].trim(), rhs.trim())),
                        ratio: true,
                    });
                }

                return resolved(value_parser::parse(r));
            }
            Target::Arbitrary { value: v, data_type } => {
                if !(ab.len() >= 2 && ab[0] == b'[' && ab[ab.len() - 1] == b']') {
                    continue;
                }
                let dt = &arg[1..arg.len() - 1];
                if dt == "*" {
                    return resolved(value_parser::parse(v));
                }
                if let Some(given) = data_type {
                    if !given.is_empty() && *given != dt {
                        continue;
                    }
                    if !given.is_empty() {
                        return resolved(value_parser::parse(v));
                    }
                }
                if DataType::from_str(dt).and_then(|t| infer_data_type(v, &[t])).is_some() {
                    return resolved(value_parser::parse(v));
                }
            }
        }
    }

    None
}

/// `/^-?[a-z][a-zA-Z0-9_-]*/` — returns the matched root length.
fn utility_root_len(name: &str) -> Option<usize> {
    let b = name.as_bytes();
    let mut i = 0;
    if b.first() == Some(&b'-') {
        i += 1;
    }
    if !b.get(i).is_some_and(|c| c.is_ascii_lowercase()) {
        return None;
    }
    i += 1;
    while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_' || b[i] == b'-') {
        i += 1;
    }
    Some(i)
}

pub fn is_valid_static_utility_name(name: &str) -> bool {
    let Some(root_len) = utility_root_len(name) else { return false };
    let root = &name[..root_len];
    let value = &name[root_len..];

    if value.is_empty() {
        return !root.ends_with('-');
    }

    let vb = value.as_bytes();
    let root_last = root.as_bytes().last().copied();
    let prev = |i: usize| -> Option<u8> { if i > 0 { Some(vb[i - 1]) } else { root_last } };

    let mut seen_slash = false;
    for (i, &c) in vb.iter().enumerate() {
        match c {
            b'%' => {
                if i != vb.len() - 1 {
                    return false;
                }
                if !prev(i).is_some_and(|p| p.is_ascii_digit()) {
                    return false;
                }
            }
            b'/' => {
                if i == vb.len() - 1 {
                    return false;
                }
                if seen_slash {
                    return false;
                }
                seen_slash = true;
            }
            b'.' => {
                if !prev(i).is_some_and(|p| p.is_ascii_digit()) {
                    return false;
                }
                if !vb.get(i + 1).is_some_and(|n| n.is_ascii_digit()) {
                    return false;
                }
            }
            b'_' | b'-' => {}
            c if c.is_ascii_alphanumeric() => {}
            _ => return false,
        }
    }

    true
}

pub fn is_valid_functional_utility_name(name: &str) -> bool {
    let Some(name) = name.strip_suffix("-*") else { return false };
    let Some(root_len) = utility_root_len(name) else { return false };
    name.len() == root_len
}
