//! Port of `tailwindcss/src/utilities.ts`.
//!
//! The upstream file registers utilities in a single giant function. The
//! registration order matters (several utilities share a root), so the parts
//! below must be called in the same order as upstream.

mod part1;
mod part2;
mod part3;
mod part4;
pub mod css_utility;

pub use css_utility::create_css_utility;

use crate::tw::ast::{at_rule, decl, AstNode};
use crate::tw::candidate::{Candidate, Modifier, UtilityValue};
use crate::tw::design_system::DesignSystem;
use crate::tw::theme::Theme;
use crate::tw::utils::{is_valid_opacity_value, is_valid_spacing_multiplier, js_num_to_string, js_number, segment};
use std::collections::HashMap;
use std::rc::Rc;

pub enum UResult {
    /// `undefined` upstream: this utility does not handle the candidate
    Skip,
    /// `null` upstream: stop trying other utilities
    Abort,
    Nodes(Vec<AstNode>),
}

impl From<Option<Vec<AstNode>>> for UResult {
    fn from(v: Option<Vec<AstNode>>) -> Self {
        match v {
            Some(n) => UResult::Nodes(n),
            None => UResult::Skip,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UtilityKind {
    Static,
    Functional,
}

pub type CompileFn = Rc<dyn Fn(&Candidate, &DesignSystem) -> UResult>;

#[derive(Clone)]
pub struct Utility {
    pub kind: UtilityKind,
    pub compile: CompileFn,
    pub types: Vec<String>,
}

#[derive(Clone, Default)]
pub struct Utilities {
    map: HashMap<String, Vec<Utility>>,
}

impl Utilities {
    pub fn static_(&mut self, name: &str, f: impl Fn(&Candidate, &DesignSystem) -> Option<Vec<AstNode>> + 'static) {
        self.map.entry(name.to_string()).or_default().push(Utility {
            kind: UtilityKind::Static,
            compile: Rc::new(move |c, ds| f(c, ds).into()),
            types: vec![],
        });
    }

    pub fn functional(&mut self, name: &str, f: impl Fn(&Candidate, &DesignSystem) -> Option<Vec<AstNode>> + 'static) {
        self.functional_with_types(name, f, vec![]);
    }

    pub fn functional_with_types(
        &mut self,
        name: &str,
        f: impl Fn(&Candidate, &DesignSystem) -> Option<Vec<AstNode>> + 'static,
        types: Vec<String>,
    ) {
        self.map.entry(name.to_string()).or_default().push(Utility {
            kind: UtilityKind::Functional,
            compile: Rc::new(move |c, ds| f(c, ds).into()),
            types,
        });
    }

    pub fn push_raw(&mut self, name: &str, utility: Utility) {
        self.map.entry(name.to_string()).or_default().push(utility);
    }

    pub fn has(&self, name: &str, functional: bool) -> bool {
        let kind = if functional { UtilityKind::Functional } else { UtilityKind::Static };
        self.map.get(name).is_some_and(|v| v.iter().any(|u| u.kind == kind))
    }

    pub fn get(&self, name: &str) -> &[Utility] {
        self.map.get(name).map(|v| v.as_slice()).unwrap_or(&[])
    }
}

/* -------------------------------------------------------------------------- */
/* Helpers                                                                     */
/* -------------------------------------------------------------------------- */

pub fn property(ident: &str, initial_value: Option<&str>, syntax: Option<&str>) -> AstNode {
    let mut nodes = vec![
        decl("syntax", match syntax {
            Some(s) => format!("\"{}\"", s),
            None => "\"*\"".to_string(),
        }),
        decl("inherits", "false"),
    ];
    if let Some(iv) = initial_value {
        if !iv.is_empty() {
            nodes.push(decl("initial-value", iv));
        }
    }
    at_rule("@property", ident, nodes)
}

fn alpha_to_percentage(alpha: &str) -> String {
    match js_number(alpha) {
        Some(n) => format!("{}%", js_num_to_string(n * 100.0)),
        None => alpha.to_string(),
    }
}

/// Apply opacity to a color using `color-mix`.
pub fn with_alpha(value: &str, alpha: &str) -> String {
    let alpha = alpha_to_percentage(alpha);
    if alpha == "100%" {
        return value.to_string();
    }
    format!("color-mix(in oklab, {} {}, transparent)", value, alpha)
}

/// Replace the opacity in a color using relative color syntax.
pub fn replace_alpha(value: &str, alpha: &str) -> String {
    let alpha = alpha_to_percentage(alpha);
    format!("oklab(from {} l a b / {})", value, alpha)
}

/// Resolve a color value + optional opacity modifier to a final color.
pub fn as_color(value: &str, modifier: Option<&Modifier>, theme: &Theme) -> Option<String> {
    let Some(modifier) = modifier else { return Some(value.to_string()) };
    match modifier {
        Modifier::Arbitrary(v) => Some(with_alpha(value, v)),
        Modifier::Named(v) => {
            if let Some(alpha) = theme.resolve(Some(v), &["--opacity"]) {
                return Some(with_alpha(value, &alpha));
            }
            if !is_valid_opacity_value(v) {
                return None;
            }
            Some(with_alpha(value, &format!("{}%", v)))
        }
    }
}

/// Finds a color in the theme. `inherit`, `transparent` and `current` are
/// special cased.
pub fn resolve_theme_color(candidate: &Candidate, theme: &Theme, theme_keys: &[&str]) -> Option<String> {
    let v = candidate.value()?.value();
    let value = match v {
        "inherit" => Some("inherit".to_string()),
        "transparent" => Some("transparent".to_string()),
        "current" => Some("currentcolor".to_string()),
        _ => theme.resolve(Some(v), theme_keys),
    }?;
    as_color(&value, candidate.modifier.as_ref(), theme)
}

/// The named value of a functional candidate (`NamedUtilityValue`).
pub struct Bare<'a> {
    pub value: &'a str,
    pub fraction: Option<&'a str>,
}

pub type BareFn = Rc<dyn Fn(&Bare, &DesignSystem) -> Option<String>>;
pub type HandleFn = Rc<dyn Fn(&str, Option<&str>, &DesignSystem) -> Option<Vec<AstNode>>>;

/// Build a `handle` that ignores the data type and design system.
pub fn hv(f: impl Fn(&str) -> Vec<AstNode> + 'static) -> HandleFn {
    Rc::new(move |v, _, _| Some(f(v)))
}

/// Build a full `handle`.
pub fn hf(f: impl Fn(&str, Option<&str>, &DesignSystem) -> Option<Vec<AstNode>> + 'static) -> HandleFn {
    Rc::new(f)
}

pub fn bare(f: impl Fn(&Bare, &DesignSystem) -> Option<String> + 'static) -> Option<BareFn> {
    Some(Rc::new(f))
}

/// `handleBareValue: ({ value }) => isPositiveInteger(value) ? value : null`
pub fn bare_positive_integer() -> Option<BareFn> {
    bare(|b, _| crate::tw::utils::is_positive_integer(b.value).then(|| b.value.to_string()))
}

pub struct Desc {
    pub supports_negative: bool,
    pub supports_fractions: bool,
    pub theme_keys: Vec<&'static str>,
    /// `None` = undefined, `Some(None)` = null
    pub default_value: Option<Option<String>>,
    pub static_values: Vec<(&'static str, Vec<AstNode>)>,
    pub handle_bare_value: Option<BareFn>,
    pub handle_negative_bare_value: Option<BareFn>,
    pub handle: HandleFn,
}

impl Default for Desc {
    fn default() -> Self {
        Desc {
            supports_negative: false,
            supports_fractions: false,
            theme_keys: vec![],
            default_value: None,
            static_values: vec![],
            handle_bare_value: None,
            handle_negative_bare_value: None,
            handle: Rc::new(|_, _, _| None),
        }
    }
}

/// Upstream's `staticUtility` registration helper.
pub fn static_utility(u: &mut Utilities, name: &str, decls: &[(&str, &str)]) {
    let decls: Vec<(String, String)> = decls.iter().map(|(p, v)| (p.to_string(), v.to_string())).collect();
    u.static_(name, move |_, _| Some(decls.iter().map(|(p, v)| decl(p.as_str(), v.as_str())).collect()));
}

/// `staticUtility` with node factories (e.g. `@property` at-roots).
pub fn static_utility_nodes(u: &mut Utilities, name: &str, f: impl Fn() -> Vec<AstNode> + 'static) {
    u.static_(name, move |_, _| Some(f()));
}

pub fn functional_utility(u: &mut Utilities, class_root: &str, desc: Desc) {
    let desc = Rc::new(desc);

    fn make(desc: Rc<Desc>, negative: bool) -> impl Fn(&Candidate, &DesignSystem) -> Option<Vec<AstNode>> {
        move |candidate, ds| {
            let theme = &ds.theme;
            let mut value: Option<String>;
            let mut data_type: Option<String> = None;

            match candidate.value() {
                None => {
                    if candidate.modifier.is_some() {
                        return None;
                    }
                    value = match &desc.default_value {
                        Some(d) => d.clone(),
                        None => theme.resolve(None, &desc.theme_keys),
                    };
                }
                Some(UtilityValue::Arbitrary { value: v, data_type: dt }) => {
                    if candidate.modifier.is_some() {
                        return None;
                    }
                    value = Some(v.clone());
                    data_type = dt.clone();
                }
                Some(UtilityValue::Named { value: v, fraction }) => {
                    value = theme.resolve(Some(fraction.as_deref().unwrap_or(v)), &desc.theme_keys);

                    if value.is_none() && desc.supports_fractions {
                        if let Some(f) = fraction {
                            let parts = segment(f, b'/');
                            let lhs = parts[0];
                            let rhs = parts.get(1).copied().unwrap_or("");
                            if !crate::tw::utils::is_positive_integer(lhs) || !crate::tw::utils::is_positive_integer(rhs) {
                                return None;
                            }
                            value = Some(format!("calc({} / {} * 100%)", lhs, rhs));
                        }
                    }

                    let bare_v = Bare { value: v, fraction: fraction.as_deref() };

                    if value.is_none() && negative {
                        if let Some(h) = &desc.handle_negative_bare_value {
                            value = h(&bare_v, ds);
                            if !value.as_deref().is_some_and(|v| v.contains('/')) && candidate.modifier.is_some() {
                                return None;
                            }
                            if let Some(v) = &value {
                                return (desc.handle)(v, None, ds);
                            }
                        }
                    }

                    if value.is_none() {
                        if let Some(h) = &desc.handle_bare_value {
                            value = h(&bare_v, ds);
                            if !value.as_deref().is_some_and(|v| v.contains('/')) && candidate.modifier.is_some() {
                                return None;
                            }
                        }
                    }

                    if value.is_none() && !negative && candidate.modifier.is_none() {
                        if let Some((_, nodes)) = desc.static_values.iter().find(|(k, _)| *k == v) {
                            return Some(nodes.clone());
                        }
                    }
                }
            }

            let value = value?;
            let value = if negative {
                crate::tw::utils::add_whitespace_around_math_operators(&format!("calc({} * -1)", value))
            } else {
                value
            };
            (desc.handle)(&value, data_type.as_deref(), ds)
        }
    }

    if desc.supports_negative {
        u.functional(&format!("-{}", class_root), make(desc.clone(), true));
    }
    u.functional(class_root, make(desc, false));
}

pub fn color_utility(
    u: &mut Utilities,
    class_root: &str,
    theme_keys: Vec<&'static str>,
    handle: impl Fn(&str) -> Option<Vec<AstNode>> + 'static,
) {
    u.functional(class_root, move |candidate, ds| {
        let v = candidate.value()?;
        let value = match v {
            UtilityValue::Arbitrary { value, .. } => as_color(value, candidate.modifier.as_ref(), &ds.theme),
            UtilityValue::Named { .. } => resolve_theme_color(candidate, &ds.theme, &theme_keys),
        }?;
        handle(&value)
    });
}

pub struct SpacingOpts {
    pub supports_negative: bool,
    pub supports_fractions: bool,
    pub static_values: Vec<(&'static str, Vec<AstNode>)>,
}

impl Default for SpacingOpts {
    fn default() -> Self {
        SpacingOpts { supports_negative: false, supports_fractions: false, static_values: vec![] }
    }
}

pub fn spacing_utility(
    u: &mut Utilities,
    name: &str,
    theme_keys: Vec<&'static str>,
    handle: impl Fn(&str) -> Vec<AstNode> + 'static,
    opts: SpacingOpts,
) {
    let handle = Rc::new(handle);
    if opts.supports_negative {
        let h = handle.clone();
        u.static_(&format!("-{}-px", name), move |_, _| Some(h("-1px")));
    }
    let h = handle.clone();
    u.static_(&format!("{}-px", name), move |_, _| Some(h("1px")));

    let h = handle.clone();
    functional_utility(
        u,
        name,
        Desc {
            theme_keys,
            supports_fractions: opts.supports_fractions,
            supports_negative: opts.supports_negative,
            default_value: Some(None),
            handle_bare_value: bare(|b, ds| {
                ds.theme.resolve(None, &["--spacing"])?;
                if !is_valid_spacing_multiplier(b.value) {
                    return None;
                }
                Some(format!("--spacing({})", b.value))
            }),
            handle_negative_bare_value: bare(|b, ds| {
                ds.theme.resolve(None, &["--spacing"])?;
                if !is_valid_spacing_multiplier(b.value) {
                    return None;
                }
                Some(format!("--spacing(-{})", b.value))
            }),
            handle: Rc::new(move |v, _, _| Some(h(v))),
            static_values: opts.static_values,
        },
    );
}

pub fn alpha_replaced_shadow_properties(
    property: &str,
    value: &str,
    alpha: Option<&str>,
    var_injector: &dyn Fn(&str) -> String,
    prefix: &str,
) -> Vec<AstNode> {
    let requires_fallback = std::cell::Cell::new(false);
    let replaced_value = crate::tw::utils::replace_shadow_colors(value, &|color| {
        let Some(alpha) = alpha else { return var_injector(color) };
        if color.starts_with("current") {
            return var_injector(&with_alpha(color, alpha));
        }
        if color.starts_with("var(") || alpha.starts_with("var(") {
            requires_fallback.set(true);
        }
        var_injector(&replace_alpha(color, alpha))
    });

    let apply_prefix = |x: &str| -> String {
        if prefix.is_empty() {
            return x.to_string();
        }
        segment(x, b',').iter().map(|v| format!("{} {}", prefix.trim(), v.trim())).collect::<Vec<_>>().join(", ")
    };

    if requires_fallback.get() {
        vec![
            decl(property, apply_prefix(&crate::tw::utils::replace_shadow_colors(value, var_injector))),
            crate::tw::ast::rule("@supports (color: lab(from red l a b))", vec![decl(property, apply_prefix(&replaced_value))]),
        ]
    } else {
        vec![decl(property, apply_prefix(&replaced_value))]
    }
}

pub fn alpha_replaced_drop_shadow_properties(
    property: &str,
    value: &str,
    alpha: Option<&str>,
    var_injector: &dyn Fn(&str) -> String,
    prefix: &str,
) -> Vec<AstNode> {
    let requires_fallback = std::cell::Cell::new(false);
    let replaced_value = segment(value, b',')
        .iter()
        .map(|v| {
            crate::tw::utils::replace_shadow_colors(v, &|color| {
                let Some(alpha) = alpha else { return var_injector(color) };
                if color.starts_with("current") {
                    return var_injector(&with_alpha(color, alpha));
                }
                if color.starts_with("var(") || alpha.starts_with("var(") {
                    requires_fallback.set(true);
                }
                var_injector(&replace_alpha(color, alpha))
            })
        })
        .map(|v| format!("drop-shadow({})", v))
        .collect::<Vec<_>>()
        .join(" ");

    if requires_fallback.get() {
        let fallback = segment(value, b',')
            .iter()
            .map(|v| format!("drop-shadow({})", crate::tw::utils::replace_shadow_colors(v, var_injector)))
            .collect::<Vec<_>>()
            .join(" ");
        vec![
            decl(property, format!("{}{}", prefix, fallback)),
            crate::tw::ast::rule(
                "@supports (color: lab(from red l a b))",
                vec![decl(property, format!("{}{}", prefix, replaced_value))],
            ),
        ]
    } else {
        vec![decl(property, format!("{}{}", prefix, replaced_value))]
    }
}

pub fn create_utilities(_theme: &Theme) -> Utilities {
    let mut u = Utilities::default();
    part1::register(&mut u);
    part2::register(&mut u);
    part3::register(&mut u);
    part4::register(&mut u);
    crate::tw::compat::register_legacy_utilities(&mut u);
    u
}
