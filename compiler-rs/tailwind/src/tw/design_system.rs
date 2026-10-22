//! Port of `tailwindcss/src/design-system.ts`.

use crate::tw::ast::AstNode;
use crate::tw::candidate::{parse_candidate, parse_variant, Candidate, Variant};
use crate::tw::compile::{compile_ast_nodes, PropertySort};
use crate::tw::theme::{opts, Theme};
use crate::tw::utilities::{create_utilities, with_alpha, Utilities};
use crate::tw::value_parser::{self, ValueNode};
use crate::tw::variants::{create_variants, Variants};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

pub struct DesignSystem {
    pub theme: Theme,
    pub utilities: Utilities,
    pub variants: Variants,
    pub invalid_candidates: RefCell<HashSet<String>>,
    pub important: bool,

    parsed_variants: RefCell<HashMap<String, Option<Rc<Variant>>>>,
    /// Insertion order of parsed variants (used for variant ordering)
    parsed_variant_order: RefCell<Vec<Rc<Variant>>>,
    parsed_candidates: RefCell<HashMap<String, Rc<Vec<Candidate>>>>,
    /// Keyed by the address of a candidate owned by `parsed_candidates`
    /// (stable until `clear_caches`).
    compiled: RefCell<HashMap<(usize, bool), Rc<Vec<(AstNode, PropertySort)>>>>,
    tracked_variables: RefCell<HashSet<String>>,
    variables_cache: RefCell<HashMap<String, Rc<Vec<String>>>>,
}

impl DesignSystem {
    pub fn new(theme: Theme) -> Self {
        let utilities = create_utilities(&theme);
        let variants = create_variants(&theme);
        DesignSystem {
            theme,
            utilities,
            variants,
            invalid_candidates: RefCell::new(HashSet::new()),
            important: false,
            parsed_variants: RefCell::new(HashMap::new()),
            parsed_variant_order: RefCell::new(Vec::new()),
            parsed_candidates: RefCell::new(HashMap::new()),
            compiled: RefCell::new(HashMap::new()),
            tracked_variables: RefCell::new(HashSet::new()),
            variables_cache: RefCell::new(HashMap::new()),
        }
    }

    /// Drop parse caches (needed after registering new utilities/variants).
    pub fn clear_caches(&self) {
        self.parsed_variants.borrow_mut().clear();
        self.parsed_variant_order.borrow_mut().clear();
        self.parsed_candidates.borrow_mut().clear();
        self.compiled.borrow_mut().clear();
    }

    pub fn parse_candidate(&self, candidate: &str) -> Rc<Vec<Candidate>> {
        if let Some(c) = self.parsed_candidates.borrow().get(candidate) {
            return c.clone();
        }
        let parsed = Rc::new(parse_candidate(candidate, self));
        self.parsed_candidates.borrow_mut().insert(candidate.to_string(), parsed.clone());
        parsed
    }

    pub fn parse_variant(&self, variant: &str) -> Option<Rc<Variant>> {
        if let Some(v) = self.parsed_variants.borrow().get(variant) {
            return v.clone();
        }
        let parsed = parse_variant(variant, self).map(Rc::new);
        let mut cache = self.parsed_variants.borrow_mut();
        if let Some(existing) = cache.get(variant) {
            return existing.clone();
        }
        cache.insert(variant.to_string(), parsed.clone());
        if let Some(p) = &parsed {
            self.parsed_variant_order.borrow_mut().push(p.clone());
        }
        parsed
    }

    /// Compile a parsed candidate. Results are cached per candidate (like
    /// upstream's `compiledAstNodes` map); they don't depend on build state.
    pub fn compile_ast_nodes(&self, candidate: &Candidate, respect_important: bool) -> Rc<Vec<(AstNode, PropertySort)>> {
        let key = (candidate as *const Candidate as usize, respect_important);
        if let Some(hit) = self.compiled.borrow().get(&key) {
            return hit.clone();
        }
        let result = Rc::new(self.compile_ast_nodes_uncached(candidate, respect_important));
        self.compiled.borrow_mut().insert(key, result.clone());
        result
    }

