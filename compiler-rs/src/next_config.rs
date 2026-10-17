//! `next.config.(ts|mjs|js|cjs)`: loaded by the worker (it may be TypeScript, an async function, or
//! import other modules) and kept here as plain JSON. Reloaded when the file changes.
//!
//! Applied by the engine: `basePath`, `trailingSlash`, `redirects`, `rewrites`, `headers`, `env`
//! (inlined like `NEXT_PUBLIC_*`), `images` (next/image). Options the engine does not implement are
//! reported once.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use serde_json::Value;

pub fn find(root: &Path) -> Option<PathBuf> {
    ["next.config.ts", "next.config.mts", "next.config.mjs", "next.config.js", "next.config.cjs"]
        .iter()
        .map(|n| root.join(n))
        .find(|p| p.is_file())
}

fn registry() -> &'static Mutex<HashMap<PathBuf, Arc<Value>>> {
    static R: OnceLock<Mutex<HashMap<PathBuf, Arc<Value>>>> = OnceLock::new();
    R.get_or_init(Default::default)
}

fn key(root: &Path) -> PathBuf {
    root.canonicalize().unwrap_or_else(|_| root.to_path_buf())
}

/// The loaded config, if the project has one and it loaded
pub fn for_root(root: &Path) -> Option<Arc<Value>> {
    registry().lock().ok()?.get(&key(root)).cloned()
}

fn first_load(root: &Path) -> Arc<tokio::sync::OnceCell<()>> {
    static F: OnceLock<Mutex<HashMap<PathBuf, Arc<tokio::sync::OnceCell<()>>>>> = OnceLock::new();
    F.get_or_init(Default::default).lock().unwrap().entry(key(root)).or_default().clone()
}

/// The config, waiting for the first load of this project's config (requests can arrive while the
/// load started at boot is still running); later reloads replace it in place
pub async fn ensure_loaded(root: &Path) -> Option<Arc<Value>> {
    first_load(root).get_or_init(|| async { load(root).await; }).await;
    for_root(root)
}

/// Options this engine understands; anything else gets a one-time notice
const SUPPORTED: [&str; 17] = [
    "basePath", "trailingSlash", "redirects", "rewrites", "headers", "env", "images", "reactStrictMode",
    "experimental", "typescript", "eslint", "poweredByHeader", "serverExternalPackages", "output",
    "transpilePackages", "compiler", "devIndicators",
];

/// (Re)loads the config through the worker; `None` when there is none or it failed
pub async fn load(root: &Path) -> Option<Arc<Value>> {
    let root = key(root);
    let Some(file) = find(&root) else {
        registry().lock().ok()?.remove(&root);
        return None;
    };
    let specifier = crate::module_graph::server_module_specifier(&root, &file).unwrap_or_else(|| file.to_string_lossy().to_string());
    let reply = crate::worker_pool::call(&root, "next_config", serde_json::json!({ "specifier": specifier }), Vec::new()).await.ok()?;
    crate::ssr::worker::print_logs(reply.stdout(), reply.stderr());
    if let Some(err) = reply.error() {
        crate::term::error("Could not load next.config", Some(err), None);
        return None;
    }
    let config = Arc::new(reply.output().clone());
    if let Some(obj) = config.as_object() {
        let unknown: Vec<&String> = obj.keys().filter(|k| !SUPPORTED.contains(&k.as_str())).collect();
        if !unknown.is_empty() {
            let list = unknown.iter().map(|k| k.as_str()).collect::<Vec<_>>().join(", ");
            crate::term::warn_once(&format!("next-config:{list}"), &format!("next.config options not supported by com dev (ignored): {list}"));
        }
        if obj.get("output").and_then(|o| o.as_str()).is_some() {
            crate::term::warn_once("next-config:output", "next.config `output` applies to `com build` (ROADMAP P3)");
        }
    }
    registry().lock().ok()?.insert(root, config.clone());
    Some(config)
}

