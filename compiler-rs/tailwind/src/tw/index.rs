//! Port of `tailwindcss/src/index.ts` (`compile` / `build`) with an embedded
//! stylesheet loader for `@import "tailwindcss"`.

use crate::tw::apply::substitute_at_apply;
use crate::tw::ast::{at_root, at_rule, comment, context, decl, style_rule, to_css, walk, AstNode, Ctx, WalkAction};
use crate::tw::compile::compile_candidates;
use crate::tw::css_functions::substitute_functions;
use crate::tw::css_parser;
use crate::tw::design_system::DesignSystem;
use crate::tw::optimize::{optimize_ast, polyfills};
use crate::tw::plugins;
use crate::tw::theme::{opts, Theme};
use crate::tw::utilities::create_css_utility;
use crate::tw::utils::{escape, expand, segment, unescape};
use crate::tw::value_parser::{self, ValueNode};
use crate::tw::variants::{compounds_for_selectors, is_valid_variant_name, substitute_at_variant};
use indexmap::{IndexMap, IndexSet};
use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

pub const VERSION: &str = "4.3.3";

const INDEX_CSS: &str = include_str!("assets/index.css");
const THEME_CSS: &str = include_str!("assets/theme.css");
const PREFLIGHT_CSS: &str = include_str!("assets/preflight.css");
const UTILITIES_CSS: &str = include_str!("assets/utilities.css");

/// Same CSS the Bun worker (`worker/engine.ts`) feeds into Tailwind.
const WORKER_BASE_CSS: &str = include_str!("assets/worker_base.css");
const WORKER_BASE_CSS_SUFFIX: &str = include_str!("assets/worker_base_suffix.css");

pub fn worker_base_css(custom_css: &str) -> String {
    let mut s = String::with_capacity(WORKER_BASE_CSS.len() + custom_css.len() + 8);
    s.push_str(WORKER_BASE_CSS);
    s.push_str(custom_css);
    s.push_str(WORKER_BASE_CSS_SUFFIX);
    hoist_tailwind_imports(&s)
}

static TAILWIND_IMPORT: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
    regex::Regex::new(r#"@import\s+(?:"tailwindcss(?:/[^"]*)?"|'tailwindcss(?:/[^']*)?')[^;]*;"#).unwrap()
});

/// The base stylesheet starts with `@import "tailwindcss";`. When the custom
/// CSS has its own `@import "tailwindcss…"` statements (with `prefix(tw)`,
/// `important`, `theme(static)`, `source(none)`, or only some parts like
/// `tailwindcss/utilities`), they replace the base one, at its position.
///
/// Mirrored by `hoistTailwindImports` in `worker/engine.ts`.
pub fn hoist_tailwind_imports(css: &str) -> String {
    let found: Vec<_> = TAILWIND_IMPORT.find_iter(css).collect();
    if found.len() < 2 {
        return css.to_string();
    }
    let mut user: Vec<&str> = vec![];
    for m in &found[1..] {
        if !user.contains(&m.as_str()) {
            user.push(m.as_str());
        }
    }
    let mut out = String::with_capacity(css.len());
    out.push_str(&css[..found[0].start()]);
    out.push_str(&user.join("\n"));
    let mut last = found[0].end();
    for m in &found[1..] {
        out.push_str(&css[last..m.start()]);
        last = m.end();
    }
    out.push_str(&css[last..]);
    out
}

const THEME_PLACEHOLDER: &str = "\u{0}tw-theme-root";
const SLOT_UTILITIES: &str = "@tailwind";

fn load_stylesheet(id: &str, base: &str) -> Option<(&'static str, &'static str)> {
    let normalized = if let Some(rel) = id.strip_prefix("./") {
        if base == "tailwindcss" {
            format!("tailwindcss/{}", rel)
        } else {
            return None;
        }
    } else {
        id.to_string()
    };
    let css = match normalized.as_str() {
        "tailwindcss" | "tailwindcss/index.css" | "tailwindcss/index" => INDEX_CSS,
        "tailwindcss/theme.css" | "tailwindcss/theme" => THEME_CSS,
        "tailwindcss/preflight.css" | "tailwindcss/preflight" => PREFLIGHT_CSS,
        "tailwindcss/utilities.css" | "tailwindcss/utilities" => UTILITIES_CSS,
        _ => return None,
    };
    Some((css, "tailwindcss"))
}

