//! Port of `tailwindcss/src/css-functions.ts`.

use crate::tw::ast::{walk, AstNode, WalkAction};
use crate::tw::design_system::DesignSystem;
use crate::tw::utilities::with_alpha;
use crate::tw::utils::segment;
use crate::tw::value_parser::{self, ValueNode};

const FUNCTION_NAMES: [&str; 4] = ["--alpha", "--spacing", "--theme", "theme"];

pub fn has_theme_function(value: &str) -> bool {
    // `--theme(` is covered by `theme(`.
    value.contains("theme(") || value.contains("--alpha(") || value.contains("--spacing(")
}

#[derive(Clone, Copy)]
enum Source {
    Decl,
    AtRule,
}

pub fn substitute_functions(ast: &mut Vec<AstNode>, ds: &DesignSystem) -> Result<bool, String> {
    let mut found = false;
    let mut error: Option<String> = None;
    walk(ast, &mut |node, _| {
        match node {
            AstNode::Decl { value: Some(value), .. } if has_theme_function(value) => {
                found = true;
                match substitute_functions_in_value(value, Source::Decl, ds) {
                    Ok(v) => *value = v,
                    Err(e) => {
                        error = Some(e);
                        return WalkAction::Stop;
                    }
                }
            }
            AstNode::AtRule { name, params, .. }
                if matches!(name.as_str(), "@media" | "@custom-media" | "@container" | "@supports")
                    && has_theme_function(params) =>
            {
                found = true;
                match substitute_functions_in_value(params, Source::AtRule, ds) {
                    Ok(v) => *params = v,
                    Err(e) => {
                        error = Some(e);
                        return WalkAction::Stop;
                    }
                }
            }
            _ => {}
        }
        WalkAction::Continue
    });
    match error {
        Some(e) => Err(e),
        None => Ok(found),
    }
}

fn substitute_functions_in_value(value: &str, source: Source, ds: &DesignSystem) -> Result<String, String> {
    let mut ast = value_parser::parse(value);
    process(&mut ast, source, ds)?;
    Ok(value_parser::to_css(&ast))
}

fn process(nodes: &mut Vec<ValueNode>, source: Source, ds: &DesignSystem) -> Result<(), String> {
    let mut i = 0;
    while i < nodes.len() {
        let replacement = match &nodes[i] {
            ValueNode::Function { value: name, nodes: children } if FUNCTION_NAMES.contains(&name.as_str()) => {
                let joined = value_parser::to_css(children);
                let args: Vec<String> = segment(joined.trim(), b',').iter().map(|x| x.trim().to_string()).collect();
                let result = call(name, &args, source, ds)?;
                Some(value_parser::parse(&result))
            }
            _ => None,
        };
        match replacement {
            Some(rep) => {
                // Upstream re-visits replaced nodes.
                nodes.splice(i..i + 1, rep);
            }
            None => {
                if let ValueNode::Function { nodes: children, .. } = &mut nodes[i] {
                    process(children, source, ds)?;
                }
                i += 1;
            }
        }
    }
    Ok(())
}

