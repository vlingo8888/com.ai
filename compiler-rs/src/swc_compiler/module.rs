//! Layer-aware module compilation: one compiler for the browser, server-side rendering and React
//! Server Components. Besides TypeScript/JSX it
//! - detects `"use client"` / `"use server"` directives and lists the module's exports,
//! - resolves every import through `oxc_resolver` for the target layer and rewrites it to the URL
//!   (browser) or namespaced absolute path (`rsc:` / `ssr:`, loaded by the worker's `Bun.plugin`)
//!   the runtime loads it from,
//! - turns `"use client"` modules into client references in the react-server layer, and
//!   `"use server"` modules into server references (see `server_actions.rs`).

use std::path::{Path, PathBuf};

use swc_core::common::{
    comments::NoopComments, errors::Handler, sync::Lrc, FileName, Globals, Mark, SourceMap, Spanned, GLOBALS,
};
use swc_core::ecma::{
    ast::*,
    codegen::{text_writer::JsWriter, Config as CodegenConfig, Emitter},
    parser::{lexer::Lexer, Parser, StringInput, Syntax, TsSyntax},
    transforms::{
        base::resolver,
        react::{react, Options as ReactOptions, RefreshOptions, Runtime},
        typescript::typescript,
    },
    visit::{Visit, VisitMut, VisitMutWith, VisitWith},
};

use super::barrel;
use super::engine::CompileError;
use super::fonts;
use super::server_actions;
use super::use_cache;
use crate::resolver::oxc::{self, LayerResolvers};
use crate::resolver::{ImportResolver, Layer, TsConfig};

/// A module-level directive
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Directive {
    UseClient,
    UseServer,
}

pub struct ModuleCompileOptions<'a> {
    pub root: &'a Path,
    /// Absolute path of the file being compiled
    pub file: &'a Path,
    pub layer: Layer,
    /// `jsxDEV` with source locations
    pub development: bool,
    /// React Fast Refresh registration (browser layer only)
    pub refresh: bool,
    /// The bare specifier this file was first imported by, for files inside `node_modules`: names
    /// the browser module of a client reference (`/_nata/deps/<specifier>`)
    pub package_specifier: Option<&'a str>,
}

/// The output of `SwcCompiler::compile_module`
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct CompiledModule {
    pub code: String,
    pub directive: Option<Directive>,
    /// Exported names (`default` included), types excluded
    pub exports: Vec<String>,
    /// Whether the module re-exports `*` from another module (its export list is incomplete)
    pub has_star_export: bool,
    /// Project and package files this module imports, as resolved for its layer
    pub imports: Vec<PathBuf>,
    /// Bare specifiers that resolved into `node_modules`, with the file they resolved to
    pub package_imports: Vec<(String, PathBuf)>,
    /// Server actions this module defines: `(action id, export name)`
    pub actions: Vec<(String, String)>,
    /// Whether the module was replaced by references (client references in the react-server layer,
    /// server references outside it)
    pub is_reference_stub: bool,
    /// `next/font` loader calls in this module
    #[serde(default)]
    pub fonts: Vec<fonts::FontRequest>,
    /// Packages a project file imports on the server that are not installed (module_server.rs
    /// installs them and compiles the file again)
    #[serde(default)]
    pub missing_packages: Vec<String>,
}

/// Next.js client components the framework shims, which become client references in the
/// react-server layer
pub const NEXT_CLIENT_SHIMS: [(&str, &[&str]); 3] =
    [("next/link", &["default"]), ("next/image", &["default", "getImageProps"]), ("next/script", &["default"])];

/// The id a client component is referenced by: the URL the browser imports it from
pub fn client_reference_id(root: &Path, file: &Path, package_specifier: Option<&str>) -> String {
    if let Some(spec) = package_specifier {
        return browser_url_for_package(spec);
    }
    let rel = relative(root, file);
    if let Some(idx) = rel.rfind("node_modules/") {
        return format!("/_nata/deps/{}", &rel[idx + "node_modules/".len()..]);
    }
    format!("/_bundle/{}", rel)
}

fn browser_url_for_package(spec: &str) -> String {
    match spec {
        "core" => "/_nata/shims/core".to_string(),
        _ if spec.starts_with("next/") || spec.starts_with("next-intl/server") => format!("/_nata/shims/{}", spec),
        _ => format!("/_nata/deps/{}", spec),
    }
}

pub fn relative(root: &Path, file: &Path) -> String {
    let root = crate::rpc::canonicalize(&root).unwrap_or_else(|_| root.to_path_buf());
    let file_c = crate::rpc::canonicalize(&file).unwrap_or_else(|_| file.to_path_buf());
    file_c.strip_prefix(&root).unwrap_or(file).to_string_lossy().replace('\\', "/")
}

/// The namespaced specifier the worker loads a server module from (`rsc:/abs/file.tsx`)
pub fn server_specifier(layer: Layer, file: &Path) -> String {
    format!("{}:{}", layer.as_str(), crate::rpc::clean_path(file))
}

pub(crate) fn parse(cm: &Lrc<SourceMap>, source: &str, file_name: &str) -> Result<Program, CompileError> {
    // `.ts` is parsed without JSX so generic arrows (`<T>(x: T) => x`) work; a `.ts` file that
    // still fails and looks like it holds JSX gets a second try as TSX
    let plain_ts = file_name.ends_with(".ts") || file_name.ends_with(".mts") || file_name.ends_with(".cts");
    match parse_with(cm, source, file_name, !plain_ts) {
        Err(err) if plain_ts && (source.contains("</") || source.contains("/>")) => parse_with(cm, source, file_name, true).map_err(|_| err),
        result => result,
    }
}

