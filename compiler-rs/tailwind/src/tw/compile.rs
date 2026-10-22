//! Port of `tailwindcss/src/compile.ts`.

use crate::tw::ast::{at_rule, decl, rule, walk, AstNode, WalkAction};
use crate::tw::candidate::{Candidate, CandidateKind, Variant};
use crate::tw::design_system::DesignSystem;
use crate::tw::property_order::PROPERTY_ORDER;
use crate::tw::utilities::{as_color, UResult, UtilityKind};
use crate::tw::utils::{compare, escape};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::sync::LazyLock;

#[derive(Debug, Clone, Default)]
pub struct PropertySort {
    pub order: Vec<usize>,
    pub count: usize,
}

static PROPERTY_INDEX: LazyLock<HashMap<&'static str, usize>> = LazyLock::new(|| {
    let mut m = HashMap::new();
    for (i, p) in PROPERTY_ORDER.iter().enumerate() {
        m.entry(*p).or_insert(i);
    }
    m
});

/// Returns the sorted AST nodes, each paired with the raw candidate that
/// produced it.
pub fn compile_candidates(
    raw_candidates: &[String],
    ds: &DesignSystem,
    respect_important: bool,
    on_invalid: &mut dyn FnMut(&str),
) -> Vec<(AstNode, String)> {
    let mut matches: Vec<(String, std::rc::Rc<Vec<Candidate>>)> = Vec::new();

    for raw in raw_candidates {
        if ds.invalid_candidates.borrow().contains(raw) {
            on_invalid(raw);
            continue;
        }
        let candidates = ds.parse_candidate(raw);
        if candidates.is_empty() {
            on_invalid(raw);
            continue;
        }
        matches.push((raw.clone(), candidates));
    }

    let variant_order = ds.get_variant_order();

    struct Entry {
        node: AstNode,
        sort: PropertySort,
        variants: Vec<usize>, // descending, unique
        candidate: String,
    }

    let mut entries: Vec<Entry> = Vec::new();

    for (raw, candidates) in &matches {
        let mut found = false;
        for candidate in candidates.iter() {
            let rules = ds.compile_ast_nodes(candidate, respect_important);
            if rules.is_empty() {
                continue;
            }
            found = true;

            let mut bits: Vec<usize> = candidate
                .variants
                .iter()
                .map(|v| *variant_order.get(&std::rc::Rc::as_ptr(v)).unwrap_or(&0))
                .collect();
            bits.sort_unstable_by(|a, b| b.cmp(a));
            bits.dedup();

            for (node, sort) in rules.iter() {
                entries.push(Entry { node: node.clone(), sort: sort.clone(), variants: bits.clone(), candidate: raw.clone() });
            }
        }
        if !found {
            on_invalid(raw);
        }
    }

    entries.sort_by(|a, z| {
        // Compare variant bitsets as big integers (highest bit first).
        let ord = cmp_bitsets(&a.variants, &z.variants);
        if ord != Ordering::Equal {
            return ord;
        }

        let ao = &a.sort.order;
        let zo = &z.sort.order;
        let mut offset = 0;
        while offset < ao.len() && offset < zo.len() && ao[offset] == zo[offset] {
            offset += 1;
        }
        match (ao.get(offset), zo.get(offset)) {
            (Some(x), Some(y)) if x != y => return x.cmp(y),
            (Some(_), None) => return Ordering::Less,
            (None, Some(_)) => return Ordering::Greater,
            _ => {}
        }

        match z.sort.count.cmp(&a.sort.count) {
            Ordering::Equal => {}
            o => return o,
        }

        compare(&a.candidate, &z.candidate)
    });

    entries.into_iter().map(|e| (e.node, e.candidate)).collect()
}

fn cmp_bitsets(a: &[usize], z: &[usize]) -> Ordering {
    let mut i = 0;
    loop {
        match (a.get(i), z.get(i)) {
            (None, None) => return Ordering::Equal,
            (Some(_), None) => return Ordering::Greater,
            (None, Some(_)) => return Ordering::Less,
            (Some(x), Some(y)) => {
                if x != y {
                    return x.cmp(y);
                }
            }
        }
        i += 1;
    }
}

pub fn compile_ast_nodes(candidate: &Candidate, ds: &DesignSystem, respect_important: bool) -> Vec<(AstNode, PropertySort)> {
    let asts = compile_base_utility(candidate, ds);
    if asts.is_empty() {
        return vec![];
    }

    let respect_important = ds.important && respect_important;
    let selector = format!(".{}", escape(&candidate.raw));
    let mut rules = vec![];

    for mut nodes in asts {
        let property_sort = get_property_sort(&nodes);

        if candidate.important || respect_important {
            apply_important(&mut nodes);
        }

        let mut node = AstNode::Rule { selector: selector.clone(), nodes };

        for variant in &candidate.variants {
            if !apply_variant(&mut node, variant, ds, 0) {
                return vec![];
            }
        }

        rules.push((node, property_sort));
    }

    rules
}

