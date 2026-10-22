//! Port of `tailwindcss/src/candidate.ts` (parsing only).

use crate::tw::design_system::DesignSystem;
use crate::tw::utils::{decode_arbitrary_value, is_valid_arbitrary, segment};
use std::rc::Rc;

#[derive(Debug, Clone, PartialEq)]
pub enum Modifier {
    Arbitrary(String),
    Named(String),
}

impl Modifier {
    pub fn value(&self) -> &str {
        match self {
            Modifier::Arbitrary(v) | Modifier::Named(v) => v,
        }
    }
    pub fn is_arbitrary(&self) -> bool {
        matches!(self, Modifier::Arbitrary(_))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum UtilityValue {
    Arbitrary { data_type: Option<String>, value: String },
    Named { value: String, fraction: Option<String> },
}

impl UtilityValue {
    pub fn value(&self) -> &str {
        match self {
            UtilityValue::Arbitrary { value, .. } | UtilityValue::Named { value, .. } => value,
        }
    }
    pub fn is_arbitrary(&self) -> bool {
        matches!(self, UtilityValue::Arbitrary { .. })
    }
    pub fn fraction(&self) -> Option<&str> {
        match self {
            UtilityValue::Named { fraction, .. } => fraction.as_deref(),
            _ => None,
        }
    }
    pub fn data_type(&self) -> Option<&str> {
        match self {
            UtilityValue::Arbitrary { data_type, .. } => data_type.as_deref(),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum VariantValue {
    Arbitrary(String),
    Named(String),
}

impl VariantValue {
    pub fn value(&self) -> &str {
        match self {
            VariantValue::Arbitrary(v) | VariantValue::Named(v) => v,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Variant {
    Arbitrary { selector: String, relative: bool },
    Static { root: String },
    Functional { root: String, value: Option<VariantValue>, modifier: Option<Modifier> },
    Compound { root: String, modifier: Option<Modifier>, variant: Rc<Variant> },
}

impl Variant {
    pub fn root(&self) -> Option<&str> {
        match self {
            Variant::Arbitrary { .. } => None,
            Variant::Static { root } | Variant::Functional { root, .. } | Variant::Compound { root, .. } => {
                Some(root)
            }
        }
    }
    pub fn kind_str(&self) -> &'static str {
        match self {
            Variant::Arbitrary { .. } => "arbitrary",
            Variant::Static { .. } => "static",
            Variant::Functional { .. } => "functional",
            Variant::Compound { .. } => "compound",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum CandidateKind {
    Arbitrary { property: String, value: String },
    Static { root: String },
    Functional { root: String, value: Option<UtilityValue> },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub kind: CandidateKind,
    pub modifier: Option<Modifier>,
    /// Stored innermost-first (same as upstream: the last written variant is first).
    pub variants: Vec<Rc<Variant>>,
    pub important: bool,
    pub raw: String,
}

impl Candidate {
    pub fn root(&self) -> &str {
        match &self.kind {
            CandidateKind::Static { root } | CandidateKind::Functional { root, .. } => root,
            CandidateKind::Arbitrary { property, .. } => property,
        }
    }
    pub fn value(&self) -> Option<&UtilityValue> {
        match &self.kind {
            CandidateKind::Functional { value, .. } => value.as_ref(),
            _ => None,
        }
    }
    pub fn is_static(&self) -> bool {
        matches!(self.kind, CandidateKind::Static { .. })
    }
    pub fn is_functional(&self) -> bool {
        matches!(self.kind, CandidateKind::Functional { .. })
    }
}

fn is_valid_named_value(v: &str) -> bool {
    !v.is_empty() && v.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'%' | b'-'))
}

pub fn parse_candidate(input: &str, ds: &DesignSystem) -> Vec<Candidate> {
    let mut out = Vec::new();
    let mut raw_variants = segment(input, b':');

    if let Some(prefix) = &ds.theme.prefix {
        if raw_variants.len() == 1 {
            return out;
        }
        if raw_variants[0] != prefix {
            return out;
        }
        raw_variants.remove(0);
    }

    let mut base = raw_variants.pop().unwrap_or("");

    let mut parsed_variants = Vec::with_capacity(raw_variants.len());
    for raw in raw_variants.iter().rev() {
        match ds.parse_variant(raw) {
            Some(v) => parsed_variants.push(v),
            None => return out,
        }
    }

    let mut important = false;
    if base.ends_with('!') {
        important = true;
        base = &base[..base.len() - 1];
    } else if base.starts_with('!') {
        important = true;
        base = &base[1..];
    }

    if ds.utilities.has(base, false) && !base.contains('[') {
        out.push(Candidate {
            kind: CandidateKind::Static { root: base.to_string() },
            modifier: None,
            variants: parsed_variants.clone(),
            important,
            raw: input.to_string(),
        });
    }

    let parts = segment(base, b'/');
    if parts.len() > 2 {
        // `additionalModifier` is truthy only when non-empty
        if !parts[2].is_empty() {
            return out;
        }
    }
    let mut base_without_modifier = parts[0];
    let modifier_segment = parts.get(1).copied();

    let parsed_modifier = modifier_segment.map(parse_modifier);
    if modifier_segment.is_some() && parsed_modifier.as_ref().unwrap().is_none() {
        return out;
    }
    let parsed_modifier = parsed_modifier.flatten();

    // Arbitrary properties
    if base_without_modifier.starts_with('[') {
        if !base_without_modifier.ends_with(']') {
            return out;
        }
        let c = base_without_modifier.as_bytes().get(1).copied().unwrap_or(0);
        if c != b'-' && !c.is_ascii_lowercase() {
            return out;
        }
        base_without_modifier = &base_without_modifier[1..base_without_modifier.len() - 1];
        let Some(idx) = base_without_modifier.find(':') else { return out };
        if idx == 0 || idx == base_without_modifier.len() - 1 {
            return out;
        }
        let property = &base_without_modifier[..idx];
        let value = decode_arbitrary_value(&base_without_modifier[idx + 1..]);
        if !is_valid_arbitrary(&value) {
            return out;
        }
        out.push(Candidate {
            kind: CandidateKind::Arbitrary { property: property.to_string(), value },
            modifier: parsed_modifier,
            variants: parsed_variants,
            important,
            raw: input.to_string(),
        });
        return out;
    }

    let roots: Vec<(String, Option<String>)>;

    if base_without_modifier.ends_with(']') {
        let Some(idx) = base_without_modifier.find("-[") else { return out };
        let root = &base_without_modifier[..idx];
        if !ds.utilities.has(root, true) {
            return out;
        }
        roots = vec![(root.to_string(), Some(base_without_modifier[idx + 1..].to_string()))];
    } else if base_without_modifier.ends_with(')') {
        let Some(idx) = base_without_modifier.find("-(") else { return out };
        let root = &base_without_modifier[..idx];
        if !ds.utilities.has(root, true) {
            return out;
        }
        let inner = &base_without_modifier[idx + 2..base_without_modifier.len() - 1];
        let p = segment(inner, b':');
        let (data_type, value) = if p.len() == 2 { (Some(p[0]), p[1]) } else { (None, inner) };
        if !value.starts_with("--") {
            return out;
        }
        if !is_valid_arbitrary(value) {
            return out;
        }
        let v = match data_type {
            None => format!("[var({})]", value),
            Some(dt) => format!("[{}:var({})]", dt, value),
        };
        roots = vec![(root.to_string(), Some(v))];
    } else {
        roots = find_roots(base_without_modifier, &|r| ds.utilities.has(r, true));
    }

    for (root, value) in roots {
        let mut candidate = Candidate {
            kind: CandidateKind::Functional { root: root.clone(), value: None },
            modifier: parsed_modifier.clone(),
            variants: parsed_variants.clone(),
            important,
            raw: input.to_string(),
        };

        let Some(value) = value else {
            out.push(candidate);
            continue;
        };

        let new_value;
        if let Some(start) = value.find('[') {
            if !value.ends_with(']') {
                return out;
            }
            let mut arbitrary_value = decode_arbitrary_value(&value[start + 1..value.len() - 1]);
            if !is_valid_arbitrary(&arbitrary_value) {
                continue;
            }

            let mut typehint: Option<String> = None;
            {
                let bytes = arbitrary_value.as_bytes();
                for i in 0..bytes.len() {
                    let code = bytes[i];
                    if code == b':' {
                        typehint = Some(arbitrary_value[..i].to_string());
                        arbitrary_value = arbitrary_value[i + 1..].to_string();
                        break;
                    }
                    if code == b'-' || code.is_ascii_lowercase() {
                        continue;
                    }
                    break;
                }
            }

            if arbitrary_value.is_empty() || arbitrary_value.trim().is_empty() {
                continue;
            }
            if typehint.as_deref() == Some("") {
                continue;
            }

            new_value = UtilityValue::Arbitrary { data_type: typehint, value: arbitrary_value };
        } else {
            let fraction = match (&modifier_segment, &parsed_modifier) {
                (None, _) => None,
                (_, Some(Modifier::Arbitrary(_))) => None,
                (Some(seg), _) => Some(format!("{}/{}", value, seg)),
            };
            if !is_valid_named_value(&value) {
                continue;
            }
            new_value = UtilityValue::Named { value, fraction };
        }

        candidate.kind = CandidateKind::Functional { root, value: Some(new_value) };
        out.push(candidate);
    }

    out
}

pub fn parse_modifier(modifier: &str) -> Option<Modifier> {
    if modifier.starts_with('[') && modifier.ends_with(']') && modifier.len() >= 2 {
        let v = decode_arbitrary_value(&modifier[1..modifier.len() - 1]);
        if !is_valid_arbitrary(&v) {
            return None;
        }
        if v.is_empty() || v.trim().is_empty() {
            return None;
        }
        return Some(Modifier::Arbitrary(v));
    }

    if modifier.starts_with('(') && modifier.ends_with(')') && modifier.len() >= 2 {
        let inner = &modifier[1..modifier.len() - 1];
        if !inner.starts_with("--") {
            return None;
        }
        if !is_valid_arbitrary(inner) {
            return None;
        }
        let wrapped = format!("var({})", inner);
        return Some(Modifier::Arbitrary(decode_arbitrary_value(&wrapped)));
    }

    if !is_valid_named_value(modifier) {
        return None;
    }
    Some(Modifier::Named(modifier.to_string()))
}

pub fn parse_variant(variant: &str, ds: &DesignSystem) -> Option<Variant> {
    if variant.starts_with('[') && variant.ends_with(']') && variant.len() >= 2 {
        if variant.as_bytes().get(1) == Some(&b'@') && variant.contains('&') {
            return None;
        }
        let mut selector = decode_arbitrary_value(&variant[1..variant.len() - 1]);
        if !is_valid_arbitrary(&selector) {
            return None;
        }
        if selector.is_empty() || selector.trim().is_empty() {
            return None;
        }
        let first = selector.as_bytes()[0];
        let relative = first == b'>' || first == b'+' || first == b'~';
        if !relative && first != b'@' && !selector.contains('&') {
            selector = format!("&:is({})", selector);
        }
        return Some(Variant::Arbitrary { selector, relative });
    }

    let parts = segment(variant, b'/');
    if parts.len() > 2 && !parts[2].is_empty() {
        return None;
    }
    let variant_without_modifier = parts[0];
    let mut modifier: Option<String> = parts.get(1).map(|s| s.to_string());

    let roots = find_roots(variant_without_modifier, &|r| ds.variants.has(r));

    for (root, value) in roots {
        match ds.variants.kind(&root) {
            Some(crate::tw::variants::VariantKind::Static) => {
                if value.is_some() {
                    return None;
                }
                if modifier.is_some() {
                    return None;
                }
                return Some(Variant::Static { root });
            }
            Some(crate::tw::variants::VariantKind::Functional) => {
                let parsed_modifier = match &modifier {
                    None => None,
                    Some(m) => match parse_modifier(m) {
                        Some(pm) => Some(pm),
                        None => return None,
                    },
                };

                let Some(value) = value else {
                    return Some(Variant::Functional { root, modifier: parsed_modifier, value: None });
                };

                if value.ends_with(']') {
                    if !value.starts_with('[') {
                        continue;
                    }
                    let av = decode_arbitrary_value(&value[1..value.len() - 1]);
                    if !is_valid_arbitrary(&av) {
                        return None;
                    }
                    if av.is_empty() || av.trim().is_empty() {
                        return None;
                    }
                    return Some(Variant::Functional {
                        root,
                        modifier: parsed_modifier,
                        value: Some(VariantValue::Arbitrary(av)),
                    });
                }

                if value.ends_with(')') {
                    if !value.starts_with('(') {
                        continue;
                    }
                    let av = decode_arbitrary_value(&value[1..value.len() - 1]);
                    if !is_valid_arbitrary(&av) {
                        return None;
                    }
                    if av.is_empty() || av.trim().is_empty() {
                        return None;
                    }
                    if !av.starts_with("--") {
                        return None;
                    }
                    return Some(Variant::Functional {
                        root,
                        modifier: parsed_modifier,
                        value: Some(VariantValue::Arbitrary(format!("var({})", av))),
                    });
                }

                if !is_valid_named_value(&value) {
                    continue;
                }

                return Some(Variant::Functional {
                    root,
                    modifier: parsed_modifier,
                    value: Some(VariantValue::Named(value)),
                });
            }
            Some(crate::tw::variants::VariantKind::Compound) => {
                let mut value = value?;

                if let Some(m) = &modifier {
                    if root == "not" || root == "has" || root == "in" {
                        value = format!("{}/{}", value, m);
                        modifier = None;
                    }
                }

                let sub = ds.parse_variant(&value)?;

                if !ds.variants.compounds_with(&root, &sub) {
                    return None;
                }

                let parsed_modifier = match &modifier {
                    None => None,
                    Some(m) => match parse_modifier(m) {
                        Some(pm) => Some(pm),
                        None => return None,
                    },
                };

                return Some(Variant::Compound { root, modifier: parsed_modifier, variant: sub });
            }
            None => {}
        }
    }

    None
}

/// Returns `(root, value)` permutations.
pub fn find_roots(input: &str, exists: &dyn Fn(&str) -> bool) -> Vec<(String, Option<String>)> {
    let mut out = Vec::new();

    if exists(input) {
        out.push((input.to_string(), None));
    }

    let mut idx = input.rfind('-');
    while let Some(i) = idx {
        if i == 0 {
            break;
        }
        let maybe_root = &input[..i];
        if exists(maybe_root) {
            let value = &input[i + 1..];
            if value.is_empty() {
                break;
            }
            if maybe_root == "@" && exists("@") && input.as_bytes()[i] == b'-' {
                break;
            }
            out.push((maybe_root.to_string(), Some(value.to_string())));
        }
        idx = input[..i].rfind('-');
    }

    if input.starts_with('@') && exists("@") {
        out.push(("@".to_string(), Some(input[1..].to_string())));
    }

    out
}