fn parse_with(cm: &Lrc<SourceMap>, source: &str, file_name: &str, tsx: bool) -> Result<Program, CompileError> {
    let handler = Handler::with_emitter_writer(Box::new(std::io::sink()), Some(cm.clone()));
    let fm = cm.new_source_file(FileName::Custom(file_name.to_string()).into(), source.to_string());
    let is_ts = !(file_name.ends_with(".js") || file_name.ends_with(".mjs") || file_name.ends_with(".cjs") || file_name.ends_with(".jsx"));
    let syntax = if is_ts {
        Syntax::Typescript(TsSyntax { tsx, decorators: true, dts: false, no_early_errors: true, ..Default::default() })
    } else {
        Syntax::Es(swc_core::ecma::parser::EsSyntax { jsx: true, decorators: true, import_attributes: true, ..Default::default() })
    };
    let lexer = Lexer::new(syntax, EsVersion::latest(), StringInput::from(&*fm), None);
    let mut parser = Parser::new_from(lexer);
    for err in parser.take_errors() {
        err.into_diagnostic(&handler).emit();
    }
    parser.parse_program().map_err(|err| {
        let pos = cm.lookup_char_pos(err.span().lo);
        let message = err.kind().msg().to_string();
        let line = pos.line;
        let column = pos.col_display + 1;
        CompileError {
            file: file_name.to_string(),
            message,
            line: Some(line),
            column: Some(column),
            frame: crate::term::code_frame(source, line, column, 2),
        }
    })
}

pub(crate) fn parse_items(cm: &Lrc<SourceMap>, code: &str) -> Vec<ModuleItem> {
    match parse(cm, code, "__nata_generated.js") {
        Ok(Program::Module(m)) => m.body,
        Ok(Program::Script(s)) => s.body.into_iter().map(ModuleItem::Stmt).collect(),
        Err(err) => panic!("generated code does not parse: {}\n{}", err.message, code),
    }
}

pub(crate) fn emit(cm: &Lrc<SourceMap>, program: &Program, file_name: &str) -> Result<String, CompileError> {
    let mut buf = vec![];
    {
        let mut emitter = Emitter {
            cfg: CodegenConfig::default().with_minify(false).with_target(EsVersion::latest()),
            cm: cm.clone(),
            comments: None,
            wr: Box::new(JsWriter::new(cm.clone(), "\n", &mut buf, None)),
        };
        emitter.emit_program(program).map_err(|e| CompileError {
            file: file_name.to_string(),
            message: format!("SWC Codegen Error: {:?}", e),
            line: None,
            column: None,
            frame: String::new(),
        })?;
    }
    String::from_utf8(buf).map_err(|e| CompileError {
        file: file_name.to_string(),
        message: format!("UTF-8 conversion error: {:?}", e),
        line: None,
        column: None,
        frame: String::new(),
    })
}

/// The directive at the top of a module (`"use client"` / `"use server"`)
pub fn module_directive(program: &Program) -> Option<Directive> {
    let stmts: Box<dyn Iterator<Item = &Stmt>> = match program {
        Program::Module(m) => Box::new(m.body.iter().map_while(|i| i.as_stmt())),
        Program::Script(s) => Box::new(s.body.iter()),
    };
    for stmt in stmts {
        let Stmt::Expr(ExprStmt { expr, .. }) = stmt else { break };
        let Expr::Lit(Lit::Str(s)) = &**expr else { break };
        match s.value.to_string_lossy().as_ref() {
            "use client" => return Some(Directive::UseClient),
            "use server" => return Some(Directive::UseServer),
            _ => {}
        }
    }
    None
}

fn pat_names(pat: &Pat, out: &mut Vec<String>) {
    match pat {
        Pat::Ident(i) => out.push(i.id.sym.to_string()),
        Pat::Array(a) => a.elems.iter().flatten().for_each(|p| pat_names(p, out)),
        Pat::Object(o) => {
            for prop in &o.props {
                match prop {
                    ObjectPatProp::KeyValue(kv) => pat_names(&kv.value, out),
                    ObjectPatProp::Assign(a) => out.push(a.key.sym.to_string()),
                    ObjectPatProp::Rest(r) => pat_names(&r.arg, out),
                }
            }
        }
        Pat::Assign(a) => pat_names(&a.left, out),
        Pat::Rest(r) => pat_names(&r.arg, out),
        _ => {}
    }
}

pub(crate) fn export_name(name: &ModuleExportName) -> String {
    match name {
        ModuleExportName::Ident(i) => i.sym.to_string(),
        ModuleExportName::Str(s) => s.value.to_string_lossy().to_string(),
    }
}

/// The value exports of a module (`default` included) and whether it has `export * from`
pub fn module_exports(program: &Program) -> (Vec<String>, bool) {
    let mut names = Vec::new();
    let mut star = false;
    let Program::Module(module) = program else { return (names, star) };
    for item in &module.body {
        let ModuleItem::ModuleDecl(decl) = item else { continue };
        match decl {
            ModuleDecl::ExportDecl(ExportDecl { decl, .. }) => match decl {
                Decl::Class(c) => names.push(c.ident.sym.to_string()),
                Decl::Fn(f) => names.push(f.ident.sym.to_string()),
                Decl::Var(v) => v.decls.iter().for_each(|d| pat_names(&d.name, &mut names)),
                Decl::TsEnum(e) if !e.declare => names.push(e.id.sym.to_string()),
                _ => {}
            },
            ModuleDecl::ExportDefaultDecl(d) => {
                if !matches!(d.decl, DefaultDecl::TsInterfaceDecl(_)) {
                    names.push("default".into())
                }
            }
            ModuleDecl::ExportDefaultExpr(_) => names.push("default".into()),
            ModuleDecl::ExportNamed(n) if !n.type_only => {
                for spec in &n.specifiers {
                    match spec {
                        ExportSpecifier::Named(s) if !s.is_type_only => {
                            names.push(export_name(s.exported.as_ref().unwrap_or(&s.orig)))
                        }
                        ExportSpecifier::Default(d) => names.push(d.exported.sym.to_string()),
                        ExportSpecifier::Namespace(ns) => names.push(export_name(&ns.name)),
                        _ => {}
                    }
                }
            }
            ModuleDecl::ExportAll(a) if !a.type_only => star = true,
            _ => {}
        }
    }
    names.dedup();
    (names, star)
}

