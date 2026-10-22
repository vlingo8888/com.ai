//! `.nata/build/manifest.json`: what `com build` produced and `com start` serves.
//!
//! - `client`: client reference id (the URL the module had in development, `/_bundle/app/x.tsx`,
//!   `/_nata/deps/pkg`) → bundled file under `/_nata/static/`. Flight payloads keep the development
//!   ids; the browser maps them through this table (`head_globals`).
//! - `pages`: prerendered documents and Flight payloads (`○` static, `◐` revalidated).
//! - `routes`: every route with how it renders, for the build report and `com start`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use serde::{Deserialize, Serialize};

pub const BUILD_DIR: &str = ".nata/build";
pub const STATIC_URL: &str = "/_nata/static/";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildManifest {
    pub build_id: String,
    /// Bundled client router (the document's bootstrap module)
    pub router: String,
    /// Chunks the router imports statically (`<link rel="modulepreload">`)
    pub router_preload: Vec<String>,
    pub client: BTreeMap<String, String>,
    /// Minified project stylesheet, relative to the build directory
    pub styles: Option<String>,
    pub routes: Vec<RouteSummary>,
    pub pages: BTreeMap<String, PrerenderedPage>,
    /// The app has intercepting routes: Flight payloads depend on where a navigation comes from, so
    /// they are never served from the page cache
    pub intercepting: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RouteKind {
    /// Prerendered, served as is (`○`)
    Static,
    /// Prerendered, regenerated in the background after `revalidate` seconds (`◐` ISR)
    Isr,
    /// Rendered for every request (`ƒ`)
    Dynamic,
    /// Route handler (`route.ts`)
    Api,
}

impl RouteKind {
    pub fn symbol(self) -> &'static str {
        match self {
            RouteKind::Static => "○",
            RouteKind::Isr => "◐",
            RouteKind::Dynamic | RouteKind::Api => "ƒ",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteSummary {
    pub pattern: String,
    pub kind: RouteKind,
    /// Why the route is dynamic (`cookies()`, `searchParams`...)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// Prerendered paths of this route
    #[serde(default)]
    pub paths: Vec<String>,
    /// JavaScript the browser loads for the route's first visit, gzip bytes
    #[serde(default)]
    pub first_load_js: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrerenderedPage {
    pub status: u16,
    /// Files relative to the build directory
    pub html: String,
    pub flight: Option<String>,
    pub revalidate: Option<u64>,
    #[serde(default)]
    pub tags: Vec<String>,
}

pub fn build_dir(root: &Path) -> PathBuf {
    root.join(BUILD_DIR)
}

pub fn load(root: &Path) -> Result<BuildManifest, String> {
    let file = build_dir(root).join("manifest.json");
    let text = std::fs::read_to_string(&file).map_err(|_| format!("No production build in {}: run `com build` first", build_dir(root).display()))?;
    serde_json::from_str(&text).map_err(|e| format!("{}: {e}", file.display()))
}

pub fn save(root: &Path, manifest: &BuildManifest) -> Result<(), String> {
    let text = serde_json::to_string_pretty(manifest).map_err(|e| e.to_string())?;
    std::fs::write(build_dir(root).join("manifest.json"), text).map_err(|e| e.to_string())
}

fn current_slot() -> &'static RwLock<Option<Arc<BuildManifest>>> {
    static M: RwLock<Option<Arc<BuildManifest>>> = RwLock::new(None);
    &M
}

/// The build being served (`com start`) or prerendered (`com build`, once the client is bundled)
pub fn current() -> Option<Arc<BuildManifest>> {
    current_slot().read().unwrap_or_else(|e| e.into_inner()).clone()
}

pub fn install(manifest: BuildManifest) {
    *current_slot().write().unwrap_or_else(|e| e.into_inner()) = Some(Arc::new(manifest));
}

/// The built stylesheet, read once
pub fn styles(root: &Path) -> Option<Arc<str>> {
    static CSS: RwLock<Option<(String, Arc<str>)>> = RwLock::new(None);
    let manifest = current()?;
    let file = manifest.styles.as_ref()?;
    if let Some((id, css)) = CSS.read().unwrap_or_else(|e| e.into_inner()).as_ref() {
        if *id == manifest.build_id {
            return Some(css.clone());
        }
    }
    let css: Arc<str> = std::fs::read_to_string(build_dir(root).join(file)).ok()?.into();
    *CSS.write().unwrap_or_else(|e| e.into_inner()) = Some((manifest.build_id.clone(), css.clone()));
    Some(css)
}

/// Document globals of a production build: client reference ids load their bundled file, and the
/// router's chunks are preloaded
pub fn head_globals(manifest: &BuildManifest) -> String {
    let chunks = serde_json::to_string(&manifest.client).unwrap_or_else(|_| "{}".into()).replace("</", "<\\/");
    let mut out = String::new();
    for url in std::iter::once(&manifest.router).chain(manifest.router_preload.iter()) {
        out.push_str(&format!("<link rel=\"modulepreload\" href=\"{url}\">"));
    }
    out.push_str(&format!(
        "<script>(function(){{var c={chunks},m=self.__nata_modules=new Map();self.__webpack_chunk_load__=function(id){{return import(c[id]||id).then(function(x){{m.set(id,x)}})}};self.__webpack_require__=function(id){{return m.get(id)}};self.__webpack_require__.u=function(c){{return c}};self.__webpack_get_script_filename__=function(c){{return c}}}})();</script>"
    ));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn head_globals_map_client_ids_to_bundles() {
        let mut manifest = BuildManifest { router: "/_nata/static/router-a1.js".into(), router_preload: vec!["/_nata/static/chunk-b2.js".into()], ..Default::default() };
        manifest.client.insert("/_bundle/app/counter.tsx".into(), "/_nata/static/counter-c3.js".into());
        let head = head_globals(&manifest);
        assert!(head.contains(r#"<link rel="modulepreload" href="/_nata/static/router-a1.js">"#));
        assert!(head.contains(r#"<link rel="modulepreload" href="/_nata/static/chunk-b2.js">"#));
        assert!(head.contains(r#""/_bundle/app/counter.tsx":"/_nata/static/counter-c3.js""#));
        assert!(head.contains("import(c[id]||id)"));
    }

    #[test]
    fn manifest_round_trips_with_camel_case_keys() {
        let mut manifest = BuildManifest { build_id: "b1".into(), ..Default::default() };
        manifest.routes.push(RouteSummary { pattern: "/".into(), kind: RouteKind::Isr, reason: None, paths: vec!["/".into()], first_load_js: 1200 });
        let text = serde_json::to_string(&manifest).unwrap();
        assert!(text.contains("\"buildId\":\"b1\"") && text.contains("\"kind\":\"isr\"") && text.contains("\"firstLoadJs\":1200"));
        let back: BuildManifest = serde_json::from_str(&text).unwrap();
        assert_eq!(back.routes[0].kind, RouteKind::Isr);
    }
}
