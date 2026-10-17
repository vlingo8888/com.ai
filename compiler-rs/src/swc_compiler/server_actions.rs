//! Server Actions (`"use server"`), compiled the way Next.js does:
//! - react-server layer: every exported function of a `"use server"` module, and every function
//!   whose body starts with `"use server"`, is registered with `registerServerReference` under a
//!   stable id. Inline actions are hoisted to module level; the variables they close over become
//!   leading parameters, bound at the original site (`$$ACTION_0.bind(null, a, b)`).
//! - browser and SSR layers: a `"use server"` module is replaced by `createServerReference` stubs,
//!   so its code never reaches the client.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use swc_core::common::{sync::Lrc, SourceMap, SyntaxContext, DUMMY_SP};
use swc_core::ecma::{
    ast::*,
    visit::{Visit, VisitMut, VisitMutWith, VisitWith},
};

use super::module::parse_items;
use crate::resolver::Layer;

/// The id of a server action: stable across restarts, unique per file and export
pub fn action_id(rel_path: &str, export: &str) -> String {
    let hash = blake3::hash(format!("{}#{}", rel_path, export).as_bytes());
    hash.to_hex()[..40].to_string()
}

pub struct ReferenceStub {
    pub code: String,
    pub actions: Vec<(String, String)>,
    pub imports: Vec<PathBuf>,
}

/// `createServerReference` stubs standing in for a `"use server"` module outside the react-server layer
pub fn reference_stub(root: &Path, _file: &Path, rel: &str, layer: Layer, exports: &[String]) -> ReferenceStub {
    let flight = match layer {
        Layer::Client => "/_nata/deps/react-server-dom-webpack/client.browser".to_string(),
        _ => "react-server-dom-webpack/client.edge".to_string(),
    };
    let _ = root;
    let mut code = format!(
        "import {{ createServerReference as __nata_server_ref }} from {};\n\
         const __nata_call = (id, args) => globalThis.__NATA_CALL_SERVER__ ? globalThis.__NATA_CALL_SERVER__(id, args) : Promise.reject(new Error(\"Server Actions cannot be called during server rendering\"));\n",
        serde_json::to_string(&flight).unwrap()
    );
    let mut actions = Vec::new();
    for name in exports {
        let id = action_id(rel, name);
        let reference = format!("__nata_server_ref({}, __nata_call)", serde_json::to_string(&id).unwrap());
        if name == "default" {
            code.push_str(&format!("export default {reference};\n"));
        } else {
            code.push_str(&format!("export const {name} = {reference};\n"));
        }
        actions.push((id, name.clone()));
    }
    ReferenceStub { code, actions, imports: Vec::new() }
}

fn has_use_server(body: &Option<FunctionBody>) -> bool {
    body.as_ref()
        .and_then(|b| b.stmts.first())
        .is_some_and(|s| matches!(s, Stmt::Expr(ExprStmt { expr, .. }) if matches!(&**expr, Expr::Lit(Lit::Str(s)) if s.value.to_string_lossy() == "use server")))
}

fn strip_directive(body: &mut Option<FunctionBody>) {
    if let Some(b) = body {
        if !b.stmts.is_empty() {
            b.stmts.remove(0);
        }
    }
}

/// Bindings declared anywhere inside a function (params, `var`/`let`/`const`, functions, classes,
/// catch clauses)
#[derive(Default)]
struct Declared(HashSet<(swc_core::atoms::Atom, SyntaxContext)>);

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
    fn visit_fn_expr(&mut self, n: &FnExpr) {
        if let Some(i) = &n.ident {
            self.0.insert((i.sym.clone(), i.ctxt));
        }
        n.visit_children_with(self);
    }
}

/// Identifiers referenced inside a function
#[derive(Default)]
struct Used(Vec<Ident>);

impl Visit for Used {
    fn visit_ident(&mut self, n: &Ident) {
        if !self.0.iter().any(|i| i.sym == n.sym && i.ctxt == n.ctxt) {
            self.0.push(n.clone());
        }
    }
    // Property names are not variable references
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

struct InlineHoister<'a> {
    rel: &'a str,
    top_level: SyntaxContext,
    unresolved: SyntaxContext,
    counter: usize,
    hoisted: Vec<(String, Function, Vec<Ident>)>,
}

