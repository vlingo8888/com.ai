//! `next/font/google` and `next/font/local`, like Next.js: font loader calls take literal options and
//! are replaced at compile time by the object they return (`className`, `style`, `variable`), so every
//! layer (server, SSR, browser) gets identical class names. Each call is recorded as a
//! `FontRequest`; the engine downloads / serves the fonts and their CSS (`font_loader.rs`).

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use swc_core::common::{sync::Lrc, SourceMap};
use swc_core::ecma::{
    ast::*,
    visit::{VisitMut, VisitMutWith},
};

use super::module::parse_items;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FontRequest {
    pub hash: String,
    /// `google` or `local`
    pub kind: String,
    pub family: String,
    pub options: Value,
    /// Absolute path of the module calling the loader (`local` sources are relative to it)
    pub importer: String,
}

impl FontRequest {
    /// The `font-family` name of the loaded font
    pub fn family_name(&self) -> String {
        let base = if self.kind == "local" { "Local".to_string() } else { self.family.replace(' ', "_") };
        format!("__{}_{}", base, self.hash)
    }

    pub fn fallback_name(&self) -> String {
        let base = if self.kind == "local" { "Local".to_string() } else { self.family.replace(' ', "_") };
        format!("__{}_Fallback_{}", base, self.hash)
    }

    /// `font-family` value used by the class and the CSS variable
    pub fn font_family(&self) -> String {
        let mut list = vec![format!("'{}'", self.family_name()), format!("'{}'", self.fallback_name())];
        for f in self.options["fallback"].as_array().into_iter().flatten().filter_map(|f| f.as_str()) {
            list.push(if f.contains(' ') { format!("'{f}'") } else { f.to_string() });
        }
        list.join(", ")
    }

    pub fn class_name(&self) -> String {
        format!("__className_{}", self.hash)
    }

    pub fn variable_class(&self) -> Option<String> {
        self.options["variable"].as_str().map(|_| format!("__variable_{}", self.hash))
    }

    fn single(&self, key: &str) -> Option<String> {
        match &self.options[key] {
            Value::String(s) if s != "variable" => Some(s.clone()),
            Value::Array(a) if a.len() == 1 => a[0].as_str().map(String::from),
            _ if key == "weight" => self.options["src"].as_array().filter(|a| a.len() == 1).and_then(|a| a[0]["weight"].as_str().map(String::from)),
            _ => None,
        }
    }

    /// The object the loader call evaluates to
    pub fn result(&self) -> Value {
        let mut style = Map::new();
        style.insert("fontFamily".into(), json!(self.font_family()));
        if let Some(w) = self.single("weight") {
            style.insert("fontWeight".into(), w.parse::<u32>().map(|n| json!(n)).unwrap_or(json!(w)));
        }
        if let Some(s) = self.single("style") {
            style.insert("fontStyle".into(), json!(s));
        }
        let mut out = Map::new();
        out.insert("className".into(), json!(self.class_name()));
        out.insert("style".into(), Value::Object(style));
        if let Some(v) = self.variable_class() {
            out.insert("variable".into(), json!(v));
        }
        Value::Object(out)
    }
}

fn lit_to_json(expr: &Expr) -> Option<Value> {
    Some(match expr {
        Expr::Lit(Lit::Str(s)) => json!(s.value.to_string_lossy()),
        Expr::Lit(Lit::Num(n)) => json!(n.value),
        Expr::Lit(Lit::Bool(b)) => json!(b.value),
        Expr::Lit(Lit::Null(_)) => Value::Null,
        Expr::Tpl(t) if t.exprs.is_empty() => json!(t.quasis.iter().map(|q| q.raw.to_string()).collect::<String>()),
        Expr::Array(a) => Value::Array(a.elems.iter().map(|e| e.as_ref().and_then(|e| lit_to_json(&e.expr)).unwrap_or(Value::Null)).collect()),
        Expr::Object(o) => {
            let mut map = Map::new();
            for prop in &o.props {
                let PropOrSpread::Prop(prop) = prop else { return None };
                let Prop::KeyValue(kv) = &**prop else { return None };
                let key = match &kv.key {
                    PropName::Ident(i) => i.sym.to_string(),
                    PropName::Str(s) => s.value.to_string_lossy().to_string(),
                    _ => return None,
                };
                map.insert(key, lit_to_json(&kv.value)?);
            }
            Value::Object(map)
        }
        Expr::Paren(p) => lit_to_json(&p.expr)?,
        Expr::TsAs(a) => lit_to_json(&a.expr)?,
        Expr::TsConstAssertion(a) => lit_to_json(&a.expr)?,
        Expr::TsSatisfies(a) => lit_to_json(&a.expr)?,
        _ => return None,
    })
}