/// Rewrites import specifiers to what the runtime of `layer` loads
struct ImportRewriter<'a> {
    root: &'a Path,
    file: PathBuf,
    rel: String,
    layer: Layer,
    resolvers: &'a LayerResolvers,
    tsconfig: Option<TsConfig>,
    imports: Vec<PathBuf>,
    package_imports: Vec<(String, PathBuf)>,
    missing_packages: Vec<String>,
}

impl ImportRewriter<'_> {
    fn legacy_tsconfig(&mut self) -> &TsConfig {
        let root = self.root;
        self.tsconfig.get_or_insert_with(|| TsConfig::load_from_dir(root))
    }

    fn legacy_url(&mut self, spec: &str) -> String {
        let parent = Path::new(&self.rel).parent().unwrap_or(Path::new("")).to_string_lossy().replace('\\', "/");
        let tsconfig = self.legacy_tsconfig().clone();
        ImportResolver::resolve_specifier_with_config(spec, &parent, &tsconfig)
    }

    fn rewrite(&mut self, spec: &str) -> Option<String> {
        if spec.starts_with("http://") || spec.starts_with("https://") || spec.starts_with("data:") {
            return None;
        }
        if oxc::is_builtin(spec) {
            return None;
        }
        let clean = spec.split('?').next().unwrap_or(spec);
        let is_css = clean.ends_with(".css");
        match self.layer {
            Layer::Client => self.rewrite_client(spec, clean, is_css),
            Layer::Ssr | Layer::Rsc => self.rewrite_server(spec, clean, is_css),
        }
    }

    fn rewrite_client(&mut self, spec: &str, clean: &str, is_css: bool) -> Option<String> {
        if spec.starts_with("/_") {
            return None;
        }
        if clean == "core" || clean.starts_with("next/") || clean.starts_with("next-intl/server") {
            return Some(browser_url_for_package(clean));
        }
        let resolved = self.resolvers.resolve(Layer::Client, &self.file, clean);
        // `import logo from "./logo.png"`: { src, width, height, blurDataURL }
        if let Some(r) = resolved.as_ref().filter(|r| crate::image_optimizer::is_image(&r.path) && !r.in_node_modules) {
            return Some(format!("/_nata/image-module/{}", relative(self.root, &r.path)));
        }
        if is_css {
            return Some(match resolved {
                Some(r) if !r.in_node_modules => format!("/_nata/css/{}", relative(self.root, &r.path)),
                Some(_) if oxc::is_bare(clean) => format!("/_nata/css/node_modules/{}", clean),
                _ => self.legacy_url(spec),
            });
        }
        match resolved {
            Some(r) if !r.in_node_modules => {
                self.imports.push(r.path.clone());
                Some(format!("/_bundle/{}", relative(self.root, &r.path)))
            }
            Some(r) if oxc::is_bare(clean) => {
                self.package_imports.push((clean.to_string(), r.path));
                Some(format!("/_nata/deps/{}", clean))
            }
            _ if oxc::is_bare(clean) && !clean.starts_with("@/") && !clean.starts_with("~/") => {
                Some(format!("/_nata/deps/{}", clean))
            }
            _ => Some(self.legacy_url(spec)),
        }
    }

    fn rewrite_server(&mut self, spec: &str, clean: &str, is_css: bool) -> Option<String> {
        if spec.starts_with("rsc:") || spec.starts_with("ssr:") {
            return None;
        }
        // Shared by both layers and loaded natively by Bun: one database connection per worker. The
        // project's own `core/` (comx builds) by path, `.nata/core.ts` through node_modules/core
        if clean == "core" {
            return crate::project::project_core(self.root).map(|p| p.to_string_lossy().replace('\\', "/"));
        }
        // next-intl's server API, for `[locale]` routes (works with or without the next-intl package)
        if clean == "next-intl/server" {
            let shim = self.root.join(".nata/next-intl-server.ts");
            if shim.is_file() {
                self.imports.push(shim.clone());
                return Some(server_specifier(self.layer, &shim));
            }
        }
        if self.layer == Layer::Rsc && clean.starts_with("next/") {
            if let Some((name, _)) = NEXT_CLIENT_SHIMS.iter().find(|(name, _)| *name == clean) {
                return Some(format!("rsc:/__nata_client_shim/{}", name));
            }
        }
        let Some(resolved) = self.resolvers.resolve(self.layer, &self.file, clean) else {
            self.note_missing_package(clean);
            return None;
        };
        let path_str = resolved.path.to_string_lossy().replace('\\', "/");
        if crate::image_optimizer::is_image(&resolved.path) {
            // Generated by module_server: the same object as in the browser
            return Some(server_specifier(self.layer, &resolved.path));
        }
        if is_css || resolved.path.extension().is_some_and(|e| e == "json" || e == "node" || e == "wasm") {
            // Handled natively (CSS Modules plugin, JSON loader), identical in both layers
            return Some(path_str);
        }
        if resolved.in_node_modules && oxc::is_bare(clean) && crate::next_config::is_server_external(self.root, clean) {
            // next.config `serverExternalPackages`: loaded natively by Bun in both layers (packages
            // that read files next to their own module, such as @electric-sql/pglite)
            return None;
        }
        if resolved.in_node_modules {
            // Installed packages are shared with the SSR renderer (one React instance): native, unless
            // React comes from the framework, which only the engine's resolution finds
            if self.layer == Layer::Ssr && crate::framework_react::active_modules_dir(self.root).is_none() {
                return if oxc::is_bare(clean) { None } else { Some(path_str) };
            }
            if oxc::is_bare(clean) {
                self.package_imports.push((clean.to_string(), resolved.path.clone()));
            }
        }
        self.imports.push(resolved.path.clone());
        Some(server_specifier(self.layer, &resolved.path))
    }

    /// A bare import of a project file that resolves nowhere: a package to install. Imports inside
    /// `node_modules` are left alone (optional dependencies a package `require`s in a `try`), as are
    /// `#name` imports and path aliases that point at a missing file
    fn note_missing_package(&mut self, clean: &str) {
        if !oxc::is_bare(clean) || clean.starts_with('#') || clean.starts_with("@/") || clean.starts_with("~/") {
            return;
        }
        if self.file.components().any(|c| c.as_os_str() == "node_modules") {
            return;
        }
        if self.legacy_tsconfig().resolve_path_alias(clean).is_some() || !crate::deps::is_valid_specifier(clean) {
            return;
        }
        let (name, _) = crate::deps::split_specifier(clean);
        if !self.missing_packages.contains(&name) {
            self.missing_packages.push(name);
        }
    }

    fn rewrite_str(&mut self, s: &mut Str) {
        let spec = s.value.to_string_lossy().to_string();
        if let Some(url) = self.rewrite(&spec) {
            s.value = url.into();
            s.raw = None;
        }
    }
}