fn call(name: &str, args: &[String], source: Source, ds: &DesignSystem) -> Result<String, String> {
    let first = args.first().map(|s| s.as_str()).unwrap_or("");
    let rest = if args.len() > 1 { &args[1..] } else { &[][..] };
    match name {
        "--alpha" => {
            let parts: Vec<String> = segment(first, b'/').iter().map(|v| v.trim().to_string()).collect();
            let color = parts.first().cloned().unwrap_or_default();
            let alpha = parts.get(1).cloned().unwrap_or_default();
            if color.is_empty() || alpha.is_empty() {
                return Err("The --alpha(…) function requires a color and an alpha value".into());
            }
            if !rest.is_empty() {
                return Err("The --alpha(…) function only accepts one argument".into());
            }
            Ok(with_alpha(&color, &alpha))
        }
        "--spacing" => {
            if first.is_empty() {
                return Err("The --spacing(…) function requires an argument, but received none.".into());
            }
            if !rest.is_empty() {
                return Err(format!(
                    "The --spacing(…) function only accepts a single argument, but received {}.",
                    rest.len() + 1
                ));
            }
            let multiplier = ds.theme.resolve(None, &["--spacing"]).ok_or_else(|| {
                "The --spacing(…) function requires that the `--spacing` theme variable exists, but it was not found."
                    .to_string()
            })?;
            // `--spacing` is a non-zero `<length>`: `0` → `0px` (stays a
            // `<length>` inside `calc(…)`), `1` → the multiplier itself.
            if let Some((n, _)) = crate::tw::utils::dimensions(first) {
                if n == 0.0 {
                    return Ok("0px".into());
                }
                if n == 1.0 {
                    return Ok(multiplier);
                }
            }
            Ok(format!("calc({} * {})", multiplier, first))
        }
        "--theme" => {
            let mut path = first.to_string();
            if !path.starts_with("--") {
                return Err("The --theme(…) function can only be used with CSS variables from your theme.".into());
            }
            let mut inline = false;
            if path.ends_with(" inline") {
                inline = true;
                path.truncate(path.len() - 7);
            }
            if matches!(source, Source::AtRule) {
                inline = true;
            }
            let resolved = ds.resolve_theme_value(&path, inline);
            let Some(resolved) = resolved.filter(|v| !v.is_empty()) else {
                if !rest.is_empty() {
                    return Ok(rest.join(", "));
                }
                return Err(format!("Could not resolve value for theme function: `theme({})`.", path));
            };
            if rest.is_empty() {
                return Ok(resolved);
            }
            let joined = rest.join(", ");
            if joined == "initial" {
                return Ok(resolved);
            }
            if resolved == "initial" {
                return Ok(joined);
            }
            if resolved.starts_with("var(") || resolved.starts_with("theme(") || resolved.starts_with("--theme(") {
                let mut ast = value_parser::parse(&resolved);
                inject_fallback(&mut ast, &joined);
                return Ok(value_parser::to_css(&ast));
            }
            Ok(resolved)
        }
        "theme" => {
            let path = eventually_unquote(first);
            let resolved = resolve_legacy_theme_value(&path, ds);
            match resolved.filter(|v| !v.is_empty()) {
                Some(v) => Ok(v),
                None if !rest.is_empty() => Ok(rest.join(", ")),
                None => Err(format!("Could not resolve value for theme function: `theme({})`.", path)),
            }
        }
        _ => Ok(String::new()),
    }
}

/// `theme(…)` supports CSS variables and legacy (v3) dot paths.
fn resolve_legacy_theme_value(path: &str, ds: &DesignSystem) -> Option<String> {
    if path.starts_with("--") {
        return ds.resolve_theme_value(path, true);
    }
    crate::tw::compat_theme::resolve_legacy_theme_value(path, ds)
}

fn eventually_unquote(value: &str) -> String {
    let b = value.as_bytes();
    if b.is_empty() || (b[0] != b'\'' && b[0] != b'"') {
        return value.to_string();
    }
    let quote = b[0] as char;
    let chars: Vec<char> = value.chars().collect();
    let mut out = String::new();
    let mut i = 1;
    while i + 1 < chars.len() {
        let c = chars[i];
        let next = chars[i + 1];
        if c == '\\' && (next == quote || next == '\\') {
            out.push(next);
            i += 2;
        } else {
            out.push(c);
            i += 1;
        }
    }
    out
}

fn inject_fallback(ast: &mut Vec<ValueNode>, fallback: &str) {
    for node in ast.iter_mut() {
        if let ValueNode::Function { value, nodes } = node {
            if value == "var" || value == "theme" || value == "--theme" {
                if nodes.len() == 1 {
                    nodes.push(ValueNode::Word(format!(", {}", fallback)));
                } else if let Some(ValueNode::Word(last)) = nodes.last_mut() {
                    if last == "initial" {
                        *last = fallback.to_string();
                    }
                }
            }
            inject_fallback(nodes, fallback);
        }
    }
}
