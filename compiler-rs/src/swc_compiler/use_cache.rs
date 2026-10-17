//! `"use cache"` (Next.js 15+), react-server layer: an async function whose body starts with
//! `"use cache"` (or every exported function of a module starting with it) returns a cached result
//! keyed by the function id, its arguments and the variables it closes over. The function keeps its
//! name and hoisting; its body runs inside `cacheCall` (see `next/cache-runtime.ts`), where
//! `cacheLife()` and `cacheTag()` apply.
//!
//! `async function f(a, b) { "use cache"; BODY }` becomes
//! `async function f(...$$args) { return __nata_cache_call("file#f", [$$args, [captured]], () => (async function (a, b) { BODY }).apply(this, $$args), {}) }`

use swc_core::common::{sync::Lrc, SourceMap, SyntaxContext, DUMMY_SP};
use swc_core::ecma::{
    ast::*,
    visit::{Visit, VisitMut, VisitMutWith, VisitWith},
};

use super::module::parse_items;

const RUNTIME_IMPORT: &str = "__nata_cache_call";

fn is_use_cache(stmt: Option<&Stmt>) -> bool {
    matches!(stmt, Some(Stmt::Expr(ExprStmt { expr, .. })) if matches!(&**expr, Expr::Lit(Lit::Str(s)) if s.value.to_string_lossy().starts_with("use cache")))
}

/// Whether a module starts with `"use cache"` (also `"use cache: private"` / `"use cache: remote"`)
pub fn is_file_level(program: &Program) -> bool {
    match program {
        Program::Module(m) => m
            .body
            .iter()
            .map_while(|i| i.as_stmt())
            .take_while(|s| matches!(s, Stmt::Expr(ExprStmt { expr, .. }) if matches!(&**expr, Expr::Lit(Lit::Str(_)))))
            .any(|s| is_use_cache(Some(s))),
        _ => false,
    }
}

#[derive(Default)]
struct Declared(std::collections::HashSet<(swc_core::atoms::Atom, SyntaxContext)>);
impl Visit for Declared {
    fn visit_binding_ident(&mut self, n: &BindingIdent) {
        self.0.insert((n.id.sym.clone(), n.id.ctxt));
    }
    fn visit_fn_decl(&mut self, n: &FnDecl) {
        self.0.insert((n.ident.sym.clone(), n.ident.ctxt));
        n.visit_children_with(self);
    }
    fn visit_class_decl(&mut self, n: &ClassDecl) {
        self.0.insert((n.ident.sym.clone(), n.ident.ctxt));
        n.visit_children_with(self);
    }
}

#[derive(Default)]
struct Used(Vec<Ident>);
impl Visit for Used {
    fn visit_ident(&mut self, n: &Ident) {
        if !self.0.iter().any(|i| i.sym == n.sym && i.ctxt == n.ctxt) {
            self.0.push(n.clone());
        }
    }
    fn visit_member_prop(&mut self, n: &MemberProp) {
        if let MemberProp::Computed(c) = n {
            c.visit_with(self);
        }
    }
    fn visit_prop_name(&mut self, n: &PropName) {
        if let PropName::Computed(c) = n {
            c.visit_with(self);
        }
    }
}

struct CacheWrapper<'a> {
    cm: &'a Lrc<SourceMap>,
    rel: &'a str,
    file_level: bool,
    top_level: SyntaxContext,
    unresolved: SyntaxContext,
    counter: usize,
    wrapped: usize,
    /// Depth of function nesting: file-level "use cache" applies to top-level functions only
    depth: usize,
}

impl CacheWrapper<'_> {
    fn captured(&self, function: &Function) -> Vec<Ident> {
        let mut declared = Declared::default();
        function.visit_with(&mut declared);
        let mut used = Used::default();
        function.visit_with(&mut used);
        used.0
            .into_iter()
            .filter(|i| i.ctxt != self.top_level && i.ctxt != self.unresolved && i.ctxt != SyntaxContext::empty())
            .filter(|i| !declared.0.contains(&(i.sym.clone(), i.ctxt)))
            .collect()
    }

    /// Replaces the body of `function` with a cached call to the original body
    fn wrap(&mut self, function: &mut Function, name: Option<&str>) {
        if !function.is_async {
            return;
        }
        let Some(body) = &mut function.body else { return };
        if is_use_cache(body.stmts.first()) {
            body.stmts.remove(0);
        }
        let id = format!("{}#{}", self.rel, name.map(String::from).unwrap_or_else(|| {
            self.counter += 1;
            format!("$cache{}", self.counter)
        }));
        let captured: Vec<String> = self.captured(function).iter().map(|i| i.sym.to_string()).collect();
        let original = function.clone();
        // Generated text is parsed, then the original function is spliced in (keeps its AST intact)
        let code = format!(
            "async function __w(...$$nata_args) {{ return {RUNTIME_IMPORT}({}, [$$nata_args, [{}]], () => __ORIGINAL__.apply(this, $$nata_args), {{ tags: [] }}); }}",
            serde_json::to_string(&id).unwrap(),
            captured.join(", ")
        );
        let ModuleItem::Stmt(Stmt::Decl(Decl::Fn(mut wrapper))) = parse_items(self.cm, &code).remove(0) else { return };
        struct Splice(Option<Function>);
        impl VisitMut for Splice {
            fn visit_mut_expr(&mut self, n: &mut Expr) {
                if matches!(n, Expr::Ident(i) if &*i.sym == "__ORIGINAL__") {
                    if let Some(f) = self.0.take() {
                        *n = Expr::Paren(ParenExpr { span: DUMMY_SP, expr: Box::new(Expr::Fn(FnExpr { ident: None, function: Box::new(f) })) });
                    }
                    return;
                }
                n.visit_mut_children_with(self);
            }
        }
        let mut inner = original;
        inner.decorators.clear();
        wrapper.function.visit_mut_with(&mut Splice(Some(inner)));
        // The captured variables keep their scope: re-point the generated references at them
        let caps = self.captured(function);
        struct Repoint(Vec<Ident>);
        impl VisitMut for Repoint {
            fn visit_mut_ident(&mut self, n: &mut Ident) {
                if n.ctxt == SyntaxContext::empty() {
                    if let Some(original) = self.0.iter().find(|c| c.sym == n.sym) {
                        n.ctxt = original.ctxt;
                    }
                }
            }
        }
        wrapper.function.visit_mut_with(&mut Repoint(caps));
        function.params = wrapper.function.params;
        function.body = wrapper.function.body;
        self.wrapped += 1;
    }

    fn should_wrap(&self, function: &Function) -> bool {
        function.body.as_ref().is_some_and(|b| is_use_cache(b.stmts.first())) || (self.file_level && self.depth == 0 && function.is_async)
    }
}

