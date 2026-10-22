//! Port of `handleNesting` from `tailwindcss/src/ast.ts` (4.3.3): flattens
//! nested style rules and hoists `@media`/`@supports`/… so the output works
//! without a CSS nesting pass. A rule that has declarations keeps its nested
//! children as-is (like upstream), which `lower.rs` resolves when minifying.

use crate::tw::ast::AstNode;
use crate::tw::selector_parser::{self as sp, SKind, SNode};
use crate::tw::utils::segment;
use std::collections::{HashMap, HashSet};

const HOISTABLE_AT_RULES: &[&str] =
    &["@container", "@layer", "@media", "@page", "@starting-style", "@supports", "@view-transition"];

const DROPPABLE_IF_EMPTY_AT_RULES: &[&str] =
    &["@container", "@media", "@page", "@starting-style", "@supports", "@view-transition"];

/// Where `emit` currently pushes to (the JS `nodes` variable).
#[derive(Clone)]
enum Target {
    None,
    /// Children of the node at this index path in `result`.
    Path(Vec<usize>),
    /// An array that is no longer reachable from `result` (upstream keeps
    /// pushing into it, so those nodes are dropped).
    Detached,
}

struct State {
    selector_stack: Vec<String>,
    at_rule_stack: Vec<(String, String)>,
    nodes: Target,
    seen_declaration_properties: HashSet<String>,
    dedupe: Vec<Vec<usize>>,
    result: Vec<AstNode>,
    parse_cache: HashMap<String, Vec<SNode>>,
}

fn children_at<'a>(result: &'a mut Vec<AstNode>, path: &[usize]) -> &'a mut Vec<AstNode> {
    let mut target = result;
    for &i in path {
        target = target[i].nodes_mut().expect("nesting path points to a parent node");
    }
    target
}

impl State {
    fn parsed(&mut self, selector: &str) -> &Vec<SNode> {
        if !self.parse_cache.contains_key(selector) {
            self.parse_cache.insert(selector.to_string(), sp::parse(selector));
        }
        &self.parse_cache[selector]
    }

    fn mark_dedupe(&mut self, path: &[usize]) {
        if !self.dedupe.iter().any(|p| p == path) {
            self.dedupe.push(path.to_vec());
        }
    }

    fn emit(&mut self, node: AstNode) {
        // Existing nodes are available, emit into those nodes
        match self.nodes.clone() {
            Target::Path(path) => {
                if let AstNode::Decl { property, .. } = &node {
                    if self.seen_declaration_properties.contains(property) {
                        self.mark_dedupe(&path);
                    } else {
                        self.seen_declaration_properties.insert(property.clone());
                    }
                }
                children_at(&mut self.result, &path).push(node);
                return;
            }
            Target::Detached => {
                if let AstNode::Decl { property, .. } = &node {
                    self.seen_declaration_properties.insert(property.clone());
                }
                return;
            }
            Target::None => {}
        }

        // No parent rules or at-rules: emit as-is
        if self.selector_stack.is_empty() && self.at_rule_stack.is_empty() {
            if let (
                Some(AstNode::AtRule { name: ln, params: lp, nodes: lnodes }),
                AstNode::AtRule { name, params, nodes },
            ) = (self.result.last(), &node)
            {
                if lnodes.is_empty() && nodes.is_empty() && ln == name && lp == params {
                    return;
                }
            }
            self.result.push(node);
            return;
        }

        self.seen_declaration_properties.clear();
        if let AstNode::Decl { property, .. } = &node {
            self.seen_declaration_properties.insert(property.clone());
        }
        let nodes = vec![node];

        // Merge into adjacent at-rules with the same name/params
        let mut target_path: Vec<usize> = vec![];
        let mut at_rule_offset = 0;
        {
            let mut target: &Vec<AstNode> = &self.result;
            if matches!(target.last(), Some(AstNode::AtRule { .. })) {
                for (name, params) in &self.at_rule_stack {
                    let Some(AstNode::AtRule { name: ln, params: lp, nodes: lnodes }) = target.last() else { break };
                    if ln != name || lp != params {
                        break;
                    }
                    at_rule_offset += 1;
                    target_path.push(target.len() - 1);
                    target = lnodes;
                }
            }
        }

        let mut root: Option<AstNode> = None;
        // Path (relative to the root being built) of the vec that holds `nodes`.
        let mut nodes_depth_in_root = 0usize;

        if let Some(selector) = self.selector_stack.last().cloned() {
            if self.at_rule_stack.len() <= at_rule_offset {
                let target = children_at(&mut self.result, &target_path);
                if let Some(AstNode::Rule { selector: ls, nodes: lnodes }) = target.last_mut() {
                    if *ls == selector {
                        lnodes.extend(nodes);
                        let idx = target.len() - 1;
                        let mut path = target_path.clone();
                        path.push(idx);
                        self.mark_dedupe(&path);
                        self.nodes = Target::Path(path);
                        return;
                    }
                }
            }
            root = Some(AstNode::Rule { selector, nodes: vec![] });
            nodes_depth_in_root = 1;
        }

        let mut nodes = Some(nodes);
        if let Some(AstNode::Rule { nodes: n, .. }) = root.as_mut() {
            *n = nodes.take().unwrap();
        }

        for i in (at_rule_offset..self.at_rule_stack.len()).rev() {
            let (name, params) = self.at_rule_stack[i].clone();
            let children = match root.take() {
                Some(r) => vec![r],
                None => nodes.take().unwrap(),
            };
            root = Some(AstNode::AtRule { name, params, nodes: children });
            nodes_depth_in_root += 1;
        }

        let target = children_at(&mut self.result, &target_path);
        match root {
            Some(root) => {
                target.push(root);
                // `nodes` is the innermost vec: descend through the chain of
                // single-child wrappers we just built.
                let mut path = target_path;
                path.push(target.len() - 1);
                for _ in 1..nodes_depth_in_root {
                    path.push(0);
                }
                self.nodes = Target::Path(path);
            }
            None => {
                target.extend(nodes.take().unwrap());
                self.nodes = Target::Detached;
            }
        }
    }

