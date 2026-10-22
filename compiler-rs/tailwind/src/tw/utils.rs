//! Small string helpers ported from `tailwindcss/src/utils/*`.

use regex::Regex;
use std::sync::LazyLock;

/// Split `input` on `separator`, but only at the top level (outside of
/// parens, brackets, braces and strings).
pub fn segment(input: &str, separator: u8) -> Vec<&str> {
    let bytes = input.as_bytes();
    let len = bytes.len();
    let mut stack: Vec<u8> = Vec::new();
    let mut parts = Vec::new();
    let mut last = 0;
    let mut idx = 0;

    while idx < len {
        let ch = bytes[idx];

        if stack.is_empty() && ch == separator {
            parts.push(&input[last..idx]);
            last = idx + 1;
            idx += 1;
            continue;
        }

        match ch {
            b'\\' => idx += 1,
            b'\'' | b'"' => {
                idx += 1;
                while idx < len {
                    let next = bytes[idx];
                    if next == b'\\' {
                        idx += 2;
                        continue;
                    }
                    if next == ch {
                        break;
                    }
                    idx += 1;
                }
            }
            b'(' => stack.push(b')'),
            b'[' => stack.push(b']'),
            b'{' => stack.push(b'}'),
            b')' | b']' | b'}' => {
                if stack.last() == Some(&ch) {
                    stack.pop();
                }
            }
            _ => {}
        }
        idx += 1;
    }

    parts.push(&input[last.min(len)..]);
    parts
}

/// Unbalanced brackets or a top-level `;` make an arbitrary value invalid.
pub fn is_valid_arbitrary(input: &str) -> bool {
    let bytes = input.as_bytes();
    let len = bytes.len();
    let mut stack: Vec<u8> = Vec::new();
    let mut idx = 0;

    while idx < len {
        let ch = bytes[idx];
        match ch {
            b'\\' => idx += 1,
            b'\'' | b'"' => {
                idx += 1;
                while idx < len {
                    let next = bytes[idx];
                    if next == b'\\' {
                        idx += 2;
                        continue;
                    }
                    if next == ch {
                        break;
                    }
                    idx += 1;
                }
            }
            b'(' => stack.push(b')'),
            b'[' => stack.push(b']'),
            b'{' => {}
            b')' | b']' | b'}' => {
                if stack.is_empty() {
                    return false;
                }
                if stack.last() == Some(&ch) {
                    stack.pop();
                }
            }
            b';' => {
                if stack.is_empty() {
                    return false;
                }
            }
            _ => {}
        }
        idx += 1;
    }

    true
}

/// Convert `_` to ` ` (except escaped `\_`, which becomes `_`).
pub fn convert_underscores_to_whitespace(input: &str, skip_underscore_to_space: bool) -> String {
    let mut out = String::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let ch = bytes[i];
        if ch == b'\\' && bytes.get(i + 1) == Some(&b'_') {
            out.push('_');
            i += 2;
            continue;
        }
        if ch == b'_' && !skip_underscore_to_space {
            out.push(' ');
            i += 1;
            continue;
        }
        // Copy the full UTF-8 sequence starting here.
        let start = i;
        i += 1;
        while i < bytes.len() && (bytes[i] & 0xC0) == 0x80 {
            i += 1;
        }
        out.push_str(&input[start..i]);
    }
    out
}

pub fn decode_arbitrary_value(input: &str) -> String {
    if !input.contains('(') {
        return convert_underscores_to_whitespace(input, false);
    }

    let mut ast = crate::tw::value_parser::parse(input);
    recursively_decode(&mut ast);
    let out = crate::tw::value_parser::to_css(&ast);
    add_whitespace_around_math_operators(&out)
}

fn recursively_decode(ast: &mut [crate::tw::value_parser::ValueNode]) {
    use crate::tw::value_parser::ValueNode;
    for node in ast.iter_mut() {
        match node {
            ValueNode::Function { value, nodes } => {
                if value == "url" || value.ends_with("_url") {
                    *value = convert_underscores_to_whitespace(value, false);
                    continue;
                }
                if value == "var" || value.ends_with("_var") || value == "theme" || value.ends_with("_theme") {
                    *value = convert_underscores_to_whitespace(value, false);
                    for (i, child) in nodes.iter_mut().enumerate() {
                        if i == 0 {
                            if let ValueNode::Word(w) = child {
                                *w = convert_underscores_to_whitespace(w, true);
                                continue;
                            }
                        }
                        recursively_decode(std::slice::from_mut(child));
                    }
                    continue;
                }
                *value = convert_underscores_to_whitespace(value, false);
                recursively_decode(nodes);
            }
            ValueNode::Separator(v) | ValueNode::Word(v) => {
                *v = convert_underscores_to_whitespace(v, false);
            }
        }
    }
}