impl VisitMut for CacheWrapper<'_> {
    fn visit_mut_fn_decl(&mut self, n: &mut FnDecl) {
        let wrap = self.should_wrap(&n.function);
        self.depth += 1;
        n.function.visit_mut_children_with(self);
        self.depth -= 1;
        if wrap {
            let name = n.ident.sym.to_string();
            self.wrap(&mut n.function, Some(&name));
        }
    }

    fn visit_mut_fn_expr(&mut self, n: &mut FnExpr) {
        let wrap = self.should_wrap(&n.function);
        self.depth += 1;
        n.function.visit_mut_children_with(self);
        self.depth -= 1;
        if wrap {
            let name = n.ident.as_ref().map(|i| i.sym.to_string());
            self.wrap(&mut n.function, name.as_deref());
        }
    }

    fn visit_mut_var_declarator(&mut self, n: &mut VarDeclarator) {
        // `const getPosts = async () => { "use cache"; ... }`: an arrow becomes a named function
        // expression (arrows have no `this`/`arguments` of their own, which the wrapper does not use)
        if let (Pat::Ident(binding), Some(init)) = (&n.name, &mut n.init) {
            if let Expr::Arrow(arrow) = &mut **init {
                let has_directive = matches!(&*arrow.body, ArrowFunctionBody::FunctionBody(b) if is_use_cache(b.stmts.first()));
                if arrow.is_async && (has_directive || (self.file_level && self.depth == 0)) {
                    let body = match &*arrow.body {
                        ArrowFunctionBody::FunctionBody(b) => b.clone(),
                        ArrowFunctionBody::Expr(e) => FunctionBody {
                            span: DUMMY_SP,
                            stmts: vec![Stmt::Return(ReturnStmt { span: DUMMY_SP, arg: Some(e.clone()) })],
                        },
                    };
                    let mut function = Function {
                        params: arrow.params.iter().cloned().map(|pat| Param { span: DUMMY_SP, decorators: vec![], pat }).collect(),
                        decorators: vec![],
                        span: arrow.span,
                        ctxt: arrow.ctxt,
                        body: Some(body),
                        is_generator: false,
                        is_async: true,
                        type_params: None,
                        return_type: None,
                        this_param: None,
                    };
                    self.depth += 1;
                    function.visit_mut_children_with(self);
                    self.depth -= 1;
                    let name = binding.id.sym.to_string();
                    self.wrap(&mut function, Some(&name));
                    **init = Expr::Fn(FnExpr { ident: None, function: Box::new(function) });
                    return;
                }
            }
        }
        n.visit_mut_children_with(self);
    }

    fn visit_mut_arrow_expr(&mut self, n: &mut ArrowExpr) {
        self.depth += 1;
        n.visit_mut_children_with(self);
        self.depth -= 1;
    }

    fn visit_mut_class(&mut self, n: &mut Class) {
        self.depth += 1;
        n.visit_mut_children_with(self);
        self.depth -= 1;
    }
}

/// Wraps the `"use cache"` functions of a react-server module; returns how many. Must run after the
/// identifier resolver and the TypeScript pass.
pub fn transform(
    cm: &Lrc<SourceMap>,
    program: &mut Program,
    rel: &str,
    runtime_path: &str,
    top_level_mark: swc_core::common::Mark,
    unresolved_mark: swc_core::common::Mark,
) -> usize {
    let file_level = is_file_level(program);
    let Program::Module(module) = program else { return 0 };
    if file_level {
        if let Some(ModuleItem::Stmt(Stmt::Expr(ExprStmt { expr, .. }))) = module.body.first() {
            if matches!(&**expr, Expr::Lit(Lit::Str(_))) {
                module.body.remove(0);
            }
        }
    }
    let mut wrapper = CacheWrapper {
        cm,
        rel,
        file_level,
        top_level: SyntaxContext::empty().apply_mark(top_level_mark),
        unresolved: SyntaxContext::empty().apply_mark(unresolved_mark),
        counter: 0,
        wrapped: 0,
        depth: 0,
    };
    module.visit_mut_with(&mut wrapper);
    if wrapper.wrapped > 0 {
        let import = format!("import {{ cacheCall as {RUNTIME_IMPORT} }} from {};\n", serde_json::to_string(runtime_path).unwrap());
        module.body.splice(0..0, parse_items(cm, &import));
    }
    wrapper.wrapped
}
