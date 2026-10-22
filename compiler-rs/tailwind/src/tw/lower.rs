//! CSS nesting lowering + minified printing.
//!
//! Upstream Tailwind emits nested CSS and relies on Lightning CSS (in
//! `@tailwindcss/node`'s `optimize`) to flatten it for its browser targets
//! (Safari 16.4, Chrome 111, Firefox 128). This module performs the same
//! flattening on our AST:
//!
//! - `&` in a nested selector is replaced by the parent selector (wrapped in
//!   `:is(…)` when the parent is a selector list), a nested selector without
//!   `&` becomes a descendant selector.
//! - Conditional group rules (`@media`, `@supports`, `@container`, …) nested in
//!   a style rule are hoisted around the (resolved) style rule.
//! - Declaration order relative to nested rules is preserved.

use crate::tw::ast::AstNode;
use crate::tw::utils::segment;

/// At-rules whose bodies may contain style rules/declarations that need to
/// be lowered.
fn is_group_rule(name: &str) -> bool {
    matches!(
        name,
        "@media" | "@supports" | "@container" | "@layer" | "@starting-style" | "@scope" | "@document"
    )
}

/// Replace every `&` that is not escaped or inside a string.
fn replace_nesting(selector: &str, parent: &str) -> String {
    let mut out = String::with_capacity(selector.len() + parent.len());
    let bytes = selector.as_bytes();
    let mut i = 0;
    let mut start = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            q @ (b'"' | b'\'') => {
                i += 1;
                while i < bytes.len() && bytes[i] != q {
                    if bytes[i] == b'\\' {
                        i += 1;
                    }
                    i += 1;
                }
                i += 1;
            }
            b'&' => {
                out.push_str(&selector[start..i]);
                out.push_str(parent);
                i += 1;
                start = i;
            }
            _ => i += 1,
        }
    }
    out.push_str(&selector[start.min(selector.len())..]);
    out
}

fn has_nesting(selector: &str) -> bool {
    let mut probe = replace_nesting(selector, "\u{0}");
    let found = probe.contains('\u{0}');
    probe.clear();
    found
}

