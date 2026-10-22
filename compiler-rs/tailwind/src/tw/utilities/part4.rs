//! utilities.ts lines ~4785–5930: will-change, content, contain, leading,
//! tracking, font-variant-numeric, outline, opacity, text, shadows, rings,
//! `@container`.

use super::part1::sv;
use super::*;
use crate::tw::ast::{at_root, at_rule, decl, decl_opt, AstNode};
use crate::tw::candidate::{Candidate, UtilityValue};
use crate::tw::design_system::DesignSystem;
use crate::tw::utils::{infer_data_type, is_positive_integer, DataType};

/// Register a functional utility that may return upstream `null` (Abort).
fn functional_raw(u: &mut Utilities, name: &str, f: impl Fn(&Candidate, &DesignSystem) -> UResult + 'static) {
    u.push_raw(name, Utility { kind: UtilityKind::Functional, compile: Rc::new(f), types: vec![] });
}

/// `candidate.value.dataType ?? inferDataType(value, types)` as a string.
fn value_type(data_type: &Option<String>, value: &str, types: &[DataType]) -> Option<String> {
    match data_type {
        Some(dt) => Some(dt.clone()),
        None => infer_data_type(value, types).map(|t| t.as_str().to_string()),
    }
}

/// Alpha from the modifier for shadow utilities.
fn shadow_alpha(c: &Candidate) -> Option<String> {
    match &c.modifier {
        Some(crate::tw::candidate::Modifier::Arbitrary(v)) => Some(v.clone()),
        Some(crate::tw::candidate::Modifier::Named(v)) => {
            if crate::tw::utils::is_valid_opacity_value(v) {
                Some(format!("{}%", v))
            } else {
                None
            }
        }
        None => None,
    }
}

const CSS_CONTAIN_VALUE: &str =
    "var(--tw-contain-size,) var(--tw-contain-layout,) var(--tw-contain-paint,) var(--tw-contain-style,)";

fn css_contain_properties() -> AstNode {
    at_root(vec![
        property("--tw-contain-size", None, None),
        property("--tw-contain-layout", None, None),
        property("--tw-contain-paint", None, None),
        property("--tw-contain-style", None, None),
    ])
}

const CSS_FONT_VARIANT_NUMERIC_VALUE: &str = "var(--tw-ordinal,) var(--tw-slashed-zero,) var(--tw-numeric-figure,) var(--tw-numeric-spacing,) var(--tw-numeric-fraction,)";

fn font_variant_numeric_properties() -> AstNode {
    at_root(vec![
        property("--tw-ordinal", None, None),
        property("--tw-slashed-zero", None, None),
        property("--tw-numeric-figure", None, None),
        property("--tw-numeric-spacing", None, None),
        property("--tw-numeric-fraction", None, None),
    ])
}

fn outline_properties() -> AstNode {
    at_root(vec![property("--tw-outline-style", Some("solid"), None)])
}

fn text_shadow_properties() -> AstNode {
    at_root(vec![
        property("--tw-text-shadow-color", None, None),
        property("--tw-text-shadow-alpha", Some("100%"), Some("<percentage>")),
    ])
}

const CSS_BOX_SHADOW_VALUE: &str =
    "var(--tw-inset-shadow), var(--tw-inset-ring-shadow), var(--tw-ring-offset-shadow), var(--tw-ring-shadow), var(--tw-shadow)";
const NULL_SHADOW: &str = "0 0 #0000";

fn box_shadow_properties() -> AstNode {
    at_root(vec![
        property("--tw-shadow", Some(NULL_SHADOW), None),
        property("--tw-shadow-color", None, None),
        property("--tw-shadow-alpha", Some("100%"), Some("<percentage>")),
        property("--tw-inset-shadow", Some(NULL_SHADOW), None),
        property("--tw-inset-shadow-color", None, None),
        property("--tw-inset-shadow-alpha", Some("100%"), Some("<percentage>")),
        property("--tw-ring-color", None, None),
        property("--tw-ring-shadow", Some(NULL_SHADOW), None),
        property("--tw-inset-ring-color", None, None),
        property("--tw-inset-ring-shadow", Some(NULL_SHADOW), None),
        // Legacy
        property("--tw-ring-inset", None, None),
        property("--tw-ring-offset-width", Some("0px"), Some("<length>")),
        property("--tw-ring-offset-color", Some("#fff"), None),
        property("--tw-ring-offset-shadow", Some(NULL_SHADOW), None),
    ])
}

