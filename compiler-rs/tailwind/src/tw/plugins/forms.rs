//! `@tailwindcss/forms` 0.5.11 (`@plugin "@tailwindcss/forms" { strategy: base | class; }`).
//!
//! Port of its `src/index.js`. Spacing, border width/radius and the base font
//! size come from `tailwindcss/defaultTheme` (static in the plugin), the
//! colors from `theme('colors.gray.500')` / `theme('colors.blue.600')` with
//! the `tailwindcss/colors` value as fallback, like the plugin.

use super::{obj, s, CssInJs, OptionValue, PluginApi};
use crate::tw::compat_theme::plugin_theme_string;
use indexmap::IndexMap;

// tailwindcss@4.3.3 `colors.gray[500]` / `colors.blue[600]`
const GRAY_500: &str = "oklch(55.1% 0.027 264.364)";
const BLUE_600: &str = "oklch(54.6% 0.245 262.881)";
// tailwindcss@4.3.3 `defaultTheme`
const SPACING_2: &str = "0.5rem";
const SPACING_3: &str = "0.75rem";
const SPACING_4: &str = "1rem";
const SPACING_10: &str = "2.5rem";
const BORDER_WIDTH: &str = "1px";
const BORDER_RADIUS_NONE: &str = "0px";
const BASE_FONT_SIZE: &str = "1rem";
const BASE_LINE_HEIGHT: &str = "1.5rem";

struct Rule {
    base: Option<Vec<&'static str>>,
    class: Option<Vec<&'static str>>,
    styles: CssInJs,
}

fn resolve_color(color: &str, opacity_variable: &str) -> String {
    color.replacen("<alpha-value>", &format!("var({opacity_variable}, 1)"), 1)
}