impl VisitMut for ImportRewriter<'_> {
    fn visit_mut_import_decl(&mut self, n: &mut ImportDecl) {
        if !n.type_only {
            self.rewrite_str(&mut n.src);
        }
    }

    fn visit_mut_named_export(&mut self, n: &mut NamedExport) {
        if let Some(src) = &mut n.src {
            if !n.type_only {
                self.rewrite_str(src);
            }
        }
    }

    fn visit_mut_export_all(&mut self, n: &mut ExportAll) {
        if !n.type_only {
            self.rewrite_str(&mut n.src);
        }
    }

    fn visit_mut_call_expr(&mut self, n: &mut CallExpr) {
        n.visit_mut_children_with(self);
        let is_import = matches!(n.callee, Callee::Import(_));
        let is_require = self.layer != Layer::Client
            && matches!(&n.callee, Callee::Expr(e) if matches!(&**e, Expr::Ident(i) if &*i.sym == "require"));
        if !(is_import || is_require) || n.args.len() != 1 {
            return;
        }
        if let Expr::Lit(Lit::Str(s)) = &mut *n.args[0].expr {
            self.rewrite_str(s);
        }
    }
}

/// Browser layer: `process.env.NEXT_PUBLIC_X` (and next.config `env`) become their values,
/// `process.env.NODE_ENV` the mode, any other `process.env.X` `undefined` (server-only values never
/// reach the browser), like Next.js
struct InlineEnv {
    public: std::collections::BTreeMap<String, String>,
    node_env: &'static str,
}

impl InlineEnv {
    fn is_process_env(expr: &Expr) -> bool {
        matches!(expr, Expr::Member(MemberExpr { obj, prop: MemberProp::Ident(p), .. })
            if &*p.sym == "env" && matches!(&**obj, Expr::Ident(i) if &*i.sym == "process"))
    }

    fn value(&self, key: &str) -> Expr {
        let value = if key == "NODE_ENV" { Some(self.node_env.to_string()) } else { self.public.get(key).cloned() };
        match value {
            Some(v) => Expr::Lit(Lit::Str(Str { span: swc_core::common::DUMMY_SP, value: v.into(), raw: None })),
            None => Expr::Ident(Ident::new_no_ctxt("undefined".into(), swc_core::common::DUMMY_SP)),
        }
    }
}

impl VisitMut for InlineEnv {
    fn visit_mut_expr(&mut self, n: &mut Expr) {
        if let Expr::Member(member) = n {
            if Self::is_process_env(&member.obj) {
                let key = match &member.prop {
                    MemberProp::Ident(i) => Some(i.sym.to_string()),
                    MemberProp::Computed(c) => match &*c.expr {
                        Expr::Lit(Lit::Str(s)) => Some(s.value.to_string_lossy().to_string()),
                        _ => None,
                    },
                    _ => None,
                };
                if let Some(key) = key {
                    *n = self.value(&key);
                    return;
                }
            }
        }
        // `process.env` itself: an object with the public values
        if Self::is_process_env(n) {
            let mut map = serde_json::Map::new();
            map.insert("NODE_ENV".into(), serde_json::Value::String(self.node_env.into()));
            for (k, v) in &self.public {
                map.insert(k.clone(), serde_json::Value::String(v.clone()));
            }
            let json = serde_json::to_string(&map).unwrap();
            if let Ok(Program::Script(script)) = parse(&Default::default(), &format!("({json})"), "env.js") {
                if let Some(Stmt::Expr(e)) = script.body.into_iter().next() {
                    *n = *e.expr;
                    return;
                }
            }
        }
        n.visit_mut_children_with(self);
    }
}

/// Browser layer: `require("x")` (the global `require`, which browsers lack) becomes a hoisted
/// `import * as __nata_req_N from "x"`, like webpack. A specifier that does not resolve throws
/// "Cannot find module" where it was called, so `try { require(...) } catch {}` for optional
/// modules keeps working
struct ClientRequire<'a> {
    file: &'a Path,
    resolvers: &'a LayerResolvers,
    unresolved: swc_core::common::SyntaxContext,
    hoisted: Vec<(String, Ident)>,
}

impl ClientRequire<'_> {
    fn resolves(&self, spec: &str) -> bool {
        let clean = spec.split('?').next().unwrap_or(spec);
        clean == "core"
            || clean.starts_with("next/")
            || oxc::is_builtin(clean)
            || self.resolvers.resolve(Layer::Client, self.file, clean).is_some()
    }

    fn module_ident(&mut self, spec: &str) -> Ident {
        if let Some((_, id)) = self.hoisted.iter().find(|(s, _)| s == spec) {
            return id.clone();
        }
        let id = Ident::new_no_ctxt(format!("__nata_req_{}", self.hoisted.len()).into(), swc_core::common::DUMMY_SP);
        self.hoisted.push((spec.to_string(), id.clone()));
        id
    }

    fn not_found(spec: &str) -> Expr {
        let message = serde_json::to_string(&format!("Cannot find module '{spec}'")).unwrap();
        let code = format!("(() => {{ throw Object.assign(new Error({message}), {{ code: \"MODULE_NOT_FOUND\" }}); }})()");
        match parse(&Default::default(), &code, "require.js") {
            Ok(Program::Script(script)) => match script.body.into_iter().next() {
                Some(Stmt::Expr(e)) => *e.expr,
                _ => unreachable!(),
            },
            _ => unreachable!(),
        }
    }

    fn imports(&self) -> Vec<ModuleItem> {
        self.hoisted
            .iter()
            .map(|(spec, id)| {
                ModuleItem::ModuleDecl(ModuleDecl::Import(ImportDecl {
                    span: swc_core::common::DUMMY_SP,
                    specifiers: vec![ImportSpecifier::Namespace(ImportStarAsSpecifier {
                        span: swc_core::common::DUMMY_SP,
                        local: id.clone(),
                    })],
                    src: Box::new(Str { span: swc_core::common::DUMMY_SP, value: spec.as_str().into(), raw: None }),
                    type_only: false,
                    with: None,
                    phase: Default::default(),
                }))
            })
            .collect()
    }
}

