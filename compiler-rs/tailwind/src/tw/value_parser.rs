//! Port of `tailwindcss/src/value-parser.ts`.

#[derive(Debug, Clone, PartialEq)]
pub enum ValueNode {
    Word(String),
    Function { value: String, nodes: Vec<ValueNode> },
    Separator(String),
}

impl ValueNode {
    pub fn value(&self) -> &str {
        match self {
            ValueNode::Word(v) | ValueNode::Separator(v) => v,
            ValueNode::Function { value, .. } => value,
        }
    }
}

pub fn to_css(ast: &[ValueNode]) -> String {
    let mut css = String::new();
    write_css(ast, &mut css);
    css
}

fn write_css(ast: &[ValueNode], css: &mut String) {
    for node in ast {
        match node {
            ValueNode::Word(v) | ValueNode::Separator(v) => css.push_str(v),
            ValueNode::Function { value, nodes } => {
                css.push_str(value);
                css.push('(');
                write_css(nodes, css);
                css.push(')');
            }
        }
    }
}

#[inline]
fn is_sep(c: u8) -> bool {
    matches!(c, b':' | b',' | b'=' | b'>' | b'<' | b'\n' | b' ' | b'\t')
}

pub fn parse(input: &str) -> Vec<ValueNode> {
    let owned;
    let input = if input.contains("\r\n") {
        owned = input.replace("\r\n", "\n");
        owned.as_str()
    } else {
        input
    };

    let bytes = input.as_bytes();
    let len = bytes.len();

    // Root list + stack of open functions. Each stack entry collects nodes.
    let mut root: Vec<ValueNode> = Vec::new();
    let mut stack: Vec<(String, Vec<ValueNode>)> = Vec::new();
    let mut buffer = String::new();

    fn push(root: &mut Vec<ValueNode>, stack: &mut [(String, Vec<ValueNode>)], node: ValueNode) {
        if let Some(top) = stack.last_mut() {
            top.1.push(node);
        } else {
            root.push(node);
        }
    }

    let mut i = 0;
    while i < len {
        let c = bytes[i];
        match c {
            b'\\' => {
                // Consume escaped char together with the backslash.
                let end = next_char_boundary(input, i + 1);
                buffer.push_str(&input[i..end]);
                i = end;
                continue;
            }
            b'/' => {
                if !buffer.is_empty() {
                    push(&mut root, &mut stack, ValueNode::Word(std::mem::take(&mut buffer)));
                }
                push(&mut root, &mut stack, ValueNode::Word("/".into()));
            }
            c if is_sep(c) => {
                if !buffer.is_empty() {
                    push(&mut root, &mut stack, ValueNode::Word(std::mem::take(&mut buffer)));
                }
                let start = i;
                let mut end = i + 1;
                while end < len && is_sep(bytes[end]) {
                    end += 1;
                }
                push(&mut root, &mut stack, ValueNode::Separator(input[start..end].to_string()));
                i = end;
                continue;
            }
            b'\'' | b'"' => {
                let start = i;
                let mut j = i + 1;
                let mut end_at = i;
                while j < len {
                    let p = bytes[j];
                    if p == b'\\' {
                        j += 2;
                        continue;
                    }
                    if p == c {
                        end_at = j;
                        break;
                    }
                    j += 1;
                }
                // Unterminated string: upstream keeps only the opening quote.
                let end = next_char_boundary(input, end_at);
                buffer.push_str(&input[start..end]);
                i = end;
                continue;
            }
            b'(' => {
                let name = std::mem::take(&mut buffer);
                stack.push((name, Vec::new()));
            }
            b')' => {
                if let Some((name, mut nodes)) = stack.pop() {
                    if !buffer.is_empty() {
                        nodes.push(ValueNode::Word(std::mem::take(&mut buffer)));
                    }
                    push(&mut root, &mut stack, ValueNode::Function { value: name, nodes });
                } else {
                    // Unbalanced `)`: upstream drops the buffer.
                    buffer.clear();
                }
            }
            _ => {
                let end = next_char_boundary(input, i);
                buffer.push_str(&input[i..end]);
                i = end;
                continue;
            }
        }
        i += 1;
    }

    // Unclosed functions: upstream already attached them to their parent at
    // open time, so emit them as-is (with whatever children they collected).
    while let Some((name, nodes)) = stack.pop() {
        push(&mut root, &mut stack, ValueNode::Function { value: name, nodes });
    }

    if !buffer.is_empty() {
        root.push(ValueNode::Word(buffer));
    }

    root
}

fn next_char_boundary(s: &str, i: usize) -> usize {
    if i >= s.len() {
        return s.len();
    }
    let mut end = i + 1;
    while end < s.len() && !s.is_char_boundary(end) {
        end += 1;
    }
    end
}

/// Walk value nodes depth first (enter only). The callback can return
/// `Some(replacement)` to replace the node (replacement is not walked).
pub fn walk_mut(ast: &mut Vec<ValueNode>, f: &mut dyn FnMut(&mut ValueNode) -> WalkResult) -> bool {
    let mut i = 0;
    while i < ast.len() {
        match f(&mut ast[i]) {
            WalkResult::Continue => {
                if let ValueNode::Function { nodes, .. } = &mut ast[i] {
                    if !walk_mut(nodes, f) {
                        return false;
                    }
                }
                i += 1;
            }
            WalkResult::Skip => i += 1,
            WalkResult::Stop => return false,
            WalkResult::Replace(nodes) => {
                let n = nodes.len();
                ast.splice(i..i + 1, nodes);
                i += n;
            }
        }
    }
    true
}

pub enum WalkResult {
    Continue,
    Skip,
    Stop,
    Replace(Vec<ValueNode>),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        for s in ["calc(1px + var(--a, 2px))", "a/b", "rgb(0 0 0 / 0.5)", "'a b' c"] {
            assert_eq!(to_css(&parse(s)), s);
        }
    }
}