    /// Resolve a nested selector against the parent selector.
    fn nest_selector(&mut self, selector: &str, last_selector: &str) -> String {
        let last_is_list = {
            let last_ast = self.parsed(last_selector);
            last_ast.len() == 1 && last_ast[0].kind() == SKind::List
        };
        let prefix = || if last_is_list { format!(":is({last_selector})") } else { last_selector.to_string() };

        let parts: Vec<String> = segment(selector, b',')
            .into_iter()
            .map(|selector| {
                if !selector.contains('&') {
                    return format!("{} {}", prefix(), selector);
                }

                let parent_ast = self.parsed(last_selector).clone();
                let mut ast = sp::parse(selector);
                let mut changed = false;
                substitute(&mut ast, None, false, last_selector, &parent_ast, &mut changed);

                if changed {
                    return sp::to_css(&ast);
                }
                format!("{} {}", prefix(), selector)
            })
            .collect();
        parts.join(", ")
    }

    fn visit(&mut self, node: AstNode) {
        match node {
            AstNode::Rule { selector, nodes } => {
                self.nodes = Target::None;

                if self.selector_stack.is_empty() {
                    if selector.contains('&') {
                        let mut ast = sp::parse(&selector);
                        let mut changed = false;
                        replace_nesting_with_scope(&mut ast, &mut changed);
                        self.selector_stack.push(if changed { sp::to_css(&ast) } else { selector });
                    } else {
                        self.selector_stack.push(selector);
                    }
                } else {
                    // A rule with just `&` is replaced by its children.
                    if selector == "&" {
                        for child in nodes {
                            self.visit(child);
                        }
                        return;
                    }
                    let last = self.selector_stack.last().unwrap().clone();
                    let resolved = self.nest_selector(&selector, &last);
                    self.selector_stack.push(resolved);
                }

                if nodes.iter().any(|c| matches!(c, AstNode::Decl { .. })) {
                    for child in nodes {
                        self.emit(child);
                    }
                } else {
                    for child in nodes {
                        self.visit(child);
                    }
                }

                self.nodes = Target::None;
                self.selector_stack.pop();
            }

            AstNode::AtRule { name, params, nodes } => {
                self.nodes = Target::None;

                if nodes.is_empty() && !DROPPABLE_IF_EMPTY_AT_RULES.contains(&name.as_str()) {
                    self.emit(AstNode::AtRule { name, params, nodes });
                } else if HOISTABLE_AT_RULES.contains(&name.as_str()) {
                    self.at_rule_stack.push((name, params));
                    for child in nodes {
                        self.visit(child);
                    }
                    self.nodes = Target::None;
                    self.at_rule_stack.pop();
                } else {
                    self.emit(AstNode::AtRule { name, params, nodes });
                }
            }

            AstNode::Decl { .. } | AstNode::Comment(_) => self.emit(node),

            AstNode::Context { nodes, .. } | AstNode::AtRoot(nodes) => {
                for child in nodes {
                    self.visit(child);
                }
            }
        }
    }
}

/// Top-level rule: `&` → `:scope`.
fn replace_nesting_with_scope(nodes: &mut [SNode], changed: &mut bool) {
    for node in nodes {
        if let SNode::Selector(v) = node {
            if v == "&" {
                *changed = true;
                *v = ":scope".into();
            }
        }
        if let Some(children) = node.nodes_mut() {
            replace_nesting_with_scope(children, changed);
        }
    }
}

