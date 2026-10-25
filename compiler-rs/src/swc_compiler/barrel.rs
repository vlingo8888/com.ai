//! Barrel imports of packages, optimized like Next.js `optimizePackageImports`:
//! `import { Star } from "lucide-react"` loads the module that defines `Star` instead of the package
//! entry, which re-exports ~2,000 icons that the worker would otherwise compile and load one by one,
//! and that the browser would download as one bundle production builds cannot tree-shake.
//!
//! Only for a package entry that does nothing but re-export (no `"use client"` / `"use server"`),
//! of a package that declares `"sideEffects": false` or is listed in next.config
//! `experimental.optimizePackageImports`. Server layers import the file by path; the browser by its
//! package subpath (`lucide-react/dist/esm/icons/star.mjs`), so only for packages without an
//! `exports` map, whose files are all importable. The dependency bundler then keeps the package's
//! files separate modules (deps_bundler.ts), so code they share loads once.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::SystemTime;

use swc_core::common::{sync::Lrc, SourceMap, DUMMY_SP};
use swc_core::ecma::ast::*;

use super::module::{export_name, module_directive, module_exports, parse};
use crate::resolver::oxc::{self, LayerResolvers};
use crate::resolver::Layer;

/// Where a re-exported name comes from
#[derive(Debug, Clone, PartialEq)]
enum Origin {
    /// `name` of `file`
    Named(PathBuf, String),
    /// A namespace (`export * as x from`, `import * as x; export { x }`): left to the entry
    Opaque,
}

type Index = HashMap<String, Origin>;

/// `export * from` chains followed below an entry
const MAX_DEPTH: usize = 8;

/// The re-exports of a barrel file, `None` when the file is not a pure barrel. Cached by path, layer
/// and mtime: an entry like lucide-react's is parsed once, whatever the number of importers.
fn barrel_index(resolvers: &LayerResolvers, layer: Layer, file: &Path, depth: usize) -> Option<Arc<Index>> {
    type Cache = Mutex<HashMap<(PathBuf, Layer), (Option<SystemTime>, Option<Arc<Index>>)>>;
    static CACHE: OnceLock<Cache> = OnceLock::new();
    let mtime = std::fs::metadata(file).and_then(|m| m.modified()).ok();
    let key = (file.to_path_buf(), layer);
    let cache = CACHE.get_or_init(Default::default);
    if let Some((cached_mtime, index)) = cache.lock().unwrap_or_else(|e| e.into_inner()).get(&key) {
        if *cached_mtime == mtime {
            return index.clone();
        }
    }
    let index = build_index(resolvers, layer, file, depth).map(Arc::new);
    cache.lock().unwrap_or_else(|e| e.into_inner()).insert(key, (mtime, index.clone()));
    index
}