pub fn register(api: &mut PluginApi<'_>, options: Option<&IndexMap<String, OptionValue>>) -> Result<(), String> {
    let ds = &*api.ds;
    let theme = |path: &str, fallback: &str| plugin_theme_string(path, ds).unwrap_or_else(|| fallback.to_string());
    let resolve_chevron_color = |path: &str, fallback: &str| match plugin_theme_string(path, ds) {
        Some(r) if !r.is_empty() && !r.contains("var(") => r.replacen("<alpha-value>", "1", 1),
        _ => fallback.to_string(),
    };

    let gray = theme("colors.gray.500", GRAY_500);
    let blue = theme("colors.blue.600", BLUE_600);
    let chevron = resolve_chevron_color("colors.gray.500", GRAY_500);

    let strategy: Vec<&str> = match options.and_then(|o| o.get("strategy")) {
        None => vec!["base", "class"],
        Some(OptionValue::Str(s)) => vec![if s == "base" { "base" } else if s == "class" { "class" } else { "" }],
        Some(_) => vec![""],
    };

    let checkbox_svg = r#"<svg viewBox="0 0 16 16" fill="white" xmlns="http://www.w3.org/2000/svg"><path d="M12.207 4.793a1 1 0 010 1.414l-5 5a1 1 0 01-1.414 0l-2-2a1 1 0 011.414-1.414L6.5 9.086l4.293-4.293a1 1 0 011.414 0z"/></svg>"#;
    let radio_svg = r#"<svg viewBox="0 0 16 16" fill="white" xmlns="http://www.w3.org/2000/svg"><circle cx="8" cy="8" r="3"/></svg>"#;
    let indeterminate_svg = r#"<svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 16 16"><path stroke="white" stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 8h8"/></svg>"#;
    let chevron_svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 20 20"><path stroke="{chevron}" stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M6 8l4 4 4-4"/></svg>"#
    );
    let url = |svg: &str| format!("url(\"{}\")", svg_to_data_uri(svg));
    let forced_colors = || ("@media (forced-colors: active) ", obj(vec![("appearance", s("auto"))]));

    let rules = vec![
        Rule {
            base: Some(vec![
                "input:where([type='text'])",
                "input:where(:not([type]))",
                "input:where([type='email'])",
                "input:where([type='url'])",
                "input:where([type='password'])",
                "input:where([type='number'])",
                "input:where([type='date'])",
                "input:where([type='datetime-local'])",
                "input:where([type='month'])",
                "input:where([type='search'])",
                "input:where([type='tel'])",
                "input:where([type='time'])",
                "input:where([type='week'])",
                "select:where([multiple])",
                "textarea",
                "select",
            ]),
            class: Some(vec![".form-input", ".form-textarea", ".form-select", ".form-multiselect"]),
            styles: obj(vec![
                ("appearance", s("none")),
                ("background-color", s("#fff")),
                ("border-color", s(resolve_color(&gray, "--tw-border-opacity"))),
                ("border-width", s(BORDER_WIDTH)),
                ("border-radius", s(BORDER_RADIUS_NONE)),
                ("padding-top", s(SPACING_2)),
                ("padding-right", s(SPACING_3)),
                ("padding-bottom", s(SPACING_2)),
                ("padding-left", s(SPACING_3)),
                ("font-size", s(BASE_FONT_SIZE)),
                ("line-height", s(BASE_LINE_HEIGHT)),
                ("--tw-shadow", s("0 0 #0000")),
                (
                    "&:focus",
                    obj(vec![
                        ("outline", s("2px solid transparent")),
                        ("outline-offset", s("2px")),
                        ("--tw-ring-inset", s("var(--tw-empty,/*!*/ /*!*/)")),
                        ("--tw-ring-offset-width", s("0px")),
                        ("--tw-ring-offset-color", s("#fff")),
                        ("--tw-ring-color", s(resolve_color(&blue, "--tw-ring-opacity"))),
                        ("--tw-ring-offset-shadow", s("var(--tw-ring-inset) 0 0 0 var(--tw-ring-offset-width) var(--tw-ring-offset-color)")),
                        ("--tw-ring-shadow", s("var(--tw-ring-inset) 0 0 0 calc(1px + var(--tw-ring-offset-width)) var(--tw-ring-color)")),
                        ("box-shadow", s("var(--tw-ring-offset-shadow), var(--tw-ring-shadow), var(--tw-shadow)")),
                        ("border-color", s(resolve_color(&blue, "--tw-border-opacity"))),
                    ]),
                ),
            ]),
        },
        Rule {
            base: Some(vec!["input::placeholder", "textarea::placeholder"]),
            class: Some(vec![".form-input::placeholder", ".form-textarea::placeholder"]),
            styles: obj(vec![("color", s(resolve_color(&gray, "--tw-text-opacity"))), ("opacity", s("1"))]),
        },
        Rule {
            base: Some(vec!["::-webkit-datetime-edit-fields-wrapper"]),
            class: Some(vec![".form-input::-webkit-datetime-edit-fields-wrapper"]),
            styles: obj(vec![("padding", s("0"))]),
        },
        Rule {
            base: Some(vec!["::-webkit-date-and-time-value"]),
            class: Some(vec![".form-input::-webkit-date-and-time-value"]),
            styles: obj(vec![("min-height", s("1.5em"))]),
        },
        Rule {
            base: Some(vec!["::-webkit-date-and-time-value"]),
            class: Some(vec![".form-input::-webkit-date-and-time-value"]),
            styles: obj(vec![("text-align", s("inherit"))]),
        },
        Rule {
            base: Some(vec!["::-webkit-datetime-edit"]),
            class: Some(vec![".form-input::-webkit-datetime-edit"]),
            styles: obj(vec![("display", s("inline-flex"))]),
        },
        Rule {
            base: Some(vec![
                "::-webkit-datetime-edit",
                "::-webkit-datetime-edit-year-field",
                "::-webkit-datetime-edit-month-field",
                "::-webkit-datetime-edit-day-field",
                "::-webkit-datetime-edit-hour-field",
                "::-webkit-datetime-edit-minute-field",
                "::-webkit-datetime-edit-second-field",
                "::-webkit-datetime-edit-millisecond-field",
                "::-webkit-datetime-edit-meridiem-field",
            ]),
            class: Some(vec![
                ".form-input::-webkit-datetime-edit",
                ".form-input::-webkit-datetime-edit-year-field",
                ".form-input::-webkit-datetime-edit-month-field",
                ".form-input::-webkit-datetime-edit-day-field",
                ".form-input::-webkit-datetime-edit-hour-field",
                ".form-input::-webkit-datetime-edit-minute-field",
                ".form-input::-webkit-datetime-edit-second-field",
                ".form-input::-webkit-datetime-edit-millisecond-field",
                ".form-input::-webkit-datetime-edit-meridiem-field",
            ]),
            styles: obj(vec![("padding-top", s("0")), ("padding-bottom", s("0"))]),
        },
        Rule {
            base: Some(vec!["select"]),
            class: Some(vec![".form-select"]),
            styles: obj(vec![
                ("background-image", s(url(&chevron_svg))),
                ("background-position", s(format!("right {SPACING_2} center"))),
                ("background-repeat", s("no-repeat")),
                ("background-size", s("1.5em 1.5em")),
                ("padding-right", s(SPACING_10)),
                ("print-color-adjust", s("exact")),
            ]),
        },
        Rule {
            base: Some(vec!["select:where([multiple])", "select:where([size]:not([size=\"1\"]))"]),
            class: Some(vec![".form-select:where([size]:not([size=\"1\"]))"]),
            styles: obj(vec![
                ("background-image", s("initial")),
                ("background-position", s("initial")),
                ("background-repeat", s("unset")),
                ("background-size", s("initial")),
                ("padding-right", s(SPACING_3)),
                ("print-color-adjust", s("unset")),
            ]),
        },
        Rule {
            base: Some(vec!["input:where([type='checkbox'])", "input:where([type='radio'])"]),
            class: Some(vec![".form-checkbox", ".form-radio"]),
            styles: obj(vec![
                ("appearance", s("none")),
                ("padding", s("0")),
                ("print-color-adjust", s("exact")),
                ("display", s("inline-block")),
                ("vertical-align", s("middle")),
                ("background-origin", s("border-box")),
                ("user-select", s("none")),
                ("flex-shrink", s("0")),
                ("height", s(SPACING_4)),
                ("width", s(SPACING_4)),
                ("color", s(resolve_color(&blue, "--tw-text-opacity"))),
                ("background-color", s("#fff")),
                ("border-color", s(resolve_color(&gray, "--tw-border-opacity"))),
                ("border-width", s(BORDER_WIDTH)),
                ("--tw-shadow", s("0 0 #0000")),
            ]),
        },
        Rule {
            base: Some(vec!["input:where([type='checkbox'])"]),
            class: Some(vec![".form-checkbox"]),
            styles: obj(vec![("border-radius", s(BORDER_RADIUS_NONE))]),
        },
        Rule {
            base: Some(vec!["input:where([type='radio'])"]),
            class: Some(vec![".form-radio"]),
            styles: obj(vec![("border-radius", s("100%"))]),
        },
        Rule {
            base: Some(vec!["input:where([type='checkbox']):focus", "input:where([type='radio']):focus"]),
            class: Some(vec![".form-checkbox:focus", ".form-radio:focus"]),
            styles: obj(vec![
                ("outline", s("2px solid transparent")),
                ("outline-offset", s("2px")),
                ("--tw-ring-inset", s("var(--tw-empty,/*!*/ /*!*/)")),
                ("--tw-ring-offset-width", s("2px")),
                ("--tw-ring-offset-color", s("#fff")),
                ("--tw-ring-color", s(resolve_color(&blue, "--tw-ring-opacity"))),
                ("--tw-ring-offset-shadow", s("var(--tw-ring-inset) 0 0 0 var(--tw-ring-offset-width) var(--tw-ring-offset-color)")),
                ("--tw-ring-shadow", s("var(--tw-ring-inset) 0 0 0 calc(2px + var(--tw-ring-offset-width)) var(--tw-ring-color)")),
                ("box-shadow", s("var(--tw-ring-offset-shadow), var(--tw-ring-shadow), var(--tw-shadow)")),
            ]),
        },
        Rule {
            base: Some(vec!["input:where([type='checkbox']):checked", "input:where([type='radio']):checked"]),
            class: Some(vec![".form-checkbox:checked", ".form-radio:checked"]),
            styles: obj(vec![
                ("border-color", s("transparent")),
                ("background-color", s("currentColor")),
                ("background-size", s("100% 100%")),
                ("background-position", s("center")),
                ("background-repeat", s("no-repeat")),
            ]),
        },
        Rule {
            base: Some(vec!["input:where([type='checkbox']):checked"]),
            class: Some(vec![".form-checkbox:checked"]),
            styles: obj(vec![("background-image", s(url(checkbox_svg))), forced_colors()]),
        },
        Rule {
            base: Some(vec!["input:where([type='radio']):checked"]),
            class: Some(vec![".form-radio:checked"]),
            styles: obj(vec![("background-image", s(url(radio_svg))), forced_colors()]),
        },
        Rule {
            base: Some(vec![
                "input:where([type='checkbox']):checked:hover",
                "input:where([type='checkbox']):checked:focus",
                "input:where([type='radio']):checked:hover",
                "input:where([type='radio']):checked:focus",
            ]),
            class: Some(vec![
                ".form-checkbox:checked:hover",
                ".form-checkbox:checked:focus",
                ".form-radio:checked:hover",
                ".form-radio:checked:focus",
            ]),
            styles: obj(vec![("border-color", s("transparent")), ("background-color", s("currentColor"))]),
        },
        Rule {
            base: Some(vec!["input:where([type='checkbox']):indeterminate"]),
            class: Some(vec![".form-checkbox:indeterminate"]),
            styles: obj(vec![
                ("background-image", s(url(indeterminate_svg))),
                ("border-color", s("transparent")),
                ("background-color", s("currentColor")),
                ("background-size", s("100% 100%")),
                ("background-position", s("center")),
                ("background-repeat", s("no-repeat")),
                forced_colors(),
            ]),
        },
        Rule {
            base: Some(vec!["input:where([type='checkbox']):indeterminate:hover", "input:where([type='checkbox']):indeterminate:focus"]),
            class: Some(vec![".form-checkbox:indeterminate:hover", ".form-checkbox:indeterminate:focus"]),
            styles: obj(vec![("border-color", s("transparent")), ("background-color", s("currentColor"))]),
        },
        Rule {
            base: Some(vec!["input:where([type='file'])"]),
            class: None,
            styles: obj(vec![
                ("background", s("unset")),
                ("border-color", s("inherit")),
                ("border-width", s("0")),
                ("border-radius", s("0")),
                ("padding", s("0")),
                ("font-size", s("unset")),
                ("line-height", s("inherit")),
            ]),
        },
        Rule {
            base: Some(vec!["input:where([type='file']):focus"]),
            class: None,
            styles: obj(vec![(
                "outline",
                CssInJs::Arr(vec![s("1px solid ButtonText"), s("1px auto -webkit-focus-ring-color")]),
            )]),
        },
    ];

    // `{ [rule[strategy]]: rule.styles }`: an array key is joined with `,`
    let strategy_rules = |pick: fn(&Rule) -> &Option<Vec<&'static str>>| -> Vec<(String, CssInJs)> {
        rules.iter().filter_map(|r| pick(r).as_ref().map(|sel| (sel.join(","), r.styles.clone()))).collect()
    };

    if strategy.contains(&"base") {
        api.add_base(&strategy_rules(|r| &r.base))?;
    }
    if strategy.contains(&"class") {
        api.add_utilities(&strategy_rules(|r| &r.class))?;
    }
    Ok(())
}

