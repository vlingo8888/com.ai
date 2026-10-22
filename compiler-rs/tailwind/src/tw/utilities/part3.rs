//! utilities.ts lines ~3657–4784: box-decoration, background clip/origin,
//! blend modes, fill/stroke, object, padding, text-align, indent, align, font,
//! text transform/decoration, placeholder, animate, filters, backdrop filters,
//! transitions, duration, ease.

use super::part1::sv;
use super::*;
use crate::tw::ast::{at_root, decl, decl_opt, style_rule, AstNode};
use crate::tw::candidate::UtilityValue;
use crate::tw::utils::{infer_data_type, is_positive_integer, DataType};

const CSS_FILTER_VALUE: &str = "var(--tw-blur,) var(--tw-brightness,) var(--tw-contrast,) var(--tw-grayscale,) var(--tw-hue-rotate,) var(--tw-invert,) var(--tw-saturate,) var(--tw-sepia,) var(--tw-drop-shadow,)";

const CSS_BACKDROP_FILTER_VALUE: &str = "var(--tw-backdrop-blur,) var(--tw-backdrop-brightness,) var(--tw-backdrop-contrast,) var(--tw-backdrop-grayscale,) var(--tw-backdrop-hue-rotate,) var(--tw-backdrop-invert,) var(--tw-backdrop-opacity,) var(--tw-backdrop-saturate,) var(--tw-backdrop-sepia,)";

const TRANSITION_DEFAULT: &str = "color, background-color, border-color, outline-color, text-decoration-color, fill, stroke, --tw-gradient-from, --tw-gradient-via, --tw-gradient-to, opacity, box-shadow, transform, translate, scale, rotate, filter, -webkit-backdrop-filter, backdrop-filter, display, content-visibility, overlay, pointer-events";

const TRANSITION_COLORS: &str = "color, background-color, border-color, outline-color, text-decoration-color, fill, stroke, --tw-gradient-from, --tw-gradient-via, --tw-gradient-to";

fn filter_properties() -> AstNode {
    at_root(vec![
        property("--tw-blur", None, None),
        property("--tw-brightness", None, None),
        property("--tw-contrast", None, None),
        property("--tw-grayscale", None, None),
        property("--tw-hue-rotate", None, None),
        property("--tw-invert", None, None),
        property("--tw-opacity", None, None),
        property("--tw-saturate", None, None),
        property("--tw-sepia", None, None),
        property("--tw-drop-shadow", None, None),
        property("--tw-drop-shadow-color", None, None),
        property("--tw-drop-shadow-alpha", Some("100%"), Some("<percentage>")),
        property("--tw-drop-shadow-size", None, None),
    ])
}

fn backdrop_filter_properties() -> AstNode {
    at_root(vec![
        property("--tw-backdrop-blur", None, None),
        property("--tw-backdrop-brightness", None, None),
        property("--tw-backdrop-contrast", None, None),
        property("--tw-backdrop-grayscale", None, None),
        property("--tw-backdrop-hue-rotate", None, None),
        property("--tw-backdrop-invert", None, None),
        property("--tw-backdrop-opacity", None, None),
        property("--tw-backdrop-saturate", None, None),
        property("--tw-backdrop-sepia", None, None),
    ])
}

/// `dataType ?? inferDataType(value, types)` as a string.
fn data_type_of(data_type: &Option<String>, value: &str, types: &[DataType]) -> Option<String> {
    match data_type {
        Some(dt) => Some(dt.clone()),
        None => infer_data_type(value, types).map(|t| t.as_str().to_string()),
    }
}

fn drop_shadow_injector(color: &str) -> String {
    format!("var(--tw-drop-shadow-color, {})", color)
}

fn drop_shadows(resolved: &str) -> String {
    segment(resolved, b',').iter().map(|v| format!("drop-shadow({})", v)).collect::<Vec<_>>().join(" ")
}

fn default_timing_function(ds: &DesignSystem) -> String {
    format!(
        "var(--tw-ease, {})",
        ds.theme.resolve(None, &["--default-transition-timing-function"]).unwrap_or_else(|| "ease".into())
    )
}

fn default_duration(ds: &DesignSystem) -> String {
    format!(
        "var(--tw-duration, {})",
        ds.theme.resolve(None, &["--default-transition-duration"]).unwrap_or_else(|| "0s".into())
    )
}