fn build_index(resolvers: &LayerResolvers, layer: Layer, file: &Path, depth: usize) -> Option<Index> {
    let source = std::fs::read_to_string(file).ok()?;
    let cm: Lrc<SourceMap> = Default::default();
    let program = parse(&cm, &source, &file.to_string_lossy()).ok()?;
    if module_directive(&program).is_some() {
        return None;
    }
    let Program::Module(module) = &program else { return None };
    let resolve = |src: &Str| -> Option<PathBuf> {
        let resolved = resolvers.resolve(layer, file, &src.value.to_string_lossy())?;
        resolved.in_node_modules.then_some(resolved.path)
    };

    // Explicit exports win over `export *` ones, whatever their order (ES module semantics)
    let mut index = Index::new();
    let mut stars: Vec<PathBuf> = Vec::new();
    let mut imported: HashMap<String, Origin> = HashMap::new();
    let mut local_exports: Vec<(String, String)> = Vec::new();
    for item in &module.body {
        match item {
            // "use strict" and other directive prologues
            ModuleItem::Stmt(Stmt::Expr(ExprStmt { expr, .. })) if matches!(&**expr, Expr::Lit(Lit::Str(_))) => {}
            ModuleItem::Stmt(Stmt::Empty(_)) => {}
            ModuleItem::ModuleDecl(ModuleDecl::Import(import)) => {
                if import.type_only {
                    continue;
                }
                let target = resolve(&import.src)?;
                for spec in &import.specifiers {
                    let (local, origin) = match spec {
                        ImportSpecifier::Named(n) => {
                            let name = n.imported.as_ref().map(export_name).unwrap_or_else(|| n.local.sym.to_string());
                            (n.local.sym.to_string(), Origin::Named(target.clone(), name))
                        }
                        ImportSpecifier::Default(d) => (d.local.sym.to_string(), Origin::Named(target.clone(), "default".into())),
                        ImportSpecifier::Namespace(n) => (n.local.sym.to_string(), Origin::Opaque),
                    };
                    imported.insert(local, origin);
                }
            }
            ModuleItem::ModuleDecl(ModuleDecl::ExportNamed(named)) => {
                if named.type_only {
                    continue;
                }
                let target = match &named.src {
                    Some(src) => Some(resolve(src)?),
                    None => None,
                };
                for spec in &named.specifiers {
                    match spec {
                        ExportSpecifier::Named(s) if s.is_type_only => {}
                        ExportSpecifier::Named(s) => {
                            let exported = export_name(s.exported.as_ref().unwrap_or(&s.orig));
                            match &target {
                                Some(target) => {
                                    index.insert(exported, Origin::Named(target.clone(), export_name(&s.orig)));
                                }
                                None => local_exports.push((exported, export_name(&s.orig))),
                            }
                        }
                        ExportSpecifier::Namespace(ns) => {
                            index.insert(export_name(&ns.name), Origin::Opaque);
                        }
                        ExportSpecifier::Default(d) => {
                            index.insert(d.exported.sym.to_string(), Origin::Opaque);
                        }
                    }
                }
            }
            ModuleItem::ModuleDecl(ModuleDecl::ExportAll(all)) => {
                if !all.type_only {
                    stars.push(resolve(&all.src)?);
                }
            }
            // Anything else (declarations, side effects): not a barrel
            _ => return None,
        }
    }
    // `import { a } from "./a"; export { a }`: every exported local must be an import
    for (exported, local) in local_exports {
        index.insert(exported, imported.get(&local)?.clone());
    }
    for star in stars {
        let names: Vec<(String, Origin)> = match (depth < MAX_DEPTH).then(|| barrel_index(resolvers, layer, &star, depth + 1)).flatten() {
            Some(nested) => nested.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
            None => {
                let source = std::fs::read_to_string(&star).ok()?;
                let program = parse(&cm, &source, &star.to_string_lossy()).ok()?;
                let (names, has_star) = module_exports(&program);
                // A module with its own `export *` and code: its full export list is unknown
                if has_star {
                    return None;
                }
                names.into_iter().map(|n| (n.clone(), Origin::Named(star.clone(), n))).collect()
            }
        };
        for (name, origin) in names {
            if name != "default" {
                index.entry(name).or_insert(origin);
            }
        }
    }
    Some(index)
}

/// The nearest `package.json` above `file` (the package the entry belongs to) and its directory
fn package_manifest(file: &Path) -> Option<(PathBuf, serde_json::Value)> {
    file.ancestors().skip(1).take_while(|dir| dir.file_name().is_some_and(|n| n != "node_modules")).find_map(|dir| {
        let manifest: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join("package.json")).ok()?).ok()?;
        manifest["name"].is_string().then(|| (dir.to_path_buf(), manifest))
    })
}

/// Whether the package of `entry` may be imported module by module
fn optimizable(root: &Path, specifier: &str, entry: &Path) -> bool {
    let Some((_, manifest)) = package_manifest(entry) else { return false };
    if manifest["sideEffects"] == serde_json::Value::Bool(false) {
        return true;
    }
    let name = manifest["name"].as_str().unwrap_or_default();
    crate::next_config::for_root(root).is_some_and(|config| {
        config["experimental"]["optimizePackageImports"]
            .as_array()
            .is_some_and(|list| list.iter().filter_map(|p| p.as_str()).any(|p| p == name || p == specifier))
    })
}