/// Replace `&` by the parent selector, dropping `:is(…)` where it is safe
/// (the `enter`/`exit` visitors of upstream's slow path).
///
/// `parent` is the kind of the node holding `nodes` (`None` at the top),
/// `gp_complex_not_first` whether that node sits in a complex selector
/// without being its first node.
fn substitute(
    nodes: &mut Vec<SNode>,
    parent: Option<SKind>,
    gp_complex_not_first: bool,
    last_selector: &str,
    parent_ast: &[SNode],
    changed: &mut bool,
) {
    let mut i = 0;
    while i < nodes.len() {
        // enter
        if matches!(&nodes[i], SNode::Selector(v) if v == "&") {
            *changed = true;
            let value = enter_value(nodes, i, parent, gp_complex_not_first, last_selector, parent_ast);
            nodes[i] = SNode::Selector(value);
        }

        // children
        let kind = nodes[i].kind();
        if let Some(children) = nodes[i].nodes_mut() {
            let child_gp = parent == Some(SKind::Complex) && i != 0;
            substitute(children, Some(kind), child_gp, last_selector, parent_ast, changed);
        }

        // exit: `*:hover` → `:hover`, but keep `*|div`
        if i == 0 && nodes.len() > 1 && parent == Some(SKind::Compound) && sp::is_universal_selector(&nodes[0]) {
            let keep = matches!(&nodes[1], SNode::Selector(v) if v.starts_with('|'));
            if !keep {
                nodes.remove(0);
                continue;
            }
        }
        i += 1;
    }
}

fn enter_value(
    siblings: &[SNode],
    index: usize,
    parent: Option<SKind>,
    gp_complex_not_first: bool,
    last_selector: &str,
    parent_ast: &[SNode],
) -> String {
    let wrapped = format!(":is({last_selector})");
    let plain = last_selector.to_string();

    let Some(parent) = parent else { return wrapped };

    // 1. Multiple selectors: keep `:is(…)`
    if parent_ast.len() == 1 && parent_ast[0].kind() == SKind::List {
        return wrapped;
    }

    let first = parent_ast.first();
    let first_is_complex = first.map(|n| n.kind() == SKind::Complex).unwrap_or(false);
    let first_is_universal_or_type =
        first.map(|n| sp::is_universal_selector(n) || sp::is_type_selector(n)).unwrap_or(false);
    match parent {
        // 2. `&` standalone in a complex selector
        SKind::Complex => {
            if index == 0 {
                plain
            } else if first_is_complex {
                wrapped
            } else {
                plain
            }
        }

        // 3. `&` attached to a compound selector
        SKind::Compound => {
            if first_is_complex && gp_complex_not_first {
                return wrapped;
            }
            if siblings[index + 1..].iter().any(|s| sp::is_universal_selector(s) || sp::is_type_selector(s)) {
                return wrapped;
            }
            // `&[after]` is always safe; `[before]&` and `[before]&[after]`
            // need `:is(…)` for complex, universal and type parents.
            if index == 0 || !(first_is_complex || first_is_universal_or_type) {
                plain
            } else {
                wrapped
            }
        }

        // 4. Function argument, e.g. `:not(&)`
        SKind::Function => plain,

        _ => wrapped,
    }
}

pub fn handle_nesting(ast: Vec<AstNode>) -> Vec<AstNode> {
    let mut st = State {
        selector_stack: vec![],
        at_rule_stack: vec![],
        nodes: Target::None,
        seen_declaration_properties: HashSet::new(),
        dedupe: vec![],
        result: vec![],
        parse_cache: HashMap::new(),
    };

    for node in ast {
        st.visit(node);
    }

    // Dedupe exact duplicate declarations, keeping the last one.
    let dedupe = std::mem::take(&mut st.dedupe);
    for path in dedupe {
        let nodes = children_at(&mut st.result, &path);
        let mut seen: HashSet<(String, Option<String>, bool)> = HashSet::new();
        let mut i = nodes.len();
        while i > 0 {
            i -= 1;
            if let AstNode::Decl { property, value, important } = &nodes[i] {
                let id = (property.clone(), value.clone(), *important);
                if seen.contains(&id) {
                    nodes.remove(i);
                } else {
                    seen.insert(id);
                }
            }
        }
    }

    st.result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tw::ast::{decl, style_rule, to_css};

    #[test]
    fn flattens_nested_rules() {
        let ast = vec![style_rule(
            ".divide-x",
            vec![style_rule(":where(& > :not(:last-child))", vec![decl("--x", "0")])],
        )];
        assert_eq!(to_css(&handle_nesting(ast)), ":where(.divide-x > :not(:last-child)) {\n  --x: 0;\n}\n");
    }

    #[test]
    fn keeps_nesting_below_declarations() {
        let ast = vec![style_rule(".a", vec![decl("color", "red"), style_rule("&:hover", vec![decl("color", "blue")])])];
        assert_eq!(to_css(&handle_nesting(ast)), ".a {\n  color: red;\n  &:hover {\n    color: blue;\n  }\n}\n");
    }

    #[test]
    fn hoists_media() {
        let ast = vec![style_rule(
            ".md\\:flex",
            vec![crate::tw::ast::at_rule("@media", "(width >= 48rem)", vec![decl("display", "flex")])],
        )];
        assert_eq!(to_css(&handle_nesting(ast)), "@media (width >= 48rem) {\n  .md\\:flex {\n    display: flex;\n  }\n}\n");
    }
}
