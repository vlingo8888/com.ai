//! Port of the AST parts of `tailwindcss/src/ast.ts` and `walk.ts`.

#[derive(Debug, Clone, PartialEq)]
pub enum AstNode {
    Rule {
        selector: String,
        nodes: Vec<AstNode>,
    },
    AtRule {
        name: String,
        params: String,
        nodes: Vec<AstNode>,
    },
    Decl {
        property: String,
        value: Option<String>,
        important: bool,
    },
    Comment(String),
    Context {
        ctx: Ctx,
        nodes: Vec<AstNode>,
    },
    AtRoot(Vec<AstNode>),
}

/// The subset of `context` keys that the compiler cares about.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Ctx {
    pub theme: bool,
    pub reference: bool,
    pub keyframes: bool,
    pub supports_color_mix: bool,
}

impl Ctx {
    pub fn merge(self, other: Ctx) -> Ctx {
        Ctx {
            theme: self.theme || other.theme,
            reference: self.reference || other.reference,
            keyframes: self.keyframes || other.keyframes,
            supports_color_mix: self.supports_color_mix || other.supports_color_mix,
        }
    }
}

pub fn style_rule(selector: impl Into<String>, nodes: Vec<AstNode>) -> AstNode {
    AstNode::Rule { selector: selector.into(), nodes }
}

pub fn at_rule(name: impl Into<String>, params: impl Into<String>, nodes: Vec<AstNode>) -> AstNode {
    AstNode::AtRule { name: name.into(), params: params.into(), nodes }
}

/// `rule()` — parses selectors starting with `@` as an at-rule.
pub fn rule(selector: impl Into<String>, nodes: Vec<AstNode>) -> AstNode {
    let selector = selector.into();
    if selector.starts_with('@') {
        return crate::tw::css_parser::parse_at_rule(&selector, nodes);
    }
    style_rule(selector, nodes)
}

pub fn decl(property: impl Into<String>, value: impl Into<String>) -> AstNode {
    AstNode::Decl { property: property.into(), value: Some(value.into()), important: false }
}

pub fn decl_opt(property: impl Into<String>, value: Option<String>) -> AstNode {
    AstNode::Decl { property: property.into(), value, important: false }
}

pub fn comment(value: impl Into<String>) -> AstNode {
    AstNode::Comment(value.into())
}

pub fn context(ctx: Ctx, nodes: Vec<AstNode>) -> AstNode {
    AstNode::Context { ctx, nodes }
}

pub fn at_root(nodes: Vec<AstNode>) -> AstNode {
    AstNode::AtRoot(nodes)
}

impl AstNode {
    pub fn nodes(&self) -> Option<&Vec<AstNode>> {
        match self {
            AstNode::Rule { nodes, .. }
            | AstNode::AtRule { nodes, .. }
            | AstNode::Context { nodes, .. }
            | AstNode::AtRoot(nodes) => Some(nodes),
            _ => None,
        }
    }

    pub fn nodes_mut(&mut self) -> Option<&mut Vec<AstNode>> {
        match self {
            AstNode::Rule { nodes, .. }
            | AstNode::AtRule { nodes, .. }
            | AstNode::Context { nodes, .. }
            | AstNode::AtRoot(nodes) => Some(nodes),
            _ => None,
        }
    }

    pub fn is_rule_or_at_rule(&self) -> bool {
        matches!(self, AstNode::Rule { .. } | AstNode::AtRule { .. })
    }

    pub fn at_rule_name(&self) -> Option<&str> {
        match self {
            AstNode::AtRule { name, .. } => Some(name),
            _ => None,
        }
    }
}

/* -------------------------------------------------------------------------- */
/* Walking                                                                     */
/* -------------------------------------------------------------------------- */

pub enum WalkAction {
    Continue,
    Skip,
    Stop,
    Replace(Vec<AstNode>),
    ReplaceSkip(Vec<AstNode>),
    ReplaceStop(Vec<AstNode>),
}

#[derive(Clone, Copy, Debug, Default)]
pub struct WalkCtx {
    /// Raw depth (including context nodes)
    pub depth: usize,
    /// Number of non-`context` ancestors. `0` means "no parent" in the
    /// `cssContext` sense.
    pub parent_depth: usize,
    /// Merged context flags of all ancestors.
    pub context: Ctx,
}