/// What `layer` imports `file` of a package by: its path on the server, its package subpath in the
/// browser (`None` when the package's `exports` hide its files)
fn origin_specifier(layer: Layer, file: &Path) -> Option<String> {
    if layer != Layer::Client {
        return Some(file.to_string_lossy().replace('\\', "/"));
    }
    let (dir, manifest) = package_manifest(file)?;
    if !manifest["exports"].is_null() {
        return None;
    }
    let rel = file.strip_prefix(&dir).ok()?.to_string_lossy().replace('\\', "/");
    Some(format!("{}/{rel}", manifest["name"].as_str()?))
}

fn import_decl(specifier: ImportSpecifier, src: &str) -> ModuleItem {
    ModuleItem::ModuleDecl(ModuleDecl::Import(ImportDecl {
        span: DUMMY_SP,
        specifiers: vec![specifier],
        src: Box::new(Str { span: DUMMY_SP, value: src.into(), raw: None }),
        type_only: false,
        with: None,
        phase: Default::default(),
    }))
}

/// Rewrites the named imports of optimizable package barrels in `module` to the modules that define
/// them (see `origin_specifier`), resolved again by the import rewriter like any import
pub fn optimize_imports(module: &mut Module, file: &Path, root: &Path, layer: Layer, resolvers: &LayerResolvers) {
    let mut body = Vec::with_capacity(module.body.len());
    for item in std::mem::take(&mut module.body) {
        let ModuleItem::ModuleDecl(ModuleDecl::Import(mut import)) = item else {
            body.push(item);
            continue;
        };
        let specifier = import.src.value.to_string_lossy().to_string();
        let candidate = !import.type_only
            && import.with.is_none()
            && oxc::is_bare(&specifier)
            && !specifier.starts_with("@/")
            && !specifier.starts_with("~/")
            && import.specifiers.iter().any(|s| matches!(s, ImportSpecifier::Named(n) if !n.is_type_only));
        let index = candidate
            .then(|| resolvers.resolve(layer, file, &specifier))
            .flatten()
            .filter(|entry| entry.in_node_modules && optimizable(root, &specifier, &entry.path))
            .and_then(|entry| barrel_index(resolvers, layer, &entry.path, 0));
        let Some(index) = index else {
            body.push(ModuleItem::ModuleDecl(ModuleDecl::Import(import)));
            continue;
        };
        let mut kept = Vec::new();
        for spec in std::mem::take(&mut import.specifiers) {
            let ImportSpecifier::Named(named) = &spec else {
                kept.push(spec);
                continue;
            };
            let name = named.imported.as_ref().map(export_name).unwrap_or_else(|| named.local.sym.to_string());
            let target = match index.get(&name) {
                Some(Origin::Named(origin, orig)) if !named.is_type_only => origin_specifier(layer, origin).map(|src| (src, orig)),
                _ => None,
            };
            match target {
                Some((src, orig)) => {
                    let rewritten = if orig == "default" {
                        ImportSpecifier::Default(ImportDefaultSpecifier { span: DUMMY_SP, local: named.local.clone() })
                    } else {
                        ImportSpecifier::Named(ImportNamedSpecifier {
                            span: DUMMY_SP,
                            local: named.local.clone(),
                            imported: (*orig != named.local.sym.to_string()).then(|| ModuleExportName::Ident(Ident::new_no_ctxt(orig.as_str().into(), DUMMY_SP))),
                            is_type_only: false,
                        })
                    };
                    body.push(import_decl(rewritten, &src));
                }
                None => kept.push(spec),
            }
        }
        if !kept.is_empty() {
            import.specifiers = kept;
            body.push(ModuleItem::ModuleDecl(ModuleDecl::Import(import)));
        }
    }
    module.body = body;
}