const RING_OFFSET_SHADOW_VALUE: &str =
    "var(--tw-ring-inset,) 0 0 0 var(--tw-ring-offset-width) var(--tw-ring-offset-color)";

fn inset_ring_shadow_value(value: &str) -> String {
    format!("inset 0 0 0 {} var(--tw-inset-ring-color, currentcolor)", value)
}

/// Resolve the line-height modifier of `text-*` utilities.
/// `Err(())` corresponds to upstream `return null`.
fn text_line_height_modifier(c: &Candidate, ds: &DesignSystem) -> Result<Option<String>, ()> {
    let Some(m) = &c.modifier else { return Ok(None) };
    let mut modifier = match m {
        crate::tw::candidate::Modifier::Arbitrary(v) => Some(v.clone()),
        crate::tw::candidate::Modifier::Named(v) => ds.theme.resolve(Some(v), &["--leading"]),
    };
    let truthy = |m: &Option<String>| m.as_deref().is_some_and(|s| !s.is_empty());
    if !truthy(&modifier) && is_valid_spacing_multiplier(m.value()) {
        if ds.theme.resolve(None, &["--spacing"]).is_none() {
            return Err(());
        }
        modifier = Some(format!("--spacing({})", m.value()));
    }
    if !truthy(&modifier) && m.value() == "none" {
        modifier = Some("1".into());
    }
    if truthy(&modifier) {
        Ok(modifier)
    } else {
        Err(())
    }
}

/// Shared implementation of the `shadow` / `inset-shadow` / `text-shadow` utilities.
struct ShadowKind {
    /// Theme key for the default value (`--shadow`)
    default_key: &'static str,
    /// Prefix of size theme keys (`--shadow-`)
    size_prefix: &'static str,
    /// Property that receives the shadow value (`--tw-shadow`)
    property: &'static str,
    alpha_var: &'static str,
    color_var: &'static str,
    color_theme_keys: &'static [&'static str],
    properties: fn() -> AstNode,
    /// Whether `box-shadow` should be emitted
    box_shadow: bool,
    /// Prefix for arbitrary values (`inset` for inset-shadow)
    arbitrary_prefix: &'static str,
    /// Declaration emitted for `*-none`
    none_decl: (&'static str, &'static str),
}

fn shadow_utility(u: &mut Utilities, name: &str, k: ShadowKind) {
    u.functional(name, move |c, ds| {
        let theme = &ds.theme;
        let alpha = shadow_alpha(c);
        let injector = |color: &str| format!("var({}, {})", k.color_var, color);

        let sized = |value: &str, prefix: &str| -> Vec<AstNode> {
            let mut nodes = vec![(k.properties)(), decl_opt(k.alpha_var, alpha.clone())];
            nodes.extend(alpha_replaced_shadow_properties(k.property, value, alpha.as_deref(), &injector, prefix));
            if k.box_shadow {
                nodes.push(decl("box-shadow", CSS_BOX_SHADOW_VALUE));
            }
            nodes
        };

        let Some(cv) = c.value() else {
            let value = theme.get(&[k.default_key])?;
            return Some(sized(&value, ""));
        };

        if let UtilityValue::Arbitrary { value, data_type } = cv {
            let ty = value_type(data_type, value, &[DataType::Color]);
            if ty.as_deref() == Some("color") {
                let value = as_color(value, c.modifier.as_ref(), theme)?;
                return Some(vec![
                    (k.properties)(),
                    decl(k.color_var, with_alpha(&value, &format!("var({})", k.alpha_var))),
                ]);
            }
            return Some(sized(value, k.arbitrary_prefix));
        }

        match cv.value() {
            "none" => {
                if c.modifier.is_some() {
                    return None;
                }
                let mut nodes = vec![(k.properties)(), decl(k.none_decl.0, k.none_decl.1)];
                if k.box_shadow {
                    nodes.push(decl("box-shadow", CSS_BOX_SHADOW_VALUE));
                }
                return Some(nodes);
            }
            "inherit" => {
                if c.modifier.is_some() {
                    return None;
                }
                return Some(vec![(k.properties)(), decl(k.color_var, "inherit")]);
            }
            _ => {}
        }

        if let Some(value) = theme.get(&[format!("{}{}", k.size_prefix, cv.value()).as_str()]) {
            if !value.is_empty() {
                return Some(sized(&value, ""));
            }
        }

        if let Some(value) = resolve_theme_color(c, theme, k.color_theme_keys) {
            if !value.is_empty() {
                return Some(vec![
                    (k.properties)(),
                    decl(k.color_var, with_alpha(&value, &format!("var({})", k.alpha_var))),
                ]);
            }
        }

        None
    });
}