impl InlineHoister<'_> {
    /// Hoists `function` (whose `"use server"` directive was stripped) and returns the expression
    /// replacing it at the original site
    fn hoist(&mut self, mut function: Function) -> Expr {
        let name = format!("$$ACTION_{}", self.counter);
        self.counter += 1;

        let mut declared = Declared::default();
        function.visit_with(&mut declared);
        let mut used = Used::default();
        function.visit_with(&mut used);
        let captured: Vec<Ident> = used
            .0
            .into_iter()
            .filter(|i| i.ctxt != self.top_level && i.ctxt != self.unresolved && i.ctxt != SyntaxContext::empty())
            .filter(|i| !declared.0.contains(&(i.sym.clone(), i.ctxt)))
            .collect();

        // Captured variables come first, bound at the call site
        let mut params: Vec<Param> = captured
            .iter()
            .map(|i| Param { span: DUMMY_SP, decorators: vec![], pat: Pat::Ident(BindingIdent { id: i.clone(), type_ann: None }) })
            .collect();
        params.append(&mut function.params);
        function.params = params;
        function.is_async = true;

        let callee = Ident::new_no_ctxt(name.clone().into(), DUMMY_SP);
        let expr = if captured.is_empty() {
            Expr::Ident(callee)
        } else {
            let mut args = vec![ExprOrSpread { spread: None, expr: Box::new(Expr::Lit(Lit::Null(Null { span: DUMMY_SP }))) }];
            args.extend(captured.iter().map(|i| ExprOrSpread { spread: None, expr: Box::new(Expr::Ident(i.clone())) }));
            Expr::Call(CallExpr {
                span: DUMMY_SP,
                ctxt: SyntaxContext::empty(),
                callee: Callee::Expr(Box::new(Expr::Member(MemberExpr {
                    span: DUMMY_SP,
                    obj: Box::new(Expr::Ident(callee)),
                    prop: MemberProp::Ident(IdentName::new("bind".into(), DUMMY_SP)),
                }))),
                args,
                type_args: None,
            })
        };
        let _ = self.rel;
        self.hoisted.push((name, function, captured));
        expr
    }
}

impl VisitMut for InlineHoister<'_> {
    fn visit_mut_expr(&mut self, n: &mut Expr) {
        n.visit_mut_children_with(self);
        match n {
            Expr::Fn(f) if has_use_server(&f.function.body) => {
                let mut function = (*f.function).clone();
                strip_directive(&mut function.body);
                *n = self.hoist(function);
            }
            Expr::Arrow(a) => {
                let ArrowFunctionBody::FunctionBody(block) = &*a.body else { return };
                let body = Some(block.clone());
                if !has_use_server(&body) {
                    return;
                }
                let mut body = body;
                strip_directive(&mut body);
                let function = Function {
                    this_param: None,
                    params: a.params.iter().cloned().map(|pat| Param { span: DUMMY_SP, decorators: vec![], pat }).collect(),
                    decorators: vec![],
                    span: a.span,
                    ctxt: a.ctxt,
                    body,
                    is_generator: false,
                    is_async: true,
                    type_params: None,
                    return_type: None,
                };
                *n = self.hoist(function);
            }
            _ => {}
        }
    }

    fn visit_mut_stmt(&mut self, n: &mut Stmt) {
        n.visit_mut_children_with(self);
        // `async function save() { "use server"; ... }` inside a component body
        if let Stmt::Decl(Decl::Fn(f)) = n {
            if has_use_server(&f.function.body) {
                let ident = f.ident.clone();
                let mut function = (*f.function).clone();
                strip_directive(&mut function.body);
                let init = self.hoist(function);
                *n = Stmt::Decl(Decl::Var(Box::new(VarDecl {
                    span: DUMMY_SP,
                    ctxt: SyntaxContext::empty(),
                    kind: VarDeclKind::Const,
                    declare: false,
                    decls: vec![VarDeclarator {
                        span: DUMMY_SP,
                        name: Pat::Ident(BindingIdent { id: ident, type_ann: None }),
                        init: Some(Box::new(init)),
                        definite: false,
                    }],
                })));
            }
        }
    }
}