impl VisitMut for ClientRequire<'_> {
    fn visit_mut_expr(&mut self, n: &mut Expr) {
        n.visit_mut_children_with(self);
        let Expr::Call(call) = n else { return };
        let Callee::Expr(callee) = &call.callee else { return };
        let Expr::Ident(i) = &**callee else { return };
        if &*i.sym != "require" || i.ctxt != self.unresolved || call.args.len() != 1 || call.args[0].spread.is_some() {
            return;
        }
        let Expr::Lit(Lit::Str(s)) = &*call.args[0].expr else { return };
        let spec = s.value.to_string_lossy().to_string();
        *n = if self.resolves(&spec) { Expr::Ident(self.module_ident(&spec)) } else { Self::not_found(&spec) };
    }
}

/// Follows `name` through the `export … from` chains of `file` to the module that declares it: that
/// module and its own name for the export, `None` when no module in the chain exports it
fn export_origin(
    resolvers: &LayerResolvers,
    file: &Path,
    name: &str,
    seen: &mut Vec<PathBuf>,
) -> Option<(PathBuf, String, Option<Directive>)> {
    if seen.iter().any(|f| f == file) || seen.len() > 64 {
        return None;
    }
    seen.push(file.to_path_buf());
    let source = std::fs::read_to_string(file).ok()?;
    let cm: Lrc<SourceMap> = Default::default();
    let program = parse(&cm, &source, &file.to_string_lossy()).ok()?;
    let directive = module_directive(&program);
    let Program::Module(module) = &program else { return None };
    let follow = |src: &Str, orig: &str, seen: &mut Vec<PathBuf>| {
        let target = resolvers.resolve(Layer::Client, file, &src.value.to_string_lossy())?;
        if target.in_node_modules {
            return None;
        }
        export_origin(resolvers, &target.path, orig, seen)
    };
    for item in &module.body {
        let ModuleItem::ModuleDecl(ModuleDecl::ExportNamed(n)) = item else { continue };
        if n.type_only {
            continue;
        }
        for spec in &n.specifiers {
            let ExportSpecifier::Named(s) = spec else { continue };
            if s.is_type_only || export_name(s.exported.as_ref().unwrap_or(&s.orig)) != name {
                continue;
            }
            return match &n.src {
                Some(src) => follow(src, &export_name(&s.orig), seen),
                None => Some((file.to_path_buf(), name.to_string(), directive)),
            };
        }
    }
    if module_exports(&program).0.iter().any(|e| e == name) {
        return Some((file.to_path_buf(), name.to_string(), directive));
    }
    if name == "default" {
        return None;
    }
    module.body.iter().find_map(|item| match item {
        ModuleItem::ModuleDecl(ModuleDecl::ExportAll(a)) if !a.type_only => follow(&a.src, name, seen),
        _ => None,
    })
}

/// Whether a `modules/` export is plain shared code the browser can run itself (constants, pure
/// helpers of `domain/`): not a server action (`"use server"`) and not in a server-side layer
fn is_shared_value(origin: &(PathBuf, String, Option<Directive>)) -> bool {
    let (file, _, directive) = origin;
    let server_layer = file.components().any(|c| matches!(c.as_os_str().to_str(), Some("presentation" | "use-cases")));
    *directive != Some(Directive::UseServer) && !server_layer
}

/// Browser layer, legacy convention: imports from `modules/` become calls to `/_nata/rpc`, except plain
/// values (`PAGE_TEMPLATES`, `slugify`) a barrel re-exports, which are imported from where they live
fn proxy_legacy_rpc_imports(cm: &Lrc<SourceMap>, module: &mut Module, file: &Path, rel: &str, root: &Path) {
    let parent = Path::new(rel).parent().unwrap_or(Path::new("")).to_string_lossy().replace('\\', "/");
    let tsconfig = TsConfig::load_from_dir(root);
    let resolvers = oxc::resolvers(root);
    let mut generated = String::new();
    module.body.retain(|item| {
        let ModuleItem::ModuleDecl(ModuleDecl::Import(import)) = item else { return true };
        let src = import.src.value.to_string_lossy();
        let Some(module_path) = ImportResolver::extract_server_action_module(&src, &parent, &tsconfig) else {
            return true;
        };
        if import.type_only {
            return false;
        }
        let target = resolvers.resolve(Layer::Client, file, &src).filter(|r| !r.in_node_modules);
        for spec in &import.specifiers {
            let (local, remote) = match spec {
                ImportSpecifier::Named(n) if !n.is_type_only => {
                    (n.local.sym.to_string(), n.imported.as_ref().map(export_name).unwrap_or_else(|| n.local.sym.to_string()))
                }
                ImportSpecifier::Default(d) => (d.local.sym.to_string(), "default".to_string()),
                ImportSpecifier::Namespace(n) => (n.local.sym.to_string(), "default".to_string()),
                _ => continue,
            };
            let origin = match (spec, &target) {
                (ImportSpecifier::Namespace(_), _) | (_, None) => None,
                (_, Some(t)) => export_origin(&resolvers, &t.path, &remote, &mut Vec::new()).filter(is_shared_value),
            };
            if let Some((origin_file, origin_name, _)) = origin {
                let path = serde_json::to_string(&origin_file.to_string_lossy().replace('\\', "/")).unwrap();
                let binding = if origin_name == local { local } else { format!("{origin_name} as {local}") };
                generated.push_str(&format!("import {{ {binding} }} from {path};\n"));
                continue;
            }
            let binding = crate::resolver::imports::ImportBinding { local_name: local, remote_action: remote };
            let code = ImportResolver::generate_proxy_code(&binding, &module_path);
            generated.push_str(&code.replacen("export const ", "const ", 1));
        }
        false
    });
    if !generated.is_empty() {
        let items = parse_items(cm, &generated);
        let at = module.body.iter().take_while(|i| matches!(i, ModuleItem::ModuleDecl(ModuleDecl::Import(_)))).count();
        module.body.splice(at..at, items);
    }
}