const MATH_FUNCTIONS: &[&str] = &[
    "calc", "min", "max", "clamp", "mod", "rem", "sin", "cos", "tan", "asin", "acos", "atan",
    "atan2", "pow", "sqrt", "hypot", "log", "exp", "round",
];

pub fn has_math_fn(input: &str) -> bool {
    input.contains('(')
        && MATH_FUNCTIONS.iter().any(|f| {
            let mut s = String::with_capacity(f.len() + 1);
            s.push_str(f);
            s.push('(');
            input.contains(&s)
        })
}

pub fn add_whitespace_around_math_operators(input: &str) -> String {
    if !MATH_FUNCTIONS.iter().any(|f| input.contains(f)) {
        return input.to_string();
    }

    let bytes = input.as_bytes();
    let mut result: Vec<u8> = Vec::with_capacity(input.len() + 16);
    let mut formattable: Vec<bool> = Vec::new();
    let mut value_pos: Option<usize> = None;
    let mut last_value_pos: Option<usize> = None;

    let top = |f: &Vec<bool>| f.last().copied().unwrap_or(false);

    for i in 0..bytes.len() {
        let ch = bytes[i];

        if ch.is_ascii_digit() {
            value_pos = Some(i);
        } else if value_pos.is_some() && (ch == b'%' || ch.is_ascii_alphabetic()) {
            value_pos = Some(i);
        } else {
            last_value_pos = value_pos;
            value_pos = None;
        }

        if ch == b'(' {
            result.push(ch);
            let mut start = i;
            let mut j = i as isize - 1;
            while j >= 0 {
                let inner = bytes[j as usize];
                if inner.is_ascii_digit() || inner.is_ascii_lowercase() {
                    start = j as usize;
                } else {
                    break;
                }
                j -= 1;
            }
            let f = &input[start..i];
            if MATH_FUNCTIONS.contains(&f) {
                formattable.push(true);
                continue;
            } else if top(&formattable) && f.is_empty() {
                formattable.push(true);
                continue;
            }
            formattable.push(false);
            continue;
        } else if ch == b')' {
            result.push(ch);
            formattable.pop();
        } else if ch == b',' && top(&formattable) {
            result.extend_from_slice(b", ");
            continue;
        } else if ch == b' ' && top(&formattable) && result.last() == Some(&b' ') {
            continue;
        } else if matches!(ch, b'+' | b'*' | b'/' | b'-') && top(&formattable) {
            let trimmed_len = {
                let mut l = result.len();
                while l > 0 && (result[l - 1] as char).is_whitespace() {
                    l -= 1;
                }
                l
            };
            let prev = if trimmed_len >= 1 { result[trimmed_len - 1] } else { 0 };
            let prev_prev = if trimmed_len >= 2 { result[trimmed_len - 2] } else { 0 };
            let next = bytes.get(i + 1).copied().unwrap_or(0);

            if (prev == b'e' || prev == b'E') && prev_prev.is_ascii_digit() {
                result.push(ch);
                continue;
            } else if matches!(prev, b'+' | b'*' | b'/' | b'-') {
                result.push(ch);
                continue;
            } else if prev == b'(' || prev == b',' {
                result.push(ch);
                continue;
            } else if i > 0 && bytes[i - 1] == b' ' {
                result.push(ch);
                result.push(b' ');
            } else if prev.is_ascii_digit()
                || next.is_ascii_digit()
                || prev == b')'
                || next == b'('
                || matches!(next, b'+' | b'*' | b'/' | b'-')
                || (last_value_pos.is_some() && last_value_pos == Some(i.wrapping_sub(1)))
            {
                result.push(b' ');
                result.push(ch);
                result.push(b' ');
            } else {
                result.push(ch);
            }
        } else {
            result.push(ch);
        }
    }

    String::from_utf8(result).unwrap_or_else(|_| input.to_string())
}

