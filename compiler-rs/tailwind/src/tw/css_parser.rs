//! Port of `tailwindcss/src/css-parser.ts`.

use crate::tw::ast::{at_rule, comment, rule, AstNode};

pub fn parse(input: &str) -> Result<Vec<AstNode>, String> {
    let owned;
    let input = if input.starts_with('\u{FEFF}') {
        owned = format!(" {}", &input['\u{FEFF}'.len_utf8()..]);
        owned.as_str()
    } else {
        input
    };

    let bytes = input.as_bytes();
    let len = bytes.len();

    let mut ast: Vec<AstNode> = Vec::new();
    let mut license_comments: Vec<AstNode> = Vec::new();

    // Stack of open blocks. The top is the current parent.
    let mut stack: Vec<AstNode> = Vec::new();

    let mut buffer = String::new();
    let mut closing: Vec<u8> = Vec::new();

    let at = |i: usize| -> u8 { if i < len { bytes[i] } else { 0 } };

    fn attach(stack: &mut [AstNode], ast: &mut Vec<AstNode>, node: AstNode) {
        if let Some(parent) = stack.last_mut() {
            parent.nodes_mut().unwrap().push(node);
        } else {
            ast.push(node);
        }
    }

    let mut i = 0;
    while i < len {
        let c = bytes[i];

        if c == b'\r' && at(i + 1) == b'\n' {
            i += 1;
            continue;
        }

        if c == b'\\' {
            let end = char_end(input, i + 1);
            buffer.push_str(&input[i..end]);
            i = end;
            continue;
        } else if c == b'/' && at(i + 1) == b'*' {
            let start = i;
            let mut j = i + 2;
            let mut end_idx = len.saturating_sub(1);
            while j < len {
                let p = bytes[j];
                if p == b'\\' {
                    j += 2;
                    continue;
                }
                if p == b'*' && at(j + 1) == b'/' {
                    end_idx = j + 1;
                    break;
                }
                j += 1;
            }
            i = end_idx;
            let comment_str = &input[start..(i + 1).min(len)];
            if comment_str.as_bytes().get(2) == Some(&b'!') && comment_str.len() >= 4 {
                license_comments.push(comment(&comment_str[2..comment_str.len() - 2]));
            }
        } else if c == b'\'' || c == b'"' {
            let end = parse_string(input, i, c)?;
            buffer.push_str(&input[i..char_end(input, end)]);
            i = end;
        } else if (c == b' ' || c == b'\n' || c == b'\t')
            && matches!(at(i + 1), b' ' | b'\n' | b'\t')
        {
            i += 1;
            continue;
        } else if (c == b' ' || c == b'\n' || c == b'\t') && at(i + 1) == b'\r' && at(i + 2) == b'\n' {
            i += 1;
            continue;
        } else if c == b'\n' {
            if buffer.is_empty() {
                i += 1;
                continue;
            }
            let last = *buffer.as_bytes().last().unwrap();
            if last != b' ' && last != b'\n' && last != b'\t' {
                buffer.push(' ');
            }
        } else if c == b'-' && at(i + 1) == b'-' && buffer.is_empty() {
            let mut local_closing: Vec<u8> = Vec::new();
            let start = i;
            let mut colon_idx: isize = -1;
            let mut j = i + 2;
            let mut consumed = false;
            while j < len {
                let p = bytes[j];
                if p == b'\\' {
                    j += 2;
                    continue;
                } else if p == b'\'' || p == b'"' {
                    j = parse_string(input, j, p)?;
                } else if p == b'/' && at(j + 1) == b'*' {
                    let mut k = j + 2;
                    while k < len {
                        let q = bytes[k];
                        if q == b'\\' {
                            k += 2;
                            continue;
                        }
                        if q == b'*' && at(k + 1) == b'/' {
                            j = k + 1;
                            break;
                        }
                        k += 1;
                    }
                } else if colon_idx == -1 && p == b':' {
                    colon_idx = (buffer.len() + j - start) as isize;
                } else if p == b';' && local_closing.is_empty() {
                    buffer.push_str(&input[start..j]);
                    i = j;
                    consumed = true;
                    break;
                } else if p == b'(' {
                    local_closing.push(b')');
                } else if p == b'[' {
                    local_closing.push(b']');
                } else if p == b'{' {
                    local_closing.push(b'}');
                } else if (p == b'}' || len - 1 == j) && local_closing.is_empty() {
                    i = j - 1;
                    buffer.push_str(&input[start..j]);
                    consumed = true;
                    break;
                } else if p == b')' || p == b']' || p == b'}' {
                    if local_closing.last() == Some(&p) {
                        local_closing.pop();
                    }
                }
                j += 1;
            }
            if !consumed {
                return Err("Invalid custom property, expected a value".into());
            }

            let declaration = parse_declaration(&buffer, if colon_idx >= 0 { Some(colon_idx as usize) } else { None })
                .ok_or_else(|| "Invalid custom property, expected a value".to_string())?;
            attach(&mut stack, &mut ast, declaration);
            buffer.clear();
        } else if c == b';' && buffer.as_bytes().first() == Some(&b'@') {
            let node = parse_at_rule(&buffer, vec![]);
            attach(&mut stack, &mut ast, node);
            buffer.clear();
        } else if c == b';' && closing.last() != Some(&b')') {
            match parse_declaration(&buffer, None) {
                Some(d) => attach(&mut stack, &mut ast, d),
                None => {
                    if !buffer.is_empty() {
                        return Err(format!("Invalid declaration: `{}`", buffer.trim()));
                    }
                }
            }
            buffer.clear();
        } else if c == b'{' && closing.last() != Some(&b')') {
            closing.push(b'}');
            let node = rule(buffer.trim(), vec![]);
            stack.push(node);
            buffer.clear();
        } else if c == b'}' && closing.last() != Some(&b')') {
            if closing.is_empty() {
                return Err("Missing opening {".into());
            }
            closing.pop();

            if !buffer.is_empty() {
                if buffer.as_bytes()[0] == b'@' {
                    let node = parse_at_rule(&buffer, vec![]);
                    attach(&mut stack, &mut ast, node);
                } else if !stack.is_empty() {
                    let colon = buffer.find(':');
                    let node = parse_declaration(&buffer, colon)
                        .ok_or_else(|| format!("Invalid declaration: `{}`", buffer.trim()))?;
                    attach(&mut stack, &mut ast, node);
                }
            }

            if let Some(node) = stack.pop() {
                attach(&mut stack, &mut ast, node);
            }
            buffer.clear();
        } else if c == b'(' {
            closing.push(b')');
            buffer.push('(');
        } else if c == b')' {
            if closing.last() != Some(&b')') {
                return Err("Missing opening (".into());
            }
            closing.pop();
            buffer.push(')');
        } else {
            if buffer.is_empty() && (c == b' ' || c == b'\n' || c == b'\t') {
                i += 1;
                continue;
            }
            let end = char_end(input, i);
            buffer.push_str(&input[i..end]);
            i = end;
            continue;
        }

        i += 1;
    }

    if buffer.as_bytes().first() == Some(&b'@') {
        ast.push(parse_at_rule(&buffer, vec![]));
    }

    if !closing.is_empty() {
        if let Some(parent) = stack.last() {
            return Err(match parent {
                AstNode::Rule { selector, .. } => format!("Missing closing }} at {}", selector),
                AstNode::AtRule { name, params, .. } => format!("Missing closing }} at {} {}", name, params),
                _ => "Missing closing }".into(),
            });
        }
    }

    if !license_comments.is_empty() {
        license_comments.extend(ast);
        return Ok(license_comments);
    }

    Ok(ast)
}