/// React APIs missing from the `react-server` build (React 19): importing one in the react-server
/// layer fails to link with a bare "Export named 'useState' not found"
const CLIENT_ONLY_REACT_APIS: &[&str] = &[
    "Component",
    "PureComponent",
    "createContext",
    "useContext",
    "useState",
    "useReducer",
    "useRef",
    "useEffect",
    "useLayoutEffect",
    "useInsertionEffect",
    "useImperativeHandle",
    "useSyncExternalStore",
    "useTransition",
    "useDeferredValue",
    "useOptimistic",
    "useActionState",
    "useEffectEvent",
    "startTransition",
];

/// Exports that only a Server Component (or a route segment) may have: a module with one stays on the
/// server, and a client-only API in it is the error below
const SERVER_ONLY_EXPORTS: &[&str] = &[
    "metadata",
    "generateMetadata",
    "viewport",
    "generateViewport",
    "generateStaticParams",
    "revalidate",
    "dynamic",
    "dynamicParams",
    "fetchCache",
    "runtime",
    "preferredRegion",
    "maxDuration",
];

/// Imports that cannot reach the browser
fn server_only_import(specifier: &str) -> bool {
    const PACKAGES: &[&str] = &["server-only", "core", "next/headers", "next/cache", "fs", "path", "os", "crypto", "child_process", "net", "worker_threads"];
    specifier.starts_with("node:") || specifier.starts_with("bun:") || PACKAGES.contains(&specifier) || specifier.starts_with("fs/")
}

/// What makes an app module without a directive a Client Component: a client-only React API (named,
/// or through `React.`), a hook of the app or of `next/navigation`, or an event handler in its JSX
#[derive(Default)]
struct ClientNeeds {
    react: Vec<String>,
    hooks: Vec<String>,
    reason: Option<String>,
}

impl Visit for ClientNeeds {
    fn visit_member_expr(&mut self, member: &MemberExpr) {
        if self.reason.is_none() {
            if let (Expr::Ident(obj), MemberProp::Ident(prop)) = (&*member.obj, &member.prop) {
                if self.react.iter().any(|r| r.as_str() == obj.sym.as_ref()) && CLIENT_ONLY_REACT_APIS.contains(&prop.sym.as_ref()) {
                    self.reason = Some(format!("`{}.{}`", obj.sym, prop.sym));
                }
            }
        }
        member.visit_children_with(self);
    }

    fn visit_call_expr(&mut self, call: &CallExpr) {
        if self.reason.is_none() {
            if let Callee::Expr(callee) = &call.callee {
                if let Expr::Ident(id) = &**callee {
                    if self.hooks.iter().any(|h| h.as_str() == id.sym.as_ref()) {
                        self.reason = Some(format!("the hook `{}`", id.sym));
                    }
                }
            }
        }
        call.visit_children_with(self);
    }

    fn visit_jsx_attr(&mut self, attr: &JSXAttr) {
        if self.reason.is_none() {
            if let (JSXAttrName::Ident(name), Some(JSXAttrValue::JSXExprContainer(_))) = (&attr.name, &attr.value) {
                let name = name.sym.as_ref();
                if name.len() > 2 && name.starts_with("on") && name.as_bytes()[2].is_ascii_uppercase() {
                    self.reason = Some(format!("the event handler `{name}`"));
                }
            }
        }
        attr.visit_children_with(self);
    }
}

fn is_async_export(item: &ModuleItem) -> bool {
    let async_expr = |expr: &Expr| match expr {
        Expr::Arrow(arrow) => arrow.is_async,
        Expr::Fn(f) => f.function.is_async,
        _ => false,
    };
    match item {
        ModuleItem::ModuleDecl(ModuleDecl::ExportDefaultDecl(ExportDefaultDecl { decl: DefaultDecl::Fn(f), .. })) => f.function.is_async,
        ModuleItem::ModuleDecl(ModuleDecl::ExportDefaultExpr(e)) => async_expr(&e.expr),
        ModuleItem::ModuleDecl(ModuleDecl::ExportDecl(ExportDecl { decl: Decl::Fn(f), .. })) => f.function.is_async,
        ModuleItem::ModuleDecl(ModuleDecl::ExportDecl(ExportDecl { decl: Decl::Var(var), .. })) => {
            var.decls.iter().any(|d| d.init.as_deref().is_some_and(async_expr))
        }
        _ => false,
    }
}

