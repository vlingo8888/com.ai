//! Port of `tailwindcss/src/apply.ts`.
//!
//! Upstream tracks AST node identities to topologically sort `@utility`
//! definitions before substituting `@apply`. Here, custom utility bodies live
//! in shared cells (see `create_css_utility`), so we process them in dependency
//! order first and then substitute every remaining `@apply` in the main AST.

use crate::tw::ast::{walk, AstNode, WalkAction};
use crate::tw::candidate::CandidateKind;
use crate::tw::compile::compile_candidates;
use crate::tw::design_system::DesignSystem;
use crate::tw::utils::segment;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

fn apply_candidates(params: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = vec![];
    for part in params.split_whitespace() {
        if seen.insert(part.to_string()) {
            out.push(part.to_string());
        }
    }
    out
}

fn dependencies_of(nodes: &[AstNode], ds: &DesignSystem) -> Vec<String> {
    let mut deps = vec![];
    crate::tw::ast::walk_ref(nodes, &mut |n| {
        if let AstNode::AtRule { name, params, .. } = n {
            if name == "@apply" {
                for candidate in params.split(|c: char| c.is_whitespace()) {
                    for c in ds.parse_candidate(candidate).iter() {
                        match &c.kind {
                            CandidateKind::Static { root } | CandidateKind::Functional { root, .. } => {
                                deps.push(root.clone())
                            }
                            CandidateKind::Arbitrary { .. } => {}
                        }
                    }
                }
            }
        }
        WalkAction::Continue
    });
    deps
}

fn compile_apply(params: &str, ds: &DesignSystem) -> Result<Vec<AstNode>, String> {
    let candidates = apply_candidates(params);
    let mut error: Option<String> = None;
    let compiled = compile_candidates(&candidates, ds, false, &mut |candidate| {
        if error.is_some() {
            return;
        }
        if let Some(prefix) = &ds.theme.prefix {
            if !candidate.starts_with(prefix.as_str()) {
                error = Some(format!(
                    "Cannot apply unprefixed utility class `{}`. Did you mean `{}:{}`?",
                    candidate, prefix, candidate
                ));
                return;
            }
        }
        if ds.invalid_candidates.borrow().contains(candidate) {
            error = Some(format!(
                "Cannot apply utility class `{}` because it has been explicitly disabled",
                candidate
            ));
            return;
        }
        let parts = segment(candidate, b':');
        if parts.len() > 1 {
            let utility = parts[parts.len() - 1];
            if ds.candidates_to_css(&[utility])[0].is_some() {
                let checks: Vec<String> =
                    parts[..parts.len() - 1].iter().map(|v| format!("{}:[--tw-variant-check:1]", v)).collect();
                let refs: Vec<&str> = checks.iter().map(|s| s.as_str()).collect();
                let compiled = ds.candidates_to_css(&refs);
                let unknown: Vec<String> = parts[..parts.len() - 1]
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| compiled[*i].is_none())
                    .map(|(_, v)| format!("`{}`", v))
                    .collect();
                if unknown.len() == 1 {
                    error = Some(format!(
                        "Cannot apply utility class `{}` because the {} variant does not exist.",
                        candidate, unknown[0]
                    ));
                    return;
                } else if unknown.len() > 1 {
                    error = Some(format!(
                        "Cannot apply utility class `{}` because the {} variants do not exist.",
                        candidate,
                        unknown.join(", ")
                    ));
                    return;
                }
            }
        }
        if ds.theme.size() == 0 {
            error = Some(format!(
                "Cannot apply unknown utility class `{}`. Are you using CSS modules or similar and missing `@reference`?",
                candidate
            ));
            return;
        }
        error = Some(format!("Cannot apply unknown utility class `{}`", candidate));
    });
    if let Some(e) = error {
        return Err(e);
    }

    let mut new_nodes = vec![];
    for (node, _) in compiled {
        match node {
            AstNode::Rule { nodes, .. } => new_nodes.extend(nodes),
            other => new_nodes.push(other),
        }
    }
    Ok(new_nodes)
}

