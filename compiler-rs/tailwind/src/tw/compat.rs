//! Port of `tailwindcss/src/compat/legacy-utilities.ts` (registered for every
//! build by `applyCompatibilityHooks`).

use crate::tw::ast::decl;
use crate::tw::candidate::UtilityValue;
use crate::tw::utilities::Utilities;
use crate::tw::utils::{is_positive_integer, is_valid_spacing_multiplier, segment};

pub fn register_legacy_utilities(u: &mut Utilities) {
    for (value, direction) in [
        ("t", "top"),
        ("tr", "top right"),
        ("r", "right"),
        ("br", "bottom right"),
        ("b", "bottom"),
        ("bl", "bottom left"),
        ("l", "left"),
        ("tl", "top left"),
    ] {
        u.static_(&format!("bg-gradient-to-{}", value), move |_, _| {
            Some(vec![
                decl("--tw-gradient-position", format!("to {} in oklab", direction)),
                decl("background-image", "linear-gradient(var(--tw-gradient-stops))"),
            ])
        });
    }

    for (name, prop, value) in [
        ("bg-left-top", "background-position", "left top"),
        ("bg-right-top", "background-position", "right top"),
        ("bg-left-bottom", "background-position", "left bottom"),
        ("bg-right-bottom", "background-position", "right bottom"),
        ("object-left-top", "object-position", "left top"),
        ("object-right-top", "object-position", "right top"),
        ("object-left-bottom", "object-position", "left bottom"),
        ("object-right-bottom", "object-position", "right bottom"),
    ] {
        u.static_(name, move |_, _| Some(vec![decl(prop, value)]));
    }

    u.functional("max-w-screen", |c, ds| {
        let value = c.value()?;
        if value.is_arbitrary() {
            return None;
        }
        let v = ds.theme.resolve(Some(value.value()), &["--breakpoint"])?;
        Some(vec![decl("max-width", v)])
    });

    u.static_("overflow-ellipsis", |_, _| Some(vec![decl("text-overflow", "ellipsis")]));

    u.static_("decoration-slice", |_, _| {
        Some(vec![decl("-webkit-box-decoration-break", "slice"), decl("box-decoration-break", "slice")])
    });
    u.static_("decoration-clone", |_, _| {
        Some(vec![decl("-webkit-box-decoration-break", "clone"), decl("box-decoration-break", "clone")])
    });

    for (root, prop) in [("flex-shrink", "flex-shrink"), ("flex-grow", "flex-grow")] {
        u.functional(root, move |c, _| {
            if c.modifier.is_some() {
                return None;
            }
            match c.value() {
                None => Some(vec![decl(prop, "1")]),
                Some(UtilityValue::Arbitrary { value, .. }) => Some(vec![decl(prop, value.as_str())]),
                Some(UtilityValue::Named { value, .. }) => {
                    if is_positive_integer(value) {
                        Some(vec![decl(prop, value.as_str())])
                    } else {
                        None
                    }
                }
            }
        });
    }

    u.static_("order-none", |_, _| Some(vec![decl("order", "0")]));
    u.static_("break-words", |_, _| Some(vec![decl("overflow-wrap", "break-word")]));

    for (name, property) in [("start", "inset-inline-start"), ("end", "inset-inline-end")] {
        u.static_(&format!("{}-auto", name), move |_, _| Some(vec![decl(property, "auto")]));
        u.static_(&format!("{}-full", name), move |_, _| Some(vec![decl(property, "100%")]));
        u.static_(&format!("-{}-full", name), move |_, _| Some(vec![decl(property, "-100%")]));
        u.static_(&format!("{}-px", name), move |_, _| Some(vec![decl(property, "1px")]));
        u.static_(&format!("-{}-px", name), move |_, _| Some(vec![decl(property, "-1px")]));

        for negative in [true, false] {
            let root = if negative { format!("-{}", name) } else { name.to_string() };
            u.functional(&root, move |c, ds| {
                let theme = &ds.theme;
                let neg = |v: String| if negative { format!("calc({} * -1)", v) } else { v };
                // `start` / `end` without a value don't generate CSS.
                let cv = c.value()?;
                let (value, fraction) = match cv {
                    UtilityValue::Arbitrary { value, .. } => {
                        if c.modifier.is_some() {
                            return None;
                        }
                        return Some(vec![decl(property, neg(value.clone()))]);
                    }
                    UtilityValue::Named { value, fraction } => (value, fraction),
                };

                let mut v = theme.resolve(Some(fraction.as_deref().unwrap_or(value)), &["--inset", "--spacing"]);

                if v.is_none() {
                    if let Some(f) = fraction {
                        let parts = segment(f, b'/');
                        if !is_positive_integer(parts[0]) || !is_positive_integer(parts.get(1).copied().unwrap_or("")) {
                            return None;
                        }
                        v = Some(format!("calc({} * 100%)", f));
                    }
                }

                if v.is_none() && negative {
                    if let Some(m) = theme.resolve(None, &["--spacing"]) {
                        if is_valid_spacing_multiplier(value) {
                            return Some(vec![decl(property, format!("calc({} * -{})", m, value))]);
                        }
                    }
                }

                if v.is_none() {
                    if let Some(m) = theme.resolve(None, &["--spacing"]) {
                        if is_valid_spacing_multiplier(value) {
                            v = Some(format!("calc({} * {})", m, value));
                        }
                    }
                }

                let v = v?;
                Some(vec![decl(property, neg(v))])
            });
        }
    }
}
