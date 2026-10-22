//! utilities.ts lines ~2018–3656: flexbox/grid alignment, gap, space, divide,
//! overflow, text wrapping, border radius/width/color, backgrounds, gradients
//! and masks.

use super::*;
use crate::tw::ast::{at_root, decl, rule, style_rule, AstNode};
use crate::tw::candidate::{Modifier, UtilityValue};
use crate::tw::utils::{infer_data_type, is_positive_integer, DataType};

/// `candidate.value.dataType ?? inferDataType(value, types)` as a string.
fn arb_type(data_type: &Option<String>, value: &str, types: &[DataType]) -> Option<String> {
    match data_type {
        Some(dt) => Some(dt.clone()),
        None => infer_data_type(value, types).map(|t| t.as_str().to_string()),
    }
}

fn non_empty(v: Option<String>) -> Option<String> {
    v.filter(|s| !s.is_empty())
}

fn border_properties() -> AstNode {
    at_root(vec![property("--tw-border-style", Some("solid"), None)])
}

fn border_side_utility(
    u: &mut Utilities,
    class_root: &str,
    width: impl Fn(&str) -> Vec<AstNode> + 'static,
    color: impl Fn(&str) -> Vec<AstNode> + 'static,
) {
    u.functional(class_root, move |c, ds| {
        let theme = &ds.theme;
        let with_props = |decls: Vec<AstNode>| {
            let mut out = vec![border_properties()];
            out.extend(decls);
            Some(out)
        };

        let Some(value) = c.value() else {
            if c.modifier.is_some() {
                return None;
            }
            let v = theme.get(&["--default-border-width"]).unwrap_or_else(|| "1px".to_string());
            return with_props(width(&v));
        };

        if let UtilityValue::Arbitrary { value, data_type } = value {
            let ty = arb_type(data_type, value, &[DataType::Color, DataType::LineWidth, DataType::Length]);
            return match ty.as_deref() {
                Some("line-width") | Some("length") => {
                    if c.modifier.is_some() {
                        return None;
                    }
                    with_props(width(value))
                }
                _ => {
                    let v = as_color(value, c.modifier.as_ref(), theme)?;
                    Some(color(&v))
                }
            };
        }

        if let Some(v) = non_empty(resolve_theme_color(c, theme, &["--border-color", "--color"])) {
            return Some(color(&v));
        }

        if c.modifier.is_some() {
            return None;
        }
        let named = value.value();
        if let Some(v) = non_empty(theme.resolve(Some(named), &["--border-width"])) {
            return with_props(width(&v));
        }
        if is_positive_integer(named) {
            return with_props(width(&format!("{}px", named)));
        }
        None
    });
}

fn resolve_interpolation_modifier(modifier: Option<&Modifier>) -> String {
    match modifier {
        Some(Modifier::Named(v)) => match v.as_str() {
            "longer" | "shorter" | "increasing" | "decreasing" => format!("in oklch {} hue", v),
            _ => format!("in {}", v),
        },
        Some(Modifier::Arbitrary(v)) => v.clone(),
        None => "in oklab".to_string(),
    }
}

fn gradient_stop_properties() -> AstNode {
    at_root(vec![
        property("--tw-gradient-position", None, None),
        property("--tw-gradient-from", Some("#0000"), Some("<color>")),
        property("--tw-gradient-via", Some("#0000"), Some("<color>")),
        property("--tw-gradient-to", Some("#0000"), Some("<color>")),
        property("--tw-gradient-stops", None, None),
        property("--tw-gradient-via-stops", None, None),
        property("--tw-gradient-from-position", Some("0%"), Some("<length-percentage>")),
        property("--tw-gradient-via-position", Some("50%"), Some("<length-percentage>")),
        property("--tw-gradient-to-position", Some("100%"), Some("<length-percentage>")),
    ])
}

fn gradient_stop_utility(
    u: &mut Utilities,
    class_root: &str,
    color: impl Fn(&str) -> Vec<AstNode> + 'static,
    position: impl Fn(&str) -> Vec<AstNode> + 'static,
) {
    u.functional(class_root, move |c, ds| {
        let theme = &ds.theme;
        let value = c.value()?;

        if let UtilityValue::Arbitrary { value, data_type } = value {
            let ty = arb_type(data_type, value, &[DataType::Color, DataType::Length, DataType::Percentage]);
            return match ty.as_deref() {
                Some("length") | Some("percentage") => {
                    if c.modifier.is_some() {
                        return None;
                    }
                    Some(position(value))
                }
                _ => {
                    let v = as_color(value, c.modifier.as_ref(), theme)?;
                    Some(color(&v))
                }
            };
        }

        if let Some(v) = non_empty(resolve_theme_color(c, theme, &["--background-color", "--color"])) {
            return Some(color(&v));
        }

        if c.modifier.is_some() {
            return None;
        }
        let named = value.value();
        if let Some(v) = non_empty(theme.resolve(Some(named), &["--gradient-color-stop-positions"])) {
            return Some(position(&v));
        } else if let Some(num) = named.strip_suffix('%') {
            if is_positive_integer(num) {
                return Some(position(named));
            }
        }
        None
    });
}

fn mask_properties_gradient() -> AstNode {
    at_root(vec![
        property("--tw-mask-linear", Some("linear-gradient(#fff, #fff)"), None),
        property("--tw-mask-radial", Some("linear-gradient(#fff, #fff)"), None),
        property("--tw-mask-conic", Some("linear-gradient(#fff, #fff)"), None),
    ])
}

fn mask_stop_utility(
    u: &mut Utilities,
    class_root: &str,
    color: impl Fn(&str) -> Vec<AstNode> + 'static,
    position: impl Fn(&str) -> Vec<AstNode> + 'static,
) {
    u.functional(class_root, move |c, ds| {
        let theme = &ds.theme;
        let value = c.value()?;

        if let UtilityValue::Arbitrary { value, data_type } = value {
            let ty = arb_type(data_type, value, &[DataType::Length, DataType::Percentage, DataType::Color]);
            return match ty.as_deref() {
                Some("color") => {
                    let v = as_color(value, c.modifier.as_ref(), theme)?;
                    Some(color(&v))
                }
                Some("percentage") => {
                    if c.modifier.is_some() {
                        return None;
                    }
                    if !is_positive_integer(slice_last(value)) {
                        return None;
                    }
                    Some(position(value))
                }
                _ => {
                    if c.modifier.is_some() {
                        return None;
                    }
                    Some(position(value))
                }
            };
        }

        if let Some(v) = non_empty(resolve_theme_color(c, theme, &["--background-color", "--color"])) {
            return Some(color(&v));
        }

        if c.modifier.is_some() {
            return None;
        }
        let named = value.value();
        match infer_data_type(named, &[DataType::Number, DataType::Percentage])? {
            DataType::Number => {
                non_empty(theme.resolve(None, &["--spacing"]))?;
                if !is_valid_spacing_multiplier(named) {
                    return None;
                }
                Some(position(&format!("--spacing({})", named)))
            }
            DataType::Percentage => {
                if !is_positive_integer(slice_last(named)) {
                    return None;
                }
                Some(position(named))
            }
            _ => None,
        }
    });
}