/// CSS.escape (https://drafts.csswg.org/cssom/#serialize-an-identifier)
pub fn escape(value: &str) -> String {
    let chars: Vec<char> = value.chars().collect();
    if chars.len() == 1 && chars[0] == '-' {
        return "\\-".to_string();
    }
    let first = chars.first().copied().unwrap_or('\0');
    let mut result = String::with_capacity(value.len() + 8);
    for (index, &c) in chars.iter().enumerate() {
        let code = c as u32;
        if code == 0 {
            result.push('\u{FFFD}');
            continue;
        }
        if (0x01..=0x1f).contains(&code)
            || code == 0x7f
            || (index == 0 && c.is_ascii_digit())
            || (index == 1 && c.is_ascii_digit() && first == '-')
        {
            result.push('\\');
            result.push_str(&format!("{:x}", code));
            result.push(' ');
            continue;
        }
        if code >= 0x80 || c == '-' || c == '_' || c.is_ascii_alphanumeric() {
            result.push(c);
            continue;
        }
        result.push('\\');
        result.push(c);
    }
    result
}

pub fn unescape(escaped: &str) -> String {
    if !escaped.contains('\\') {
        return escaped.to_string();
    }
    static RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\\([\dA-Fa-f]{1,6}[\t\n\x0C\r ]?|[\s\S])").unwrap());
    RE.replace_all(escaped, |caps: &regex::Captures| {
        let m = caps.get(0).unwrap().as_str();
        let inner = &m[1..];
        if m.chars().count() > 2 {
            let hex = inner.trim();
            if let Ok(cp) = u32::from_str_radix(hex, 16) {
                // Invalid or surrogate code points become U+FFFD
                return char::from_u32(cp).filter(|_| cp != 0).unwrap_or('\u{FFFD}').to_string();
            }
            inner.to_string()
        } else {
            inner.to_string()
        }
    })
    .into_owned()
}

/// Alphanumeric compare where digit runs compare numerically.
pub fn compare(a: &str, z: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let ab = a.as_bytes();
    let zb = z.as_bytes();
    let min_len = ab.len().min(zb.len());
    let mut i = 0;
    while i < min_len {
        let ac = ab[i];
        let zc = zb[i];
        if ac.is_ascii_digit() && zc.is_ascii_digit() {
            let a_start = i;
            let mut a_end = i + 1;
            while a_end < ab.len() && ab[a_end].is_ascii_digit() {
                a_end += 1;
            }
            let z_start = i;
            let mut z_end = i + 1;
            while z_end < zb.len() && zb[z_end].is_ascii_digit() {
                z_end += 1;
            }
            let an = &a[a_start..a_end];
            let zn = &z[z_start..z_end];
            let av: f64 = an.parse().unwrap_or(0.0);
            let zv: f64 = zn.parse().unwrap_or(0.0);
            if av != zv {
                return av.partial_cmp(&zv).unwrap_or(Ordering::Equal);
            }
            match an.cmp(zn) {
                Ordering::Equal => {}
                o => return o,
            }
            // JS continues from i+1 (not from the end of the number)
            i += 1;
            continue;
        }
        if ac == zc {
            i += 1;
            continue;
        }
        return ac.cmp(&zc);
    }
    ab.len().cmp(&zb.len())
}

/* -------------------------------------------------------------------------- */
/* JS number semantics                                                         */
/* -------------------------------------------------------------------------- */

/// Emulates JS `Number(value)` for strings. Returns `None` for `NaN`.
pub fn js_number(s: &str) -> Option<f64> {
    let t = s.trim_matches(|c: char| c.is_whitespace() || c == '\u{FEFF}');
    if t.is_empty() {
        return Some(0.0);
    }
    match t {
        "Infinity" | "+Infinity" => return Some(f64::INFINITY),
        "-Infinity" => return Some(f64::NEG_INFINITY),
        _ => {}
    }
    let lower = t.to_ascii_lowercase();
    for (prefix, radix) in [("0x", 16), ("0o", 8), ("0b", 2)] {
        if let Some(rest) = lower.strip_prefix(prefix) {
            if rest.is_empty() {
                return None;
            }
            return u64::from_str_radix(rest, radix).ok().map(|v| v as f64);
        }
    }
    // Validate StrDecimalLiteral
    let b = t.as_bytes();
    let mut i = 0;
    if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
        i += 1;
    }
    let mut digits = 0;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
        digits += 1;
    }
    if i < b.len() && b[i] == b'.' {
        i += 1;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
            digits += 1;
        }
    }
    if digits == 0 {
        return None;
    }
    if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
        i += 1;
        if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
            i += 1;
        }
        let mut exp_digits = 0;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
            exp_digits += 1;
        }
        if exp_digits == 0 {
            return None;
        }
    }
    if i != b.len() {
        return None;
    }
    t.parse::<f64>().ok()
}