struct ImportParams {
    uri: String,
    layer: Option<String>,
    media: Option<String>,
    supports: Option<String>,
}

fn parse_import_params(params: &[ValueNode]) -> Option<ImportParams> {
    let mut uri: Option<String> = None;
    let mut layer: Option<String> = None;
    let mut media: Option<String> = None;
    let mut supports: Option<String> = None;

    let mut i = 0;
    while i < params.len() {
        let node = &params[i];
        match node {
            ValueNode::Separator(_) => {
                i += 1;
                continue;
            }
            ValueNode::Word(w) if uri.is_none() => {
                if w.is_empty() || !(w.starts_with('"') || w.starts_with('\'')) {
                    return None;
                }
                uri = Some(w[1..w.len().saturating_sub(1).max(1)].to_string());
                i += 1;
                continue;
            }
            ValueNode::Function { value, .. } if value.eq_ignore_ascii_case("url") => return None,
            _ => {}
        }
        uri.as_ref()?;

        let name = node.value().to_ascii_lowercase();
        if name == "layer" && !matches!(node, ValueNode::Separator(_)) {
            if layer.is_some() {
                return None;
            }
            if supports.is_some() {
                return None;
            }
            layer = Some(match node {
                ValueNode::Function { nodes, .. } => value_parser::to_css(nodes),
                _ => String::new(),
            });
            i += 1;
            continue;
        }
        if let ValueNode::Function { value, nodes } = node {
            if value.eq_ignore_ascii_case("supports") {
                if supports.is_some() {
                    return None;
                }
                supports = Some(value_parser::to_css(nodes));
                i += 1;
                continue;
            }
        }

        media = Some(value_parser::to_css(&params[i..]));
        break;
    }

    Some(ImportParams { uri: uri?, layer, media, supports })
}

fn substitute_at_imports(ast: &mut Vec<AstNode>, base: &str, depth: usize) -> Result<bool, String> {
    if depth > 100 {
        return Err("Exceeded maximum recursion depth while resolving imports".into());
    }
    let mut found = false;
    let mut error: Option<String> = None;
    walk(ast, &mut |node, _| {
        let AstNode::AtRule { name, params, .. } = node else { return WalkAction::Continue };
        if name != "@import" && name != "@reference" {
            return WalkAction::Continue;
        }
        let Some(mut parsed) = parse_import_params(&value_parser::parse(params)) else {
            return WalkAction::Continue;
        };
        if name == "@reference" {
            parsed.media = Some("reference".into());
        }
        if parsed.uri.starts_with("data:") || parsed.uri.starts_with("http://") || parsed.uri.starts_with("https://") {
            return WalkAction::Continue;
        }
        let Some((css, new_base)) = load_stylesheet(&parsed.uri, base) else {
            // Unknown stylesheet: keep the `@import` as-is.
            return WalkAction::Skip;
        };
        found = true;
        let mut imported = match css_parser::parse(css) {
            Ok(a) => a,
            Err(e) => {
                error = Some(e);
                return WalkAction::Stop;
            }
        };
        if let Err(e) = substitute_at_imports(&mut imported, new_base, depth + 1) {
            error = Some(e);
            return WalkAction::Stop;
        }

        let mut root = vec![context(Ctx::default(), imported)];
        if let Some(layer) = parsed.layer {
            root = vec![at_rule("@layer", layer, root)];
        }
        if let Some(media) = parsed.media {
            root = vec![at_rule("@media", media, root)];
        }
        if let Some(supports) = parsed.supports {
            let p = if supports.starts_with('(') { supports } else { format!("({})", supports) };
            root = vec![at_rule("@supports", p, root)];
        }
        WalkAction::ReplaceSkip(vec![context(Ctx::default(), root)])
    });
    match error {
        Some(e) => Err(e),
        None => Ok(found),
    }
}

fn parse_theme_options(params: &str) -> (u8, Option<String>) {
    let mut options = opts::NONE;
    let mut prefix = None;
    for option in segment(params, b' ') {
        match option {
            "reference" => options |= opts::REFERENCE,
            "inline" => options |= opts::INLINE,
            "default" => options |= opts::DEFAULT,
            "static" => options |= opts::STATIC,
            o if o.starts_with("prefix(") && o.ends_with(')') => prefix = Some(o[7..o.len() - 1].to_string()),
            _ => {}
        }
    }
    (options, prefix)
}

