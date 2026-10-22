//! utilities.ts lines ~577–2017: layout, position, grid placement, sizing,
//! flex, transforms, interactivity.

use super::*;
use crate::tw::ast::{at_root, at_rule, decl, AstNode};
use crate::tw::candidate::UtilityValue;
use crate::tw::utils::{compare_breakpoints, infer_data_type, is_positive_integer, is_strict_positive_integer, DataType};

/// Build a static values table.
pub fn sv(entries: &[(&'static str, &[(&str, &str)])]) -> Vec<(&'static str, Vec<AstNode>)> {
    entries
        .iter()
        .map(|(k, decls)| (*k, decls.iter().map(|(p, v)| decl(*p, *v)).collect()))
        .collect()
}

pub fn register(u: &mut Utilities) {
    static_utility(
        u,
        "sr-only",
        &[
            ("position", "absolute"),
            ("width", "1px"),
            ("height", "1px"),
            ("padding", "0"),
            ("margin", "-1px"),
            ("overflow", "hidden"),
            ("clip-path", "inset(50%)"),
            ("white-space", "nowrap"),
            ("border-width", "0"),
        ],
    );
    static_utility(
        u,
        "not-sr-only",
        &[
            ("position", "static"),
            ("width", "auto"),
            ("height", "auto"),
            ("padding", "0"),
            ("margin", "0"),
            ("overflow", "visible"),
            ("clip-path", "none"),
            ("white-space", "normal"),
        ],
    );

    static_utility(u, "pointer-events-none", &[("pointer-events", "none")]);
    static_utility(u, "pointer-events-auto", &[("pointer-events", "auto")]);

    static_utility(u, "visible", &[("visibility", "visible")]);
    static_utility(u, "invisible", &[("visibility", "hidden")]);
    static_utility(u, "collapse", &[("visibility", "collapse")]);

    static_utility(u, "static", &[("position", "static")]);
    static_utility(u, "fixed", &[("position", "fixed")]);
    static_utility(u, "absolute", &[("position", "absolute")]);
    static_utility(u, "relative", &[("position", "relative")]);
    static_utility(u, "sticky", &[("position", "sticky")]);

    for (name, property) in [
        ("inset", "inset"),
        ("inset-x", "inset-inline"),
        ("inset-y", "inset-block"),
        ("inset-s", "inset-inline-start"),
        ("inset-e", "inset-inline-end"),
        ("inset-bs", "inset-block-start"),
        ("inset-be", "inset-block-end"),
        ("top", "top"),
        ("right", "right"),
        ("bottom", "bottom"),
        ("left", "left"),
    ] {
        static_utility(u, &format!("{}-auto", name), &[(property, "auto")]);
        static_utility(u, &format!("{}-full", name), &[(property, "100%")]);
        static_utility(u, &format!("-{}-full", name), &[(property, "-100%")]);
        spacing_utility(
            u,
            name,
            vec!["--inset", "--spacing"],
            move |v| vec![decl(property, v)],
            SpacingOpts { supports_negative: true, supports_fractions: true, ..Default::default() },
        );
    }

    static_utility(u, "isolate", &[("isolation", "isolate")]);
    static_utility(u, "isolation-auto", &[("isolation", "auto")]);

    functional_utility(
        u,
        "z",
        Desc {
            supports_negative: true,
            handle_bare_value: bare_positive_integer(),
            theme_keys: vec!["--z-index"],
            handle: hv(|v| vec![decl("z-index", v)]),
            static_values: sv(&[("auto", &[("z-index", "auto")])]),
            ..Default::default()
        },
    );

    functional_utility(
        u,
        "order",
        Desc {
            supports_negative: true,
            handle_bare_value: bare_positive_integer(),
            theme_keys: vec!["--order"],
            handle: hv(|v| vec![decl("order", v)]),
            static_values: sv(&[("first", &[("order", "-9999")]), ("last", &[("order", "9999")])]),
            ..Default::default()
        },
    );

    functional_utility(
        u,
        "col",
        Desc {
            supports_negative: true,
            handle_bare_value: bare_positive_integer(),
            theme_keys: vec!["--grid-column"],
            handle: hv(|v| vec![decl("grid-column", v)]),
            static_values: sv(&[("auto", &[("grid-column", "auto")])]),
            ..Default::default()
        },
    );

    functional_utility(
        u,
        "col-span",
        Desc {
            handle_bare_value: bare_positive_integer(),
            handle: hv(|v| vec![decl("grid-column", format!("span {} / span {}", v, v))]),
            static_values: sv(&[("full", &[("grid-column", "1 / -1")])]),
            ..Default::default()
        },
    );

    functional_utility(
        u,
        "col-start",
        Desc {
            supports_negative: true,
            handle_bare_value: bare_positive_integer(),
            theme_keys: vec!["--grid-column-start"],
            handle: hv(|v| vec![decl("grid-column-start", v)]),
            static_values: sv(&[("auto", &[("grid-column-start", "auto")])]),
            ..Default::default()
        },
    );

    functional_utility(
        u,
        "col-end",
        Desc {
            supports_negative: true,
            handle_bare_value: bare_positive_integer(),
            theme_keys: vec!["--grid-column-end"],
            handle: hv(|v| vec![decl("grid-column-end", v)]),
            static_values: sv(&[("auto", &[("grid-column-end", "auto")])]),
            ..Default::default()
        },
    );

    functional_utility(
        u,
        "row",
        Desc {
            supports_negative: true,
            handle_bare_value: bare_positive_integer(),
            theme_keys: vec!["--grid-row"],
            handle: hv(|v| vec![decl("grid-row", v)]),
            static_values: sv(&[("auto", &[("grid-row", "auto")])]),
            ..Default::default()
        },
    );

    functional_utility(
        u,
        "row-span",
        Desc {
            handle_bare_value: bare_positive_integer(),
            handle: hv(|v| vec![decl("grid-row", format!("span {} / span {}", v, v))]),
            static_values: sv(&[("full", &[("grid-row", "1 / -1")])]),
            ..Default::default()
        },
    );

    functional_utility(
        u,
        "row-start",
        Desc {
            supports_negative: true,
            handle_bare_value: bare_positive_integer(),
            theme_keys: vec!["--grid-row-start"],
            handle: hv(|v| vec![decl("grid-row-start", v)]),
            static_values: sv(&[("auto", &[("grid-row-start", "auto")])]),
            ..Default::default()
        },
    );

    functional_utility(
        u,
        "row-end",
        Desc {
            supports_negative: true,
            handle_bare_value: bare_positive_integer(),
            theme_keys: vec!["--grid-row-end"],
            handle: hv(|v| vec![decl("grid-row-end", v)]),
            static_values: sv(&[("auto", &[("grid-row-end", "auto")])]),
            ..Default::default()
        },
    );

    static_utility(u, "float-start", &[("float", "inline-start")]);
    static_utility(u, "float-end", &[("float", "inline-end")]);
    static_utility(u, "float-right", &[("float", "right")]);
    static_utility(u, "float-left", &[("float", "left")]);
    static_utility(u, "float-none", &[("float", "none")]);

    static_utility(u, "clear-start", &[("clear", "inline-start")]);
    static_utility(u, "clear-end", &[("clear", "inline-end")]);
    static_utility(u, "clear-right", &[("clear", "right")]);
    static_utility(u, "clear-left", &[("clear", "left")]);
    static_utility(u, "clear-both", &[("clear", "both")]);
    static_utility(u, "clear-none", &[("clear", "none")]);

    for (namespace, property) in [
        ("m", "margin"),
        ("mx", "margin-inline"),
        ("my", "margin-block"),
        ("ms", "margin-inline-start"),
        ("me", "margin-inline-end"),
        ("mbs", "margin-block-start"),
        ("mbe", "margin-block-end"),
        ("mt", "margin-top"),
        ("mr", "margin-right"),
        ("mb", "margin-bottom"),
        ("ml", "margin-left"),
    ] {
        static_utility(u, &format!("{}-auto", namespace), &[(property, "auto")]);
        spacing_utility(
            u,
            namespace,
            vec!["--margin", "--spacing"],
            move |v| vec![decl(property, v)],
            SpacingOpts { supports_negative: true, ..Default::default() },
        );
    }

    static_utility(u, "box-border", &[("box-sizing", "border-box")]);
    static_utility(u, "box-content", &[("box-sizing", "content-box")]);

    functional_utility(
        u,
        "line-clamp",
        Desc {
            theme_keys: vec!["--line-clamp"],
            handle_bare_value: bare_positive_integer(),
            handle: hv(|v| {
                vec![
                    decl("overflow", "hidden"),
                    decl("display", "-webkit-box"),
                    decl("-webkit-box-orient", "vertical"),
                    decl("-webkit-line-clamp", v),
                ]
            }),
            static_values: sv(&[(
                "none",
                &[
                    ("overflow", "visible"),
                    ("display", "block"),
                    ("-webkit-box-orient", "horizontal"),
                    ("-webkit-line-clamp", "unset"),
                ],
            )]),
            ..Default::default()
        },
    );

    for (name, value) in [
        ("block", "block"),
        ("inline-block", "inline-block"),
        ("inline", "inline"),
        ("hidden", "none"),
        ("inline-flex", "inline-flex"),
        ("table", "table"),
        ("inline-table", "inline-table"),
        ("table-caption", "table-caption"),
        ("table-cell", "table-cell"),
        ("table-column", "table-column"),
        ("table-column-group", "table-column-group"),
        ("table-footer-group", "table-footer-group"),
        ("table-header-group", "table-header-group"),
        ("table-row-group", "table-row-group"),
        ("table-row", "table-row"),
        ("flow-root", "flow-root"),
        ("flex", "flex"),
        ("grid", "grid"),
        ("inline-grid", "inline-grid"),
        ("contents", "contents"),
        ("list-item", "list-item"),
    ] {
        static_utility(u, name, &[("display", value)]);
    }

    static_utility(u, "field-sizing-content", &[("field-sizing", "content")]);
    static_utility(u, "field-sizing-fixed", &[("field-sizing", "fixed")]);

    functional_utility(
        u,
        "aspect",
        Desc {
            theme_keys: vec!["--aspect"],
            handle_bare_value: bare(|b, _| {
                let fraction = b.fraction?;
                let parts = segment(fraction, b'/');
                let rhs = parts.get(1).copied().unwrap_or("");
                if !is_valid_spacing_multiplier(parts[0]) || !is_valid_spacing_multiplier(rhs) {
                    return None;
                }
                Some(fraction.to_string())
            }),
            handle: hv(|v| vec![decl("aspect-ratio", v)]),
            static_values: sv(&[("auto", &[("aspect-ratio", "auto")]), ("square", &[("aspect-ratio", "1 / 1")])]),
            ..Default::default()
        },
    );

    for (key, value) in [
        ("full", "100%"),
        ("svw", "100svw"),
        ("lvw", "100lvw"),
        ("dvw", "100dvw"),
        ("svh", "100svh"),
        ("lvh", "100lvh"),
        ("dvh", "100dvh"),
        ("min", "min-content"),
        ("max", "max-content"),
        ("fit", "fit-content"),
    ] {
        static_utility(u, &format!("size-{}", key), &[("--tw-sort", "size"), ("width", value), ("height", value)]);
        static_utility(u, &format!("w-{}", key), &[("width", value)]);
        static_utility(u, &format!("h-{}", key), &[("height", value)]);
        static_utility(u, &format!("min-w-{}", key), &[("min-width", value)]);
        static_utility(u, &format!("min-h-{}", key), &[("min-height", value)]);
        static_utility(u, &format!("max-w-{}", key), &[("max-width", value)]);
        static_utility(u, &format!("max-h-{}", key), &[("max-height", value)]);
    }

    static_utility(u, "size-auto", &[("--tw-sort", "size"), ("width", "auto"), ("height", "auto")]);
    static_utility(u, "w-auto", &[("width", "auto")]);
    static_utility(u, "h-auto", &[("height", "auto")]);
    static_utility(u, "min-w-auto", &[("min-width", "auto")]);
    static_utility(u, "min-h-auto", &[("min-height", "auto")]);

    static_utility(u, "h-lh", &[("height", "1lh")]);
    static_utility(u, "min-h-lh", &[("min-height", "1lh")]);
    static_utility(u, "max-h-lh", &[("max-height", "1lh")]);

    static_utility(u, "w-screen", &[("width", "100vw")]);
    static_utility(u, "min-w-screen", &[("min-width", "100vw")]);
    static_utility(u, "max-w-screen", &[("max-width", "100vw")]);
    static_utility(u, "h-screen", &[("height", "100vh")]);
    static_utility(u, "min-h-screen", &[("min-height", "100vh")]);
    static_utility(u, "max-h-screen", &[("max-height", "100vh")]);

    static_utility(u, "max-w-none", &[("max-width", "none")]);
    static_utility(u, "max-h-none", &[("max-height", "none")]);

    spacing_utility(
        u,
        "size",
        vec!["--size", "--spacing"],
        |v| vec![decl("--tw-sort", "size"), decl("width", v), decl("height", v)],
        SpacingOpts { supports_fractions: true, ..Default::default() },
    );

    for (name, namespaces, property) in [
        ("w", vec!["--width", "--spacing", "--container"], "width"),
        ("min-w", vec!["--min-width", "--spacing", "--container"], "min-width"),
        ("max-w", vec!["--max-width", "--spacing", "--container"], "max-width"),
        ("h", vec!["--height", "--spacing"], "height"),
        ("min-h", vec!["--min-height", "--height", "--spacing"], "min-height"),
        ("max-h", vec!["--max-height", "--height", "--spacing"], "max-height"),
    ] {
        spacing_utility(
            u,
            name,
            namespaces,
            move |v| vec![decl(property, v)],
            SpacingOpts { supports_fractions: true, ..Default::default() },
        );
    }

    for (key, value) in [("full", "100%"), ("min", "min-content"), ("max", "max-content"), ("fit", "fit-content")] {
        static_utility(u, &format!("inline-{}", key), &[("inline-size", value)]);
        static_utility(u, &format!("block-{}", key), &[("block-size", value)]);
        static_utility(u, &format!("min-inline-{}", key), &[("min-inline-size", value)]);
        static_utility(u, &format!("min-block-{}", key), &[("min-block-size", value)]);
        static_utility(u, &format!("max-inline-{}", key), &[("max-inline-size", value)]);
        static_utility(u, &format!("max-block-{}", key), &[("max-block-size", value)]);
    }

    for (key, value) in [("svw", "100svw"), ("lvw", "100lvw"), ("dvw", "100dvw")] {
        static_utility(u, &format!("inline-{}", key), &[("inline-size", value)]);
        static_utility(u, &format!("min-inline-{}", key), &[("min-inline-size", value)]);
        static_utility(u, &format!("max-inline-{}", key), &[("max-inline-size", value)]);
    }

    for (key, value) in [("svh", "100svh"), ("lvh", "100lvh"), ("dvh", "100dvh")] {
        static_utility(u, &format!("block-{}", key), &[("block-size", value)]);
        static_utility(u, &format!("min-block-{}", key), &[("min-block-size", value)]);
        static_utility(u, &format!("max-block-{}", key), &[("max-block-size", value)]);
    }

    static_utility(u, "inline-auto", &[("inline-size", "auto")]);
    static_utility(u, "block-auto", &[("block-size", "auto")]);
    static_utility(u, "min-inline-auto", &[("min-inline-size", "auto")]);
    static_utility(u, "min-block-auto", &[("min-block-size", "auto")]);

    static_utility(u, "block-lh", &[("block-size", "1lh")]);
    static_utility(u, "min-block-lh", &[("min-block-size", "1lh")]);
    static_utility(u, "max-block-lh", &[("max-block-size", "1lh")]);

    static_utility(u, "inline-screen", &[("inline-size", "100vw")]);
    static_utility(u, "min-inline-screen", &[("min-inline-size", "100vw")]);
    static_utility(u, "max-inline-screen", &[("max-inline-size", "100vw")]);
    static_utility(u, "block-screen", &[("block-size", "100vh")]);
    static_utility(u, "min-block-screen", &[("min-block-size", "100vh")]);
    static_utility(u, "max-block-screen", &[("max-block-size", "100vh")]);

    static_utility(u, "max-inline-none", &[("max-inline-size", "none")]);
    static_utility(u, "max-block-none", &[("max-block-size", "none")]);

    for (name, namespaces, property) in [
        ("inline", vec!["--spacing", "--container"], "inline-size"),
        ("min-inline", vec!["--spacing", "--container"], "min-inline-size"),
        ("max-inline", vec!["--spacing", "--container"], "max-inline-size"),
        ("block", vec!["--spacing"], "block-size"),
        ("min-block", vec!["--spacing"], "min-block-size"),
        ("max-block", vec!["--spacing"], "max-block-size"),
    ] {
        spacing_utility(
            u,
            name,
            namespaces,
            move |v| vec![decl(property, v)],
            SpacingOpts { supports_fractions: true, ..Default::default() },
        );
    }

    u.static_("container", |_, ds| {
        let mut breakpoints: Vec<String> =
            ds.theme.namespace("--breakpoint").into_iter().map(|(_, v)| v).collect();
        breakpoints.sort_by(|a, z| compare_breakpoints(a, z, true));
        let mut decls = vec![decl("--tw-sort", "--tw-container-component"), decl("width", "100%")];
        for bp in breakpoints {
            decls.push(at_rule("@media", format!("(width >= {})", bp), vec![decl("max-width", bp)]));
        }
        Some(decls)
    });

    static_utility(u, "flex-auto", &[("flex", "auto")]);
    static_utility(u, "flex-initial", &[("flex", "0 auto")]);
    static_utility(u, "flex-none", &[("flex", "none")]);

    u.functional("flex", |c, _| {
        let value = c.value()?;
        match value {
            UtilityValue::Arbitrary { value, .. } => {
                if c.modifier.is_some() {
                    return None;
                }
                Some(vec![decl("flex", value.as_str())])
            }
            UtilityValue::Named { value, fraction } => {
                if let Some(f) = fraction {
                    let parts = segment(f, b'/');
                    if !is_positive_integer(parts[0]) || !is_positive_integer(parts.get(1).copied().unwrap_or("")) {
                        return None;
                    }
                    return Some(vec![decl("flex", format!("calc({} * 100%)", f))]);
                }
                if is_positive_integer(value) {
                    if c.modifier.is_some() {
                        return None;
                    }
                    return Some(vec![decl("flex", value.as_str())]);
                }
                None
            }
        }
    });

    functional_utility(
        u,
        "shrink",
        Desc {
            default_value: Some(Some("1".into())),
            handle_bare_value: bare_positive_integer(),
            handle: hv(|v| vec![decl("flex-shrink", v)]),
            ..Default::default()
        },
    );

    functional_utility(
        u,
        "grow",
        Desc {
            default_value: Some(Some("1".into())),
            handle_bare_value: bare_positive_integer(),
            handle: hv(|v| vec![decl("flex-grow", v)]),
            ..Default::default()
        },
    );

    static_utility(u, "basis-auto", &[("flex-basis", "auto")]);
    static_utility(u, "basis-full", &[("flex-basis", "100%")]);
    spacing_utility(
        u,
        "basis",
        vec!["--flex-basis", "--spacing", "--container"],
        |v| vec![decl("flex-basis", v)],
        SpacingOpts { supports_fractions: true, ..Default::default() },
    );

    static_utility(u, "table-auto", &[("table-layout", "auto")]);
    static_utility(u, "table-fixed", &[("table-layout", "fixed")]);

    static_utility(u, "caption-top", &[("caption-side", "top")]);
    static_utility(u, "caption-bottom", &[("caption-side", "bottom")]);

    static_utility(u, "border-collapse", &[("border-collapse", "collapse")]);
    static_utility(u, "border-separate", &[("border-collapse", "separate")]);

    fn border_spacing_properties() -> AstNode {
        at_root(vec![
            property("--tw-border-spacing-x", Some("0"), Some("<length>")),
            property("--tw-border-spacing-y", Some("0"), Some("<length>")),
        ])
    }

    spacing_utility(
        u,
        "border-spacing",
        vec!["--border-spacing", "--spacing"],
        |v| {
            vec![
                border_spacing_properties(),
                decl("--tw-border-spacing-x", v),
                decl("--tw-border-spacing-y", v),
                decl("border-spacing", "var(--tw-border-spacing-x) var(--tw-border-spacing-y)"),
            ]
        },
        SpacingOpts::default(),
    );

    spacing_utility(
        u,
        "border-spacing-x",
        vec!["--border-spacing", "--spacing"],
        |v| {
            vec![
                border_spacing_properties(),
                decl("--tw-border-spacing-x", v),
                decl("border-spacing", "var(--tw-border-spacing-x) var(--tw-border-spacing-y)"),
            ]
        },
        SpacingOpts::default(),
    );

    spacing_utility(
        u,
        "border-spacing-y",
        vec!["--border-spacing", "--spacing"],
        |v| {
            vec![
                border_spacing_properties(),
                decl("--tw-border-spacing-y", v),
                decl("border-spacing", "var(--tw-border-spacing-x) var(--tw-border-spacing-y)"),
            ]
        },
        SpacingOpts::default(),
    );

    for (root, prop, ns) in [
        ("origin", "transform-origin", "--transform-origin"),
        ("perspective-origin", "perspective-origin", "--perspective-origin"),
    ] {
        functional_utility(
            u,
            root,
            Desc {
                theme_keys: vec![ns],
                handle: hv(move |v| vec![decl(prop, v)]),
                static_values: sv(&[
                    ("center", &[(prop, "center")]),
                    ("top", &[(prop, "top")]),
                    ("top-right", &[(prop, "100% 0")]),
                    ("right", &[(prop, "100%")]),
                    ("bottom-right", &[(prop, "100% 100%")]),
                    ("bottom", &[(prop, "bottom")]),
                    ("bottom-left", &[(prop, "0 100%")]),
                    ("left", &[(prop, "0")]),
                    ("top-left", &[(prop, "0 0")]),
                ]),
                ..Default::default()
            },
        );
    }

    functional_utility(
        u,
        "perspective",
        Desc {
            theme_keys: vec!["--perspective"],
            handle: hv(|v| vec![decl("perspective", v)]),
            static_values: sv(&[("none", &[("perspective", "none")])]),
            ..Default::default()
        },
    );

    fn translate_properties() -> AstNode {
        at_root(vec![
            property("--tw-translate-x", Some("0"), None),
            property("--tw-translate-y", Some("0"), None),
            property("--tw-translate-z", Some("0"), None),
        ])
    }

    static_utility(u, "translate-none", &[("translate", "none")]);
    static_utility_nodes(u, "-translate-full", || {
        vec![
            translate_properties(),
            decl("--tw-translate-x", "-100%"),
            decl("--tw-translate-y", "-100%"),
            decl("translate", "var(--tw-translate-x) var(--tw-translate-y)"),
        ]
    });
    static_utility_nodes(u, "translate-full", || {
        vec![
            translate_properties(),
            decl("--tw-translate-x", "100%"),
            decl("--tw-translate-y", "100%"),
            decl("translate", "var(--tw-translate-x) var(--tw-translate-y)"),
        ]
    });

    spacing_utility(
        u,
        "translate",
        vec!["--translate", "--spacing"],
        |v| {
            vec![
                translate_properties(),
                decl("--tw-translate-x", v),
                decl("--tw-translate-y", v),
                decl("translate", "var(--tw-translate-x) var(--tw-translate-y)"),
            ]
        },
        SpacingOpts { supports_negative: true, supports_fractions: true, ..Default::default() },
    );

    for axis in ["x", "y"] {
        static_utility_nodes(u, &format!("-translate-{}-full", axis), move || {
            vec![
                translate_properties(),
                decl(format!("--tw-translate-{}", axis), "-100%"),
                decl("translate", "var(--tw-translate-x) var(--tw-translate-y)"),
            ]
        });
        static_utility_nodes(u, &format!("translate-{}-full", axis), move || {
            vec![
                translate_properties(),
                decl(format!("--tw-translate-{}", axis), "100%"),
                decl("translate", "var(--tw-translate-x) var(--tw-translate-y)"),
            ]
        });
        spacing_utility(
            u,
            &format!("translate-{}", axis),
            vec!["--translate", "--spacing"],
            move |v| {
                vec![
                    translate_properties(),
                    decl(format!("--tw-translate-{}", axis), v),
                    decl("translate", "var(--tw-translate-x) var(--tw-translate-y)"),
                ]
            },
            SpacingOpts { supports_negative: true, supports_fractions: true, ..Default::default() },
        );
    }

    spacing_utility(
        u,
        "translate-z",
        vec!["--translate", "--spacing"],
        |v| {
            vec![
                translate_properties(),
                decl("--tw-translate-z", v),
                decl("translate", "var(--tw-translate-x) var(--tw-translate-y) var(--tw-translate-z)"),
            ]
        },
        SpacingOpts { supports_negative: true, ..Default::default() },
    );

    static_utility_nodes(u, "translate-3d", || {
        vec![
            translate_properties(),
            decl("translate", "var(--tw-translate-x) var(--tw-translate-y) var(--tw-translate-z)"),
        ]
    });

    fn scale_properties() -> AstNode {
        at_root(vec![
            property("--tw-scale-x", Some("1"), None),
            property("--tw-scale-y", Some("1"), None),
            property("--tw-scale-z", Some("1"), None),
        ])
    }

    static_utility(u, "scale-none", &[("scale", "none")]);

    for (name, negative) in [("-scale", true), ("scale", false)] {
        u.functional(name, move |c, ds| {
            let value = c.value()?;
            if c.modifier.is_some() {
                return None;
            }
            let v = match value {
                UtilityValue::Arbitrary { value, .. } => {
                    let v = if negative { format!("calc({} * -1)", value) } else { value.clone() };
                    return Some(vec![decl("scale", v)]);
                }
                UtilityValue::Named { value, .. } => {
                    let mut v = ds.theme.resolve(Some(value), &["--scale"]);
                    if v.is_none() && is_positive_integer(value) {
                        v = Some(format!("{}%", value));
                    }
                    v?
                }
            };
            let v = if negative { format!("calc({} * -1)", v) } else { v };
            Some(vec![
                scale_properties(),
                decl("--tw-scale-x", v.as_str()),
                decl("--tw-scale-y", v.as_str()),
                decl("--tw-scale-z", v.as_str()),
                decl("scale", "var(--tw-scale-x) var(--tw-scale-y)"),
            ])
        });
    }

    for axis in ["x", "y", "z"] {
        functional_utility(
            u,
            &format!("scale-{}", axis),
            Desc {
                supports_negative: true,
                theme_keys: vec!["--scale"],
                handle_bare_value: bare(|b, _| is_positive_integer(b.value).then(|| format!("{}%", b.value))),
                handle: hv(move |v| {
                    vec![
                        scale_properties(),
                        decl(format!("--tw-scale-{}", axis), v),
                        decl(
                            "scale",
                            format!("var(--tw-scale-x) var(--tw-scale-y){}", if axis == "z" { " var(--tw-scale-z)" } else { "" }),
                        ),
                    ]
                }),
                ..Default::default()
            },
        );
    }

    static_utility_nodes(u, "scale-3d", || {
        vec![scale_properties(), decl("scale", "var(--tw-scale-x) var(--tw-scale-y) var(--tw-scale-z)")]
    });

    static_utility(u, "rotate-none", &[("rotate", "none")]);

    for (name, negative) in [("-rotate", true), ("rotate", false)] {
        u.functional(name, move |c, ds| {
            let value = c.value()?;
            if c.modifier.is_some() {
                return None;
            }
            let v = match value {
                UtilityValue::Arbitrary { value, data_type } => {
                    let ty = match data_type {
                        Some(dt) => DataType::from_str(dt),
                        None => infer_data_type(value, &[DataType::Angle, DataType::Vector]),
                    };
                    let ty_str = data_type.clone().or_else(|| ty.map(|t| t.as_str().to_string()));
                    if ty_str.as_deref() == Some("vector") {
                        return Some(vec![decl("rotate", format!("{} var(--tw-rotate)", value))]);
                    } else if ty_str.as_deref() != Some("angle") {
                        let v = if negative { format!("calc({} * -1)", value) } else { value.clone() };
                        return Some(vec![decl("rotate", v)]);
                    }
                    value.clone()
                }
                UtilityValue::Named { value, .. } => {
                    let mut v = ds.theme.resolve(Some(value), &["--rotate"]);
                    if v.is_none() && is_positive_integer(value) {
                        v = Some(format!("{}deg", value));
                    }
                    v?
                }
            };
            let v = if negative { format!("calc({} * -1)", v) } else { v };
            Some(vec![decl("rotate", v)])
        });
    }

    const TRANSFORM_VALUE: &str =
        "var(--tw-rotate-x,) var(--tw-rotate-y,) var(--tw-rotate-z,) var(--tw-skew-x,) var(--tw-skew-y,)";

    fn transform_properties() -> AstNode {
        at_root(vec![
            property("--tw-rotate-x", None, None),
            property("--tw-rotate-y", None, None),
            property("--tw-rotate-z", None, None),
            property("--tw-skew-x", None, None),
            property("--tw-skew-y", None, None),
        ])
    }

    for axis in ["x", "y", "z"] {
        functional_utility(
            u,
            &format!("rotate-{}", axis),
            Desc {
                supports_negative: true,
                theme_keys: vec!["--rotate"],
                handle_bare_value: bare(|b, _| is_positive_integer(b.value).then(|| format!("{}deg", b.value))),
                handle: hv(move |v| {
                    vec![
                        transform_properties(),
                        decl(format!("--tw-rotate-{}", axis), format!("rotate{}({})", axis.to_uppercase(), v)),
                        decl("transform", TRANSFORM_VALUE),
                    ]
                }),
                ..Default::default()
            },
        );
    }

    functional_utility(
        u,
        "skew",
        Desc {
            supports_negative: true,
            theme_keys: vec!["--skew"],
            handle_bare_value: bare(|b, _| is_positive_integer(b.value).then(|| format!("{}deg", b.value))),
            handle: hv(|v| {
                vec![
                    transform_properties(),
                    decl("--tw-skew-x", format!("skewX({})", v)),
                    decl("--tw-skew-y", format!("skewY({})", v)),
                    decl("transform", TRANSFORM_VALUE),
                ]
            }),
            ..Default::default()
        },
    );

    for (axis, fname) in [("x", "skewX"), ("y", "skewY")] {
        functional_utility(
            u,
            &format!("skew-{}", axis),
            Desc {
                supports_negative: true,
                theme_keys: vec!["--skew"],
                handle_bare_value: bare(|b, _| is_positive_integer(b.value).then(|| format!("{}deg", b.value))),
                handle: hv(move |v| {
                    vec![
                        transform_properties(),
                        decl(format!("--tw-skew-{}", axis), format!("{}({})", fname, v)),
                        decl("transform", TRANSFORM_VALUE),
                    ]
                }),
                ..Default::default()
            },
        );
    }

    u.functional("transform", |c, _| {
        if c.modifier.is_some() {
            return None;
        }
        let value = match c.value() {
            None => TRANSFORM_VALUE.to_string(),
            Some(UtilityValue::Arbitrary { value, .. }) => value.clone(),
            Some(_) => return None,
        };
        Some(vec![transform_properties(), decl("transform", value)])
    });

    static_utility(u, "transform-cpu", &[("transform", TRANSFORM_VALUE)]);
    static_utility(
        u,
        "transform-gpu",
        &[("transform", "translateZ(0) var(--tw-rotate-x,) var(--tw-rotate-y,) var(--tw-rotate-z,) var(--tw-skew-x,) var(--tw-skew-y,)")],
    );
    static_utility(u, "transform-none", &[("transform", "none")]);

    functional_utility(
        u,
        "zoom",
        Desc {
            handle_bare_value: bare(|b, _| is_positive_integer(b.value).then(|| format!("{}%", b.value))),
            handle: hv(|v| vec![decl("zoom", v)]),
            ..Default::default()
        },
    );

    static_utility(u, "transform-flat", &[("transform-style", "flat")]);
    static_utility(u, "transform-3d", &[("transform-style", "preserve-3d")]);

    static_utility(u, "transform-content", &[("transform-box", "content-box")]);
    static_utility(u, "transform-border", &[("transform-box", "border-box")]);
    static_utility(u, "transform-fill", &[("transform-box", "fill-box")]);
    static_utility(u, "transform-stroke", &[("transform-box", "stroke-box")]);
    static_utility(u, "transform-view", &[("transform-box", "view-box")]);

    static_utility(u, "backface-visible", &[("backface-visibility", "visible")]);
    static_utility(u, "backface-hidden", &[("backface-visibility", "hidden")]);

    for value in [
        "auto", "default", "pointer", "wait", "text", "move", "help", "not-allowed", "none", "context-menu",
        "progress", "cell", "crosshair", "vertical-text", "alias", "copy", "no-drop", "grab", "grabbing",
        "all-scroll", "col-resize", "row-resize", "n-resize", "e-resize", "s-resize", "w-resize", "ne-resize",
        "nw-resize", "se-resize", "sw-resize", "ew-resize", "ns-resize", "nesw-resize", "nwse-resize", "zoom-in",
        "zoom-out",
    ] {
        static_utility(u, &format!("cursor-{}", value), &[("cursor", value)]);
    }

    functional_utility(
        u,
        "cursor",
        Desc { theme_keys: vec!["--cursor"], handle: hv(|v| vec![decl("cursor", v)]), ..Default::default() },
    );

    for value in ["auto", "none", "manipulation"] {
        static_utility(u, &format!("touch-{}", value), &[("touch-action", value)]);
    }

    fn touch_properties() -> AstNode {
        at_root(vec![
            property("--tw-pan-x", None, None),
            property("--tw-pan-y", None, None),
            property("--tw-pinch-zoom", None, None),
        ])
    }

    for (axis_var, values) in [("--tw-pan-x", ["x", "left", "right"]), ("--tw-pan-y", ["y", "up", "down"])] {
        for value in values {
            static_utility_nodes(u, &format!("touch-pan-{}", value), move || {
                vec![
                    touch_properties(),
                    decl(axis_var, format!("pan-{}", value)),
                    decl("touch-action", "var(--tw-pan-x,) var(--tw-pan-y,) var(--tw-pinch-zoom,)"),
                ]
            });
        }
    }

    static_utility_nodes(u, "touch-pinch-zoom", || {
        vec![
            touch_properties(),
            decl("--tw-pinch-zoom", "pinch-zoom"),
            decl("touch-action", "var(--tw-pan-x,) var(--tw-pan-y,) var(--tw-pinch-zoom,)"),
        ]
    });

    for value in ["none", "text", "all", "auto"] {
        static_utility(u, &format!("select-{}", value), &[("-webkit-user-select", value), ("user-select", value)]);
    }

    static_utility(u, "resize-none", &[("resize", "none")]);
    static_utility(u, "resize-x", &[("resize", "horizontal")]);
    static_utility(u, "resize-y", &[("resize", "vertical")]);
    static_utility(u, "resize", &[("resize", "both")]);

    static_utility(u, "snap-none", &[("scroll-snap-type", "none")]);

    fn snap_properties() -> AstNode {
        at_root(vec![property("--tw-scroll-snap-strictness", Some("proximity"), Some("*"))])
    }

    for value in ["x", "y", "both"] {
        static_utility_nodes(u, &format!("snap-{}", value), move || {
            vec![snap_properties(), decl("scroll-snap-type", format!("{} var(--tw-scroll-snap-strictness)", value))]
        });
    }

    static_utility_nodes(u, "snap-mandatory", || {
        vec![snap_properties(), decl("--tw-scroll-snap-strictness", "mandatory")]
    });
    static_utility_nodes(u, "snap-proximity", || {
        vec![snap_properties(), decl("--tw-scroll-snap-strictness", "proximity")]
    });

    static_utility(u, "snap-align-none", &[("scroll-snap-align", "none")]);
    static_utility(u, "snap-start", &[("scroll-snap-align", "start")]);
    static_utility(u, "snap-end", &[("scroll-snap-align", "end")]);
    static_utility(u, "snap-center", &[("scroll-snap-align", "center")]);

    static_utility(u, "snap-normal", &[("scroll-snap-stop", "normal")]);
    static_utility(u, "snap-always", &[("scroll-snap-stop", "always")]);

    for (namespace, property) in [
        ("scroll-m", "scroll-margin"),
        ("scroll-mx", "scroll-margin-inline"),
        ("scroll-my", "scroll-margin-block"),
        ("scroll-ms", "scroll-margin-inline-start"),
        ("scroll-me", "scroll-margin-inline-end"),
        ("scroll-mbs", "scroll-margin-block-start"),
        ("scroll-mbe", "scroll-margin-block-end"),
        ("scroll-mt", "scroll-margin-top"),
        ("scroll-mr", "scroll-margin-right"),
        ("scroll-mb", "scroll-margin-bottom"),
        ("scroll-ml", "scroll-margin-left"),
    ] {
        spacing_utility(
            u,
            namespace,
            vec!["--scroll-margin", "--spacing"],
            move |v| vec![decl(property, v)],
            SpacingOpts { supports_negative: true, ..Default::default() },
        );
    }

    for (namespace, property) in [
        ("scroll-p", "scroll-padding"),
        ("scroll-px", "scroll-padding-inline"),
        ("scroll-py", "scroll-padding-block"),
        ("scroll-ps", "scroll-padding-inline-start"),
        ("scroll-pe", "scroll-padding-inline-end"),
        ("scroll-pbs", "scroll-padding-block-start"),
        ("scroll-pbe", "scroll-padding-block-end"),
        ("scroll-pt", "scroll-padding-top"),
        ("scroll-pr", "scroll-padding-right"),
        ("scroll-pb", "scroll-padding-bottom"),
        ("scroll-pl", "scroll-padding-left"),
    ] {
        spacing_utility(
            u,
            namespace,
            vec!["--scroll-padding", "--spacing"],
            move |v| vec![decl(property, v)],
            SpacingOpts::default(),
        );
    }

    static_utility(u, "list-inside", &[("list-style-position", "inside")]);
    static_utility(u, "list-outside", &[("list-style-position", "outside")]);

    functional_utility(
        u,
        "list",
        Desc {
            theme_keys: vec!["--list-style-type"],
            handle: hv(|v| vec![decl("list-style-type", v)]),
            static_values: sv(&[
                ("none", &[("list-style-type", "none")]),
                ("disc", &[("list-style-type", "disc")]),
                ("decimal", &[("list-style-type", "decimal")]),
            ]),
            ..Default::default()
        },
    );

    functional_utility(
        u,
        "list-image",
        Desc {
            theme_keys: vec!["--list-style-image"],
            handle: hv(|v| vec![decl("list-style-image", v)]),
            static_values: sv(&[("none", &[("list-style-image", "none")])]),
            ..Default::default()
        },
    );

    static_utility(u, "appearance-none", &[("appearance", "none")]);
    static_utility(u, "appearance-auto", &[("appearance", "auto")]);

    static_utility(u, "scheme-normal", &[("color-scheme", "normal")]);
    static_utility(u, "scheme-dark", &[("color-scheme", "dark")]);
    static_utility(u, "scheme-light", &[("color-scheme", "light")]);
    static_utility(u, "scheme-light-dark", &[("color-scheme", "light dark")]);
    static_utility(u, "scheme-only-dark", &[("color-scheme", "only dark")]);
    static_utility(u, "scheme-only-light", &[("color-scheme", "only light")]);

    functional_utility(
        u,
        "columns",
        Desc {
            theme_keys: vec!["--columns", "--container"],
            handle_bare_value: bare_positive_integer(),
            handle: hv(|v| vec![decl("columns", v)]),
            static_values: sv(&[("auto", &[("columns", "auto")])]),
            ..Default::default()
        },
    );

    for value in ["auto", "avoid", "all", "avoid-page", "page", "left", "right", "column"] {
        static_utility(u, &format!("break-before-{}", value), &[("break-before", value)]);
    }
    for value in ["auto", "avoid", "avoid-page", "avoid-column"] {
        static_utility(u, &format!("break-inside-{}", value), &[("break-inside", value)]);
    }
    for value in ["auto", "avoid", "all", "avoid-page", "page", "left", "right", "column"] {
        static_utility(u, &format!("break-after-{}", value), &[("break-after", value)]);
    }

    static_utility(u, "grid-flow-row", &[("grid-auto-flow", "row")]);
    static_utility(u, "grid-flow-col", &[("grid-auto-flow", "column")]);
    static_utility(u, "grid-flow-dense", &[("grid-auto-flow", "dense")]);
    static_utility(u, "grid-flow-row-dense", &[("grid-auto-flow", "row dense")]);
    static_utility(u, "grid-flow-col-dense", &[("grid-auto-flow", "column dense")]);

    for (root, prop, ns) in [
        ("auto-cols", "grid-auto-columns", "--grid-auto-columns"),
        ("auto-rows", "grid-auto-rows", "--grid-auto-rows"),
    ] {
        functional_utility(
            u,
            root,
            Desc {
                theme_keys: vec![ns],
                handle_bare_value: bare(|b, ds| {
                    ds.theme.resolve(None, &["--spacing"])?;
                    if !is_valid_spacing_multiplier(b.value) {
                        return None;
                    }
                    Some(format!("--spacing({})", b.value))
                }),
                handle: hv(move |v| vec![decl(prop, v)]),
                static_values: sv(&[
                    ("auto", &[(prop, "auto")]),
                    ("min", &[(prop, "min-content")]),
                    ("max", &[(prop, "max-content")]),
                    ("fr", &[(prop, "minmax(0, 1fr)")]),
                ]),
                ..Default::default()
            },
        );
    }

    for (root, prop, ns) in [
        ("grid-cols", "grid-template-columns", "--grid-template-columns"),
        ("grid-rows", "grid-template-rows", "--grid-template-rows"),
    ] {
        functional_utility(
            u,
            root,
            Desc {
                theme_keys: vec![ns],
                handle_bare_value: bare(|b, _| {
                    is_strict_positive_integer(b.value).then(|| format!("repeat({}, minmax(0, 1fr))", b.value))
                }),
                handle: hv(move |v| vec![decl(prop, v)]),
                static_values: sv(&[("none", &[(prop, "none")]), ("subgrid", &[(prop, "subgrid")])]),
                ..Default::default()
            },
        );
    }
}