/// Emulates JS `String(number)`.
pub fn js_num_to_string(n: f64) -> String {
    if n.is_nan() {
        return "NaN".into();
    }
    if n.is_infinite() {
        return if n > 0.0 { "Infinity".into() } else { "-Infinity".into() };
    }
    if n == 0.0 {
        return "0".into();
    }
    let abs = n.abs();
    if (1e-7..1e21).contains(&abs) {
        let s = format!("{}", n);
        return s;
    }
    // Exponential form
    let s = format!("{:e}", n); // e.g. 1e21, 1.5e-7
    let (mantissa, exp) = s.split_once('e').unwrap();
    let exp: i32 = exp.parse().unwrap_or(0);
    if exp >= 0 {
        format!("{}e+{}", mantissa, exp)
    } else {
        format!("{}e{}", mantissa, exp)
    }
}

/// JS `parseInt(s)` (base 10) — returns None for NaN.
pub fn js_parse_int(s: &str) -> Option<f64> {
    let t = s.trim_start();
    let b = t.as_bytes();
    let mut i = 0;
    let mut neg = false;
    if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
        neg = b[i] == b'-';
        i += 1;
    }
    let start = i;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    if i == start {
        return None;
    }
    let v: f64 = t[start..i].parse().ok()?;
    Some(if neg { -v } else { v })
}

/// JS `parseFloat(s)` — returns None for NaN.
pub fn js_parse_float(s: &str) -> Option<f64> {
    let t = s.trim_start();
    static RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^[+-]?(Infinity|(\d+\.?\d*|\.\d+)([eE][+-]?\d+)?)").unwrap());
    let m = RE.find(t)?;
    let v = m.as_str();
    if v.ends_with("Infinity") {
        return Some(if v.starts_with('-') { f64::NEG_INFINITY } else { f64::INFINITY });
    }
    v.parse::<f64>().ok()
}

pub fn is_positive_integer(value: &str) -> bool {
    match js_number(value) {
        Some(n) => n.fract() == 0.0 && n.is_finite() && n >= 0.0 && js_num_to_string(n) == value,
        None => false,
    }
}

pub fn is_strict_positive_integer(value: &str) -> bool {
    match js_number(value) {
        Some(n) => n.fract() == 0.0 && n.is_finite() && n > 0.0 && js_num_to_string(n) == value,
        None => false,
    }
}

fn is_multiple_of(value: &str, divisor: f64) -> bool {
    match js_number(value) {
        Some(n) => n >= 0.0 && n % divisor == 0.0 && js_num_to_string(n) == value,
        None => false,
    }
}

pub fn is_valid_spacing_multiplier(value: &str) -> bool {
    is_multiple_of(value, 0.25)
}

pub fn is_valid_opacity_value(value: &str) -> bool {
    is_multiple_of(value, 0.25)
}

/* -------------------------------------------------------------------------- */
/* Data types                                                                  */
/* -------------------------------------------------------------------------- */

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DataType {
    Color,
    Length,
    Percentage,
    Ratio,
    Number,
    Integer,
    Url,
    Position,
    BgSize,
    LineWidth,
    Image,
    FamilyName,
    GenericName,
    AbsoluteSize,
    RelativeSize,
    Angle,
    Vector,
    /// Only used for the `types` lists of utilities (`any`)
    Any,
}