pub fn resolve_selector(selector: &str, parent: Option<&str>) -> String {
    let Some(parent) = parent else {
        // Top-level `&` refers to the scoping root.
        return if has_nesting(selector) { replace_nesting(selector, ":scope") } else { selector.to_string() };
    };

    let parent_parts = segment(parent, b',');
    let parent_ref = if parent_parts.len() > 1 { format!(":is({})", parent) } else { parent.to_string() };

    segment(selector, b',')
        .iter()
        .map(|part| {
            let part = part.trim();
            if has_nesting(part) {
                replace_nesting(part, &parent_ref)
            } else {
                format!("{} {}", parent_ref, part)
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Flatten nested style rules. `parent` is the resolved selector of the
/// enclosing style rule (if any).
pub fn lower(nodes: Vec<AstNode>, parent: Option<&str>) -> Vec<AstNode> {
    let mut out = Vec::with_capacity(nodes.len());
    let mut pending_decls: Vec<AstNode> = vec![];

    let flush = |pending: &mut Vec<AstNode>, out: &mut Vec<AstNode>| {
        if pending.is_empty() {
            return;
        }
        let decls = std::mem::take(pending);
        match parent {
            Some(p) => out.push(AstNode::Rule { selector: p.to_string(), nodes: decls }),
            // Declarations without a parent (invalid at top-level): keep them.
            None => out.extend(decls),
        }
    };

    for node in nodes {
        match node {
            AstNode::Decl { .. } => pending_decls.push(node),
            AstNode::Rule { selector, nodes } => {
                flush(&mut pending_decls, &mut out);
                let resolved = resolve_selector(&selector, parent);
                out.extend(lower(nodes, Some(&resolved)));
            }
            AstNode::AtRule { name, params, nodes } if is_group_rule(&name) && !nodes.is_empty() => {
                flush(&mut pending_decls, &mut out);
                let inner = lower(nodes, parent);
                if !inner.is_empty() {
                    out.push(AstNode::AtRule { name, params, nodes: inner });
                }
            }
            AstNode::Context { nodes, .. } | AstNode::AtRoot(nodes) => {
                flush(&mut pending_decls, &mut out);
                out.extend(lower(nodes, parent));
            }
            other => {
                // `@keyframes`, `@property`, `@font-face`, comments, statements…
                flush(&mut pending_decls, &mut out);
                out.push(other);
            }
        }
    }
    flush(&mut pending_decls, &mut out);

    // Merge adjacent rules with the same selector inside the same block.
    let mut merged: Vec<AstNode> = Vec::with_capacity(out.len());
    for node in out {
        if let (Some(AstNode::Rule { selector: prev_sel, nodes: prev_nodes }), AstNode::Rule { selector, nodes }) =
            (merged.last_mut(), &node)
        {
            if prev_sel == selector && prev_nodes.iter().all(|n| matches!(n, AstNode::Decl { .. })) && nodes.iter().all(|n| matches!(n, AstNode::Decl { .. })) {
                prev_nodes.extend(nodes.iter().cloned());
                continue;
            }
        }
        merged.push(node);
    }
    merged
}

/// Append `s` to `out`, collapsing whitespace outside of strings.
fn write_compact(out: &mut String, s: &str) {
    let s = s.trim();
    // Fast path: nothing to collapse.
    if !s.contains(|c: char| c == '\n' || c == '\t' || c == '\r') && !s.contains("  ") {
        out.push_str(s);
        return;
    }
    let mut chars = s.chars();
    let mut quote: Option<char> = None;
    let mut prev_space = false;
    while let Some(c) = chars.next() {
        if let Some(q) = quote {
            out.push(c);
            if c == '\\' {
                if let Some(n) = chars.next() {
                    out.push(n);
                }
            } else if c == q {
                quote = None;
            }
            continue;
        }
        if c == '"' || c == '\'' {
            quote = Some(c);
            prev_space = false;
            out.push(c);
            continue;
        }
        if c == '\\' {
            out.push(c);
            if let Some(n) = chars.next() {
                out.push(n);
            }
            prev_space = false;
            continue;
        }
        if c.is_whitespace() {
            if !prev_space {
                out.push(' ');
            }
            prev_space = true;
            continue;
        }
        prev_space = false;
        out.push(c);
    }
}

/// Write a selector, removing spaces after top-level commas.
fn write_selector(out: &mut String, sel: &str) {
    if !sel.contains(',') {
        return write_compact(out, sel);
    }
    for (i, part) in segment(sel, b',').iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        write_compact(out, part);
    }
}

pub fn to_minified_css(ast: &[AstNode]) -> String {
    let mut out = String::with_capacity(16 * 1024);
    for node in ast {
        print_min(node, &mut out);
    }
    out
}

fn print_min(node: &AstNode, out: &mut String) {
    match node {
        AstNode::Decl { property, value, important } => {
            out.push_str(property);
            out.push(':');
            let before = out.len();
            if let Some(v) = value {
                write_compact(out, v);
            }
            if out.len() == before {
                // `--x:;` is rejected by older engines; keep the guaranteed-
                // invalid (empty) value as a single space like Lightning CSS.
                out.push(' ');
            }
            if *important {
                out.push_str("!important");
            }
            out.push(';');
        }
        AstNode::Rule { selector, nodes } => {
            write_selector(out, selector);
            out.push('{');
            print_block(nodes, out);
            out.push('}');
        }
        AstNode::AtRule { name, params, nodes } => {
            out.push_str(name);
            if !params.trim().is_empty() {
                out.push(' ');
                write_compact(out, params);
            }
            if nodes.is_empty() {
                out.push(';');
                return;
            }
            out.push('{');
            print_block(nodes, out);
            out.push('}');
        }
        AstNode::Comment(v) => {
            // Only keep license comments.
            if v.starts_with('!') {
                out.push_str("/*");
                out.push_str(v);
                out.push_str("*/");
            }
        }
        AstNode::Context { nodes, .. } | AstNode::AtRoot(nodes) => {
            for n in nodes {
                print_min(n, out);
            }
        }
    }
}

fn print_block(nodes: &[AstNode], out: &mut String) {
    for (i, n) in nodes.iter().enumerate() {
        print_min(n, out);
        // Drop the trailing `;` of the last declaration in a block.
        if i == nodes.len() - 1 && matches!(n, AstNode::Decl { .. }) && out.ends_with(';') {
            out.pop();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tw::css_parser::parse;

    fn lower_css(css: &str) -> String {
        to_minified_css(&lower(parse(css).unwrap(), None))
    }

    #[test]
    fn flattens_nesting() {
        assert_eq!(
            lower_css(".hover\\:x { &:hover { @media (hover: hover) { color: red; } } }"),
            "@media (hover: hover){.hover\\:x:hover{color:red}}"
        );
        assert_eq!(lower_css(".a, .b { color: red; & > p { color: blue } }"), ".a,.b{color:red}:is(.a, .b) > p{color:blue}");
        assert_eq!(lower_css(".x { :where(.dark, .dark *) & { color: red } }"), ":where(.dark, .dark *) .x{color:red}");
        assert_eq!(lower_css(".x { color: red; .y { color: blue } color: green }"), ".x{color:red}.x .y{color:blue}.x{color:green}");
        assert_eq!(lower_css(".x { content: \" : \"; }"), ".x{content:\" : \"}");
    }
}