/// Next.js rejects an app module without "use client" that needs the browser. com renders it as a
/// Client Component instead, as if it had the directive, unless it can only be a Server Component
/// (route segment exports, async components, server-only imports): returns what needs the client
fn inferred_client(program: &Program, exports: &[String]) -> Option<String> {
    let Program::Module(module) = program else { return None };
    if exports.iter().any(|e| SERVER_ONLY_EXPORTS.contains(&e.as_str())) || module.body.iter().any(is_async_export) {
        return None;
    }
    let mut needs = ClientNeeds::default();
    for item in &module.body {
        let ModuleItem::ModuleDecl(ModuleDecl::Import(import)) = item else { continue };
        let specifier = import.src.value.to_string_lossy();
        if server_only_import(&specifier) {
            return None;
        }
        if import.type_only {
            continue;
        }
        let app_hooks = specifier.starts_with('.') || specifier.starts_with("@/") || specifier.starts_with("~/") || specifier == "next/navigation";
        for spec in &import.specifiers {
            match spec {
                ImportSpecifier::Named(named) if !named.is_type_only => {
                    let local = named.local.sym.to_string();
                    let imported = named.imported.as_ref().map(export_name).unwrap_or_else(|| local.clone());
                    if specifier == "react" && CLIENT_ONLY_REACT_APIS.contains(&imported.as_str()) {
                        needs.reason.get_or_insert(format!("`{imported}`"));
                    } else if app_hooks && local.len() > 3 && local.starts_with("use") && local.as_bytes()[3].is_ascii_uppercase() {
                        needs.hooks.push(local);
                    }
                }
                ImportSpecifier::Default(d) if specifier == "react" => needs.react.push(d.local.sym.to_string()),
                ImportSpecifier::Namespace(n) if specifier == "react" => needs.react.push(n.local.sym.to_string()),
                _ => {}
            }
        }
    }
    if needs.reason.is_none() {
        module.visit_with(&mut needs);
    }
    needs.reason
}

/// Like Next.js: a Server Component (a module without "use client") importing a client-only React API
/// is a compile error at that import, which names the API and the fix
fn check_server_react_imports(cm: &Lrc<SourceMap>, program: &Program, source: &str, file_name: &str) -> Result<(), CompileError> {
    let Program::Module(module) = program else { return Ok(()) };
    for item in &module.body {
        let ModuleItem::ModuleDecl(ModuleDecl::Import(import)) = item else { continue };
        if import.type_only || import.src.value.to_string_lossy() != "react" {
            continue;
        }
        for spec in &import.specifiers {
            let ImportSpecifier::Named(named) = spec else { continue };
            if named.is_type_only {
                continue;
            }
            let api = named.imported.as_ref().map(export_name).unwrap_or_else(|| named.local.sym.to_string());
            if !CLIENT_ONLY_REACT_APIS.contains(&api.as_str()) {
                continue;
            }
            let kind = if api.starts_with("use") { "React Hook" } else { "React API" };
            let pos = cm.lookup_char_pos(named.span.lo);
            let (line, column) = (pos.line, pos.col_display + 1);
            return Err(CompileError {
                file: file_name.to_string(),
                message: format!(
                    "You're importing a component that needs `{api}`. This {kind} only works in a Client Component. \
                     To fix, mark the file (or its parent) with the `\"use client\"` directive.\n\n \
                     Learn more: https://nextjs.org/docs/app/api-reference/directives/use-client"
                ),
                line: Some(line),
                column: Some(column),
                frame: crate::term::code_frame(source, line, column, 2),
            });
        }
    }
    Ok(())
}

/// Client references for a `"use client"` module, as seen from the react-server layer
fn client_reference_stub(root: &Path, file: &Path, id: &str, exports: &[String]) -> Result<String, String> {
    let resolvers = oxc::resolvers(root);
    let flight = resolvers
        .resolve_from_root(Layer::Rsc, "react-server-dom-webpack/server.edge")
        .ok_or_else(|| "react-server-dom-webpack is not installed".to_string())?;
    let flight = server_specifier(Layer::Rsc, &flight.path);
    let id_lit = serde_json::to_string(id).unwrap();
    let file_name = relative(root, file);
    let mut code = format!("import {{ registerClientReference as __nata_client_ref }} from {};\n", serde_json::to_string(&flight).unwrap());
    for name in exports {
        let message = serde_json::to_string(&format!(
            "Attempted to call {name}() from the server but {name} is on the client. It's not possible to invoke a client function from the server, it can only be rendered as a Component or passed to props of a Client Component. ({file_name})"
        ))
        .unwrap();
        let reference = format!(
            "__nata_client_ref(function () {{ throw new Error({message}); }}, {id_lit}, {})",
            serde_json::to_string(name).unwrap()
        );
        if name == "default" {
            code.push_str(&format!("export default {reference};\n"));
        } else {
            code.push_str(&format!("export const {name} = {reference};\n"));
        }
    }
    Ok(code)
}

/// Client references for one of the framework's `next/*` client component shims
pub fn next_client_shim_stub(root: &Path, name: &str) -> Option<String> {
    let (_, exports) = NEXT_CLIENT_SHIMS.iter().find(|(n, _)| *n == name)?;
    let exports: Vec<String> = exports.iter().map(|e| e.to_string()).collect();
    client_reference_stub(root, Path::new(name), &browser_url_for_package(name), &exports).ok()
}

