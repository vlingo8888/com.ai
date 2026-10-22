//! Port of `tailwindcss/src/selector-parser.ts` (4.3.3).

#[derive(Debug, Clone, PartialEq)]
pub enum SNode {
    List(Vec<SNode>),
    Combinator(String),
    Complex(Vec<SNode>),
    Compound(Vec<SNode>),
    Selector(String),
    Function(String, Vec<SNode>),
    Value(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SKind {
    List,
    Combinator,
    Complex,
    Compound,
    Selector,
    Function,
    Value,
}

impl SNode {
    pub fn kind(&self) -> SKind {
        match self {
            SNode::List(_) => SKind::List,
            SNode::Combinator(_) => SKind::Combinator,
            SNode::Complex(_) => SKind::Complex,
            SNode::Compound(_) => SKind::Compound,
            SNode::Selector(_) => SKind::Selector,
            SNode::Function(..) => SKind::Function,
            SNode::Value(_) => SKind::Value,
        }
    }

    pub fn nodes_mut(&mut self) -> Option<&mut Vec<SNode>> {
        match self {
            SNode::List(n) | SNode::Complex(n) | SNode::Compound(n) | SNode::Function(_, n) => Some(n),
            _ => None,
        }
    }
}

fn first_byte(node: &SNode) -> Option<u8> {
    match node {
        SNode::Selector(v) => Some(v.as_bytes().first().copied().unwrap_or(0)),
        _ => None,
    }
}

pub fn is_universal_selector(node: &SNode) -> bool {
    first_byte(node) == Some(b'*')
}

pub fn is_type_selector(node: &SNode) -> bool {
    match first_byte(node) {
        None => false,
        Some(b'*' | b'&' | b'.' | b'#' | b':' | b'[') => false,
        Some(_) => true,
    }
}

pub fn to_css(ast: &[SNode]) -> String {
    let mut css = String::new();
    write_css(ast, &mut css);
    css
}

fn write_css(ast: &[SNode], css: &mut String) {
    for node in ast {
        match node {
            SNode::Selector(v) | SNode::Value(v) => css.push_str(v),
            SNode::Combinator(v) => {
                if v == " " {
                    css.push(' ');
                } else {
                    css.push(' ');
                    css.push_str(v);
                    css.push(' ');
                }
            }
            SNode::Function(name, nodes) => {
                css.push_str(name);
                css.push('(');
                write_css(nodes, css);
                css.push(')');
            }
            SNode::Complex(nodes) | SNode::Compound(nodes) => write_css(nodes, css),
            SNode::List(nodes) => {
                for (i, n) in nodes.iter().enumerate() {
                    if i > 0 {
                        css.push_str(", ");
                    }
                    write_css(std::slice::from_ref(n), css);
                }
            }
        }
    }
}

/// One nesting level of the parser (the top level or a `:is(…)`-like
/// function body). `list` mirrors `currentList`: once a `,` is seen, the
/// level's own target becomes `[list]` and `target` collects the next item.
struct Frame {
    target: Vec<SNode>,
    list: Option<Vec<SNode>>,
    contains_combinator: bool,
}

impl Frame {
    fn new() -> Self {
        Frame { target: vec![], list: None, contains_combinator: false }
    }

    fn current(nodes: Vec<SNode>, contains_combinator: bool) -> SNode {
        if nodes.len() == 1 {
            nodes.into_iter().next().unwrap()
        } else if contains_combinator {
            SNode::Complex(nodes)
        } else {
            SNode::Compound(nodes)
        }
    }

    fn append(&mut self, node: SNode) {
        match self.target.last_mut() {
            Some(SNode::Compound(nodes)) => nodes.push(node),
            Some(existing) if !matches!(existing, SNode::List(_) | SNode::Combinator(_)) => {
                let prev = std::mem::replace(existing, SNode::Compound(vec![]));
                *existing = SNode::Compound(vec![prev, node]);
            }
            _ => self.target.push(node),
        }
    }

    /// The final nodes of this level (what `ast` / `fn.nodes` hold at the end).
    fn finish(mut self) -> Vec<SNode> {
        if let Some(mut list) = self.list.take() {
            list.push(Frame::current(std::mem::take(&mut self.target), self.contains_combinator));
            vec![SNode::List(list)]
        } else if self.contains_combinator {
            vec![SNode::Complex(self.target)]
        } else {
            self.target
        }
    }
}

fn is_ws(b: u8) -> bool {
    matches!(b, b'\n' | b' ' | b'\t')
}

fn is_combinator_byte(b: u8) -> bool {
    matches!(b, b'>' | b'\n' | b' ' | b'+' | b'\t' | b'~')
}

pub fn parse(input: &str) -> Vec<SNode> {
    let owned;
    let input = if input.contains("\r\n") {
        owned = input.replace("\r\n", "\n");
        owned.as_str()
    } else {
        input
    };
    let bytes = input.as_bytes();
    let len = bytes.len();

    // `stack[0]` is the top level; deeper frames are open selector functions
    // (`:not(`, `:where(`, `:has(`, `:is(`).
    let mut stack: Vec<Frame> = vec![Frame::new()];
    let mut buffer: Vec<u8> = Vec::new();

    macro_rules! flush {
        () => {
            if !buffer.is_empty() {
                let s = String::from_utf8_lossy(&buffer).into_owned();
                stack.last_mut().unwrap().append(SNode::Selector(s));
                buffer.clear();
            }
        };
    }

    let mut i = 0;
    while i < len {
        let c = bytes[i];
        match c {
            b',' => {
                flush!();
                while i + 1 < len && is_ws(bytes[i + 1]) {
                    i += 1;
                }
                let frame = stack.last_mut().unwrap();
                let nodes = std::mem::take(&mut frame.target);
                let item = Frame::current(nodes, frame.contains_combinator);
                frame.list.get_or_insert_with(Vec::new).push(item);
                frame.contains_combinator = false;
            }

            b'>' | b'\n' | b' ' | b'+' | b'\t' | b'~' => {
                flush!();
                let start = i;
                let mut end = i + 1;
                while end < len && is_combinator_byte(bytes[end]) {
                    end += 1;
                }
                i = end - 1;
                let value = input[start..end].trim();
                let frame = stack.last_mut().unwrap();
                if value.is_empty() && (frame.target.is_empty() || end >= len || bytes[end] == b',') {
                    i += 1;
                    continue;
                }
                frame.target.push(SNode::Combinator(if value.is_empty() { " ".into() } else { value.into() }));
                frame.contains_combinator = true;
            }

            b'(' => {
                let name = String::from_utf8_lossy(&buffer).into_owned();
                buffer.clear();

                if name != ":not" && name != ":where" && name != ":has" && name != ":is" {
                    let start = i + 1;
                    let mut nesting = 0;
                    let mut j = i + 1;
                    while j < len {
                        let p = bytes[j];
                        if p == b'(' {
                            nesting += 1;
                        } else if p == b')' {
                            if nesting == 0 {
                                i = j;
                                break;
                            }
                            nesting -= 1;
                        }
                        j += 1;
                    }
                    let end = i;
                    let contents = if start <= end { &input[start..end] } else { "" };

                    let mut nodes = vec![];
                    if name == ":nth-child" || name == ":nth-last-child" {
                        if let Some(idx) = contents.find("of ") {
                            nodes.push(SNode::Value(contents[..idx + 3].to_string()));
                            nodes.extend(parse(&contents[idx + 3..]));
                            stack.last_mut().unwrap().append(SNode::Function(name, nodes));
                            i = end + 1;
                            continue;
                        }
                    }
                    nodes.push(SNode::Value(contents.to_string()));
                    stack.last_mut().unwrap().append(SNode::Function(name, nodes));
                    i = end + 1;
                    continue;
                }

                stack.last_mut().unwrap().append(SNode::Function(name, vec![]));
                stack.push(Frame::new());
            }

            b')' => {
                flush!();
                if stack.len() > 1 {
                    let nodes = stack.pop().unwrap().finish();
                    // The function node is the last leaf appended to the parent.
                    let parent = stack.last_mut().unwrap();
                    let target = match parent.target.last_mut() {
                        Some(SNode::Compound(n)) => n.last_mut(),
                        other => other,
                    };
                    if let Some(SNode::Function(_, fn_nodes)) = target {
                        *fn_nodes = nodes;
                    }
                } else {
                    // Unbalanced `)`: finish the top level in place.
                    let frame = stack.pop().unwrap();
                    let nodes = frame.finish();
                    stack.push(Frame { target: nodes, list: None, contains_combinator: false });
                }
            }

            b'.' | b':' | b'#' => {
                if c == b':' && buffer.as_slice() == b":" {
                    buffer.push(c);
                } else {
                    flush!();
                    buffer.push(c);
                }
            }

            b'[' => {
                flush!();
                let start = i;
                let mut nesting = 0;
                let mut j = i + 1;
                while j < len {
                    let p = bytes[j];
                    if p == b'[' {
                        nesting += 1;
                    } else if p == b']' {
                        if nesting == 0 {
                            i = j;
                            break;
                        }
                        nesting -= 1;
                    }
                    j += 1;
                }
                stack.last_mut().unwrap().append(SNode::Selector(input[start..i + 1].to_string()));
            }

            b'\'' | b'"' => {
                let start = i;
                let mut j = i + 1;
                while j < len {
                    let p = bytes[j];
                    if p == b'\\' {
                        j += 1;
                    } else if p == c {
                        i = j;
                        break;
                    }
                    j += 1;
                }
                buffer.extend_from_slice(&bytes[start..i + 1]);
            }

            b'&' | b'*' => {
                flush!();
                stack.last_mut().unwrap().append(SNode::Selector((c as char).to_string()));
            }

            b'\\' => {
                buffer.push(c);
                if i + 1 < len {
                    buffer.push(bytes[i + 1]);
                }
                i += 1;
            }

            _ => buffer.push(c),
        }
        i += 1;
    }

    flush!();

    // Unclosed functions: JS leaves them attached with their partial nodes.
    while stack.len() > 1 {
        let nodes = stack.pop().unwrap().finish();
        let parent = stack.last_mut().unwrap();
        let target = match parent.target.last_mut() {
            Some(SNode::Compound(n)) => n.last_mut(),
            other => other,
        };
        if let Some(SNode::Function(_, fn_nodes)) = target {
            *fn_nodes = nodes;
        }
    }

    stack.pop().unwrap().finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rt(s: &str) -> String {
        to_css(&parse(s))
    }

    #[test]
    fn roundtrip() {
        assert_eq!(rt(".a .b"), ".a .b");
        assert_eq!(rt(".a>.b"), ".a > .b");
        assert_eq!(rt(".a, .b"), ".a, .b");
        assert_eq!(rt(":where(.a > :not(:last-child))"), ":where(.a > :not(:last-child))");
        assert_eq!(rt(":nth-child(2n + 1 of .a, .b)"), ":nth-child(2n + 1 of .a, .b)");
        assert_eq!(rt("&::placeholder"), "&::placeholder");
        assert_eq!(rt("[data-foo]div"), "[data-foo]div");
    }

    #[test]
    fn structure() {
        assert_eq!(
            parse(".a .b"),
            vec![SNode::Complex(vec![SNode::Selector(".a".into()), SNode::Combinator(" ".into()), SNode::Selector(".b".into())])]
        );
        assert_eq!(parse("&:hover"), vec![SNode::Compound(vec![SNode::Selector("&".into()), SNode::Selector(":hover".into())])]);
        assert!(matches!(&parse(".a, .b")[0], SNode::List(n) if n.len() == 2));
    }
}