struct Loaders<'a> {
    cm: &'a Lrc<SourceMap>,
    bindings: Vec<(swc_core::atoms::Atom, String, String)>,
    importer: String,
    requests: Vec<FontRequest>,
    errors: Vec<String>,
}

impl VisitMut for Loaders<'_> {
    fn visit_mut_expr(&mut self, n: &mut Expr) {
        n.visit_mut_children_with(self);
        let Expr::Call(call) = n else { return };
        let Callee::Expr(callee) = &call.callee else { return };
        let Expr::Ident(ident) = &**callee else { return };
        let Some((_, kind, family)) = self.bindings.iter().find(|(sym, _, _)| *sym == ident.sym).cloned() else { return };
        let options = match call.args.first() {
            None => json!({}),
            Some(arg) => match lit_to_json(&arg.expr) {
                Some(v) => v,
                None => {
                    self.errors.push(format!("Font loader values must be explicitly written literals ({family})"));
                    json!({})
                }
            },
        };
        let canonical = serde_json::to_string(&options).unwrap_or_default();
        let scope = if kind == "local" { self.importer.clone() } else { String::new() };
        let hash = blake3::hash(format!("{kind}|{family}|{canonical}|{scope}").as_bytes()).to_hex()[..8].to_string();
        let request = FontRequest { hash, kind, family, options, importer: self.importer.clone() };
        let code = format!("const __font = {};", serde_json::to_string(&request.result()).unwrap());
        if let Some(ModuleItem::Stmt(Stmt::Decl(Decl::Var(var)))) = parse_items(self.cm, &code).into_iter().next() {
            if let Some(init) = var.decls.into_iter().next().and_then(|d| d.init) {
                *n = *init;
            }
        }
        self.requests.push(request);
    }
}

/// Replaces font loader imports and calls; returns the fonts the module loads and any errors
pub fn transform(cm: &Lrc<SourceMap>, module: &mut Module, importer: &str) -> (Vec<FontRequest>, Vec<String>) {
    let mut bindings = Vec::new();
    module.body.retain(|item| {
        let ModuleItem::ModuleDecl(ModuleDecl::Import(import)) = item else { return true };
        let src = import.src.value.to_string_lossy();
        let kind = match src.as_ref() {
            "next/font/google" | "@next/font/google" => "google",
            "next/font/local" | "@next/font/local" => "local",
            _ => return true,
        };
        for spec in &import.specifiers {
            match spec {
                ImportSpecifier::Named(n) => {
                    let imported = match &n.imported {
                        Some(ModuleExportName::Ident(i)) => i.sym.to_string(),
                        Some(ModuleExportName::Str(s)) => s.value.to_string_lossy().to_string(),
                        None => n.local.sym.to_string(),
                    };
                    bindings.push((n.local.sym.clone(), kind.to_string(), imported.replace('_', " ")));
                }
                ImportSpecifier::Default(d) => bindings.push((d.local.sym.clone(), kind.to_string(), "local".to_string())),
                ImportSpecifier::Namespace(_) => {}
            }
        }
        false
    });
    if bindings.is_empty() {
        return (vec![], vec![]);
    }
    let mut loaders = Loaders { cm, bindings, importer: importer.to_string(), requests: vec![], errors: vec![] };
    module.visit_mut_with(&mut loaders);
    (loaders.requests, loaders.errors)
}