impl WalkCtx {
    pub fn has_parent(&self) -> bool {
        self.parent_depth > 0
    }
}

pub fn walk(nodes: &mut Vec<AstNode>, f: &mut dyn FnMut(&mut AstNode, &WalkCtx) -> WalkAction) {
    walk_inner(nodes, f, WalkCtx::default());
}

fn walk_inner(
    nodes: &mut Vec<AstNode>,
    f: &mut dyn FnMut(&mut AstNode, &WalkCtx) -> WalkAction,
    ctx: WalkCtx,
) -> bool {
    let mut i = 0;
    while i < nodes.len() {
        match f(&mut nodes[i], &ctx) {
            WalkAction::Continue => {
                let node = &mut nodes[i];
                let child_ctx = match node {
                    AstNode::Context { ctx: c, .. } => WalkCtx {
                        depth: ctx.depth + 1,
                        parent_depth: ctx.parent_depth,
                        context: ctx.context.merge(*c),
                    },
                    _ => WalkCtx { depth: ctx.depth + 1, parent_depth: ctx.parent_depth + 1, context: ctx.context },
                };
                if let Some(children) = node.nodes_mut() {
                    if !children.is_empty() && !walk_inner(children, f, child_ctx) {
                        return false;
                    }
                }
                i += 1;
            }
            WalkAction::Skip => i += 1,
            WalkAction::Stop => return false,
            WalkAction::Replace(replacement) => {
                nodes.splice(i..i + 1, replacement);
            }
            WalkAction::ReplaceSkip(replacement) => {
                let n = replacement.len();
                nodes.splice(i..i + 1, replacement);
                i += n;
            }
            WalkAction::ReplaceStop(replacement) => {
                nodes.splice(i..i + 1, replacement);
                return false;
            }
        }
    }
    true
}

/// Read-only walk.
pub fn walk_ref(nodes: &[AstNode], f: &mut dyn FnMut(&AstNode) -> WalkAction) -> bool {
    for node in nodes {
        match f(node) {
            WalkAction::Continue => {
                if let Some(children) = node.nodes() {
                    if !walk_ref(children, f) {
                        return false;
                    }
                }
            }
            WalkAction::Skip => {}
            WalkAction::Stop => return false,
            _ => {}
        }
    }
    true
}

/* -------------------------------------------------------------------------- */
/* Printing                                                                    */
/* -------------------------------------------------------------------------- */

pub fn to_css(ast: &[AstNode]) -> String {
    let mut css = String::with_capacity(4096);
    for node in ast {
        stringify(node, 0, &mut css);
    }
    css
}

fn indent(css: &mut String, depth: usize) {
    for _ in 0..depth {
        css.push_str("  ");
    }
}

fn stringify(node: &AstNode, depth: usize, css: &mut String) {
    match node {
        AstNode::Decl { property, value, important } => {
            indent(css, depth);
            css.push_str(property);
            css.push_str(": ");
            match value {
                Some(v) => css.push_str(v),
                None => css.push_str("undefined"),
            }
            if *important {
                css.push_str(" !important");
            }
            css.push_str(";\n");
        }
        AstNode::Rule { selector, nodes } => {
            indent(css, depth);
            css.push_str(selector);
            css.push_str(" {\n");
            for child in nodes {
                stringify(child, depth + 1, css);
            }
            indent(css, depth);
            css.push_str("}\n");
        }
        AstNode::AtRule { name, params, nodes } => {
            if nodes.is_empty() {
                indent(css, depth);
                css.push_str(name);
                css.push(' ');
                css.push_str(params);
                css.push_str(";\n");
                return;
            }
            indent(css, depth);
            css.push_str(name);
            if !params.is_empty() {
                css.push(' ');
                css.push_str(params);
                css.push(' ');
            } else {
                css.push(' ');
            }
            css.push_str("{\n");
            for child in nodes {
                stringify(child, depth + 1, css);
            }
            indent(css, depth);
            css.push_str("}\n");
        }
        AstNode::Comment(value) => {
            indent(css, depth);
            css.push_str("/*");
            css.push_str(value);
            css.push_str("*/\n");
        }
        AstNode::Context { .. } | AstNode::AtRoot(_) => {}
    }
}