enum CustomVariant {
    Selector { style: Vec<String>, at: Vec<String> },
    Body(Vec<AstNode>),
}

pub struct Compiler {
    ds: DesignSystem,
    ast: Vec<AstNode>,
    has_utilities: bool,
    has_features: bool,
    inline_candidates: Vec<String>,
    initial_invalid: HashSet<String>,
    initial_theme_options: Vec<u8>,
}

pub fn compile(css: &str) -> Result<Compiler, String> {
    let mut ast = css_parser::parse(css)?;
    let mut has_features = substitute_at_imports(&mut ast, "", 0)?;

    let mut theme = Theme::new();
    let mut important = false;
    let mut custom_variants: IndexMap<String, CustomVariant> = IndexMap::new();
    let mut custom_variant_deps: IndexMap<String, Vec<String>> = IndexMap::new();
    let mut custom_utilities: Vec<Rc<RefCell<AstNode>>> = vec![];
    let mut first_theme = false;
    let mut has_utilities = false;
    let mut inline_candidates: Vec<String> = vec![];
    let mut ignored_candidates: Vec<String> = vec![];
    let mut plugin_refs: Vec<plugins::PluginRef> = vec![];
    let mut error: Option<String> = None;

    walk(&mut ast, &mut |node, ctx| {
        let AstNode::AtRule { name, params, nodes } = node else { return WalkAction::Continue };

        macro_rules! fail {
            ($($arg:tt)*) => {{
                error = Some(format!($($arg)*));
                return WalkAction::Stop;
            }};
        }

        if name == "@tailwind" && (params == "utilities" || params.starts_with("utilities")) {
            if has_utilities || ctx.context.reference {
                return WalkAction::Replace(vec![]);
            }
            has_utilities = true;
            has_features = true;
            *params = "utilities".into();
            return WalkAction::Skip;
        }

        if name == "@utility" {
            if ctx.has_parent() {
                fail!("`@utility` cannot be nested.");
            }
            if nodes.is_empty() {
                fail!("`@utility {}` is empty. Utilities should include at least one property.", params);
            }
            custom_utilities.push(Rc::new(RefCell::new(node.clone())));
            return WalkAction::ReplaceSkip(vec![]);
        }

        if name == "@source" {
            if !nodes.is_empty() {
                fail!("`@source` cannot have a body.");
            }
            if ctx.has_parent() {
                fail!("`@source` cannot be nested.");
            }
            let mut not = false;
            let mut inline = false;
            let mut path = params.as_str();
            if let Some(p) = path.strip_prefix("not ") {
                not = true;
                path = p;
            }
            if path.starts_with("inline(") && path.ends_with(')') {
                inline = true;
                path = path[7..path.len() - 1].trim();
            }
            let pb = path.as_bytes();
            if pb.is_empty() || (pb[0] != b'"' && pb[0] != b'\'') || pb[pb.len() - 1] != pb[0] {
                fail!("`@source` paths must be quoted.");
            }
            if inline {
                let source = &path[1..path.len() - 1];
                let dest = if not { &mut ignored_candidates } else { &mut inline_candidates };
                for s in segment(source, b' ') {
                    dest.extend(expand(s));
                }
            }
            return WalkAction::ReplaceSkip(vec![]);
        }

        if name == "@variant" && !ctx.has_parent() {
            if nodes.is_empty() {
                *name = "@custom-variant".into();
            } else {
                let mut has_slot = false;
                crate::tw::ast::walk_ref(nodes, &mut |c| {
                    if c.at_rule_name() == Some("@slot") {
                        has_slot = true;
                        return WalkAction::Stop;
                    }
                    WalkAction::Continue
                });
                if has_slot {
                    *name = "@custom-variant".into();
                }
            }
        }

        if name == "@custom-variant" {
            if ctx.has_parent() {
                fail!("`@custom-variant` cannot be nested.");
            }
            let parts = segment(params, b' ');
            let variant_name = parts[0].to_string();
            let selector = parts.get(1).map(|s| s.to_string());
            if !is_valid_variant_name(&variant_name) {
                fail!("`@custom-variant {}` defines an invalid variant name.", variant_name);
            }
            if !nodes.is_empty() && selector.is_some() {
                fail!("`@custom-variant {}` cannot have both a selector and a body.", variant_name);
            }
            if nodes.is_empty() {
                let Some(selector) = selector else {
                    fail!("`@custom-variant {}` has no selector or body.", variant_name);
                };
                let inner = if selector.len() >= 2 { &selector[1..selector.len() - 1] } else { "" };
                let selectors = segment(inner, b',');
                if selectors.is_empty() || selectors.iter().any(|s| s.trim().is_empty()) {
                    fail!("`@custom-variant {} ({})` selector is invalid.", variant_name, selectors.join(","));
                }
                let mut at = vec![];
                let mut style = vec![];
                for s in selectors {
                    let s = s.trim();
                    if s.starts_with('@') {
                        at.push(s.to_string());
                    } else {
                        style.push(s.to_string());
                    }
                }
                custom_variants.insert(variant_name.clone(), CustomVariant::Selector { style, at });
                custom_variant_deps.insert(variant_name, vec![]);
            } else {
                let mut deps = vec![];
                crate::tw::ast::walk_ref(nodes, &mut |c| {
                    if let AstNode::AtRule { name, params, .. } = c {
                        if name == "@variant" {
                            deps.push(params.clone());
                        }
                    }
                    WalkAction::Continue
                });
                custom_variants.insert(variant_name.clone(), CustomVariant::Body(nodes.clone()));
                custom_variant_deps.insert(variant_name, deps);
            }
            return WalkAction::ReplaceSkip(vec![]);
        }

        if name == "@plugin" {
            if ctx.has_parent() {
                fail!("`@plugin` cannot be nested.");
            }
            let id = if params.len() >= 2 { params[1..params.len() - 1].to_string() } else { String::new() };
            if id.is_empty() {
                fail!("`@plugin` must have a path.");
            }
            let options = match plugins::parse_options(nodes) {
                Ok(o) => o,
                Err(e) => fail!("{}", e),
            };
            plugin_refs.push(plugins::PluginRef {
                id,
                options: (!options.is_empty()).then_some(options),
                reference: ctx.context.reference,
            });
            has_features = true;
            return WalkAction::Replace(vec![]);
        }

        if name == "@config" {
            // JS configs are not supported by the native compiler.
            return WalkAction::Replace(vec![]);
        }

        if name == "@media" {
            let parts: Vec<String> = segment(params, b' ').iter().map(|s| s.to_string()).collect();
            let mut unknown: Vec<String> = vec![];
            for param in &parts {
                if param.starts_with("source(") {
                    // Source detection is handled by the scanner.
                } else if param.starts_with("theme(") {
                    let theme_params = param[6..param.len() - 1].to_string();
                    walk(nodes, &mut |child, _| {
                        if let AstNode::AtRule { name, params, .. } = child {
                            if name == "@theme" {
                                params.push(' ');
                                params.push_str(&theme_params);
                                return WalkAction::Skip;
                            }
                        }
                        WalkAction::Continue
                    });
                } else if param.starts_with("prefix(") {
                    let prefix = param[7..param.len() - 1].to_string();
                    walk(nodes, &mut |child, _| {
                        if let AstNode::AtRule { name, params, .. } = child {
                            if name == "@theme" {
                                params.push_str(&format!(" prefix({})", prefix));
                                return WalkAction::Skip;
                            }
                        }
                        WalkAction::Continue
                    });
                } else if param == "important" {
                    important = true;
                } else if param == "reference" {
                    let inner = std::mem::take(nodes);
                    *nodes = vec![context(Ctx { reference: true, ..Default::default() }, inner)];
                } else {
                    unknown.push(param.clone());
                }
            }
            if !unknown.is_empty() {
                *params = unknown.join(" ");
            } else if !parts.is_empty() {
                return WalkAction::Replace(std::mem::take(nodes));
            }
            return WalkAction::Continue;
        }

        if name == "@theme" {
            let (mut theme_options, theme_prefix) = parse_theme_options(params);
            has_features = true;
            if ctx.context.reference {
                theme_options |= opts::REFERENCE;
            }
            if let Some(p) = theme_prefix {
                if p.is_empty() || !p.bytes().all(|b| b.is_ascii_lowercase()) {
                    fail!("The prefix \"{}\" is invalid. Prefixes must be lowercase ASCII letters (a-z) only.", p);
                }
                theme.prefix = Some(p);
            }

            for child in nodes.iter() {
                match child {
                    AstNode::AtRule { name, .. } if name == "@keyframes" => theme.add_keyframes(child.clone()),
                    AstNode::Comment(_) => {}
                    AstNode::Decl { property, value, .. } if property.starts_with("--") => {
                        if let Err(e) = theme.add(&unescape(property), value.as_deref().unwrap_or(""), theme_options) {
                            fail!("{}", e);
                        }
                    }
                    _ => fail!("`@theme` blocks must only contain custom properties or `@keyframes`."),
                }
            }

            if !first_theme {
                first_theme = true;
                return WalkAction::ReplaceSkip(vec![style_rule(THEME_PLACEHOLDER, vec![])]);
            }
            return WalkAction::ReplaceSkip(vec![]);
        }

        WalkAction::Continue
    });

    if let Some(e) = error {
        return Err(e);
    }

    let mut ds = DesignSystem::new(theme);
    ds.important = important;
    for c in &ignored_candidates {
        ds.invalid_candidates.borrow_mut().insert(c.clone());
    }

    // `@plugin`s (upstream: `applyCompatibilityHooks`, before custom variants
    // and utilities are registered).
    plugins::run_plugins(&plugin_refs, &mut ds, &mut ast)?;

    // Pre-register custom variants so their order follows the CSS.
    for name in custom_variants.keys() {
        ds.variants.static_(name, Rc::new(|_, _, _| true), None);
    }

    // Register custom variants in dependency order.
    let order = topological_sort(&custom_variant_deps)?;
    for name in order {
        let Some(variant) = custom_variants.get(&name) else { continue };
        match variant {
            CustomVariant::Selector { style, at } => {
                let style = style.clone();
                let at = at.clone();
                let mut all: Vec<&str> = style.iter().map(|s| s.as_str()).collect();
                all.extend(at.iter().map(|s| s.as_str()));
                let compounds = compounds_for_selectors(&all);
                ds.variants.static_(
                    &name,
                    Rc::new(move |r, _, _| {
                        let children = std::mem::take(r.nodes_mut().unwrap());
                        let mut nodes = vec![];
                        if !style.is_empty() {
                            nodes.push(style_rule(style.join(", "), children.clone()));
                        }
                        for s in &at {
                            nodes.push(crate::tw::ast::rule(s.as_str(), children.clone()));
                        }
                        *r.nodes_mut().unwrap() = nodes;
                        true
                    }),
                    Some(compounds),
                );
            }
            CustomVariant::Body(body) => {
                ds.variants.from_ast(&name, body.clone());
            }
        }
    }

    for node in &custom_utilities {
        let params = match &*node.borrow() {
            AstNode::AtRule { params, .. } => params.clone(),
            _ => String::new(),
        };
        match create_css_utility(node.clone()) {
            Some(register) => register(&mut ds.utilities),
            None => {
                if !params.ends_with("-*") && params.contains('*') {
                    return Err(format!(
                        "`@utility {}` defines an invalid utility name. A functional utility must end in `-*`.",
                        params
                    ));
                }
                return Err(format!(
                    "`@utility {}` defines an invalid utility name. Utilities should be alphanumeric and start with a lowercase letter.",
                    params
                ));
            }
        }
    }

    // Output the final set of theme variables at the first `@theme`.
    if first_theme {
        let mut decls = vec![];
        for (key, value) in ds.theme.entries() {
            if value.options.get() & opts::REFERENCE != 0 {
                continue;
            }
            decls.push(decl(escape(&key), value.value.clone()));
        }
        for kf in ds.theme.get_keyframes() {
            ast.push(context(Ctx { theme: true, ..Default::default() }, vec![at_root(vec![kf.clone()])]));
        }
        let theme_rule = style_rule(":root, :host", vec![context(Ctx { theme: true, ..Default::default() }, decls)]);
        replace_placeholder(&mut ast, theme_rule);
    }

    substitute_at_variant(&mut ast, &ds)?;
    substitute_functions(&mut ast, &ds)?;
    if substitute_at_apply(&mut ast, &custom_utilities, &ds)? {
        has_features = true;
    }

    ast.insert(0, comment(format!("! tailwindcss v{} | MIT License | https://tailwindcss.com ", VERSION)));

    let initial_invalid = ds.invalid_candidates.borrow().clone();
    let initial_theme_options = ds.theme.snapshot_options();

    Ok(Compiler { ds, ast, has_utilities, has_features, inline_candidates, initial_invalid, initial_theme_options })
}