/// `mini-svg-data-uri` 1.4.4 (`svgToTinyDataUri`).
pub fn svg_to_data_uri(svg: &str) -> String {
    let svg = svg.strip_prefix('\u{feff}').unwrap_or(svg);
    let collapsed = svg.split_whitespace().collect::<Vec<_>>().join(" ");
    let body = color_codes_to_shorter_names(&collapsed).replace('"', "'");

    // encodeURIComponent, then the `%XX` pairs browsers tolerate are decoded back
    let mut out = String::from("data:image/svg+xml,");
    for b in body.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'!' | b'~' | b'*' | b'\'' | b'(' | b')') {
            out.push(b as char);
            continue;
        }
        match b {
            b' ' => out.push(' '),
            b'=' => out.push('='),
            b':' => out.push(':'),
            b'/' => out.push('/'),
            _ => out.push_str(&format!("%{b:02x}")),
        }
    }
    out
}

/// `shorter-css-color-names.js`: `#hex` (optionally with an opaque alpha)
/// not followed by a word character becomes the color name, in this order.
const SHORTER_NAMES: &[(&str, &[(&str, &str)])] = &[
    ("aqua", &[("00ffff", "ff"), ("0ff", "f")]),
    ("azure", &[("f0ffff", "ff")]),
    ("beige", &[("f5f5dc", "ff")]),
    ("bisque", &[("ffe4c4", "ff")]),
    ("black", &[("000000", "ff"), ("000", "f")]),
    ("blue", &[("0000ff", "ff"), ("00f", "f")]),
    ("brown", &[("a52a2a", "ff")]),
    ("coral", &[("ff7f50", "ff")]),
    ("cornsilk", &[("fff8dc", "ff")]),
    ("crimson", &[("dc143c", "ff")]),
    ("cyan", &[("00ffff", "ff"), ("0ff", "f")]),
    ("darkblue", &[("00008b", "ff")]),
    ("darkcyan", &[("008b8b", "ff")]),
    ("darkgrey", &[("a9a9a9", "ff")]),
    ("darkred", &[("8b0000", "ff")]),
    ("deeppink", &[("ff1493", "ff")]),
    ("dimgrey", &[("696969", "ff")]),
    ("gold", &[("ffd700", "ff")]),
    ("green", &[("008000", "ff")]),
    ("grey", &[("808080", "ff")]),
    ("honeydew", &[("f0fff0", "ff")]),
    ("hotpink", &[("ff69b4", "ff")]),
    ("indigo", &[("4b0082", "ff")]),
    ("ivory", &[("fffff0", "ff")]),
    ("khaki", &[("f0e68c", "ff")]),
    ("lavender", &[("e6e6fa", "ff")]),
    ("lime", &[("00ff00", "ff"), ("0f0", "f")]),
    ("linen", &[("faf0e6", "ff")]),
    ("maroon", &[("800000", "ff")]),
    ("moccasin", &[("ffe4b5", "ff")]),
    ("navy", &[("000080", "ff")]),
    ("oldlace", &[("fdf5e6", "ff")]),
    ("olive", &[("808000", "ff")]),
    ("orange", &[("ffa500", "ff")]),
    ("orchid", &[("da70d6", "ff")]),
    ("peru", &[("cd853f", "ff")]),
    ("pink", &[("ffc0cb", "ff")]),
    ("plum", &[("dda0dd", "ff")]),
    ("purple", &[("800080", "ff")]),
    ("red", &[("ff0000", "ff"), ("f00", "f")]),
    ("salmon", &[("fa8072", "ff")]),
    ("seagreen", &[("2e8b57", "ff")]),
    ("seashell", &[("fff5ee", "ff")]),
    ("sienna", &[("a0522d", "ff")]),
    ("silver", &[("c0c0c0", "ff")]),
    ("skyblue", &[("87ceeb", "ff")]),
    ("snow", &[("fffafa", "ff")]),
    ("tan", &[("d2b48c", "ff")]),
    ("teal", &[("008080", "ff")]),
    ("thistle", &[("d8bfd8", "ff")]),
    ("tomato", &[("ff6347", "ff")]),
    ("violet", &[("ee82ee", "ff")]),
    ("wheat", &[("f5deb3", "ff")]),
    ("white", &[("ffffff", "ff"), ("fff", "f")]),
];