/// `basePath` without a trailing slash ("" when unset)
pub fn base_path(root: &Path) -> String {
    for_root(root).and_then(|c| c["basePath"].as_str().map(|s| s.trim_end_matches('/').to_string())).unwrap_or_default()
}

// ---------- routing: basePath, trailingSlash, headers, redirects, rewrites ----------

use axum::http::HeaderMap;
use regex::Regex;

use crate::middleware::{conditions_hold, parse_conditions, Condition, RequestInfo};

/// A compiled `source` with its named parameters (`/blog/:slug`, `/docs/:path*`, `/:id(\\d+)`)
pub struct RouteSource {
    regex: Regex,
    names: Vec<String>,
}

impl RouteSource {
    pub fn compile(source: &str) -> Result<Self, String> {
        let chars: Vec<char> = source.chars().collect();
        let mut out = String::from("^");
        let mut names = Vec::new();
        let mut i = 0;
        let mut unnamed = 0;
        while i < chars.len() {
            let c = chars[i];
            let slash = c == '/' && chars.get(i + 1) == Some(&':');
            if slash || (c == ':' && (i == 0 || chars[i - 1] != '/')) {
                let mut j = i + if slash { 2 } else { 1 };
                let mut name = String::new();
                while j < chars.len() && (chars[j].is_alphanumeric() || chars[j] == '_') {
                    name.push(chars[j]);
                    j += 1;
                }
                let (pattern, after) = if chars.get(j) == Some(&'(') { crate::middleware::read_group(&chars, j)? } else { ("[^/]+?".to_string(), j) };
                let modifier = chars.get(after).copied().filter(|m| matches!(m, '*' | '+' | '?'));
                let group = format!("p{}", names.len());
                names.push(name);
                let prefix = if slash { "/" } else { "" };
                out.push_str(&match modifier {
                    Some('*') => format!("(?:{prefix}(?P<{group}>(?:{pattern})(?:/(?:{pattern}))*))?"),
                    Some('+') => format!("{prefix}(?P<{group}>(?:{pattern})(?:/(?:{pattern}))*)"),
                    Some('?') => format!("(?:{prefix}(?P<{group}>{pattern}))?"),
                    _ => format!("{prefix}(?P<{group}>{pattern})"),
                });
                i = after + modifier.map_or(0, |_| 1);
                continue;
            }
            if c == '(' {
                let (pattern, after) = crate::middleware::read_group(&chars, i)?;
                let group = format!("p{}", names.len());
                names.push(unnamed.to_string());
                unnamed += 1;
                out.push_str(&format!("(?P<{group}>{pattern})"));
                i = after;
                continue;
            }
            out.push_str(&regex::escape(&c.to_string()));
            i += 1;
        }
        out.push_str("/?$");
        Ok(Self { regex: Regex::new(&out).map_err(|e| format!("invalid source {source:?}: {e}"))?, names })
    }

    /// Named parameters of `path`, when it matches
    pub fn matches(&self, path: &str) -> Option<Vec<(String, String)>> {
        let caps = self.regex.captures(path)?;
        Some(
            self.names
                .iter()
                .enumerate()
                .map(|(i, name)| (name.clone(), caps.name(&format!("p{i}")).map(|m| m.as_str().to_string()).unwrap_or_default()))
                .collect(),
        )
    }
}

/// `/blog/:slug` with slug=x → `/blog/x` (also in the query string and host of external URLs)
pub fn substitute(destination: &str, params: &[(String, String)]) -> String {
    let mut sorted: Vec<&(String, String)> = params.iter().collect();
    // Longer names first, so `:slug` does not eat `:slugs`
    sorted.sort_by(|a, b| b.0.len().cmp(&a.0.len()));
    let mut out = destination.to_string();
    for (name, value) in sorted {
        let token = format!(":{name}");
        let mut result = String::new();
        let mut rest = out.as_str();
        while let Some(idx) = rest.find(&token) {
            result.push_str(&rest[..idx]);
            result.push_str(value);
            rest = &rest[idx + token.len()..];
            // Modifiers belong to the token; a `?` only when it is not the start of a query string
            if rest.starts_with('*') || rest.starts_with('+') || (rest.starts_with('?') && (rest.len() == 1 || rest[1..].starts_with('/'))) {
                rest = &rest[1..];
            }
        }
        result.push_str(rest);
        out = result;
    }
    out
}