fn replace_placeholder(ast: &mut Vec<AstNode>, replacement: AstNode) {
    let mut replacement = Some(replacement);
    walk(ast, &mut |node, _| {
        if let AstNode::Rule { selector, .. } = node {
            if selector == THEME_PLACEHOLDER {
                if let Some(r) = replacement.take() {
                    return WalkAction::ReplaceStop(vec![r]);
                }
            }
        }
        WalkAction::Continue
    });
}

fn topological_sort(graph: &IndexMap<String, Vec<String>>) -> Result<Vec<String>, String> {
    let mut seen = HashSet::new();
    let mut wip = HashSet::new();
    let mut sorted = vec![];

    fn visit(
        node: &str,
        graph: &IndexMap<String, Vec<String>>,
        seen: &mut HashSet<String>,
        wip: &mut HashSet<String>,
        sorted: &mut Vec<String>,
    ) -> Result<(), String> {
        if !graph.contains_key(node) || seen.contains(node) {
            return Ok(());
        }
        if wip.contains(node) {
            return Err(format!("Circular dependency detected in custom variants: {}", node));
        }
        wip.insert(node.to_string());
        for dep in &graph[node] {
            visit(dep, graph, seen, wip, sorted)?;
        }
        seen.insert(node.to_string());
        wip.remove(node);
        sorted.push(node.to_string());
        Ok(())
    }

    for key in graph.keys() {
        visit(key, graph, &mut seen, &mut wip, &mut sorted)?;
    }
    Ok(sorted)
}