impl DataType {
    pub fn from_str(s: &str) -> Option<DataType> {
        Some(match s {
            "color" => DataType::Color,
            "length" => DataType::Length,
            "percentage" => DataType::Percentage,
            "ratio" => DataType::Ratio,
            "number" => DataType::Number,
            "integer" => DataType::Integer,
            "url" => DataType::Url,
            "position" => DataType::Position,
            "bg-size" => DataType::BgSize,
            "line-width" => DataType::LineWidth,
            "image" => DataType::Image,
            "family-name" => DataType::FamilyName,
            "generic-name" => DataType::GenericName,
            "absolute-size" => DataType::AbsoluteSize,
            "relative-size" => DataType::RelativeSize,
            "angle" => DataType::Angle,
            "vector" => DataType::Vector,
            "any" => DataType::Any,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            DataType::Color => "color",
            DataType::Length => "length",
            DataType::Percentage => "percentage",
            DataType::Ratio => "ratio",
            DataType::Number => "number",
            DataType::Integer => "integer",
            DataType::Url => "url",
            DataType::Position => "position",
            DataType::BgSize => "bg-size",
            DataType::LineWidth => "line-width",
            DataType::Image => "image",
            DataType::FamilyName => "family-name",
            DataType::GenericName => "generic-name",
            DataType::AbsoluteSize => "absolute-size",
            DataType::RelativeSize => "relative-size",
            DataType::Angle => "angle",
            DataType::Vector => "vector",
            DataType::Any => "any",
        }
    }

    fn check(self, value: &str) -> bool {
        match self {
            DataType::Color => is_color(value),
            DataType::Length => is_length(value),
            DataType::Percentage => is_percentage(value),
            DataType::Ratio => is_fraction(value),
            DataType::Number => is_number(value),
            DataType::Integer => is_positive_integer(value),
            DataType::Url => is_url(value),
            DataType::Position => is_background_position(value),
            DataType::BgSize => is_background_size(value),
            DataType::LineWidth => is_line_width(value),
            DataType::Image => is_image(value),
            DataType::FamilyName => is_family_name(value),
            DataType::GenericName => is_generic_name(value),
            DataType::AbsoluteSize => is_absolute_size(value),
            DataType::RelativeSize => is_relative_size(value),
            DataType::Angle => is_angle(value),
            DataType::Vector => is_vector(value),
            DataType::Any => false,
        }
    }
}

pub fn infer_data_type(value: &str, types: &[DataType]) -> Option<DataType> {
    if value.starts_with("var(") {
        return None;
    }
    types.iter().copied().find(|t| t.check(value))
}

fn is_url(value: &str) -> bool {
    value.starts_with("url(") && value.ends_with(')') && !value.contains('\n')
}

fn is_line_width(value: &str) -> bool {
    segment(value, b' ')
        .iter()
        .all(|v| is_length(v) || is_number(v) || *v == "thin" || *v == "medium" || *v == "thick")
}

fn is_image(value: &str) -> bool {
    static IMAGE_FN: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^(?:element|image|cross-fade|image-set)\(").unwrap());
    static GRADIENT_FN: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^(repeating-)?(conic|linear|radial)-gradient\(").unwrap());
    let mut count = 0;
    for part in segment(value, b',') {
        if part.starts_with("var(") {
            continue;
        }
        if is_url(part) || GRADIENT_FN.is_match(part) || IMAGE_FN.is_match(part) {
            count += 1;
            continue;
        }
        return false;
    }
    count > 0
}

fn is_generic_name(value: &str) -> bool {
    matches!(
        value,
        "serif"
            | "sans-serif"
            | "monospace"
            | "cursive"
            | "fantasy"
            | "system-ui"
            | "ui-serif"
            | "ui-sans-serif"
            | "ui-monospace"
            | "ui-rounded"
            | "math"
            | "emoji"
            | "fangsong"
    )
}

fn is_family_name(value: &str) -> bool {
    let mut count = 0;
    for part in segment(value, b',') {
        if part.as_bytes().first().is_some_and(|c| c.is_ascii_digit()) {
            return false;
        }
        if part.starts_with("var(") {
            continue;
        }
        count += 1;
    }
    count > 0
}

fn is_absolute_size(value: &str) -> bool {
    matches!(
        value,
        "xx-small" | "x-small" | "small" | "medium" | "large" | "x-large" | "xx-large" | "xxx-large"
    )
}

fn is_relative_size(value: &str) -> bool {
    value == "larger" || value == "smaller"
}

const HAS_NUMBER: &str = r"[+-]?\d*\.?\d+(?:[eE][+-]?\d+)?";

static IS_NUMBER: LazyLock<Regex> = LazyLock::new(|| Regex::new(&format!("^{}$", HAS_NUMBER)).unwrap());
static IS_PERCENTAGE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(&format!("^{}%$", HAS_NUMBER)).unwrap());
// Note: upstream uses `\s` inside a template string which becomes a plain `s`.
static IS_FRACTION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(&format!(r"^{}\s*/\s*{}$", HAS_NUMBER, HAS_NUMBER)).unwrap());
static IS_LENGTH: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        "^{}(cm|mm|Q|in|pc|pt|px|em|ex|ch|rem|lh|rlh|vw|vh|vmin|vmax|vb|vi|svw|svh|lvw|lvh|dvw|dvh|cqw|cqh|cqi|cqb|cqmin|cqmax)$",
        HAS_NUMBER
    ))
    .unwrap()
});
static IS_ANGLE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(&format!("^{}(deg|rad|grad|turn)$", HAS_NUMBER)).unwrap());
static IS_VECTOR: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!("^{} +{} +{}$", HAS_NUMBER, HAS_NUMBER, HAS_NUMBER)).unwrap()
});