fn transition_nodes(property: &str, ds: &DesignSystem) -> Vec<AstNode> {
    vec![
        decl("transition-property", property),
        decl("transition-timing-function", default_timing_function(ds)),
        decl("transition-duration", default_duration(ds)),
    ]
}

pub fn register(u: &mut Utilities) {
    static_utility(
        u,
        "box-decoration-slice",
        &[("-webkit-box-decoration-break", "slice"), ("box-decoration-break", "slice")],
    );
    static_utility(
        u,
        "box-decoration-clone",
        &[("-webkit-box-decoration-break", "clone"), ("box-decoration-break", "clone")],
    );

    static_utility(u, "bg-clip-text", &[("background-clip", "text")]);
    static_utility(u, "bg-clip-border", &[("background-clip", "border-box")]);
    static_utility(u, "bg-clip-padding", &[("background-clip", "padding-box")]);
    static_utility(u, "bg-clip-content", &[("background-clip", "content-box")]);

    static_utility(u, "bg-origin-border", &[("background-origin", "border-box")]);
    static_utility(u, "bg-origin-padding", &[("background-origin", "padding-box")]);
    static_utility(u, "bg-origin-content", &[("background-origin", "content-box")]);

    for value in [
        "normal",
        "multiply",
        "screen",
        "overlay",
        "darken",
        "lighten",
        "color-dodge",
        "color-burn",
        "hard-light",
        "soft-light",
        "difference",
        "exclusion",
        "hue",
        "saturation",
        "color",
        "luminosity",
    ] {
        static_utility(u, &format!("bg-blend-{}", value), &[("background-blend-mode", value)]);
        static_utility(u, &format!("mix-blend-{}", value), &[("mix-blend-mode", value)]);
    }

    static_utility(u, "mix-blend-plus-darker", &[("mix-blend-mode", "plus-darker")]);
    static_utility(u, "mix-blend-plus-lighter", &[("mix-blend-mode", "plus-lighter")]);

    static_utility(u, "fill-none", &[("fill", "none")]);
    u.functional("fill", |c, ds| {
        let value = c.value()?;
        if let UtilityValue::Arbitrary { value, .. } = value {
            let v = as_color(value, c.modifier.as_ref(), &ds.theme)?;
            return Some(vec![decl("fill", v)]);
        }
        let v = resolve_theme_color(c, &ds.theme, &["--fill", "--color"])?;
        Some(vec![decl("fill", v)])
    });

    static_utility(u, "stroke-none", &[("stroke", "none")]);
    u.functional("stroke", |c, ds| {
        let cv = c.value()?;
        match cv {
            UtilityValue::Arbitrary { value, data_type } => {
                let ty = data_type_of(
                    data_type,
                    value,
                    &[DataType::Color, DataType::Number, DataType::Length, DataType::Percentage],
                );
                match ty.as_deref() {
                    Some("number") | Some("length") | Some("percentage") => {
                        if c.modifier.is_some() {
                            return None;
                        }
                        Some(vec![decl("stroke-width", value.as_str())])
                    }
                    _ => {
                        let v = as_color(value, c.modifier.as_ref(), &ds.theme)?;
                        Some(vec![decl("stroke", v)])
                    }
                }
            }
            UtilityValue::Named { value, .. } => {
                if let Some(v) = resolve_theme_color(c, &ds.theme, &["--stroke", "--color"]) {
                    return Some(vec![decl("stroke", v)]);
                }
                if let Some(v) = ds.theme.resolve(Some(value), &["--stroke-width"]) {
                    return Some(vec![decl("stroke-width", v)]);
                } else if is_positive_integer(value) {
                    return Some(vec![decl("stroke-width", value.as_str())]);
                }
                None
            }
        }
    });

    static_utility(u, "object-contain", &[("object-fit", "contain")]);
    static_utility(u, "object-cover", &[("object-fit", "cover")]);
    static_utility(u, "object-fill", &[("object-fit", "fill")]);
    static_utility(u, "object-none", &[("object-fit", "none")]);
    static_utility(u, "object-scale-down", &[("object-fit", "scale-down")]);

    functional_utility(
        u,
        "object",
        Desc {
            theme_keys: vec!["--object-position"],
            handle: hv(|v| vec![decl("object-position", v)]),
            static_values: sv(&[
                ("top", &[("object-position", "top")]),
                ("top-left", &[("object-position", "left top")]),
                ("top-right", &[("object-position", "right top")]),
                ("bottom", &[("object-position", "bottom")]),
                ("bottom-left", &[("object-position", "left bottom")]),
                ("bottom-right", &[("object-position", "right bottom")]),
                ("left", &[("object-position", "left")]),
                ("right", &[("object-position", "right")]),
                ("center", &[("object-position", "center")]),
            ]),
            ..Default::default()
        },
    );

    for (name, property) in [
        ("p", "padding"),
        ("px", "padding-inline"),
        ("py", "padding-block"),
        ("ps", "padding-inline-start"),
        ("pe", "padding-inline-end"),
        ("pbs", "padding-block-start"),
        ("pbe", "padding-block-end"),
        ("pt", "padding-top"),
        ("pr", "padding-right"),
        ("pb", "padding-bottom"),
        ("pl", "padding-left"),
    ] {
        spacing_utility(
            u,
            name,
            vec!["--padding", "--spacing"],
            move |v| vec![decl(property, v)],
            SpacingOpts::default(),
        );
    }

    static_utility(u, "text-left", &[("text-align", "left")]);
    static_utility(u, "text-center", &[("text-align", "center")]);
    static_utility(u, "text-right", &[("text-align", "right")]);
    static_utility(u, "text-justify", &[("text-align", "justify")]);
    static_utility(u, "text-start", &[("text-align", "start")]);
    static_utility(u, "text-end", &[("text-align", "end")]);

    spacing_utility(
        u,
        "indent",
        vec!["--text-indent", "--spacing"],
        |v| vec![decl("text-indent", v)],
        SpacingOpts { supports_negative: true, ..Default::default() },
    );

    static_utility(u, "align-baseline", &[("vertical-align", "baseline")]);
    static_utility(u, "align-top", &[("vertical-align", "top")]);
    static_utility(u, "align-middle", &[("vertical-align", "middle")]);
    static_utility(u, "align-bottom", &[("vertical-align", "bottom")]);
    static_utility(u, "align-text-top", &[("vertical-align", "text-top")]);
    static_utility(u, "align-text-bottom", &[("vertical-align", "text-bottom")]);
    static_utility(u, "align-sub", &[("vertical-align", "sub")]);
    static_utility(u, "align-super", &[("vertical-align", "super")]);

    functional_utility(
        u,
        "align",
        Desc { handle: hv(|v| vec![decl("vertical-align", v)]), ..Default::default() },
    );

    u.functional("font", |c, ds| {
        let cv = c.value()?;
        if c.modifier.is_some() {
            return None;
        }
        match cv {
            UtilityValue::Arbitrary { value, data_type } => {
                let ty = data_type_of(
                    data_type,
                    value,
                    &[DataType::Number, DataType::GenericName, DataType::FamilyName],
                );
                match ty.as_deref() {
                    Some("generic-name") | Some("family-name") => Some(vec![decl("font-family", value.as_str())]),
                    _ => Some(vec![
                        at_root(vec![property("--tw-font-weight", None, None)]),
                        decl("--tw-font-weight", value.as_str()),
                        decl("font-weight", value.as_str()),
                    ]),
                }
            }
            UtilityValue::Named { value, .. } => {
                if let Some((families, options)) = ds.theme.resolve_with(
                    value,
                    &["--font"],
                    &["--font-feature-settings", "--font-variation-settings"],
                ) {
                    let get = |k: &str| options.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone());
                    return Some(vec![
                        decl("font-family", families),
                        decl_opt("font-feature-settings", get("--font-feature-settings")),
                        decl_opt("font-variation-settings", get("--font-variation-settings")),
                    ]);
                }
                if let Some(v) = ds.theme.resolve(Some(value), &["--font-weight"]) {
                    return Some(vec![
                        at_root(vec![property("--tw-font-weight", None, None)]),
                        decl("--tw-font-weight", v.as_str()),
                        decl("font-weight", v),
                    ]);
                }
                None
            }
        }
    });

    functional_utility(
        u,
        "font-features",
        Desc { handle: hv(|v| vec![decl("font-feature-settings", v)]), ..Default::default() },
    );

    static_utility(u, "uppercase", &[("text-transform", "uppercase")]);
    static_utility(u, "lowercase", &[("text-transform", "lowercase")]);
    static_utility(u, "capitalize", &[("text-transform", "capitalize")]);
    static_utility(u, "normal-case", &[("text-transform", "none")]);

    static_utility(u, "italic", &[("font-style", "italic")]);
    static_utility(u, "not-italic", &[("font-style", "normal")]);
    static_utility(u, "underline", &[("text-decoration-line", "underline")]);
    static_utility(u, "overline", &[("text-decoration-line", "overline")]);
    static_utility(u, "line-through", &[("text-decoration-line", "line-through")]);
    static_utility(u, "no-underline", &[("text-decoration-line", "none")]);

    for value in [
        "normal",
        "ultra-condensed",
        "extra-condensed",
        "condensed",
        "semi-condensed",
        "semi-expanded",
        "expanded",
        "extra-expanded",
        "ultra-expanded",
    ] {
        static_utility(u, &format!("font-stretch-{}", value), &[("font-stretch", value)]);
    }
    functional_utility(
        u,
        "font-stretch",
        Desc {
            handle_bare_value: bare(|b, _| {
                let num_str = b.value.strip_suffix('%')?;
                let num = js_number(num_str)?;
                if !(num.is_finite() && num.fract() == 0.0 && num >= 0.0) {
                    return None;
                }
                if !(50.0..=200.0).contains(&num) {
                    return None;
                }
                Some(b.value.to_string())
            }),
            handle: hv(|v| vec![decl("font-stretch", v)]),
            ..Default::default()
        },
    );

    color_utility(u, "placeholder", vec!["--placeholder-color", "--color"], |v| {
        Some(vec![style_rule(
            "&::placeholder",
            vec![decl("--tw-sort", "placeholder-color"), decl("color", v)],
        )])
    });

    static_utility(u, "decoration-solid", &[("text-decoration-style", "solid")]);
    static_utility(u, "decoration-double", &[("text-decoration-style", "double")]);
    static_utility(u, "decoration-dotted", &[("text-decoration-style", "dotted")]);
    static_utility(u, "decoration-dashed", &[("text-decoration-style", "dashed")]);
    static_utility(u, "decoration-wavy", &[("text-decoration-style", "wavy")]);

    static_utility(u, "decoration-auto", &[("text-decoration-thickness", "auto")]);
    static_utility(u, "decoration-from-font", &[("text-decoration-thickness", "from-font")]);

    u.functional("decoration", |c, ds| {
        let cv = c.value()?;
        match cv {
            UtilityValue::Arbitrary { value, data_type } => {
                let ty = data_type_of(data_type, value, &[DataType::Color, DataType::Length, DataType::Percentage]);
                match ty.as_deref() {
                    Some("length") | Some("percentage") => {
                        if c.modifier.is_some() {
                            return None;
                        }
                        Some(vec![decl("text-decoration-thickness", value.as_str())])
                    }
                    _ => {
                        let v = as_color(value, c.modifier.as_ref(), &ds.theme)?;
                        Some(vec![decl("text-decoration-color", v)])
                    }
                }
            }
            UtilityValue::Named { value, .. } => {
                if let Some(v) = ds.theme.resolve(Some(value), &["--text-decoration-thickness"]) {
                    if c.modifier.is_some() {
                        return None;
                    }
                    return Some(vec![decl("text-decoration-thickness", v)]);
                }
                if is_positive_integer(value) {
                    if c.modifier.is_some() {
                        return None;
                    }
                    return Some(vec![decl("text-decoration-thickness", format!("{}px", value))]);
                }
                let v = resolve_theme_color(c, &ds.theme, &["--text-decoration-color", "--color"])?;
                Some(vec![decl("text-decoration-color", v)])
            }
        }
    });

    functional_utility(
        u,
        "animate",
        Desc {
            theme_keys: vec!["--animate"],
            handle: hv(|v| vec![decl("animation", v)]),
            static_values: sv(&[("none", &[("animation", "none")])]),
            ..Default::default()
        },
    );

    // Filters
    u.functional("filter", |c, _| {
        if c.modifier.is_some() {
            return None;
        }
        match c.value() {
            None => Some(vec![filter_properties(), decl("filter", CSS_FILTER_VALUE)]),
            Some(UtilityValue::Arbitrary { value, .. }) => Some(vec![decl("filter", value.as_str())]),
            Some(UtilityValue::Named { value, .. }) => match value.as_str() {
                "none" => Some(vec![decl("filter", "none")]),
                _ => None,
            },
        }
    });

    u.functional("backdrop-filter", |c, _| {
        if c.modifier.is_some() {
            return None;
        }
        match c.value() {
            None => Some(vec![
                backdrop_filter_properties(),
                decl("-webkit-backdrop-filter", CSS_BACKDROP_FILTER_VALUE),
                decl("backdrop-filter", CSS_BACKDROP_FILTER_VALUE),
            ]),
            Some(UtilityValue::Arbitrary { value, .. }) => Some(vec![
                decl("-webkit-backdrop-filter", value.as_str()),
                decl("backdrop-filter", value.as_str()),
            ]),
            Some(UtilityValue::Named { value, .. }) => match value.as_str() {
                "none" => Some(vec![decl("-webkit-backdrop-filter", "none"), decl("backdrop-filter", "none")]),
                _ => None,
            },
        }
    });

    functional_utility(
        u,
        "blur",
        Desc {
            theme_keys: vec!["--blur"],
            handle: hv(|v| {
                vec![filter_properties(), decl("--tw-blur", format!("blur({})", v)), decl("filter", CSS_FILTER_VALUE)]
            }),
            static_values: vec![(
                "none",
                vec![filter_properties(), decl("--tw-blur", " "), decl("filter", CSS_FILTER_VALUE)],
            )],
            ..Default::default()
        },
    );

    functional_utility(
        u,
        "backdrop-blur",
        Desc {
            theme_keys: vec!["--backdrop-blur", "--blur"],
            handle: hv(|v| {
                vec![
                    backdrop_filter_properties(),
                    decl("--tw-backdrop-blur", format!("blur({})", v)),
                    decl("-webkit-backdrop-filter", CSS_BACKDROP_FILTER_VALUE),
                    decl("backdrop-filter", CSS_BACKDROP_FILTER_VALUE),
                ]
            }),
            static_values: vec![(
                "none",
                vec![
                    backdrop_filter_properties(),
                    decl("--tw-backdrop-blur", " "),
                    decl("-webkit-backdrop-filter", CSS_BACKDROP_FILTER_VALUE),
                    decl("backdrop-filter", CSS_BACKDROP_FILTER_VALUE),
                ],
            )],
            ..Default::default()
        },
    );

    // (name, function, unit, supports_negative, default_value, theme ns, backdrop theme ns)
    #[allow(clippy::type_complexity)]
    let filters: [(&'static str, &'static str, &'static str, bool, Option<&'static str>, &'static str, &'static str); 7] = [
        ("brightness", "brightness", "%", false, None, "--brightness", "--backdrop-brightness"),
        ("contrast", "contrast", "%", false, None, "--contrast", "--backdrop-contrast"),
        ("grayscale", "grayscale", "%", false, Some("100%"), "--grayscale", "--backdrop-grayscale"),
        ("hue-rotate", "hue-rotate", "deg", true, None, "--hue-rotate", "--backdrop-hue-rotate"),
        ("invert", "invert", "%", false, Some("100%"), "--invert", "--backdrop-invert"),
        ("saturate", "saturate", "%", false, None, "--saturate", "--backdrop-saturate"),
        ("sepia", "sepia", "%", false, Some("100%"), "--sepia", "--backdrop-sepia"),
    ];

    for (name, func, unit, supports_negative, default_value, ns, backdrop_ns) in filters {
        let bare_fn = move || bare(move |b, _| is_positive_integer(b.value).then(|| format!("{}{}", b.value, unit)));

        functional_utility(
            u,
            name,
            Desc {
                supports_negative,
                theme_keys: vec![ns],
                handle_bare_value: bare_fn(),
                default_value: default_value.map(|d| Some(d.to_string())),
                handle: hv(move |v| {
                    vec![
                        filter_properties(),
                        decl(format!("--tw-{}", name), format!("{}({})", func, v)),
                        decl("filter", CSS_FILTER_VALUE),
                    ]
                }),
                ..Default::default()
            },
        );

        functional_utility(
            u,
            &format!("backdrop-{}", name),
            Desc {
                supports_negative,
                theme_keys: vec![backdrop_ns, ns],
                handle_bare_value: bare_fn(),
                default_value: default_value.map(|d| Some(d.to_string())),
                handle: hv(move |v| {
                    vec![
                        backdrop_filter_properties(),
                        decl(format!("--tw-backdrop-{}", name), format!("{}({})", func, v)),
                        decl("-webkit-backdrop-filter", CSS_BACKDROP_FILTER_VALUE),
                        decl("backdrop-filter", CSS_BACKDROP_FILTER_VALUE),
                    ]
                }),
                ..Default::default()
            },
        );
    }

    static_utility_nodes(u, "drop-shadow-none", || {
        vec![filter_properties(), decl("--tw-drop-shadow", " "), decl("filter", CSS_FILTER_VALUE)]
    });

    u.functional("drop-shadow", |c, ds| {
        let theme = &ds.theme;
        let mut alpha: Option<String> = None;
        if let Some(m) = &c.modifier {
            match m {
                Modifier::Arbitrary(v) => alpha = Some(v.clone()),
                Modifier::Named(v) => {
                    if crate::tw::utils::is_valid_opacity_value(v) {
                        alpha = Some(format!("{}%", v));
                    }
                }
            }
        }

        let size_nodes = |value: &str, alpha: &Option<String>| -> Vec<AstNode> {
            alpha_replaced_drop_shadow_properties(
                "--tw-drop-shadow-size",
                value,
                alpha.as_deref(),
                &drop_shadow_injector,
                "",
            )
        };

        let Some(cv) = c.value() else {
            let value = theme.get(&["--drop-shadow"])?;
            let resolved = theme.resolve(None, &["--drop-shadow"])?;
            let mut nodes = vec![filter_properties(), decl_opt("--tw-drop-shadow-alpha", alpha.clone())];
            nodes.extend(size_nodes(&value, &alpha));
            nodes.push(decl("--tw-drop-shadow", drop_shadows(&resolved)));
            nodes.push(decl("filter", CSS_FILTER_VALUE));
            return Some(nodes);
        };

        if let UtilityValue::Arbitrary { value, data_type } = cv {
            let ty = data_type_of(data_type, value, &[DataType::Color]);
            if ty.as_deref() == Some("color") {
                let v = as_color(value, c.modifier.as_ref(), theme)?;
                return Some(vec![
                    filter_properties(),
                    decl("--tw-drop-shadow-color", with_alpha(&v, "var(--tw-drop-shadow-alpha)")),
                    decl("--tw-drop-shadow", "var(--tw-drop-shadow-size)"),
                ]);
            }
            if c.modifier.is_some() && alpha.is_none() {
                return None;
            }
            let mut nodes = vec![filter_properties(), decl_opt("--tw-drop-shadow-alpha", alpha.clone())];
            nodes.extend(size_nodes(value, &alpha));
            nodes.push(decl("--tw-drop-shadow", "var(--tw-drop-shadow-size)"));
            nodes.push(decl("filter", CSS_FILTER_VALUE));
            return Some(nodes);
        }

        let name = cv.value();

        // Shadow size
        {
            let key = format!("--drop-shadow-{}", name);
            let value = theme.get(&[key.as_str()]);
            let resolved = theme.resolve(Some(name), &["--drop-shadow"]);
            if let (Some(value), Some(resolved)) = (value, resolved) {
                if !value.is_empty() && !resolved.is_empty() {
                    if c.modifier.is_some() && alpha.is_none() {
                        return None;
                    }
                    let mut nodes = vec![filter_properties(), decl_opt("--tw-drop-shadow-alpha", alpha.clone())];
                    nodes.extend(size_nodes(&value, &alpha));
                    if alpha.is_some() {
                        nodes.push(decl("--tw-drop-shadow", "var(--tw-drop-shadow-size)"));
                    } else {
                        nodes.push(decl("--tw-drop-shadow", drop_shadows(&resolved)));
                    }
                    nodes.push(decl("filter", CSS_FILTER_VALUE));
                    return Some(nodes);
                }
            }
        }

        // Shadow color
        let v = resolve_theme_color(c, theme, &["--drop-shadow-color", "--color"])?;
        if v.is_empty() {
            return None;
        }
        if v == "inherit" {
            return Some(vec![
                filter_properties(),
                decl("--tw-drop-shadow-color", "inherit"),
                decl("--tw-drop-shadow", "var(--tw-drop-shadow-size)"),
            ]);
        }
        Some(vec![
            filter_properties(),
            decl("--tw-drop-shadow-color", with_alpha(&v, "var(--tw-drop-shadow-alpha)")),
            decl("--tw-drop-shadow", "var(--tw-drop-shadow-size)"),
        ])
    });

    functional_utility(
        u,
        "backdrop-opacity",
        Desc {
            theme_keys: vec!["--backdrop-opacity", "--opacity"],
            handle_bare_value: bare(|b, _| is_valid_opacity_value(b.value).then(|| format!("{}%", b.value))),
            handle: hv(|v| {
                vec![
                    backdrop_filter_properties(),
                    decl("--tw-backdrop-opacity", format!("opacity({})", v)),
                    decl("-webkit-backdrop-filter", CSS_BACKDROP_FILTER_VALUE),
                    decl("backdrop-filter", CSS_BACKDROP_FILTER_VALUE),
                ]
            }),
            ..Default::default()
        },
    );

    // Transitions. Upstream uses `functionalUtility` with static values that
    // depend on the theme, so the functional logic is inlined here.
    u.functional("transition", |c, ds| {
        let value: Option<String> = match c.value() {
            None => {
                if c.modifier.is_some() {
                    return None;
                }
                Some(TRANSITION_DEFAULT.to_string())
            }
            Some(UtilityValue::Arbitrary { value, .. }) => {
                if c.modifier.is_some() {
                    return None;
                }
                Some(value.clone())
            }
            Some(UtilityValue::Named { value, fraction }) => {
                let v = ds.theme.resolve(Some(fraction.as_deref().unwrap_or(value)), &["--transition-property"]);
                if v.is_none() && c.modifier.is_none() {
                    let nodes = match value.as_str() {
                        "none" => Some(vec![decl("transition-property", "none")]),
                        "all" => Some(transition_nodes("all", ds)),
                        "colors" => Some(transition_nodes(TRANSITION_COLORS, ds)),
                        "opacity" => Some(transition_nodes("opacity", ds)),
                        "shadow" => Some(transition_nodes("box-shadow", ds)),
                        "transform" => Some(transition_nodes("transform, translate, scale, rotate", ds)),
                        _ => None,
                    };
                    if nodes.is_some() {
                        return nodes;
                    }
                }
                v
            }
        };
        let value = value?;
        Some(transition_nodes(&value, ds))
    });

    static_utility(u, "transition-discrete", &[("transition-behavior", "allow-discrete")]);
    static_utility(u, "transition-normal", &[("transition-behavior", "normal")]);

    functional_utility(
        u,
        "delay",
        Desc {
            handle_bare_value: bare(|b, _| is_positive_integer(b.value).then(|| format!("{}ms", b.value))),
            theme_keys: vec!["--transition-delay"],
            handle: hv(|v| vec![decl("transition-delay", v)]),
            ..Default::default()
        },
    );

    fn transition_duration_property() -> AstNode {
        at_root(vec![property("--tw-duration", None, None)])
    }

    static_utility_nodes(u, "duration-initial", || {
        vec![transition_duration_property(), decl("--tw-duration", "initial")]
    });

    u.functional("duration", |c, ds| {
        if c.modifier.is_some() {
            return None;
        }
        let cv = c.value()?;
        let value = match cv {
            UtilityValue::Arbitrary { value, .. } => Some(value.clone()),
            UtilityValue::Named { value, fraction } => {
                let mut v = ds.theme.resolve(Some(fraction.as_deref().unwrap_or(value)), &["--transition-duration"]);
                if v.is_none() && is_positive_integer(value) {
                    v = Some(format!("{}ms", value));
                }
                v
            }
        }?;
        Some(vec![
            transition_duration_property(),
            decl("--tw-duration", value.as_str()),
            decl("transition-duration", value),
        ])
    });

    fn transition_timing_function_property() -> AstNode {
        at_root(vec![property("--tw-ease", None, None)])
    }

    functional_utility(
        u,
        "ease",
        Desc {
            theme_keys: vec!["--ease"],
            handle: hv(|v| {
                vec![
                    transition_timing_function_property(),
                    decl("--tw-ease", v),
                    decl("transition-timing-function", v),
                ]
            }),
            static_values: vec![
                ("initial", vec![transition_timing_function_property(), decl("--tw-ease", "initial")]),
                (
                    "linear",
                    vec![
                        transition_timing_function_property(),
                        decl("--tw-ease", "linear"),
                        decl("transition-timing-function", "linear"),
                    ],
                ),
            ],
            ..Default::default()
        },
    );
}