pub fn register(u: &mut Utilities) {
    static_utility(u, "will-change-auto", &[("will-change", "auto")]);
    static_utility(u, "will-change-scroll", &[("will-change", "scroll-position")]);
    static_utility(u, "will-change-contents", &[("will-change", "contents")]);
    static_utility(u, "will-change-transform", &[("will-change", "transform")]);
    functional_utility(
        u,
        "will-change",
        Desc { theme_keys: vec![], handle: hv(|v| vec![decl("will-change", v)]), ..Default::default() },
    );

    static_utility(u, "content-none", &[("--tw-content", "none"), ("content", "none")]);
    functional_utility(
        u,
        "content",
        Desc {
            theme_keys: vec!["--content"],
            handle: hv(|v| {
                vec![
                    at_root(vec![property("--tw-content", Some("\"\""), None)]),
                    decl("--tw-content", v),
                    decl("content", "var(--tw-content)"),
                ]
            }),
            ..Default::default()
        },
    );

    static_utility(u, "contain-none", &[("contain", "none")]);
    static_utility(u, "contain-content", &[("contain", "content")]);
    static_utility(u, "contain-strict", &[("contain", "strict")]);
    for (name, var, value) in [
        ("contain-size", "--tw-contain-size", "size"),
        ("contain-inline-size", "--tw-contain-size", "inline-size"),
        ("contain-layout", "--tw-contain-layout", "layout"),
        ("contain-paint", "--tw-contain-paint", "paint"),
        ("contain-style", "--tw-contain-style", "style"),
    ] {
        static_utility_nodes(u, name, move || {
            vec![css_contain_properties(), decl(var, value), decl("contain", CSS_CONTAIN_VALUE)]
        });
    }
    functional_utility(
        u,
        "contain",
        Desc { theme_keys: vec![], handle: hv(|v| vec![decl("contain", v)]), ..Default::default() },
    );

    static_utility(u, "forced-color-adjust-none", &[("forced-color-adjust", "none")]);
    static_utility(u, "forced-color-adjust-auto", &[("forced-color-adjust", "auto")]);

    spacing_utility(
        u,
        "leading",
        vec!["--leading", "--spacing"],
        |v| vec![at_root(vec![property("--tw-leading", None, None)]), decl("--tw-leading", v), decl("line-height", v)],
        SpacingOpts {
            static_values: vec![(
                "none",
                vec![
                    at_root(vec![property("--tw-leading", None, None)]),
                    decl("--tw-leading", "1"),
                    decl("line-height", "1"),
                ],
            )],
            ..Default::default()
        },
    );

    functional_utility(
        u,
        "tracking",
        Desc {
            supports_negative: true,
            theme_keys: vec!["--tracking"],
            handle: hv(|v| {
                vec![
                    at_root(vec![property("--tw-tracking", None, None)]),
                    decl("--tw-tracking", v),
                    decl("letter-spacing", v),
                ]
            }),
            ..Default::default()
        },
    );

    static_utility(
        u,
        "antialiased",
        &[("-webkit-font-smoothing", "antialiased"), ("-moz-osx-font-smoothing", "grayscale")],
    );
    static_utility(
        u,
        "subpixel-antialiased",
        &[("-webkit-font-smoothing", "auto"), ("-moz-osx-font-smoothing", "auto")],
    );

    static_utility(u, "normal-nums", &[("font-variant-numeric", "normal")]);
    for (name, var, value) in [
        ("ordinal", "--tw-ordinal", "ordinal"),
        ("slashed-zero", "--tw-slashed-zero", "slashed-zero"),
        ("lining-nums", "--tw-numeric-figure", "lining-nums"),
        ("oldstyle-nums", "--tw-numeric-figure", "oldstyle-nums"),
        ("proportional-nums", "--tw-numeric-spacing", "proportional-nums"),
        ("tabular-nums", "--tw-numeric-spacing", "tabular-nums"),
        ("diagonal-fractions", "--tw-numeric-fraction", "diagonal-fractions"),
        ("stacked-fractions", "--tw-numeric-fraction", "stacked-fractions"),
    ] {
        static_utility_nodes(u, name, move || {
            vec![
                font_variant_numeric_properties(),
                decl(var, value),
                decl("font-variant-numeric", CSS_FONT_VARIANT_NUMERIC_VALUE),
            ]
        });
    }

    u.static_("outline-hidden", |_, _| {
        Some(vec![
            decl("--tw-outline-style", "none"),
            decl("outline-style", "none"),
            at_rule(
                "@media",
                "(forced-colors: active)",
                vec![decl("outline", "2px solid transparent"), decl("outline-offset", "2px")],
            ),
        ])
    });

    for style in ["none", "solid", "dashed", "dotted", "double"] {
        static_utility(u, &format!("outline-{}", style), &[("--tw-outline-style", style), ("outline-style", style)]);
    }

    u.functional("outline", |c, ds| {
        let theme = &ds.theme;
        let width_nodes = |value: String| {
            vec![outline_properties(), decl("outline-style", "var(--tw-outline-style)"), decl("outline-width", value)]
        };

        let Some(cv) = c.value() else {
            if c.modifier.is_some() {
                return None;
            }
            let value = theme.get(&["--default-outline-width"]).unwrap_or_else(|| "1px".into());
            return Some(width_nodes(value));
        };

        if let UtilityValue::Arbitrary { value, data_type } = cv {
            let ty = value_type(
                data_type,
                value,
                &[DataType::Color, DataType::Length, DataType::Number, DataType::Percentage],
            );
            return match ty.as_deref() {
                Some("length") | Some("number") | Some("percentage") => {
                    if c.modifier.is_some() {
                        return None;
                    }
                    Some(width_nodes(value.clone()))
                }
                _ => {
                    let value = as_color(value, c.modifier.as_ref(), theme)?;
                    Some(vec![decl("outline-color", value)])
                }
            };
        }

        if let Some(value) = resolve_theme_color(c, theme, &["--outline-color", "--color"]) {
            if !value.is_empty() {
                return Some(vec![decl("outline-color", value)]);
            }
        }

        if c.modifier.is_some() {
            return None;
        }
        let v = cv.value();
        match theme.resolve(Some(v), &["--outline-width"]) {
            Some(value) if !value.is_empty() => Some(width_nodes(value)),
            _ if is_positive_integer(v) => Some(width_nodes(format!("{}px", v))),
            _ => None,
        }
    });

    functional_utility(
        u,
        "outline-offset",
        Desc {
            supports_negative: true,
            theme_keys: vec!["--outline-offset"],
            handle_bare_value: bare(|b, _| is_positive_integer(b.value).then(|| format!("{}px", b.value))),
            handle: hv(|v| vec![decl("outline-offset", v)]),
            ..Default::default()
        },
    );

    functional_utility(
        u,
        "opacity",
        Desc {
            theme_keys: vec!["--opacity"],
            handle_bare_value: bare(|b, _| is_valid_opacity_value(b.value).then(|| format!("{}%", b.value))),
            handle: hv(|v| vec![decl("opacity", v)]),
            ..Default::default()
        },
    );

    functional_utility(
        u,
        "underline-offset",
        Desc {
            supports_negative: true,
            theme_keys: vec!["--text-underline-offset"],
            handle_bare_value: bare(|b, _| is_positive_integer(b.value).then(|| format!("{}px", b.value))),
            handle: hv(|v| vec![decl("text-underline-offset", v)]),
            static_values: sv(&[("auto", &[("text-underline-offset", "auto")])]),
            ..Default::default()
        },
    );

    functional_raw(u, "text", |c, ds| {
        let theme = &ds.theme;
        let Some(cv) = c.value() else { return UResult::Skip };

        if let UtilityValue::Arbitrary { value, data_type } = cv {
            let ty = value_type(
                data_type,
                value,
                &[
                    DataType::Color,
                    DataType::Length,
                    DataType::Percentage,
                    DataType::AbsoluteSize,
                    DataType::RelativeSize,
                ],
            );
            return match ty.as_deref() {
                Some("size") | Some("length") | Some("percentage") | Some("absolute-size") | Some("relative-size") => {
                    if c.modifier.is_some() {
                        match text_line_height_modifier(c, ds) {
                            Ok(Some(m)) => {
                                UResult::Nodes(vec![decl("font-size", value.as_str()), decl("line-height", m)])
                            }
                            _ => UResult::Abort,
                        }
                    } else {
                        UResult::Nodes(vec![decl("font-size", value.as_str())])
                    }
                }
                _ => match as_color(value, c.modifier.as_ref(), theme) {
                    Some(v) => UResult::Nodes(vec![decl("color", v)]),
                    None => UResult::Skip,
                },
            };
        }

        if let Some(value) = resolve_theme_color(c, theme, &["--text-color", "--color"]) {
            if !value.is_empty() {
                return UResult::Nodes(vec![decl("color", value)]);
            }
        }

        if let Some((font_size, options)) =
            theme.resolve_with(cv.value(), &["--text"], &["--line-height", "--letter-spacing", "--font-weight"])
        {
            if c.modifier.is_some() {
                return match text_line_height_modifier(c, ds) {
                    Ok(Some(m)) => UResult::Nodes(vec![decl("font-size", font_size), decl("line-height", m)]),
                    _ => UResult::Abort,
                };
            }

            let opt = |key: &str| options.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone()).filter(|v| !v.is_empty());
            return UResult::Nodes(vec![
                decl("font-size", font_size),
                decl_opt("line-height", opt("--line-height").map(|v| format!("var(--tw-leading, {})", v))),
                decl_opt("letter-spacing", opt("--letter-spacing").map(|v| format!("var(--tw-tracking, {})", v))),
                decl_opt("font-weight", opt("--font-weight").map(|v| format!("var(--tw-font-weight, {})", v))),
            ]);
        }

        UResult::Skip
    });

    static_utility_nodes(u, "text-shadow-initial", || {
        vec![text_shadow_properties(), decl("--tw-text-shadow-color", "initial")]
    });

    shadow_utility(
        u,
        "text-shadow",
        ShadowKind {
            default_key: "--text-shadow",
            size_prefix: "--text-shadow-",
            property: "text-shadow",
            alpha_var: "--tw-text-shadow-alpha",
            color_var: "--tw-text-shadow-color",
            color_theme_keys: &["--text-shadow-color", "--color"],
            properties: text_shadow_properties,
            box_shadow: false,
            arbitrary_prefix: "",
            none_decl: ("text-shadow", "none"),
        },
    );

    static_utility_nodes(u, "shadow-initial", || vec![box_shadow_properties(), decl("--tw-shadow-color", "initial")]);

    shadow_utility(
        u,
        "shadow",
        ShadowKind {
            default_key: "--shadow",
            size_prefix: "--shadow-",
            property: "--tw-shadow",
            alpha_var: "--tw-shadow-alpha",
            color_var: "--tw-shadow-color",
            color_theme_keys: &["--box-shadow-color", "--color"],
            properties: box_shadow_properties,
            box_shadow: true,
            arbitrary_prefix: "",
            none_decl: ("--tw-shadow", NULL_SHADOW),
        },
    );

    static_utility_nodes(u, "inset-shadow-initial", || {
        vec![box_shadow_properties(), decl("--tw-inset-shadow-color", "initial")]
    });

    shadow_utility(
        u,
        "inset-shadow",
        ShadowKind {
            default_key: "--inset-shadow",
            size_prefix: "--inset-shadow-",
            property: "--tw-inset-shadow",
            alpha_var: "--tw-inset-shadow-alpha",
            color_var: "--tw-inset-shadow-color",
            color_theme_keys: &["--box-shadow-color", "--color"],
            properties: box_shadow_properties,
            box_shadow: true,
            arbitrary_prefix: "inset",
            none_decl: ("--tw-inset-shadow", "inset 0 0 #0000"),
        },
    );

    static_utility_nodes(u, "ring-inset", || vec![box_shadow_properties(), decl("--tw-ring-inset", "inset")]);

    // `ring` and `inset-ring` share the same structure.
    for (name, color_var) in [("ring", "--tw-ring-color"), ("inset-ring", "--tw-inset-ring-color")] {
        let is_ring = name == "ring";
        u.functional(name, move |c, ds| {
            let theme = &ds.theme;
            let shadow = |value: &str| -> Vec<AstNode> {
                let (prop, v) = if is_ring {
                    let default_ring_color = theme.get(&["--default-ring-color"]).unwrap_or_else(|| "currentcolor".into());
                    (
                        "--tw-ring-shadow",
                        format!(
                            "var(--tw-ring-inset,) 0 0 0 calc({} + var(--tw-ring-offset-width)) var(--tw-ring-color, {})",
                            value, default_ring_color
                        ),
                    )
                } else {
                    ("--tw-inset-ring-shadow", inset_ring_shadow_value(value))
                };
                vec![box_shadow_properties(), decl(prop, v), decl("box-shadow", CSS_BOX_SHADOW_VALUE)]
            };

            let Some(cv) = c.value() else {
                if c.modifier.is_some() {
                    return None;
                }
                let value = if is_ring {
                    theme.get(&["--default-ring-width"]).unwrap_or_else(|| "1px".into())
                } else {
                    "1px".into()
                };
                return Some(shadow(&value));
            };

            if let UtilityValue::Arbitrary { value, data_type } = cv {
                let ty = value_type(data_type, value, &[DataType::Color, DataType::Length]);
                if ty.as_deref() == Some("length") {
                    if c.modifier.is_some() {
                        return None;
                    }
                    return Some(shadow(value));
                }
                let value = as_color(value, c.modifier.as_ref(), theme)?;
                return Some(vec![decl(color_var, value)]);
            }

            if let Some(value) = resolve_theme_color(c, theme, &["--ring-color", "--color"]) {
                if !value.is_empty() {
                    return Some(vec![decl(color_var, value)]);
                }
            }

            if c.modifier.is_some() {
                return None;
            }
            let v = cv.value();
            let mut value = theme.resolve(Some(v), &["--ring-width"]);
            if value.is_none() && is_positive_integer(v) {
                value = Some(format!("{}px", v));
            }
            match value {
                Some(value) if !value.is_empty() => Some(shadow(&value)),
                _ => None,
            }
        });
    }

    u.functional("ring-offset", |c, ds| {
        let theme = &ds.theme;
        let cv = c.value()?;

        if let UtilityValue::Arbitrary { value, data_type } = cv {
            let ty = value_type(data_type, value, &[DataType::Color, DataType::Length]);
            if ty.as_deref() == Some("length") {
                if c.modifier.is_some() {
                    return None;
                }
                return Some(vec![
                    decl("--tw-ring-offset-width", value.as_str()),
                    decl("--tw-ring-offset-shadow", RING_OFFSET_SHADOW_VALUE),
                ]);
            }
            let value = as_color(value, c.modifier.as_ref(), theme)?;
            return Some(vec![decl("--tw-ring-offset-color", value)]);
        }

        let v = cv.value();
        match theme.resolve(Some(v), &["--ring-offset-width"]) {
            Some(value) if !value.is_empty() => {
                if c.modifier.is_some() {
                    return None;
                }
                return Some(vec![
                    decl("--tw-ring-offset-width", value),
                    decl("--tw-ring-offset-shadow", RING_OFFSET_SHADOW_VALUE),
                ]);
            }
            _ => {
                if is_positive_integer(v) {
                    if c.modifier.is_some() {
                        return None;
                    }
                    return Some(vec![
                        decl("--tw-ring-offset-width", format!("{}px", v)),
                        decl("--tw-ring-offset-shadow", RING_OFFSET_SHADOW_VALUE),
                    ]);
                }
            }
        }

        if let Some(value) = resolve_theme_color(c, theme, &["--ring-offset-color", "--color"]) {
            if !value.is_empty() {
                return Some(vec![decl("--tw-ring-offset-color", value)]);
            }
        }
        None
    });

    u.functional("@container", |c, _| {
        let value = match c.value() {
            None => "inline-size".to_string(),
            Some(UtilityValue::Arbitrary { value, .. }) => value.clone(),
            Some(UtilityValue::Named { value, .. }) if value == "normal" => "normal".to_string(),
            Some(UtilityValue::Named { value, .. }) if value == "size" => "size".to_string(),
            Some(_) => return None,
        };
        match &c.modifier {
            Some(m) => Some(vec![decl("container-type", value), decl("container-name", m.value())]),
            None => Some(vec![decl("container-type", value)]),
        }
    });
}
