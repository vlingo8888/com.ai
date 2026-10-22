//! Port of `tailwindcss/src/attribute-selector-parser.ts` (4.3.3). Only the
//! validity check is needed (`aria-*` / `data-*` variants).

fn is_ascii_whitespace(b: Option<&u8>) -> bool {
    matches!(b, Some(b' ' | b'\t' | b'\n' | b'\r'))
}

/// Whether `input` (e.g. `[data-foo="bar" i]`) is a valid attribute selector.
pub fn is_valid(input: &str) -> bool {
    let b = input.as_bytes();
    if b.first() != Some(&b'[') || b.last() != Some(&b']') || b.len() < 2 {
        return false;
    }
    let end = b.len() - 1;
    let mut i = 1;

    while is_ascii_whitespace(b.get(i)) {
        i += 1;
    }

    // Attribute name
    let start = i;
    while i < end {
        let c = b[i];
        if c == b'\\' {
            i += 2;
            continue;
        }
        if c.is_ascii_alphanumeric() || c == b'-' || c == b'_' || c >= 128 {
            i += 1;
            continue;
        }
        break;
    }
    if start == i {
        return false;
    }

    while is_ascii_whitespace(b.get(i)) {
        i += 1;
    }
    if i == end {
        return true;
    }

    // Operator
    match b.get(i) {
        Some(b'=') => i += 1,
        Some(b'~' | b'|' | b'^' | b'$' | b'*') if b.get(i + 1) == Some(&b'=') => i += 2,
        _ => return false,
    }

    while is_ascii_whitespace(b.get(i)) {
        i += 1;
    }
    if i == end {
        return false;
    }

    // Value
    let c = b[i];
    if c == b'\'' || c == b'"' {
        i += 1;
        let mut j = i;
        while j < end {
            if b[j] == c {
                i = j + 1;
            } else if b[j] == b'\\' {
                j += 1;
            }
            j += 1;
        }
    } else {
        while i < end && !is_ascii_whitespace(b.get(i)) {
            i += 1;
        }
    }

    while is_ascii_whitespace(b.get(i)) {
        i += 1;
    }
    if i == end {
        return true;
    }

    // Sensitivity
    match b.get(i) {
        Some(b'i' | b'I' | b's' | b'S') => i += 1,
        _ => return false,
    }

    while is_ascii_whitespace(b.get(i)) {
        i += 1;
    }
    i == end
}

#[cfg(test)]
mod tests {
    use super::is_valid;

    #[test]
    fn validates() {
        assert!(is_valid("[data-foo]"));
        assert!(is_valid("[data-état]"));
        assert!(is_valid("[data-foo=\"bar\" i]"));
        assert!(is_valid("[aria-sort=ascending]"));
        assert!(!is_valid("[data-foo*=]"));
        assert!(!is_valid("[data-foo=bar x]"));
        assert!(!is_valid("[=x]"));
    }
}