pub fn is_number(value: &str) -> bool {
    IS_NUMBER.is_match(value) || has_math_fn(value)
}

pub fn is_percentage(value: &str) -> bool {
    IS_PERCENTAGE.is_match(value) || has_math_fn(value)
}

pub fn is_fraction(value: &str) -> bool {
    IS_FRACTION.is_match(value) || has_math_fn(value)
}

pub fn is_length(value: &str) -> bool {
    IS_LENGTH.is_match(value) || value.get(..10).is_some_and(|p| p.eq_ignore_ascii_case("--spacing(")) || has_math_fn(value)
}

fn is_background_position(value: &str) -> bool {
    let mut count = 0;
    for part in segment(value, b' ') {
        if matches!(part, "center" | "top" | "right" | "bottom" | "left") {
            count += 1;
            continue;
        }
        if part.starts_with("var(") {
            continue;
        }
        if is_length(part) || is_percentage(part) {
            count += 1;
            continue;
        }
        return false;
    }
    count > 0
}

fn is_background_size(value: &str) -> bool {
    let mut count = 0;
    for size in segment(value, b',') {
        if size == "cover" || size == "contain" {
            count += 1;
            continue;
        }
        let values = segment(size, b' ');
        if values.len() != 1 && values.len() != 2 {
            return false;
        }
        if values.iter().all(|v| *v == "auto" || is_length(v) || is_percentage(v)) {
            count += 1;
            continue;
        }
    }
    count > 0
}

fn is_angle(value: &str) -> bool {
    IS_ANGLE.is_match(value)
}

fn is_vector(value: &str) -> bool {
    IS_VECTOR.is_match(value)
}

static NAMED_COLORS: &[&str] = &[
    "black", "silver", "gray", "white", "maroon", "red", "purple", "fuchsia", "green", "lime",
    "olive", "yellow", "navy", "blue", "teal", "aqua", "aliceblue", "antiquewhite", "aquamarine",
    "azure", "beige", "bisque", "blanchedalmond", "blueviolet", "brown", "burlywood", "cadetblue",
    "chartreuse", "chocolate", "coral", "cornflowerblue", "cornsilk", "crimson", "cyan",
    "darkblue", "darkcyan", "darkgoldenrod", "darkgray", "darkgreen", "darkgrey", "darkkhaki",
    "darkmagenta", "darkolivegreen", "darkorange", "darkorchid", "darkred", "darksalmon",
    "darkseagreen", "darkslateblue", "darkslategray", "darkslategrey", "darkturquoise",
    "darkviolet", "deeppink", "deepskyblue", "dimgray", "dimgrey", "dodgerblue", "firebrick",
    "floralwhite", "forestgreen", "gainsboro", "ghostwhite", "gold", "goldenrod", "greenyellow",
    "grey", "honeydew", "hotpink", "indianred", "indigo", "ivory", "khaki", "lavender",
    "lavenderblush", "lawngreen", "lemonchiffon", "lightblue", "lightcoral", "lightcyan",
    "lightgoldenrodyellow", "lightgray", "lightgreen", "lightgrey", "lightpink", "lightsalmon",
    "lightseagreen", "lightskyblue", "lightslategray", "lightslategrey", "lightsteelblue",
    "lightyellow", "limegreen", "linen", "magenta", "mediumaquamarine", "mediumblue",
    "mediumorchid", "mediumpurple", "mediumseagreen", "mediumslateblue", "mediumspringgreen",
    "mediumturquoise", "mediumvioletred", "midnightblue", "mintcream", "mistyrose", "moccasin",
    "navajowhite", "oldlace", "olivedrab", "orange", "orangered", "orchid", "palegoldenrod",
    "palegreen", "paleturquoise", "palevioletred", "papayawhip", "peachpuff", "peru", "pink",
    "plum", "powderblue", "rebeccapurple", "rosybrown", "royalblue", "saddlebrown", "salmon",
    "sandybrown", "seagreen", "seashell", "sienna", "skyblue", "slateblue", "slategray",
    "slategrey", "snow", "springgreen", "steelblue", "tan", "thistle", "tomato", "turquoise",
    "violet", "wheat", "whitesmoke", "yellowgreen", "transparent", "currentcolor", "canvas",
    "canvastext", "linktext", "visitedtext", "activetext", "buttonface", "buttontext",
    "buttonborder", "field", "fieldtext", "highlight", "highlighttext", "selecteditem",
    "selecteditemtext", "mark", "marktext", "graytext", "accentcolor", "accentcolortext",
];