fn substitute_in(nodes: &mut Vec<AstNode>, ds: &DesignSystem, applied: &mut bool) -> Result<(), String> {
    let mut error: Option<String> = None;
    walk(nodes, &mut |node, ctx| {
        let AstNode::AtRule { name, params, nodes: children } = node else { return WalkAction::Continue };
        if name == "@keyframes" {
            let mut has_apply = false;
            crate::tw::ast::walk_ref(children, &mut |c| {
                if c.at_rule_name() == Some("@apply") {
                    has_apply = true;
                    return WalkAction::Stop;
                }
                WalkAction::Continue
            });
            if has_apply {
                error = Some("You cannot use `@apply` inside `@keyframes`.".into());
                return WalkAction::Stop;
            }
            return WalkAction::Skip;
        }
        if name != "@apply" {
            return WalkAction::Continue;
        }
        let _ = ctx;

        // `@apply --foo` is a CSS mixin (css-mixins-1), emitted literally.
        let mut normal: Vec<&str> = vec![];
        let mut dashed: Vec<&str> = vec![];
        let mut parts: Vec<&str> = params.split(char::is_whitespace).filter(|p| !p.is_empty()).collect();
        if parts.is_empty() {
            parts.push("");
        }
        for part in parts {
            if part.starts_with("--") {
                dashed.push(part);
            } else {
                normal.push(part);
            }
        }
        if !dashed.is_empty() {
            if normal.is_empty() {
                return WalkAction::Skip;
            }
            error = Some(format!(
                "You cannot use `@apply` with both mixins and utilities. Please move `@apply {}` into a separate rule.",
                dashed.join(" ")
            ));
            return WalkAction::Stop;
        }
        if !children.is_empty() {
            error = Some(format!("The rule `@apply {}` must not have a body.", normal.join(" ")));
            return WalkAction::Stop;
        }

        *applied = true;
        match compile_apply(params, ds) {
            Ok(replacement) => WalkAction::Replace(replacement),
            Err(e) => {
                error = Some(e);
                WalkAction::Stop
            }
        }
    });
    match error {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

/// Substitute `@apply` inside custom `@utility` definitions (in dependency
/// order) and then in the main AST. Returns whether `@apply` was used outside
/// of `@utility`.
pub fn substitute_at_apply(
    ast: &mut Vec<AstNode>,
    utilities: &[Rc<RefCell<AstNode>>],
    ds: &DesignSystem,
) -> Result<bool, String> {
    // Map utility root -> definitions
    let mut definitions: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, u) in utilities.iter().enumerate() {
        if let AstNode::AtRule { params, .. } = &*u.borrow() {
            let name = params.strip_suffix("-*").unwrap_or(params).to_string();
            definitions.entry(name).or_default().push(i);
        }
    }

    let deps: Vec<Vec<String>> = utilities
        .iter()
        .map(|u| u.borrow().nodes().map(|n| dependencies_of(n, ds)).unwrap_or_default())
        .collect();

    // Topological order (DFS)
    let mut order = vec![];
    let mut seen = HashSet::new();
    let mut wip = HashSet::new();

    fn visit(
        i: usize,
        deps: &[Vec<String>],
        definitions: &HashMap<String, Vec<usize>>,
        utilities: &[Rc<RefCell<AstNode>>],
        seen: &mut HashSet<usize>,
        wip: &mut HashSet<usize>,
        order: &mut Vec<usize>,
    ) -> Result<(), String> {
        if seen.contains(&i) {
            return Ok(());
        }
        if !wip.insert(i) {
            let name = match &*utilities[i].borrow() {
                AstNode::AtRule { params, .. } => params.clone(),
                _ => String::new(),
            };
            return Err(format!("Circular dependency detected in `@utility {}`", name));
        }
        for dep in &deps[i] {
            if let Some(defs) = definitions.get(dep) {
                for &d in defs {
                    if d == i {
                        let name = match &*utilities[i].borrow() {
                            AstNode::AtRule { params, .. } => params.clone(),
                            _ => String::new(),
                        };
                        return Err(format!(
                            "You cannot `@apply` the `{}` utility here because it creates a circular dependency.",
                            name
                        ));
                    }
                    visit(d, deps, definitions, utilities, seen, wip, order)?;
                }
            }
        }
        seen.insert(i);
        wip.remove(&i);
        order.push(i);
        Ok(())
    }

    for i in 0..utilities.len() {
        if !deps[i].is_empty() {
            visit(i, &deps, &definitions, utilities, &mut seen, &mut wip, &mut order)?;
        }
    }

    let mut unused = false;
    for i in order {
        let mut body = utilities[i].borrow().nodes().cloned().unwrap_or_default();
        substitute_in(&mut body, ds, &mut unused)?;
        if let Some(n) = utilities[i].borrow_mut().nodes_mut() {
            *n = body;
        }
    }

    let mut applied = false;
    substitute_in(ast, ds, &mut applied)?;
    Ok(applied)
}