impl Compiler {
    pub fn design_system(&self) -> &DesignSystem {
        &self.ds
    }

    fn reset(&self) {
        self.ds.reset_tracking();
        *self.ds.invalid_candidates.borrow_mut() = self.initial_invalid.clone();
        // Only the USED bits change during a build.
        self.ds.theme.restore_options(&self.initial_theme_options);
    }

    /// Generate the CSS for the given candidates. Every call starts from a
    /// clean state (unlike upstream, builds are not cumulative).
    pub fn build(&self, candidates: &[String]) -> String {
        to_css(&self.build_ast(candidates))
    }

    pub fn build_ast(&self, candidates: &[String]) -> Vec<AstNode> {
        self.reset();
        let ds = &self.ds;
        let mut ast = self.ast.clone();

        if self.has_features && self.has_utilities {
            let mut all: IndexSet<String> = IndexSet::new();
            for c in &self.inline_candidates {
                if !ds.invalid_candidates.borrow().contains(c) {
                    all.insert(c.clone());
                }
            }
            for c in candidates {
                if ds.invalid_candidates.borrow().contains(c) {
                    continue;
                }
                if c.starts_with("--") {
                    ds.theme.mark_used_variable(c);
                } else {
                    all.insert(c.clone());
                }
            }
            let all: Vec<String> = all.into_iter().collect();
            let mut invalid = vec![];
            let nodes: Vec<AstNode> = compile_candidates(&all, ds, true, &mut |c| invalid.push(c.to_string()))
                .into_iter()
                .map(|(n, _)| n)
                .collect();
            ds.invalid_candidates.borrow_mut().extend(invalid);

            let mut nodes = Some(nodes);
            walk(&mut ast, &mut |node, _| {
                if let AstNode::AtRule { name, params, .. } = node {
                    if name == SLOT_UTILITIES && params == "utilities" {
                        let n = nodes.take().unwrap_or_default();
                        return WalkAction::ReplaceStop(vec![context(Ctx::default(), n)]);
                    }
                }
                WalkAction::Continue
            });
        }

        optimize_ast(ast, ds, polyfills::ALL)
    }
}