pub fn is_color(value: &str) -> bool {
    static COLOR_FN: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^(rgba?|hsla?|hwb|color|(ok)?(lab|lch)|light-dark|color-mix|--alpha)\(").unwrap()
    });
    if value.starts_with('#') || COLOR_FN.is_match(value) {
        return true;
    }
    let lower = value.to_ascii_lowercase();
    NAMED_COLORS.contains(&lower.as_str())
}

pub fn is_named_color(value: &str) -> bool {
    NAMED_COLORS.contains(&value.to_ascii_lowercase().as_str())
}

/// Parse a dimension such as `64rem` into `(64, Some("rem"))`.
pub fn dimensions(input: &str) -> Option<(f64, Option<String>)> {
    static RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)^([-+]?(?:\d*\.)?\d+)([a-z]+|%)?$").unwrap());
    let caps = RE.captures(input)?;
    let value: f64 = caps.get(1)?.as_str().parse().ok()?;
    Some((value, caps.get(2).map(|m| m.as_str().to_string())))
}

pub fn compare_breakpoints(a: &str, z: &str, asc: bool) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    if a == z {
        return Ordering::Equal;
    }
    static DIGITS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[\d.]+").unwrap());
    let a_bucket = match a.find('(') {
        None => DIGITS.replace_all(a, "").into_owned(),
        Some(i) => a[..i].to_string(),
    };
    let z_bucket = match z.find('(') {
        None => DIGITS.replace_all(z, "").into_owned(),
        Some(i) => z[..i].to_string(),
    };
    let bucket = a_bucket.cmp(&z_bucket);
    if bucket != Ordering::Equal {
        return bucket;
    }
    let (ai, zi) = (js_parse_int(a), js_parse_int(z));
    match (ai, zi) {
        (Some(ai), Some(zi)) => {
            let d = if asc { ai - zi } else { zi - ai };
            if d < 0.0 {
                Ordering::Less
            } else if d > 0.0 {
                Ordering::Greater
            } else {
                Ordering::Equal
            }
        }
        _ => {
            if a < z {
                Ordering::Less
            } else {
                Ordering::Greater
            }
        }
    }
}

pub fn replace_shadow_colors(input: &str, replacement: &dyn Fn(&str) -> String) -> String {
    use crate::tw::value_parser::{self as vp, ValueNode};
    static LENGTH: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^-?(\d+|\.\d+)(.*?)$").unwrap());
    const KEYWORDS: &[&str] = &["inset", "inherit", "initial", "revert", "unset"];
    const LENGTH_FUNCTIONS: &[&str] = &["calc", "clamp", "max", "min", "--spacing"];
    const COLOR_FUNCTIONS: &[&str] = &[
        "color", "color-mix", "contrast-color", "device-cmyk", "hsl", "hsla", "hwb", "lab", "lch", "light-dark",
        "oklab", "oklch", "rgb", "rgba", "--alpha",
    ];

    let replace_at = |ast: &mut Vec<ValueNode>, i: usize| {
        let color = vp::to_css(std::slice::from_ref(&ast[i]));
        let replaced = vp::parse(&replacement(&color));
        ast.splice(i..i + 1, replaced);
    };

    let shadows: Vec<String> = segment(input, b',')
        .into_iter()
        .map(|shadow| {
            let shadow = shadow.trim();
            let mut ast = vp::parse(shadow);

            // Functions are never descended into, so only top-level nodes matter.
            let mut unknown: Option<usize> = None;
            let mut unknowns = 0;
            let mut lengths = 0;
            for i in 0..ast.len() {
                match &ast[i] {
                    ValueNode::Word(w) => {
                        let lower = w.to_ascii_lowercase();
                        if KEYWORDS.contains(&lower.as_str()) {
                            continue;
                        }
                        if LENGTH.is_match(&lower) {
                            lengths += 1;
                            continue;
                        }
                        if w.starts_with('#') || is_named_color(w) {
                            replace_at(&mut ast, i);
                            return vp::to_css(&ast);
                        }
                        unknown = Some(i);
                        unknowns += 1;
                    }
                    ValueNode::Function { value, .. } => {
                        let lower = value.to_ascii_lowercase();
                        if COLOR_FUNCTIONS.contains(&lower.as_str()) {
                            replace_at(&mut ast, i);
                            return vp::to_css(&ast);
                        }
                        if LENGTH_FUNCTIONS.contains(&lower.as_str()) {
                            lengths += 1;
                            continue;
                        }
                        unknown = Some(i);
                        unknowns += 1;
                    }
                    ValueNode::Separator(_) => {}
                }
            }

            if lengths < 2 {
                return shadow.to_string();
            }
            if unknowns == 0 {
                return format!("{} {}", shadow, replacement("currentcolor"));
            }
            // A single left-over is assumed to be the color
            if unknowns == 1 {
                replace_at(&mut ast, unknown.unwrap());
                return vp::to_css(&ast);
            }
            shadow.to_string()
        })
        .collect();
    shadows.join(", ")
}