pub fn apply_variant(node: &mut AstNode, variant: &Variant, ds: &DesignSystem, depth: usize) -> bool {
    if let Variant::Arbitrary { selector, relative } = variant {
        if *relative && depth == 0 {
            return false;
        }
        let children = std::mem::take(node.nodes_mut().unwrap());
        *node.nodes_mut().unwrap() = vec![rule(selector.as_str(), children)];
        return true;
    }

    let Some(def) = ds.variants.get(variant.root().unwrap()) else { return false };
    let apply = def.apply.clone();

    if let Variant::Compound { root, variant: sub, .. } = variant {
        let mut isolated = at_rule("@slot", "", vec![]);
        if !apply_variant(&mut isolated, sub, ds, depth + 1) {
            return false;
        }

        let children = isolated.nodes_mut().unwrap();
        if root == "not" && children.len() > 1 {
            return false;
        }

        for child in children.iter_mut() {
            if !child.is_rule_or_at_rule() {
                return false;
            }
            if !apply(child, variant, ds) {
                return false;
            }
        }

        let original = std::mem::take(node.nodes_mut().unwrap());
        walk(children, &mut |child, _| {
            if let AstNode::Rule { nodes, .. } | AstNode::AtRule { nodes, .. } = child {
                if nodes.is_empty() {
                    *nodes = original.clone();
                    return WalkAction::Skip;
                }
            }
            WalkAction::Continue
        });
        *node.nodes_mut().unwrap() = std::mem::take(children);
        return true;
    }

    apply(node, variant, ds)
}

fn is_fallback_utility(types: &[String]) -> bool {
    types.len() > 1 && types.iter().any(|t| t == "any")
}

fn compile_base_utility(candidate: &Candidate, ds: &DesignSystem) -> Vec<Vec<AstNode>> {
    if let CandidateKind::Arbitrary { property, value } = &candidate.kind {
        let mut value = Some(value.clone());
        if let Some(m) = &candidate.modifier {
            value = as_color(value.as_deref().unwrap(), Some(m), &ds.theme);
        }
        return match value {
            Some(v) => vec![vec![decl(property.as_str(), v)]],
            None => vec![],
        };
    }

    let utilities = ds.utilities.get(candidate.root());
    let mut asts: Vec<Vec<AstNode>> = vec![];

    let kind = if candidate.is_static() { UtilityKind::Static } else { UtilityKind::Functional };

    for pass in [false, true] {
        for utility in utilities.iter().filter(|u| is_fallback_utility(&u.types) == pass) {
            if utility.kind != kind {
                continue;
            }
            match (utility.compile)(candidate, ds) {
                UResult::Skip => continue,
                // `null` means this utility produced invalid CSS; only typed
                // (plugin) utilities stop the search for other utilities.
                UResult::Abort if !utility.types.is_empty() => return asts,
                UResult::Abort => continue,
                UResult::Nodes(nodes) => asts.push(nodes),
            }
        }
        if !asts.is_empty() {
            return asts;
        }
    }

    asts
}

fn apply_important(ast: &mut [AstNode]) {
    for node in ast {
        match node {
            AstNode::AtRoot(_) => continue,
            AstNode::Decl { important, .. } => *important = true,
            AstNode::Rule { nodes, .. } | AstNode::AtRule { nodes, .. } => apply_important(nodes),
            _ => {}
        }
    }
}

fn get_property_sort(nodes: &[AstNode]) -> PropertySort {
    let mut order: Vec<usize> = vec![];
    let mut count = 0;
    let mut q: std::collections::VecDeque<&AstNode> = nodes.iter().collect();
    let mut seen_tw_sort = false;

    while let Some(node) = q.pop_front() {
        match node {
            AstNode::Decl { property, value, .. } => {
                let Some(value) = value else { continue };
                count += 1;
                if seen_tw_sort {
                    continue;
                }
                if property == "--tw-sort" {
                    if let Some(&idx) = PROPERTY_INDEX.get(value.as_str()) {
                        order.push(idx);
                        seen_tw_sort = true;
                        continue;
                    }
                }
                if let Some(&idx) = PROPERTY_INDEX.get(property.as_str()) {
                    order.push(idx);
                }
            }
            AstNode::Rule { nodes, .. } | AstNode::AtRule { nodes, .. } => {
                for child in nodes {
                    q.push_back(child);
                }
            }
            _ => {}
        }
    }

    order.sort_unstable();
    order.dedup();
    PropertySort { order, count }
}