/// JS `value.slice(0, -1)`
fn slice_last(s: &str) -> &str {
    match s.char_indices().last() {
        Some((i, _)) => &s[..i],
        None => s,
    }
}

fn mask_properties_edge() -> AstNode {
    at_root(vec![
        property("--tw-mask-left", Some("linear-gradient(#fff, #fff)"), None),
        property("--tw-mask-right", Some("linear-gradient(#fff, #fff)"), None),
        property("--tw-mask-bottom", Some("linear-gradient(#fff, #fff)"), None),
        property("--tw-mask-top", Some("linear-gradient(#fff, #fff)"), None),
    ])
}

const MASK_IMAGE: &str = "var(--tw-mask-linear), var(--tw-mask-radial), var(--tw-mask-conic)";

fn mask_edge_nodes(edges: [bool; 4], stop: &str, kind: &str, value: &str) -> Vec<AstNode> {
    let mut nodes = vec![
        mask_properties_gradient(),
        mask_properties_edge(),
        decl("mask-image", MASK_IMAGE),
        decl("mask-composite", "intersect"),
        decl("--tw-mask-linear", "var(--tw-mask-left), var(--tw-mask-right), var(--tw-mask-bottom), var(--tw-mask-top)"),
    ];
    for (i, edge) in ["top", "right", "bottom", "left"].iter().enumerate() {
        if !edges[i] {
            continue;
        }
        nodes.push(decl(
            format!("--tw-mask-{}", edge),
            format!(
                "linear-gradient(to {e}, var(--tw-mask-{e}-from-color) var(--tw-mask-{e}-from-position), var(--tw-mask-{e}-to-color) var(--tw-mask-{e}-to-position))",
                e = edge
            ),
        ));
        nodes.push(at_root(vec![
            property(&format!("--tw-mask-{}-from-position", edge), Some("0%"), None),
            property(&format!("--tw-mask-{}-to-position", edge), Some("100%"), None),
            property(&format!("--tw-mask-{}-from-color", edge), Some("black"), None),
            property(&format!("--tw-mask-{}-to-color", edge), Some("transparent"), None),
        ]));
        nodes.push(decl(format!("--tw-mask-{}-{}-{}", edge, stop, kind), value));
    }
    nodes
}

fn mask_edge_utility(u: &mut Utilities, name: &str, stop: &'static str, edges: [bool; 4]) {
    mask_stop_utility(
        u,
        name,
        move |v| mask_edge_nodes(edges, stop, "color", v),
        move |v| mask_edge_nodes(edges, stop, "position", v),
    );
}

fn mask_properties_linear() -> AstNode {
    at_root(vec![
        property("--tw-mask-linear-position", Some("0deg"), None),
        property("--tw-mask-linear-from-position", Some("0%"), None),
        property("--tw-mask-linear-to-position", Some("100%"), None),
        property("--tw-mask-linear-from-color", Some("black"), None),
        property("--tw-mask-linear-to-color", Some("transparent"), None),
    ])
}

fn mask_properties_radial() -> AstNode {
    at_root(vec![
        property("--tw-mask-radial-from-position", Some("0%"), None),
        property("--tw-mask-radial-to-position", Some("100%"), None),
        property("--tw-mask-radial-from-color", Some("black"), None),
        property("--tw-mask-radial-to-color", Some("transparent"), None),
        property("--tw-mask-radial-shape", Some("ellipse"), None),
        property("--tw-mask-radial-size", Some("farthest-corner"), None),
        property("--tw-mask-radial-position", Some("center"), None),
    ])
}

fn mask_properties_conic() -> AstNode {
    at_root(vec![
        property("--tw-mask-conic-position", Some("0deg"), None),
        property("--tw-mask-conic-from-position", Some("0%"), None),
        property("--tw-mask-conic-to-position", Some("100%"), None),
        property("--tw-mask-conic-from-color", Some("black"), None),
        property("--tw-mask-conic-to-color", Some("transparent"), None),
    ])
}

const LINEAR_STOPS: &str = "var(--tw-mask-linear-position), var(--tw-mask-linear-from-color) var(--tw-mask-linear-from-position), var(--tw-mask-linear-to-color) var(--tw-mask-linear-to-position)";
const RADIAL_STOPS: &str = "var(--tw-mask-radial-shape) var(--tw-mask-radial-size) at var(--tw-mask-radial-position), var(--tw-mask-radial-from-color) var(--tw-mask-radial-from-position), var(--tw-mask-radial-to-color) var(--tw-mask-radial-to-position)";
const CONIC_STOPS: &str = "from var(--tw-mask-conic-position), var(--tw-mask-conic-from-color) var(--tw-mask-conic-from-position), var(--tw-mask-conic-to-color) var(--tw-mask-conic-to-position)";

fn mask_linear_stop(prop: &str, value: &str) -> Vec<AstNode> {
    vec![
        mask_properties_gradient(),
        mask_properties_linear(),
        decl("mask-image", MASK_IMAGE),
        decl("mask-composite", "intersect"),
        decl("--tw-mask-linear-stops", LINEAR_STOPS),
        decl("--tw-mask-linear", "linear-gradient(var(--tw-mask-linear-stops))"),
        decl(prop, value),
    ]
}

fn mask_radial_stop(prop: &str, value: &str) -> Vec<AstNode> {
    vec![
        mask_properties_gradient(),
        mask_properties_radial(),
        decl("mask-image", MASK_IMAGE),
        decl("mask-composite", "intersect"),
        decl("--tw-mask-radial-stops", RADIAL_STOPS),
        decl("--tw-mask-radial", "radial-gradient(var(--tw-mask-radial-stops))"),
        decl(prop, value),
    ]
}