/// The local binding each export of a `"use server"` module refers to; anonymous default exports
/// are given a name first
fn exported_locals(module: &mut Module) -> Vec<(String, Ident)> {
    let mut out = Vec::new();
    let default_ident = || Ident::new_no_ctxt("__nata_default_action".into(), DUMMY_SP);
    let mut replacement: Option<(usize, Vec<ModuleItem>)> = None;
    for (idx, item) in module.body.iter_mut().enumerate() {
        let ModuleItem::ModuleDecl(decl) = item else { continue };
        match decl {
            ModuleDecl::ExportDecl(ExportDecl { decl, .. }) => match decl {
                Decl::Fn(f) => out.push((f.ident.sym.to_string(), f.ident.clone())),
                Decl::Var(v) => {
                    for d in &v.decls {
                        if let Pat::Ident(b) = &d.name {
                            out.push((b.id.sym.to_string(), b.id.clone()));
                        }
                    }
                }
                _ => {}
            },
            ModuleDecl::ExportDefaultDecl(d) => {
                if let DefaultDecl::Fn(f) = &mut d.decl {
                    let ident = f.ident.get_or_insert_with(default_ident).clone();
                    out.push(("default".into(), ident));
                }
            }
            ModuleDecl::ExportDefaultExpr(e) => {
                let ident = default_ident();
                let var = ModuleItem::Stmt(Stmt::Decl(Decl::Var(Box::new(VarDecl {
                    span: DUMMY_SP,
                    ctxt: SyntaxContext::empty(),
                    kind: VarDeclKind::Const,
                    declare: false,
                    decls: vec![VarDeclarator {
                        span: DUMMY_SP,
                        name: Pat::Ident(BindingIdent { id: ident.clone(), type_ann: None }),
                        init: Some(e.expr.clone()),
                        definite: false,
                    }],
                }))));
                let export = ModuleItem::ModuleDecl(ModuleDecl::ExportDefaultExpr(ExportDefaultExpr {
                    span: DUMMY_SP,
                    expr: Box::new(Expr::Ident(ident.clone())),
                }));
                replacement = Some((idx, vec![var, export]));
                out.push(("default".into(), ident));
            }
            ModuleDecl::ExportNamed(n) if n.src.is_none() => {
                for spec in &n.specifiers {
                    if let ExportSpecifier::Named(s) = spec {
                        if let ModuleExportName::Ident(local) = &s.orig {
                            let exported = match &s.exported {
                                Some(ModuleExportName::Ident(i)) => i.sym.to_string(),
                                Some(ModuleExportName::Str(s)) => s.value.to_string_lossy().to_string(),
                                None => local.sym.to_string(),
                            };
                            out.push((exported, local.clone()));
                        }
                    }
                }
            }
            _ => {}
        }
    }
    if let Some((idx, items)) = replacement {
        module.body.splice(idx..idx + 1, items);
    }
    out
}

/// Registers the server actions of a react-server module. Must run after the identifier resolver
/// (it uses scopes to find the variables inline actions close over) and the TypeScript pass.
pub fn register_actions(
    cm: &Lrc<SourceMap>,
    program: &mut Program,
    rel: &str,
    file_level: bool,
    top_level_mark: swc_core::common::Mark,
    unresolved_mark: swc_core::common::Mark,
) -> Vec<(String, String)> {
    let Program::Module(module) = program else { return Vec::new() };
    let mut actions = Vec::new();
    let mut registrations = Vec::new();

    if file_level {
        // Drop the directive itself
        if let Some(ModuleItem::Stmt(Stmt::Expr(ExprStmt { expr, .. }))) = module.body.first() {
            if matches!(&**expr, Expr::Lit(Lit::Str(_))) {
                module.body.remove(0);
            }
        }
        for (export, local) in exported_locals(module) {
            let id = action_id(rel, &export);
            registrations.push((local, id.clone()));
            actions.push((id, export));
        }
    }

    let mut hoister = InlineHoister {
        rel,
        top_level: SyntaxContext::empty().apply_mark(top_level_mark),
        unresolved: SyntaxContext::empty().apply_mark(unresolved_mark),
        counter: 0,
        hoisted: Vec::new(),
    };
    module.visit_mut_with(&mut hoister);
    for (name, function, _) in hoister.hoisted.drain(..) {
        let ident = Ident::new_no_ctxt(name.clone().into(), DUMMY_SP);
        module.body.push(ModuleItem::ModuleDecl(ModuleDecl::ExportDecl(ExportDecl {
            span: DUMMY_SP,
            decl: Decl::Fn(FnDecl { ident: ident.clone(), declare: false, function: Box::new(function) }),
        })));
        let id = action_id(rel, &name);
        registrations.push((ident, id.clone()));
        actions.push((id, name));
    }

    if registrations.is_empty() {
        return actions;
    }

    let helper = "import { registerServerReference as __nata_rsr } from \"react-server-dom-webpack/server.edge\";\n\
        function __nata_action(fn, id) {\n\
          if (typeof fn !== \"function\") return fn;\n\
          __nata_rsr(fn, id, null);\n\
          (globalThis.__NATA_ACTIONS__ ??= new Map()).set(id, fn);\n\
          return fn;\n\
        }\n";
    let at = module.body.iter().take_while(|i| matches!(i, ModuleItem::ModuleDecl(ModuleDecl::Import(_)))).count();
    module.body.splice(at..at, parse_items(cm, helper));
    for (local, id) in registrations {
        let call = format!("__nata_action(__NATA_LOCAL__, {});", serde_json::to_string(&id).unwrap());
        let mut items = parse_items(cm, &call);
        // Point the placeholder at the real binding (keeps its syntax context)
        struct Swap(Ident);
        impl VisitMut for Swap {
            fn visit_mut_ident(&mut self, n: &mut Ident) {
                if &*n.sym == "__NATA_LOCAL__" {
                    *n = self.0.clone();
                }
            }
        }
        items.visit_mut_with(&mut Swap(local));
        module.body.extend(items);
    }
    actions
}