struct Rule {
    source: RouteSource,
    has: Vec<Condition>,
    missing: Vec<Condition>,
    value: Value,
}

fn rules(list: &Value) -> Vec<Rule> {
    list.as_array()
        .into_iter()
        .flatten()
        .filter_map(|r| {
            let source = match RouteSource::compile(r["source"].as_str()?) {
                Ok(s) => s,
                Err(e) => {
                    crate::term::warn_once(&format!("next-config-source:{e}"), &format!("next.config: {e}"));
                    return None;
                }
            };
            Some(Rule { source, has: parse_conditions(&r["has"]), missing: parse_conditions(&r["missing"]), value: r.clone() })
        })
        .collect()
}

fn first_match<'a>(rules: &'a [Rule], req: &RequestInfo) -> Option<(&'a Rule, Vec<(String, String)>)> {
    rules.iter().find_map(|rule| {
        let params = rule.source.matches(req.path)?;
        conditions_hold(&rule.has, &rule.missing, req).then_some((rule, params))
    })
}

/// What next.config decides for a request, before routing
pub enum Decision {
    /// Respond with a redirect (`trailingSlash` normalization or `redirects`)
    Redirect { status: u16, location: String },
    NotFound,
    /// Route this path (basePath removed, `beforeFiles` rewrite applied)
    Continue { path_and_query: String },
}

fn join_query(path: &str, query: &str) -> String {
    match (path.contains('?'), query.is_empty()) {
        (_, true) => path.to_string(),
        (true, false) => format!("{path}&{query}"),
        (false, false) => format!("{path}?{query}"),
    }
}

/// basePath, trailingSlash, redirects and `beforeFiles` rewrites
pub fn before_routing(config: &Value, path: &str, query: &str, headers: &HeaderMap) -> Decision {
    let base = config["basePath"].as_str().unwrap_or("").trim_end_matches('/');
    let mut path = path.to_string();
    if !base.is_empty() {
        if path == base {
            path = "/".into();
        } else if let Some(rest) = path.strip_prefix(&format!("{base}/")) {
            path = format!("/{rest}");
        } else {
            return Decision::NotFound;
        }
    }
    let has_extension = path.rsplit('/').next().is_some_and(|last| last.contains('.'));
    if path != "/" && !has_extension {
        let trailing = config["trailingSlash"].as_bool().unwrap_or(false);
        if trailing && !path.ends_with('/') {
            return Decision::Redirect { status: 308, location: join_query(&format!("{base}{path}/"), query) };
        }
        if !trailing && path.ends_with('/') {
            return Decision::Redirect { status: 308, location: join_query(&format!("{base}{}", path.trim_end_matches('/')), query) };
        }
    }
    let req = RequestInfo { path: &path, headers, query };
    if let Some((rule, params)) = first_match(&rules(&config["redirects"]), &req) {
        let destination = substitute(rule.value["destination"].as_str().unwrap_or("/"), &params);
        let status = rule.value["statusCode"].as_u64().unwrap_or(if rule.value["permanent"].as_bool().unwrap_or(false) { 308 } else { 307 }) as u16;
        let prefixed = if destination.starts_with('/') && rule.value["basePath"] != Value::Bool(false) { format!("{base}{destination}") } else { destination };
        return Decision::Redirect { status, location: join_query(&prefixed, query) };
    }
    if let Some((rule, params)) = first_match(&rules(&config["rewrites"]["beforeFiles"]), &req) {
        let destination = substitute(rule.value["destination"].as_str().unwrap_or("/"), &params);
        return Decision::Continue { path_and_query: join_query(&destination, query) };
    }
    Decision::Continue { path_and_query: join_query(&path, query) }
}