pub fn compile_module(source: &str, opts: &ModuleCompileOptions) -> Result<CompiledModule, CompileError> {
    let cm: Lrc<SourceMap> = Default::default();
    let rel = relative(opts.root, opts.file);
    let file_name = rel.clone();
    let mut program = parse(&cm, source, &file_name)?;

    let (exports, has_star_export) = module_exports(&program);
    // Only the app's own files: packages are checked by whoever publishes them
    let is_app_file = !rel.starts_with("node_modules/") && !rel.contains("/node_modules/") && !rel.starts_with(".nata/");
    let mut directive = module_directive(&program);
    if directive.is_none() && is_app_file {
        if let Some(reason) = inferred_client(&program, &exports) {
            crate::term::warn_once(
                &format!("inferred-client:{rel}"),
                &format!("{rel} uses {reason} without \"use client\": it runs as a Client Component. Add \"use client\" at the top of the file to say so."),
            );
            directive = Some(Directive::UseClient);
        }
    }
    let mut out = CompiledModule { directive, exports: exports.clone(), has_star_export, ..Default::default() };

    // React Server Components never execute client modules: they reference them
    if opts.layer == Layer::Rsc && directive == Some(Directive::UseClient) {
        let id = client_reference_id(opts.root, opts.file, opts.package_specifier);
        if has_star_export {
            crate::term::warn(&format!(
                "`export *` in the \"use client\" module {rel} cannot be referenced from Server Components; export the names explicitly"
            ));
        }
        out.code = client_reference_stub(opts.root, opts.file, &id, &exports)
            .map_err(|message| CompileError { file: file_name.clone(), message, line: None, column: None, frame: String::new() })?;
        out.is_reference_stub = true;
        return Ok(out);
    }

    if opts.layer == Layer::Rsc && is_app_file {
        check_server_react_imports(&cm, &program, source, &file_name)?;
    }

    // Outside the react-server layer, a "use server" module is only a set of references
    if opts.layer != Layer::Rsc && directive == Some(Directive::UseServer) {
        let refs = server_actions::reference_stub(opts.root, opts.file, &rel, opts.layer, &exports)
            .map_err(|message| CompileError { file: file_name.clone(), message, line: None, column: None, frame: String::new() })?;
        out.code = refs.code;
        out.actions = refs.actions;
        out.imports = refs.imports;
        out.is_reference_stub = true;
        return Ok(out);
    }

    let is_jsx = !(rel.ends_with(".ts") || rel.ends_with(".mts") || rel.ends_with(".cts")) || source.contains("</") || source.contains("/>");
    let resolvers = oxc::resolvers(opts.root);

    let globals = Globals::default();
    GLOBALS.set(&globals, || {
        let unresolved_mark = Mark::new();
        let top_level_mark = Mark::new();

        if let Program::Module(module) = &mut program {
            let importer = opts.file.to_string_lossy().replace('\\', "/");
            let (fonts, errors) = fonts::transform(&cm, module, &importer);
            for error in errors {
                crate::term::warn_once(&format!("font:{rel}:{error}"), &format!("{rel}: {error}"));
            }
            out.fonts = fonts;
            if opts.layer == Layer::Client {
                proxy_legacy_rpc_imports(&cm, module, opts.file, &rel, opts.root);
            }
            barrel::optimize_imports(module, opts.file, opts.root, opts.layer, &resolvers);
        }

        program.visit_mut_with(&mut resolver(unresolved_mark, top_level_mark, true));
        typescript(Default::default(), unresolved_mark, top_level_mark).process(&mut program);

        // Server actions: registered in the react-server layer, file-level and inline
        if opts.layer == Layer::Rsc {
            out.actions = server_actions::register_actions(
                &cm,
                &mut program,
                &rel,
                directive == Some(Directive::UseServer),
                top_level_mark,
                unresolved_mark,
            );
            let runtime = opts.root.join(".nata/next/cache-runtime.ts").to_string_lossy().replace('\\', "/");
            use_cache::transform(&cm, &mut program, &rel, &runtime, top_level_mark, unresolved_mark);
        }

        if is_jsx {
            let options = ReactOptions {
                runtime: Some(Runtime::Automatic),
                development: Some(opts.development),
                refresh: (opts.refresh && opts.layer == Layer::Client).then(RefreshOptions::default),
                ..Default::default()
            };
            react::<NoopComments>(cm.clone(), None, options, top_level_mark, unresolved_mark).process(&mut program);
        }

        if opts.layer == Layer::Client {
            program.visit_mut_with(&mut InlineEnv {
                public: crate::env::public(opts.root),
                node_env: if crate::deps::deps_node_env() == "production" { "production" } else { "development" },
            });
            if let Program::Module(module) = &mut program {
                let mut requires = ClientRequire {
                    file: opts.file,
                    resolvers: &resolvers,
                    unresolved: swc_core::common::SyntaxContext::empty().apply_mark(unresolved_mark),
                    hoisted: Vec::new(),
                };
                module.visit_mut_with(&mut requires);
                let imports = requires.imports();
                module.body.splice(0..0, imports);
            }
        }

        let mut rewriter = ImportRewriter {
            root: opts.root,
            file: opts.file.to_path_buf(),
            rel: rel.clone(),
            layer: opts.layer,
            resolvers: &resolvers,
            tsconfig: None,
            imports: Vec::new(),
            package_imports: Vec::new(),
            missing_packages: Vec::new(),
        };
        program.visit_mut_with(&mut rewriter);
        out.imports = rewriter.imports;
        out.package_imports = rewriter.package_imports;
        out.missing_packages = rewriter.missing_packages;

        out.code = emit(&cm, &program, &file_name)?;
        if opts.refresh && opts.layer == Layer::Client && is_jsx {
            out.code = with_refresh_registration(&out.code, &format!("/_bundle/{}", rel));
        }
        Ok(out)
    })
}

/// Whether hot-updating this browser module can keep React state: every export is a component
/// (PascalCase) or the default export
pub fn is_refresh_boundary(exports: &[String], directive: Option<Directive>) -> bool {
    directive != Some(Directive::UseServer)
        && !exports.is_empty()
        && exports.iter().all(|e| e == "default" || e.chars().next().is_some_and(|c| c.is_ascii_uppercase()))
}

/// Points the `$RefreshReg$` / `$RefreshSig$` calls emitted by SWC's refresh transform at the React
/// Refresh runtime (`/_nata/refresh.js`), with component ids prefixed by the module's URL so a new
/// version of the module (`?v=`) updates the same component families
fn with_refresh_registration(code: &str, url: &str) -> String {
    let url = serde_json::to_string(url).unwrap();
    format!(
        "const __nata_rr = self.__NATA_REFRESH__, __nata_prev_reg = self.$RefreshReg$, __nata_prev_sig = self.$RefreshSig$;\n\
         self.$RefreshReg$ = __nata_rr ? (type, id) => __nata_rr.register(type, {url} + \" \" + id) : () => {{}};\n\
         self.$RefreshSig$ = __nata_rr ? __nata_rr.createSignatureFunctionForTransform : () => (type) => type;\n\
         {code}\n\
         self.$RefreshReg$ = __nata_prev_reg;\n\
         self.$RefreshSig$ = __nata_prev_sig;\n"
    )
}