/// Brace expansion used by `@source inline(…)`.
pub fn expand(pattern: &str) -> Vec<String> {
    let Some(index) = pattern.find('{') else {
        return vec![pattern.to_string()];
    };
    let pre = &pattern[..index];
    let rest = &pattern[index..];
    let mut depth = 0i32;
    let mut end_index: Option<usize> = rest.rfind('}');
    for (i, ch) in rest.char_indices() {
        if ch == '{' {
            depth += 1;
        } else if ch == '}' {
            depth -= 1;
            if depth == 0 {
                end_index = Some(i);
                break;
            }
        }
    }
    let Some(end_index) = end_index else {
        return vec![pattern.to_string()];
    };
    let inside = &rest[1..end_index];
    let post = &rest[end_index + 1..];

    static RANGE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^(-?\d+)\.\.(-?\d+)(?:\.\.(-?\d+))?$").unwrap());
    let parts: Vec<String> = if let Some(caps) = RANGE.captures(inside) {
        let start: i64 = caps[1].parse().unwrap_or(0);
        let end: i64 = caps[2].parse().unwrap_or(0);
        let mut step: i64 = caps.get(3).and_then(|m| m.as_str().parse().ok()).unwrap_or(if start <= end { 1 } else { -1 });
        if step == 0 {
            return vec![];
        }
        let increasing = start < end;
        if increasing && step < 0 {
            step = -step;
        }
        if !increasing && step > 0 {
            step = -step;
        }
        let mut out = vec![];
        let mut i = start;
        while if increasing { i <= end } else { i >= end } {
            out.push(i.to_string());
            i += step;
        }
        out
    } else {
        segment(inside, b',').into_iter().map(|s| s.to_string()).collect()
    };
    let parts: Vec<String> = parts.iter().flat_map(|p| expand(p)).collect();
    let mut result = vec![];
    for tail in expand(post) {
        for part in &parts {
            result.push(format!("{}{}{}", pre, part, tail));
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segment_works() {
        assert_eq!(segment("a:b:[c:d]", b':'), vec!["a", "b", "[c:d]"]);
        assert_eq!(segment("var(--a, 0 0 1px rgb(0, 0, 0)), 0 0 1px rgb(0, 0, 0)", b','), vec!["var(--a, 0 0 1px rgb(0, 0, 0))", " 0 0 1px rgb(0, 0, 0)"]);
    }

    #[test]
    fn escape_works() {
        assert_eq!(escape("hover:bg-red-500/50"), "hover\\:bg-red-500\\/50");
        assert_eq!(escape("2xl:flex"), "\\32 xl\\:flex");
        assert_eq!(unescape("\\32 xl\\:flex"), "2xl:flex");
    }

    #[test]
    fn math_ops() {
        assert_eq!(add_whitespace_around_math_operators("calc(100%-1rem)"), "calc(100% - 1rem)");
        assert_eq!(add_whitespace_around_math_operators("calc(var(--a)*2)"), "calc(var(--a) * 2)");
    }

    #[test]
    fn js_numbers() {
        assert!(is_positive_integer("12"));
        assert!(!is_positive_integer("012"));
        assert!(is_valid_spacing_multiplier("2.5"));
        assert!(!is_valid_spacing_multiplier("2.50"));
        assert_eq!(js_num_to_string(0.5), "0.5");
        assert_eq!(js_num_to_string(1e21), "1e+21");
    }
}