fn is_word(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Length of a `#hex(alpha)?(?!\w)` match at `i` (case-insensitive), trying
/// the alternatives in order and the optional alpha greedily first.
fn match_at(s: &[u8], i: usize, alternatives: &[(&str, &str)]) -> Option<usize> {
    if s[i] != b'#' {
        return None;
    }
    let starts = |at: usize, lit: &str| s.len() >= at + lit.len() && s[at..at + lit.len()].eq_ignore_ascii_case(lit.as_bytes());
    let boundary = |end: usize| end >= s.len() || !is_word(s[end]);
    for (hex, alpha) in alternatives {
        if !starts(i + 1, hex) {
            continue;
        }
        let end = i + 1 + hex.len();
        if starts(end, alpha) && boundary(end + alpha.len()) {
            return Some(end + alpha.len() - i);
        }
        if boundary(end) {
            return Some(end - i);
        }
    }
    None
}

fn color_codes_to_shorter_names(input: &str) -> String {
    let mut string = input.to_string();
    for (name, alternatives) in SHORTER_NAMES {
        let b = string.as_bytes();
        let mut out = String::with_capacity(string.len());
        let (mut i, mut last, mut changed) = (0, 0, false);
        while i < b.len() {
            if let Some(len) = match_at(b, i, alternatives) {
                out.push_str(&string[last..i]);
                out.push_str(name);
                i += len;
                last = i;
                changed = true;
            } else {
                i += 1;
            }
        }
        if changed {
            out.push_str(&string[last..]);
            string = out;
        }
    }
    string
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn svg_data_uri_matches_mini_svg_data_uri() {
        // Outputs of mini-svg-data-uri@1.4.4
        assert_eq!(
            svg_to_data_uri(r#"<svg viewBox="0 0 16 16" fill="white" xmlns="http://www.w3.org/2000/svg"><circle cx="8" cy="8" r="3"/></svg>"#),
            "data:image/svg+xml,%3csvg viewBox='0 0 16 16' fill='white' xmlns='http://www.w3.org/2000/svg'%3e%3ccircle cx='8' cy='8' r='3'/%3e%3c/svg%3e"
        );
        assert_eq!(
            svg_to_data_uri(r##"<path stroke="oklch(55.1% 0.027 264.364)" fill="#FFF" d="M1"/>  "##),
            "data:image/svg+xml,%3cpath stroke='oklch(55.1%25 0.027 264.364)' fill='white' d='M1'/%3e"
        );
        assert_eq!(color_codes_to_shorter_names("#ff0000ff #f00a #f00 #FF0000"), "red #f00a red red");
    }
}