fn mask_conic_stop(prop: &str, value: &str) -> Vec<AstNode> {
    vec![
        mask_properties_gradient(),
        mask_properties_conic(),
        decl("mask-image", MASK_IMAGE),
        decl("mask-composite", "intersect"),
        decl("--tw-mask-conic-stops", CONIC_STOPS),
        decl("--tw-mask-conic", "conic-gradient(var(--tw-mask-conic-stops))"),
        decl(prop, value),
    ]
}

/// `functionalUtility(root, { handle(value) { if (!value) return; return [decl(prop, value)] } })`
fn simple_functional(u: &mut Utilities, root: &str, prop: &'static str) {
    functional_utility(
        u,
        root,
        Desc {
            handle: hf(move |v, _, _| if v.is_empty() { None } else { Some(vec![decl(prop, v)]) }),
            ..Default::default()
        },
    );
}

/// `space-*-0` and friends emit `0` instead of a `calc(…)` expression.
fn is_zero_spacing(value: &str) -> bool {
    if value == "--spacing(0)" || value == "--spacing(-0)" {
        return true;
    }
    matches!(crate::tw::utils::dimensions(value), Some((n, unit)) if n == 0.0 && (unit.is_none() || crate::tw::utils::is_length(value)))
}

pub fn register(u: &mut Utilities) {
    static_utility(u, "flex-row", &[("flex-direction", "row")]);
    static_utility(u, "flex-row-reverse", &[("flex-direction", "row-reverse")]);
    static_utility(u, "flex-col", &[("flex-direction", "column")]);
    static_utility(u, "flex-col-reverse", &[("flex-direction", "column-reverse")]);

    static_utility(u, "flex-wrap", &[("flex-wrap", "wrap")]);
    static_utility(u, "flex-nowrap", &[("flex-wrap", "nowrap")]);
    static_utility(u, "flex-wrap-reverse", &[("flex-wrap", "wrap-reverse")]);

    for (name, value) in [
        ("center", "center"),
        ("start", "start"),
        ("end", "end"),
        ("center-safe", "safe center"),
        ("end-safe", "safe end"),
        ("between", "space-between"),
        ("around", "space-around"),
        ("evenly", "space-evenly"),
        ("baseline", "baseline"),
        ("stretch", "stretch"),
    ] {
        static_utility(u, &format!("place-content-{}", name), &[("place-content", value)]);
    }

    for (name, value) in [
        ("center", "center"),
        ("start", "start"),
        ("end", "end"),
        ("center-safe", "safe center"),
        ("end-safe", "safe end"),
        ("baseline", "baseline"),
        ("stretch", "stretch"),
    ] {
        static_utility(u, &format!("place-items-{}", name), &[("place-items", value)]);
    }

    for (name, value) in [
        ("normal", "normal"),
        ("center", "center"),
        ("start", "flex-start"),
        ("end", "flex-end"),
        ("center-safe", "safe center"),
        ("end-safe", "safe flex-end"),
        ("between", "space-between"),
        ("around", "space-around"),
        ("evenly", "space-evenly"),
        ("baseline", "baseline"),
        ("stretch", "stretch"),
    ] {
        static_utility(u, &format!("content-{}", name), &[("align-content", value)]);
    }

    for (name, value) in [
        ("center", "center"),
        ("start", "flex-start"),
        ("end", "flex-end"),
        ("center-safe", "safe center"),
        ("end-safe", "safe flex-end"),
        ("baseline", "baseline"),
        ("baseline-last", "last baseline"),
        ("stretch", "stretch"),
    ] {
        static_utility(u, &format!("items-{}", name), &[("align-items", value)]);
    }

    for (name, value) in [
        ("normal", "normal"),
        ("center", "center"),
        ("start", "flex-start"),
        ("end", "flex-end"),
        ("center-safe", "safe center"),
        ("end-safe", "safe flex-end"),
        ("between", "space-between"),
        ("around", "space-around"),
        ("evenly", "space-evenly"),
        ("baseline", "baseline"),
        ("stretch", "stretch"),
    ] {
        static_utility(u, &format!("justify-{}", name), &[("justify-content", value)]);
    }

    for (name, value) in [
        ("normal", "normal"),
        ("center", "center"),
        ("start", "start"),
        ("end", "end"),
        ("center-safe", "safe center"),
        ("end-safe", "safe end"),
        ("stretch", "stretch"),
    ] {
        static_utility(u, &format!("justify-items-{}", name), &[("justify-items", value)]);
    }

    spacing_utility(u, "gap", vec!["--gap", "--spacing"], |v| vec![decl("gap", v)], SpacingOpts::default());
    spacing_utility(u, "gap-x", vec!["--gap", "--spacing"], |v| vec![decl("column-gap", v)], SpacingOpts::default());
    spacing_utility(u, "gap-y", vec!["--gap", "--spacing"], |v| vec![decl("row-gap", v)], SpacingOpts::default());

    spacing_utility(
        u,
        "space-x",
        vec!["--space", "--spacing"],
        |v| {
            let zero = is_zero_spacing(v);
            vec![
                at_root(vec![property("--tw-space-x-reverse", Some("0"), None)]),
                style_rule(
                    ":where(& > :not(:last-child))",
                    vec![
                        decl("--tw-sort", "row-gap"),
                        decl("--tw-space-x-reverse", "0"),
                        decl("margin-inline-start", if zero { "0".to_string() } else { format!("calc({} * var(--tw-space-x-reverse))", v) }),
                        decl(
                            "margin-inline-end",
                            if zero { "0".to_string() } else { format!("calc({} * calc(1 - var(--tw-space-x-reverse)))", v) },
                        ),
                    ],
                ),
            ]
        },
        SpacingOpts { supports_negative: true, ..Default::default() },
    );

    spacing_utility(
        u,
        "space-y",
        vec!["--space", "--spacing"],
        |v| {
            let zero = is_zero_spacing(v);
            vec![
                at_root(vec![property("--tw-space-y-reverse", Some("0"), None)]),
                style_rule(
                    ":where(& > :not(:last-child))",
                    vec![
                        decl("--tw-sort", "column-gap"),
                        decl("--tw-space-y-reverse", "0"),
                        decl("margin-block-start", if zero { "0".to_string() } else { format!("calc({} * var(--tw-space-y-reverse))", v) }),
                        decl(
                            "margin-block-end",
                            if zero { "0".to_string() } else { format!("calc({} * calc(1 - var(--tw-space-y-reverse)))", v) },
                        ),
                    ],
                ),
            ]
        },
        SpacingOpts { supports_negative: true, ..Default::default() },
    );

    static_utility_nodes(u, "space-x-reverse", || {
        vec![
            at_root(vec![property("--tw-space-x-reverse", Some("0"), None)]),
            style_rule(
                ":where(& > :not(:last-child))",
                vec![decl("--tw-sort", "row-gap"), decl("--tw-space-x-reverse", "1")],
            ),
        ]
    });

    static_utility_nodes(u, "space-y-reverse", || {
        vec![
            at_root(vec![property("--tw-space-y-reverse", Some("0"), None)]),
            style_rule(
                ":where(& > :not(:last-child))",
                vec![decl("--tw-sort", "column-gap"), decl("--tw-space-y-reverse", "1")],
            ),
        ]
    });

    static_utility(u, "accent-auto", &[("accent-color", "auto")]);
    color_utility(u, "accent", vec!["--accent-color", "--color"], |v| Some(vec![decl("accent-color", v)]));
    color_utility(u, "caret", vec!["--caret-color", "--color"], |v| Some(vec![decl("caret-color", v)]));
    color_utility(u, "divide", vec!["--divide-color", "--border-color", "--color"], |v| {
        Some(vec![style_rule(
            ":where(& > :not(:last-child))",
            vec![decl("--tw-sort", "divide-color"), decl("border-color", v)],
        )])
    });

    for (name, value) in [
        ("auto", "auto"),
        ("start", "start"),
        ("end", "end"),
        ("center", "center"),
        ("end-safe", "safe end"),
        ("center-safe", "safe center"),
        ("stretch", "stretch"),
    ] {
        static_utility(u, &format!("place-self-{}", name), &[("place-self", value)]);
    }

    for (name, value) in [
        ("auto", "auto"),
        ("start", "flex-start"),
        ("end", "flex-end"),
        ("center", "center"),
        ("end-safe", "safe flex-end"),
        ("center-safe", "safe center"),
        ("stretch", "stretch"),
        ("baseline", "baseline"),
        ("baseline-last", "last baseline"),
    ] {
        static_utility(u, &format!("self-{}", name), &[("align-self", value)]);
    }

    for (name, value) in [
        ("auto", "auto"),
        ("start", "flex-start"),
        ("end", "flex-end"),
        ("center", "center"),
        ("end-safe", "safe flex-end"),
        ("center-safe", "safe center"),
        ("stretch", "stretch"),
    ] {
        static_utility(u, &format!("justify-self-{}", name), &[("justify-self", value)]);
    }

    for value in ["auto", "hidden", "clip", "visible", "scroll"] {
        static_utility(u, &format!("overflow-{}", value), &[("overflow", value)]);
        static_utility(u, &format!("overflow-x-{}", value), &[("overflow-x", value)]);
        static_utility(u, &format!("overflow-y-{}", value), &[("overflow-y", value)]);
    }

    for value in ["auto", "contain", "none"] {
        static_utility(u, &format!("overscroll-{}", value), &[("overscroll-behavior", value)]);
        static_utility(u, &format!("overscroll-x-{}", value), &[("overscroll-behavior-x", value)]);
        static_utility(u, &format!("overscroll-y-{}", value), &[("overscroll-behavior-y", value)]);
    }

    static_utility(u, "scroll-auto", &[("scroll-behavior", "auto")]);
    static_utility(u, "scroll-smooth", &[("scroll-behavior", "smooth")]);

    static_utility(u, "scrollbar-auto", &[("scrollbar-width", "auto")]);
    static_utility(u, "scrollbar-thin", &[("scrollbar-width", "thin")]);
    static_utility(u, "scrollbar-none", &[("scrollbar-width", "none")]);

    for (root, var) in [("scrollbar-thumb", "--tw-scrollbar-thumb"), ("scrollbar-track", "--tw-scrollbar-track")] {
        color_utility(u, root, vec!["--color"], move |v| {
            Some(vec![
                at_root(vec![
                    property("--tw-scrollbar-thumb", Some("#0000"), Some("<color>")),
                    property("--tw-scrollbar-track", Some("#0000"), Some("<color>")),
                ]),
                decl(var, v),
                decl("scrollbar-color", "var(--tw-scrollbar-thumb) var(--tw-scrollbar-track)"),
            ])
        });
    }

    static_utility(u, "scrollbar-gutter-auto", &[("scrollbar-gutter", "auto")]);
    static_utility(u, "scrollbar-gutter-stable", &[("scrollbar-gutter", "stable")]);
    static_utility(u, "scrollbar-gutter-both", &[("scrollbar-gutter", "stable both-edges")]);

    static_utility(u, "truncate", &[("overflow", "hidden"), ("text-overflow", "ellipsis"), ("white-space", "nowrap")]);

    static_utility(u, "text-ellipsis", &[("text-overflow", "ellipsis")]);
    static_utility(u, "text-clip", &[("text-overflow", "clip")]);

    for value in ["none", "manual", "auto"] {
        static_utility(u, &format!("hyphens-{}", value), &[("-webkit-hyphens", value), ("hyphens", value)]);
    }

    for (name, value) in [
        ("normal", "normal"),
        ("nowrap", "nowrap"),
        ("pre", "pre"),
        ("pre-line", "pre-line"),
        ("pre-wrap", "pre-wrap"),
        ("break-spaces", "break-spaces"),
    ] {
        static_utility(u, &format!("whitespace-{}", name), &[("white-space", value)]);
    }

    functional_utility(
        u,
        "tab",
        Desc {
            handle_bare_value: bare(|b, _| is_positive_integer(b.value).then(|| b.value.to_string())),
            handle: hv(|v| vec![decl("tab-size", v)]),
            ..Default::default()
        },
    );

    static_utility(u, "text-wrap", &[("text-wrap", "wrap")]);
    static_utility(u, "text-nowrap", &[("text-wrap", "nowrap")]);
    static_utility(u, "text-balance", &[("text-wrap", "balance")]);
    static_utility(u, "text-pretty", &[("text-wrap", "pretty")]);
    static_utility(u, "break-normal", &[("overflow-wrap", "normal"), ("word-break", "normal")]);
    static_utility(u, "break-all", &[("word-break", "break-all")]);
    static_utility(u, "break-keep", &[("word-break", "keep-all")]);

    static_utility(u, "wrap-anywhere", &[("overflow-wrap", "anywhere")]);
    static_utility(u, "wrap-break-word", &[("overflow-wrap", "break-word")]);
    static_utility(u, "wrap-normal", &[("overflow-wrap", "normal")]);

    for (root, properties) in [
        ("rounded", &["border-radius"][..]),
        ("rounded-s", &["border-start-start-radius", "border-end-start-radius"][..]),
        ("rounded-e", &["border-start-end-radius", "border-end-end-radius"][..]),
        ("rounded-t", &["border-top-left-radius", "border-top-right-radius"][..]),
        ("rounded-r", &["border-top-right-radius", "border-bottom-right-radius"][..]),
        ("rounded-b", &["border-bottom-right-radius", "border-bottom-left-radius"][..]),
        ("rounded-l", &["border-top-left-radius", "border-bottom-left-radius"][..]),
        ("rounded-ss", &["border-start-start-radius"][..]),
        ("rounded-se", &["border-start-end-radius"][..]),
        ("rounded-ee", &["border-end-end-radius"][..]),
        ("rounded-es", &["border-end-start-radius"][..]),
        ("rounded-tl", &["border-top-left-radius"][..]),
        ("rounded-tr", &["border-top-right-radius"][..]),
        ("rounded-br", &["border-bottom-right-radius"][..]),
        ("rounded-bl", &["border-bottom-left-radius"][..]),
    ] {
        let props: Vec<&'static str> = properties.to_vec();
        let handle_props = props.clone();
        functional_utility(
            u,
            root,
            Desc {
                theme_keys: vec!["--radius"],
                handle: hv(move |v| handle_props.iter().map(|p| decl(*p, v)).collect()),
                static_values: vec![
                    ("none", props.iter().map(|p| decl(*p, "0")).collect()),
                    ("full", props.iter().map(|p| decl(*p, "calc(infinity * 1px)")).collect()),
                ],
                ..Default::default()
            },
        );
    }

    for value in ["solid", "dashed", "dotted", "double", "hidden", "none"] {
        static_utility(u, &format!("border-{}", value), &[("--tw-border-style", value), ("border-style", value)]);
    }

    for (root, side) in [
        ("border", ""),
        ("border-x", "-inline"),
        ("border-y", "-block"),
        ("border-s", "-inline-start"),
        ("border-e", "-inline-end"),
        ("border-bs", "-block-start"),
        ("border-be", "-block-end"),
        ("border-t", "-top"),
        ("border-r", "-right"),
        ("border-b", "-bottom"),
        ("border-l", "-left"),
    ] {
        border_side_utility(
            u,
            root,
            move |v| {
                vec![
                    decl(format!("border{}-style", side), "var(--tw-border-style)"),
                    decl(format!("border{}-width", side), v),
                ]
            },
            move |v| vec![decl(format!("border{}-color", side), v)],
        );
    }

    // divide-x / divide-y. Upstream's `defaultValue` is read from the theme at
    // registration time; here the no-value case is handled by a separate
    // functional utility that reads the theme at compile time instead.
    for axis in ["x", "y"] {
        let handle = move |v: &str| -> Vec<AstNode> {
            if axis == "x" {
                vec![
                    at_root(vec![property("--tw-divide-x-reverse", Some("0"), None)]),
                    style_rule(
                        ":where(& > :not(:last-child))",
                        vec![
                            decl("--tw-sort", "divide-x-width"),
                            border_properties(),
                            decl("--tw-divide-x-reverse", "0"),
                            decl("border-inline-style", "var(--tw-border-style)"),
                            decl("border-inline-start-width", format!("calc({} * var(--tw-divide-x-reverse))", v)),
                            decl(
                                "border-inline-end-width",
                                format!("calc({} * calc(1 - var(--tw-divide-x-reverse)))", v),
                            ),
                        ],
                    ),
                ]
            } else {
                vec![
                    at_root(vec![property("--tw-divide-y-reverse", Some("0"), None)]),
                    style_rule(
                        ":where(& > :not(:last-child))",
                        vec![
                            decl("--tw-sort", "divide-y-width"),
                            border_properties(),
                            decl("--tw-divide-y-reverse", "0"),
                            decl("border-bottom-style", "var(--tw-border-style)"),
                            decl("border-top-style", "var(--tw-border-style)"),
                            decl("border-top-width", format!("calc({} * var(--tw-divide-y-reverse))", v)),
                            decl(
                                "border-bottom-width",
                                format!("calc({} * calc(1 - var(--tw-divide-y-reverse)))", v),
                            ),
                        ],
                    ),
                ]
            }
        };
        let root = format!("divide-{}", axis);
        u.functional(&root, move |c, ds| {
            if c.value().is_some() || c.modifier.is_some() {
                return None;
            }
            let v = ds.theme.get(&["--default-border-width"]).unwrap_or_else(|| "1px".to_string());
            Some(handle(&v))
        });
        functional_utility(
            u,
            &root,
            Desc {
                default_value: Some(None),
                theme_keys: vec!["--divide-width", "--border-width"],
                handle_bare_value: bare(|b, _| is_positive_integer(b.value).then(|| format!("{}px", b.value))),
                handle: hv(handle),
                ..Default::default()
            },
        );
    }

    for axis in ["x", "y"] {
        static_utility_nodes(u, &format!("divide-{}-reverse", axis), move || {
            vec![
                at_root(vec![property(&format!("--tw-divide-{}-reverse", axis), Some("0"), None)]),
                style_rule(
                    ":where(& > :not(:last-child))",
                    vec![decl(format!("--tw-divide-{}-reverse", axis), "1")],
                ),
            ]
        });
    }

    for value in ["solid", "dashed", "dotted", "double", "none"] {
        static_utility_nodes(u, &format!("divide-{}", value), move || {
            vec![style_rule(
                ":where(& > :not(:last-child))",
                vec![
                    decl("--tw-sort", "divide-style"),
                    decl("--tw-border-style", value),
                    decl("border-style", value),
                ],
            )]
        });
    }

    static_utility(u, "bg-auto", &[("background-size", "auto")]);
    static_utility(u, "bg-cover", &[("background-size", "cover")]);
    static_utility(u, "bg-contain", &[("background-size", "contain")]);
    simple_functional(u, "bg-size", "background-size");

    static_utility(u, "bg-fixed", &[("background-attachment", "fixed")]);
    static_utility(u, "bg-local", &[("background-attachment", "local")]);
    static_utility(u, "bg-scroll", &[("background-attachment", "scroll")]);

    for (name, value) in [
        ("top", "top"),
        ("top-left", "left top"),
        ("top-right", "right top"),
        ("bottom", "bottom"),
        ("bottom-left", "left bottom"),
        ("bottom-right", "right bottom"),
        ("left", "left"),
        ("right", "right"),
        ("center", "center"),
    ] {
        static_utility(u, &format!("bg-{}", name), &[("background-position", value)]);
    }
    simple_functional(u, "bg-position", "background-position");

    static_utility(u, "bg-repeat", &[("background-repeat", "repeat")]);
    static_utility(u, "bg-no-repeat", &[("background-repeat", "no-repeat")]);
    static_utility(u, "bg-repeat-x", &[("background-repeat", "repeat-x")]);
    static_utility(u, "bg-repeat-y", &[("background-repeat", "repeat-y")]);
    static_utility(u, "bg-repeat-round", &[("background-repeat", "round")]);
    static_utility(u, "bg-repeat-space", &[("background-repeat", "space")]);

    static_utility(u, "bg-none", &[("background-image", "none")]);

    for (name, negative) in [("-bg-linear", true), ("bg-linear", false)] {
        u.functional(name, move |c, _| {
            let value = c.value()?;
            if let UtilityValue::Arbitrary { value, data_type } = value {
                if c.modifier.is_some() {
                    return None;
                }
                let ty = arb_type(data_type, value, &[DataType::Angle]);
                let value = if ty.as_deref() == Some("angle") {
                    if negative {
                        format!("calc({} * -1)", value)
                    } else {
                        value.clone()
                    }
                } else {
                    if negative {
                        return None;
                    }
                    value.clone()
                };
                return Some(vec![
                    decl("--tw-gradient-position", value.as_str()),
                    decl("background-image", format!("linear-gradient(var(--tw-gradient-stops,{}))", value)),
                ]);
            }

            let v = value.value();
            let direction = match v {
                "to-t" => Some("to top"),
                "to-tr" => Some("to top right"),
                "to-r" => Some("to right"),
                "to-br" => Some("to bottom right"),
                "to-b" => Some("to bottom"),
                "to-bl" => Some("to bottom left"),
                "to-l" => Some("to left"),
                "to-tl" => Some("to top left"),
                _ => None,
            };
            let value = if let (false, Some(d)) = (negative, direction) {
                d.to_string()
            } else if is_positive_integer(v) {
                if negative {
                    format!("calc({}deg * -1)", v)
                } else {
                    format!("{}deg", v)
                }
            } else {
                return None;
            };

            let interpolation = resolve_interpolation_modifier(c.modifier.as_ref());
            Some(vec![
                decl("--tw-gradient-position", value.as_str()),
                rule(
                    "@supports (background-image: linear-gradient(in lab, red, red))",
                    vec![decl("--tw-gradient-position", format!("{} {}", value, interpolation))],
                ),
                decl("background-image", "linear-gradient(var(--tw-gradient-stops))"),
            ])
        });
    }

    for (name, negative) in [("-bg-conic", true), ("bg-conic", false)] {
        u.functional(name, move |c, _| {
            if let Some(UtilityValue::Arbitrary { value, .. }) = c.value() {
                if c.modifier.is_some() {
                    return None;
                }
                return Some(vec![
                    decl("--tw-gradient-position", value.as_str()),
                    decl("background-image", format!("conic-gradient(var(--tw-gradient-stops,{}))", value)),
                ]);
            }

            let interpolation = resolve_interpolation_modifier(c.modifier.as_ref());

            let Some(value) = c.value() else {
                return Some(vec![
                    decl("--tw-gradient-position", interpolation),
                    decl("background-image", "conic-gradient(var(--tw-gradient-stops))"),
                ]);
            };

            let v = value.value();
            if !is_positive_integer(v) {
                return None;
            }
            let v = if negative { format!("calc({}deg * -1)", v) } else { format!("{}deg", v) };
            Some(vec![
                decl("--tw-gradient-position", format!("from {} {}", v, interpolation)),
                decl("background-image", "conic-gradient(var(--tw-gradient-stops))"),
            ])
        });
    }

    u.functional("bg-radial", |c, _| {
        let Some(value) = c.value() else {
            let interpolation = resolve_interpolation_modifier(c.modifier.as_ref());
            return Some(vec![
                decl("--tw-gradient-position", interpolation),
                decl("background-image", "radial-gradient(var(--tw-gradient-stops))"),
            ]);
        };
        if let UtilityValue::Arbitrary { value, .. } = value {
            if c.modifier.is_some() {
                return None;
            }
            return Some(vec![
                decl("--tw-gradient-position", value.as_str()),
                decl("background-image", format!("radial-gradient(var(--tw-gradient-stops,{}))", value)),
            ]);
        }
        None
    });

    u.functional("bg", |c, ds| {
        let theme = &ds.theme;
        let value = c.value()?;

        if let UtilityValue::Arbitrary { value, data_type } = value {
            let ty = arb_type(
                data_type,
                value,
                &[
                    DataType::Image,
                    DataType::Color,
                    DataType::Percentage,
                    DataType::Position,
                    DataType::BgSize,
                    DataType::Length,
                    DataType::Url,
                ],
            );
            return match ty.as_deref() {
                Some("percentage") | Some("position") => {
                    if c.modifier.is_some() {
                        return None;
                    }
                    Some(vec![decl("background-position", value.as_str())])
                }
                Some("bg-size") | Some("length") | Some("size") => {
                    if c.modifier.is_some() {
                        return None;
                    }
                    Some(vec![decl("background-size", value.as_str())])
                }
                Some("image") | Some("url") => {
                    if c.modifier.is_some() {
                        return None;
                    }
                    Some(vec![decl("background-image", value.as_str())])
                }
                _ => {
                    let v = as_color(value, c.modifier.as_ref(), theme)?;
                    Some(vec![decl("background-color", v)])
                }
            };
        }

        if let Some(v) = non_empty(resolve_theme_color(c, theme, &["--background-color", "--color"])) {
            return Some(vec![decl("background-color", v)]);
        }

        if c.modifier.is_some() {
            return None;
        }
        if let Some(v) = non_empty(theme.resolve(Some(value.value()), &["--background-image"])) {
            return Some(vec![decl("background-image", v)]);
        }
        None
    });

    const GRADIENT_STOPS: &str = "var(--tw-gradient-via-stops, var(--tw-gradient-position), var(--tw-gradient-from) var(--tw-gradient-from-position), var(--tw-gradient-to) var(--tw-gradient-to-position))";

    gradient_stop_utility(
        u,
        "from",
        |v| {
            vec![
                gradient_stop_properties(),
                decl("--tw-sort", "--tw-gradient-from"),
                decl("--tw-gradient-from", v),
                decl("--tw-gradient-stops", GRADIENT_STOPS),
            ]
        },
        |v| vec![gradient_stop_properties(), decl("--tw-gradient-from-position", v)],
    );
    static_utility(u, "via-none", &[("--tw-gradient-via-stops", "initial")]);
    gradient_stop_utility(
        u,
        "via",
        |v| {
            vec![
                gradient_stop_properties(),
                decl("--tw-sort", "--tw-gradient-via"),
                decl("--tw-gradient-via", v),
                decl(
                    "--tw-gradient-via-stops",
                    "var(--tw-gradient-position), var(--tw-gradient-from) var(--tw-gradient-from-position), var(--tw-gradient-via) var(--tw-gradient-via-position), var(--tw-gradient-to) var(--tw-gradient-to-position)",
                ),
                decl("--tw-gradient-stops", "var(--tw-gradient-via-stops)"),
            ]
        },
        |v| vec![gradient_stop_properties(), decl("--tw-gradient-via-position", v)],
    );
    gradient_stop_utility(
        u,
        "to",
        |v| {
            vec![
                gradient_stop_properties(),
                decl("--tw-sort", "--tw-gradient-to"),
                decl("--tw-gradient-to", v),
                decl("--tw-gradient-stops", GRADIENT_STOPS),
            ]
        },
        |v| vec![gradient_stop_properties(), decl("--tw-gradient-to-position", v)],
    );

    // mask-image
    static_utility(u, "mask-none", &[("mask-image", "none")]);

    u.functional("mask", |c, _| {
        let value = c.value()?;
        if c.modifier.is_some() {
            return None;
        }
        let UtilityValue::Arbitrary { value, data_type } = value else { return None };
        let ty = arb_type(
            data_type,
            value,
            &[
                DataType::Image,
                DataType::Percentage,
                DataType::Position,
                DataType::BgSize,
                DataType::Length,
                DataType::Url,
            ],
        );
        Some(match ty.as_deref() {
            Some("percentage") | Some("position") => vec![decl("mask-position", value.as_str())],
            Some("bg-size") | Some("length") | Some("size") => vec![decl("mask-size", value.as_str())],
            _ => vec![decl("mask-image", value.as_str())],
        })
    });

    static_utility(u, "mask-add", &[("mask-composite", "add")]);
    static_utility(u, "mask-subtract", &[("mask-composite", "subtract")]);
    static_utility(u, "mask-intersect", &[("mask-composite", "intersect")]);
    static_utility(u, "mask-exclude", &[("mask-composite", "exclude")]);

    static_utility(u, "mask-alpha", &[("mask-mode", "alpha")]);
    static_utility(u, "mask-luminance", &[("mask-mode", "luminance")]);
    static_utility(u, "mask-match", &[("mask-mode", "match-source")]);

    static_utility(u, "mask-type-alpha", &[("mask-type", "alpha")]);
    static_utility(u, "mask-type-luminance", &[("mask-type", "luminance")]);

    static_utility(u, "mask-auto", &[("mask-size", "auto")]);
    static_utility(u, "mask-cover", &[("mask-size", "cover")]);
    static_utility(u, "mask-contain", &[("mask-size", "contain")]);
    simple_functional(u, "mask-size", "mask-size");

    for (name, value) in [
        ("top", "top"),
        ("top-left", "left top"),
        ("top-right", "right top"),
        ("bottom", "bottom"),
        ("bottom-left", "left bottom"),
        ("bottom-right", "right bottom"),
        ("left", "left"),
        ("right", "right"),
        ("center", "center"),
    ] {
        static_utility(u, &format!("mask-{}", name), &[("mask-position", value)]);
    }
    simple_functional(u, "mask-position", "mask-position");

    static_utility(u, "mask-repeat", &[("mask-repeat", "repeat")]);
    static_utility(u, "mask-no-repeat", &[("mask-repeat", "no-repeat")]);
    static_utility(u, "mask-repeat-x", &[("mask-repeat", "repeat-x")]);
    static_utility(u, "mask-repeat-y", &[("mask-repeat", "repeat-y")]);
    static_utility(u, "mask-repeat-round", &[("mask-repeat", "round")]);
    static_utility(u, "mask-repeat-space", &[("mask-repeat", "space")]);

    static_utility(u, "mask-clip-border", &[("mask-clip", "border-box")]);
    static_utility(u, "mask-clip-padding", &[("mask-clip", "padding-box")]);
    static_utility(u, "mask-clip-content", &[("mask-clip", "content-box")]);
    static_utility(u, "mask-clip-fill", &[("mask-clip", "fill-box")]);
    static_utility(u, "mask-clip-stroke", &[("mask-clip", "stroke-box")]);
    static_utility(u, "mask-clip-view", &[("mask-clip", "view-box")]);
    static_utility(u, "mask-no-clip", &[("mask-clip", "no-clip")]);

    static_utility(u, "mask-origin-border", &[("mask-origin", "border-box")]);
    static_utility(u, "mask-origin-padding", &[("mask-origin", "padding-box")]);
    static_utility(u, "mask-origin-content", &[("mask-origin", "content-box")]);
    static_utility(u, "mask-origin-fill", &[("mask-origin", "fill-box")]);
    static_utility(u, "mask-origin-stroke", &[("mask-origin", "stroke-box")]);
    static_utility(u, "mask-origin-view", &[("mask-origin", "view-box")]);

    // Edge masks: [top, right, bottom, left]
    mask_edge_utility(u, "mask-x-from", "from", [false, true, false, true]);
    mask_edge_utility(u, "mask-x-to", "to", [false, true, false, true]);
    mask_edge_utility(u, "mask-y-from", "from", [true, false, true, false]);
    mask_edge_utility(u, "mask-y-to", "to", [true, false, true, false]);
    mask_edge_utility(u, "mask-t-from", "from", [true, false, false, false]);
    mask_edge_utility(u, "mask-t-to", "to", [true, false, false, false]);
    mask_edge_utility(u, "mask-r-from", "from", [false, true, false, false]);
    mask_edge_utility(u, "mask-r-to", "to", [false, true, false, false]);
    mask_edge_utility(u, "mask-b-from", "from", [false, false, true, false]);
    mask_edge_utility(u, "mask-b-to", "to", [false, false, true, false]);
    mask_edge_utility(u, "mask-l-from", "from", [false, false, false, true]);
    mask_edge_utility(u, "mask-l-to", "to", [false, false, false, true]);

    // Linear masks
    functional_utility(
        u,
        "mask-linear",
        Desc {
            default_value: Some(None),
            supports_negative: true,
            supports_fractions: false,
            handle_bare_value: bare(|b, _| {
                if !is_positive_integer(b.value) {
                    return None;
                }
                Some(match js_number(b.value) {
                    Some(n) if n == 0.0 => "0deg".to_string(),
                    Some(n) if n == 1.0 => "1deg".to_string(),
                    _ => format!("calc(1deg * {})", b.value),
                })
            }),
            handle_negative_bare_value: bare(|b, _| {
                if !is_positive_integer(b.value) {
                    return None;
                }
                Some(match js_number(b.value) {
                    Some(n) if n == 0.0 => "0deg".to_string(),
                    Some(n) if n == 1.0 => "-1deg".to_string(),
                    _ => format!("calc(1deg * -{})", b.value),
                })
            }),
            handle: hv(|v| {
                vec![
                    mask_properties_gradient(),
                    mask_properties_linear(),
                    decl("mask-image", MASK_IMAGE),
                    decl("mask-composite", "intersect"),
                    decl("--tw-mask-linear", "linear-gradient(var(--tw-mask-linear-stops, var(--tw-mask-linear-position)))"),
                    decl("--tw-mask-linear-position", v),
                ]
            }),
            ..Default::default()
        },
    );

    mask_stop_utility(
        u,
        "mask-linear-from",
        |v| mask_linear_stop("--tw-mask-linear-from-color", v),
        |v| mask_linear_stop("--tw-mask-linear-from-position", v),
    );
    mask_stop_utility(
        u,
        "mask-linear-to",
        |v| mask_linear_stop("--tw-mask-linear-to-color", v),
        |v| mask_linear_stop("--tw-mask-linear-to-position", v),
    );

    // Radial masks
    static_utility(u, "mask-circle", &[("--tw-mask-radial-shape", "circle")]);
    static_utility(u, "mask-ellipse", &[("--tw-mask-radial-shape", "ellipse")]);
    static_utility(u, "mask-radial-closest-side", &[("--tw-mask-radial-size", "closest-side")]);
    static_utility(u, "mask-radial-farthest-side", &[("--tw-mask-radial-size", "farthest-side")]);
    static_utility(u, "mask-radial-closest-corner", &[("--tw-mask-radial-size", "closest-corner")]);
    static_utility(u, "mask-radial-farthest-corner", &[("--tw-mask-radial-size", "farthest-corner")]);
    for (name, value) in [
        ("top", "top"),
        ("top-left", "top left"),
        ("top-right", "top right"),
        ("bottom", "bottom"),
        ("bottom-left", "bottom left"),
        ("bottom-right", "bottom right"),
        ("left", "left"),
        ("right", "right"),
        ("center", "center"),
    ] {
        static_utility(u, &format!("mask-radial-at-{}", name), &[("--tw-mask-radial-position", value)]);
    }
    functional_utility(
        u,
        "mask-radial-at",
        Desc {
            default_value: Some(None),
            handle: hv(|v| vec![decl("--tw-mask-radial-position", v)]),
            ..Default::default()
        },
    );

    functional_utility(
        u,
        "mask-radial",
        Desc {
            default_value: Some(None),
            handle: hv(|v| {
                vec![
                    mask_properties_gradient(),
                    mask_properties_radial(),
                    decl("mask-image", MASK_IMAGE),
                    decl("mask-composite", "intersect"),
                    decl("--tw-mask-radial", "radial-gradient(var(--tw-mask-radial-stops, var(--tw-mask-radial-size)))"),
                    decl("--tw-mask-radial-size", v),
                ]
            }),
            ..Default::default()
        },
    );

    mask_stop_utility(
        u,
        "mask-radial-from",
        |v| mask_radial_stop("--tw-mask-radial-from-color", v),
        |v| mask_radial_stop("--tw-mask-radial-from-position", v),
    );
    mask_stop_utility(
        u,
        "mask-radial-to",
        |v| mask_radial_stop("--tw-mask-radial-to-color", v),
        |v| mask_radial_stop("--tw-mask-radial-to-position", v),
    );

    // Conic masks
    functional_utility(
        u,
        "mask-conic",
        Desc {
            default_value: Some(None),
            supports_negative: true,
            supports_fractions: false,
            handle_bare_value: bare(|b, _| {
                if !is_positive_integer(b.value) {
                    return None;
                }
                Some(match js_number(b.value) {
                    Some(n) if n == 0.0 => "0deg".to_string(),
                    Some(n) if n == 1.0 => "1deg".to_string(),
                    _ => format!("calc(1deg * {})", b.value),
                })
            }),
            handle_negative_bare_value: bare(|b, _| {
                if !is_positive_integer(b.value) {
                    return None;
                }
                Some(match js_number(b.value) {
                    Some(n) if n == 0.0 => "0deg".to_string(),
                    Some(n) if n == 1.0 => "-1deg".to_string(),
                    _ => format!("calc(1deg * -{})", b.value),
                })
            }),
            handle: hv(|v| {
                vec![
                    mask_properties_gradient(),
                    mask_properties_conic(),
                    decl("mask-image", MASK_IMAGE),
                    decl("mask-composite", "intersect"),
                    decl("--tw-mask-conic", "conic-gradient(var(--tw-mask-conic-stops, var(--tw-mask-conic-position)))"),
                    decl("--tw-mask-conic-position", v),
                ]
            }),
            ..Default::default()
        },
    );

    mask_stop_utility(
        u,
        "mask-conic-from",
        |v| mask_conic_stop("--tw-mask-conic-from-color", v),
        |v| mask_conic_stop("--tw-mask-conic-from-position", v),
    );
    mask_stop_utility(
        u,
        "mask-conic-to",
        |v| mask_conic_stop("--tw-mask-conic-to-color", v),
        |v| mask_conic_stop("--tw-mask-conic-to-position", v),
    );

}