/// `afterFiles` then `fallback` rewrites, for a path no page, route handler or file answers
pub fn rewrite_unmatched(config: &Value, path: &str, query: &str, headers: &HeaderMap) -> Option<String> {
    let req = RequestInfo { path, headers, query };
    for phase in ["afterFiles", "fallback"] {
        if let Some((rule, params)) = first_match(&rules(&config["rewrites"][phase]), &req) {
            return Some(join_query(&substitute(rule.value["destination"].as_str().unwrap_or("/"), &params), query));
        }
    }
    None
}

/// Response headers from `headers()` for `path` (all matching rules apply, in order)
pub fn response_headers(config: &Value, path: &str, query: &str, headers: &HeaderMap) -> Vec<(String, String)> {
    let req = RequestInfo { path, headers, query };
    let mut out = Vec::new();
    for rule in rules(&config["headers"]) {
        if let Some(params) = rule.source.matches(path) {
            if conditions_hold(&rule.has, &rule.missing, &req) {
                for h in rule.value["headers"].as_array().into_iter().flatten() {
                    if let (Some(k), Some(v)) = (h["key"].as_str(), h["value"].as_str()) {
                        out.push((k.to_string(), substitute(v, &params)));
                    }
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn sources_capture_named_params() {
        let s = RouteSource::compile("/blog/:slug").unwrap();
        assert_eq!(s.matches("/blog/hello"), Some(vec![("slug".into(), "hello".into())]));
        assert!(s.matches("/blog/a/b").is_none());
        let s = RouteSource::compile("/docs/:path*").unwrap();
        assert_eq!(s.matches("/docs/a/b").unwrap()[0].1, "a/b");
        assert_eq!(s.matches("/docs").unwrap()[0].1, "");
        let s = RouteSource::compile("/post/:id(\\d+)").unwrap();
        assert!(s.matches("/post/12").is_some() && s.matches("/post/x").is_none());
        assert_eq!(substitute("/news/:slug?ref=:slug", &[("slug".into(), "x".into())]), "/news/x?ref=x");
    }

    #[test]
    fn redirects_rewrites_trailing_slash_and_base_path() {
        let config = json!({
            "basePath": "/app",
            "redirects": [{ "source": "/old/:slug", "destination": "/new/:slug", "permanent": true }],
            "rewrites": { "beforeFiles": [{ "source": "/alias", "destination": "/real" }], "afterFiles": [{ "source": "/proxy/:path*", "destination": "https://api.example.com/:path*" }], "fallback": [] },
            "headers": [{ "source": "/(.*)", "headers": [{ "key": "x-frame-options", "value": "DENY" }] }],
        });
        let h = HeaderMap::new();
        assert!(matches!(before_routing(&config, "/outside", "", &h), Decision::NotFound));
        match before_routing(&config, "/app/old/post", "a=1", &h) {
            Decision::Redirect { status, location } => {
                assert_eq!(status, 308);
                assert_eq!(location, "/app/new/post?a=1");
            }
            _ => panic!("expected a redirect"),
        }
        match before_routing(&config, "/app/about/", "", &h) {
            Decision::Redirect { status, location } => assert_eq!((status, location.as_str()), (308, "/app/about")),
            _ => panic!("trailing slash"),
        }
        match before_routing(&config, "/app/alias", "", &h) {
            Decision::Continue { path_and_query } => assert_eq!(path_and_query, "/real"),
            _ => panic!("rewrite"),
        }
        assert_eq!(rewrite_unmatched(&config, "/proxy/v1/users", "", &h).as_deref(), Some("https://api.example.com/v1/users"));
        assert_eq!(response_headers(&config, "/anything", "", &h), vec![("x-frame-options".to_string(), "DENY".to_string())]);
    }
}