    fn compile_ast_nodes_uncached(&self, candidate: &Candidate, respect_important: bool) -> Vec<(AstNode, PropertySort)> {
        let ast = compile_ast_nodes(candidate, self, respect_important);
        let (mut nodes, sorts): (Vec<AstNode>, Vec<PropertySort>) = ast.into_iter().unzip();
        if crate::tw::css_functions::substitute_functions(&mut nodes, self).is_err() {
            return vec![];
        }
        if crate::tw::variants::substitute_at_variant(&mut nodes, self).is_err() {
            return vec![];
        }
        nodes.into_iter().zip(sorts).collect()
    }

    /// Map from variant pointer to its order index.
    pub fn get_variant_order(&self) -> HashMap<*const Variant, usize> {
        let mut variants: Vec<Rc<Variant>> = self.parsed_variant_order.borrow().clone();
        variants.sort_by(|a, z| self.variants.compare(a, z, self));

        let mut order = HashMap::new();
        let mut prev: Option<&Rc<Variant>> = None;
        let mut index = 0usize;
        for variant in &variants {
            if let Some(p) = prev {
                if self.variants.compare(p, variant, self) != std::cmp::Ordering::Equal {
                    index += 1;
                }
            }
            order.insert(Rc::as_ptr(variant), index);
            prev = Some(variant);
        }
        order
    }

    pub fn resolve_theme_value(&self, path: &str, force_inline: bool) -> Option<String> {
        let mut path = path;
        let mut modifier: Option<&str> = None;
        if let Some(idx) = path.rfind('/') {
            modifier = Some(path[idx + 1..].trim());
            path = path[..idx].trim();
        }
        let value = self.theme.resolve_opts(None, &[path], if force_inline { opts::INLINE } else { opts::NONE })?;
        match modifier {
            Some(m) if !m.is_empty() => Some(with_alpha(&value, m)),
            _ => Some(value),
        }
    }

    /// Forget which raw values were already scanned for variables (used when
    /// a cached compiler starts a new build).
    pub fn reset_tracking(&self) {
        self.tracked_variables.borrow_mut().clear();
    }

    pub fn track_used_variables(&self, raw: &str) {
        if self.tracked_variables.borrow().contains(raw) {
            return;
        }
        self.tracked_variables.borrow_mut().insert(raw.to_string());
        for variable in self.used_variables(raw).iter() {
            self.theme.mark_used_variable(variable);
        }
    }

    /// `extract_used_variables`, memoized (the result only depends on `raw`).
    pub fn used_variables(&self, raw: &str) -> Rc<Vec<String>> {
        if let Some(v) = self.variables_cache.borrow().get(raw) {
            return v.clone();
        }
        let v = Rc::new(extract_used_variables(raw));
        self.variables_cache.borrow_mut().insert(raw.to_string(), v.clone());
        v
    }

    /// Compile classes for `@apply` error messages / IntelliSense style checks.
    pub fn candidates_to_css(&self, classes: &[&str]) -> Vec<Option<String>> {
        classes
            .iter()
            .map(|c| {
                let mut valid = true;
                let nodes = crate::tw::compile::compile_candidates(&[c.to_string()], self, true, &mut |_| valid = false);
                let nodes: Vec<AstNode> = nodes.into_iter().map(|(n, _)| n).collect();
                if valid && !nodes.is_empty() {
                    Some(crate::tw::ast::to_css(&nodes))
                } else {
                    None
                }
            })
            .collect()
    }
}

pub fn extract_used_variables(raw: &str) -> Vec<String> {
    let mut variables = vec![];
    fn visit(nodes: &[ValueNode], out: &mut Vec<String>) {
        for node in nodes {
            if let ValueNode::Function { value, nodes } = node {
                if value == "var" {
                    collect_words(nodes, out);
                    continue;
                }
                visit(nodes, out);
            }
        }
    }
    fn collect_words(nodes: &[ValueNode], out: &mut Vec<String>) {
        for node in nodes {
            match node {
                ValueNode::Word(w) if w.starts_with("--") => out.push(w.clone()),
                ValueNode::Function { nodes, .. } => collect_words(nodes, out),
                _ => {}
            }
        }
    }
    visit(&value_parser::parse(raw), &mut variables);
    variables
}