/// Index just past the UTF-8 character starting at `i`.
fn char_end(s: &str, i: usize) -> usize {
    if i >= s.len() {
        return s.len();
    }
    let mut end = i + 1;
    while end < s.len() && !s.is_char_boundary(end) {
        end += 1;
    }
    end
}

pub fn parse_at_rule(buffer: &str, nodes: Vec<AstNode>) -> AstNode {
    let bytes = buffer.as_bytes();
    let mut name = buffer;
    let mut params = "";
    let mut i = 5;
    while i < bytes.len() {
        let c = bytes[i];
        if c == b' ' || c == b'\t' || c == b'(' {
            name = &buffer[..i];
            params = &buffer[i..];
            break;
        }
        i += 1;
    }
    at_rule(name.trim(), params.trim(), nodes)
}

fn parse_declaration(buffer: &str, colon_idx: Option<usize>) -> Option<AstNode> {
    let colon_idx = colon_idx.or_else(|| buffer.find(':'))?;
    let after = colon_idx + 1;
    let important_idx = buffer.get(after..).and_then(|s| s.find("!important")).map(|i| i + after);
    let property = buffer[..colon_idx].trim();
    let value_end = important_idx.unwrap_or(buffer.len());
    let value = if after <= value_end { buffer[after..value_end].trim() } else { "" };
    Some(AstNode::Decl {
        property: property.to_string(),
        value: Some(value.to_string()),
        important: important_idx.is_some(),
    })
}

fn parse_string(input: &str, start: usize, quote: u8) -> Result<usize, String> {
    let bytes = input.as_bytes();
    let len = bytes.len();
    let mut i = start + 1;
    while i < len {
        let p = bytes[i];
        if p == b'\\' {
            i += 2;
            continue;
        } else if p == quote {
            return Ok(i);
        } else if p == b';' && (bytes.get(i + 1) == Some(&b'\n') || (bytes.get(i + 1) == Some(&b'\r') && bytes.get(i + 2) == Some(&b'\n'))) {
            return Err(format!("Unterminated string: {}{}", &input[start..=i], quote as char));
        } else if p == b'\n' || (p == b'\r' && bytes.get(i + 1) == Some(&b'\n')) {
            return Err(format!("Unterminated string: {}{}", &input[start..i], quote as char));
        }
        i += 1;
    }
    Ok(start)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tw::ast::to_css;

    #[test]
    fn parses_basic() {
        let ast = parse(".a { color: red; &:hover { color: blue } }\n@media (x) { .b { --x: {a:b}; } }").unwrap();
        assert_eq!(
            to_css(&ast),
            ".a {\n  color: red;\n  &:hover {\n    color: blue;\n  }\n}\n@media (x) {\n  .b {\n    --x: {a:b};\n  }\n}\n"
        );
    }
}
