//! Middleware (`middleware.ts`, Next.js 16 `proxy.ts`) at the project root or in `src/`: runs in the
//! worker before routing. Its `config.matcher` is compiled to regexes here, so requests it does not
//! match never pay a worker round trip.
//!
//! Matcher syntax (path-to-regexp, like Next.js): `/about`, `/blog/:slug`, `/docs/:path*`,
//! `/shop/:path+`, `/:lang?/x`, custom groups `/:id(\\d+)`, plain regex groups
//! `/((?!api|_next/static|favicon.ico).*)`, and objects `{ source, has, missing }` whose conditions
//! test headers, cookies, query parameters and the host.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use axum::body::Bytes;
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use regex::Regex;
use serde_json::{json, Value};

/// The middleware file of a project, if any
pub fn find(root: &Path) -> Option<PathBuf> {
    for dir in [root.to_path_buf(), root.join("src")] {
        for name in ["proxy", "middleware"] {
            for ext in ["ts", "js", "mjs", "tsx"] {
                let file = dir.join(format!("{name}.{ext}"));
                if file.is_file() {
                    return Some(file);
                }
            }
        }
    }
    None
}

#[derive(Debug, Clone)]
pub struct Condition {
    kind: String,
    key: String,
    value: Option<Regex>,
}

#[derive(Debug, Clone)]
pub struct Matcher {
    regex: Regex,
    has: Vec<Condition>,
    missing: Vec<Condition>,
}

/// Converts one path-to-regexp source to an anchored regex
pub fn source_to_regex(source: &str) -> Result<Regex, String> {
    let chars: Vec<char> = source.chars().collect();
    let mut out = String::from("^");
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '/' && i + 1 < chars.len() && chars[i + 1] == ':' {
            // `/:name(pattern)?` with modifier
            let mut j = i + 2;
            let mut name = String::new();
            while j < chars.len() && (chars[j].is_alphanumeric() || chars[j] == '_') {
                name.push(chars[j]);
                j += 1;
            }
            let (pattern, after) = if j < chars.len() && chars[j] == '(' { read_group(&chars, j)? } else { ("[^/]+?".to_string(), j) };
            let modifier = chars.get(after).copied().filter(|m| matches!(m, '*' | '+' | '?'));
            let next = after + modifier.map_or(0, |_| 1);
            let _ = name;
            out.push_str(&match modifier {
                Some('*') => format!("(?:/(?:{pattern})(?:/(?:{pattern}))*)?"),
                Some('+') => format!("/(?:{pattern})(?:/(?:{pattern}))*"),
                Some('?') => format!("(?:/(?:{pattern}))?"),
                _ => format!("/(?:{pattern})"),
            });
            i = next;
            continue;
        }
        if c == '(' {
            let (pattern, after) = read_group(&chars, i)?;
            let modifier = chars.get(after).copied().filter(|m| matches!(m, '*' | '+' | '?'));
            out.push_str(&format!("(?:{pattern}){}", modifier.map(String::from).unwrap_or_default()));
            i = after + modifier.map_or(0, |_| 1);
            continue;
        }
        out.push_str(&regex::escape(&c.to_string()));
        i += 1;
    }
    // Trailing slash optional, like Next.js
    if out.ends_with("\\/") && out.len() > 3 {
        out.truncate(out.len() - 2);
    }
    out.push_str("/?$");
    Regex::new(&out).map_err(|e| format!("invalid matcher {source:?}: {e}"))
}

/// Reads a parenthesised group starting at `start` (`(` … `)`, nesting aware); returns its content
pub(crate) fn read_group(chars: &[char], start: usize) -> Result<(String, usize), String> {
    let mut depth = 0;
    let mut content = String::new();
    let mut i = start;
    while i < chars.len() {
        let c = chars[i];
        if c == '\\' && i + 1 < chars.len() {
            content.push(c);
            content.push(chars[i + 1]);
            i += 2;
            continue;
        }
        if c == '(' {
            depth += 1;
            if depth == 1 {
                i += 1;
                continue;
            }
        } else if c == ')' {
            depth -= 1;
            if depth == 0 {
                return Ok((content, i + 1));
            }
        }
        content.push(c);
        i += 1;
    }
    Err("unbalanced parenthesis in matcher".into())
}

/// `has` / `missing` entries (`{ type: "header" | "cookie" | "query" | "host", key, value? }`)
pub fn parse_conditions(v: &Value) -> Vec<Condition> {
    v.as_array().map(|a| a.iter().filter_map(condition).collect()).unwrap_or_default()
}

/// Every `has` condition holds and no `missing` one does
pub fn conditions_hold(has: &[Condition], missing: &[Condition], req: &RequestInfo) -> bool {
    has.iter().all(|c| Matcher::condition_holds(c, req)) && !missing.iter().any(|c| Matcher::condition_holds(c, req))
}

fn condition(v: &Value) -> Option<Condition> {
    Some(Condition {
        kind: v["type"].as_str()?.to_string(),
        key: v["key"].as_str().unwrap_or("").to_string(),
        value: v["value"].as_str().and_then(|p| Regex::new(&format!("^(?:{p})$")).ok()),
    })
}

/// Matchers from `config.matcher` (string, list of strings or objects); none = every path
pub fn compile(config: &Value) -> Result<Vec<Matcher>, String> {
    let list: Vec<Value> = match &config["matcher"] {
        Value::Null => return Ok(vec![]),
        Value::Array(items) => items.clone(),
        other => vec![other.clone()],
    };
    list.iter()
        .map(|item| {
            let (source, has, missing) = match item {
                Value::String(s) => (s.clone(), vec![], vec![]),
                Value::Object(_) => (
                    item["source"].as_str().unwrap_or("/:path*").to_string(),
                    item["has"].as_array().map(|a| a.iter().filter_map(condition).collect()).unwrap_or_default(),
                    item["missing"].as_array().map(|a| a.iter().filter_map(condition).collect()).unwrap_or_default(),
                ),
                _ => return Err("matcher entries must be strings or { source }".to_string()),
            };
            Ok(Matcher { regex: compile_source(&source)?, has, missing })
        })
        .collect()
}

/// Negative look-ahead groups (`(?!api|_next)`) are not supported by the regex crate: compiled as a
/// regex for the rest plus a list of excluded prefixes
fn compile_source(source: &str) -> Result<Regex, String> {
    source_to_regex(&strip_negative_lookahead(source).0)
}

fn strip_negative_lookahead(source: &str) -> (String, Vec<String>) {
    let mut out = source.to_string();
    let mut excluded = Vec::new();
    while let Some(start) = out.find("(?!") {
        let chars: Vec<char> = out.chars().collect();
        let begin = out[..start].chars().count();
        let mut depth = 0;
        let mut end = None;
        for (i, c) in chars.iter().enumerate().skip(begin) {
            match c {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(i);
                        break;
                    }
                }
                _ => {}
            }
        }
        let Some(end) = end else { break };
        let inner: String = chars[begin + 3..end].iter().collect();
        excluded.extend(inner.split('|').map(|s| s.replace("\\.", ".").replace("\\/", "/")));
        let rebuilt: String = chars[..begin].iter().chain(chars[end + 1..].iter()).collect();
        out = rebuilt;
    }
    (out, excluded)
}

pub struct RequestInfo<'a> {
    pub path: &'a str,
    pub headers: &'a HeaderMap,
    pub query: &'a str,
}

impl Matcher {
    fn condition_holds(c: &Condition, req: &RequestInfo) -> bool {
        let actual: Option<String> = match c.kind.as_str() {
            "header" => req.headers.get(c.key.as_str()).and_then(|v| v.to_str().ok()).map(String::from),
            "cookie" => req
                .headers
                .get(header::COOKIE)
                .and_then(|v| v.to_str().ok())
                .and_then(|cookies| cookies.split(';').find_map(|p| p.trim().split_once('=').filter(|(k, _)| *k == c.key).map(|(_, v)| v.to_string()))),
            "query" => req.query.split('&').find_map(|p| p.split_once('=').filter(|(k, _)| *k == c.key).map(|(_, v)| v.to_string()).or_else(|| (p == c.key).then(String::new))),
            "host" => req.headers.get(header::HOST).and_then(|v| v.to_str().ok()).map(|h| h.split(':').next().unwrap_or(h).to_string()),
            _ => None,
        };
        match (&actual, &c.value) {
            (Some(v), Some(re)) => re.is_match(v),
            (Some(_), None) => true,
            (None, _) => false,
        }
    }

    pub fn matches(&self, req: &RequestInfo, excluded: &[String]) -> bool {
        let rest = req.path.trim_start_matches('/');
        if excluded.iter().any(|e| !e.is_empty() && rest.starts_with(e.trim_start_matches('/'))) {
            return false;
        }
        self.regex.is_match(req.path) && self.has.iter().all(|c| Self::condition_holds(c, req)) && !self.missing.iter().any(|c| Self::condition_holds(c, req))
    }
}

/// The compiled matchers of the current middleware, with the excluded prefixes of each
pub struct Compiled {
    pub matchers: Vec<(Matcher, Vec<String>)>,
}

impl Compiled {
    pub fn from_config(config: &Value) -> Result<Self, String> {
        let sources: Vec<Value> = match &config["matcher"] {
            Value::Null => vec![],
            Value::Array(items) => items.clone(),
            other => vec![other.clone()],
        };
        let mut matchers = Vec::new();
        for (matcher, source) in compile(config)?.into_iter().zip(sources.iter()) {
            let src = source.as_str().or_else(|| source["source"].as_str()).unwrap_or("");
            matchers.push((matcher, strip_negative_lookahead(src).1));
        }
        Ok(Self { matchers })
    }

    pub fn applies(&self, req: &RequestInfo) -> bool {
        self.matchers.is_empty() || self.matchers.iter().any(|(m, excluded)| m.matches(req, excluded))
    }
}

/// Engine endpoints never go through middleware
pub fn is_internal(path: &str) -> bool {
    path.starts_with("/_nata/") || path.starts_with("/_bundle/") || path == "/_hmr" || path.starts_with("/__nata")
}

fn cache() -> &'static std::sync::Mutex<HashMap<String, std::sync::Arc<Compiled>>> {
    static CACHE: std::sync::OnceLock<std::sync::Mutex<HashMap<String, std::sync::Arc<Compiled>>>> = std::sync::OnceLock::new();
    CACHE.get_or_init(Default::default)
}

/// What the middleware decided
pub enum Outcome {
    /// Continue; request headers to override, response headers to add to the final response
    Next { request_headers: Vec<(String, String)>, response_headers: Vec<(String, String)> },
    /// Serve another path of this app (same response headers semantics as `Next`)
    Rewrite { path_and_query: String, request_headers: Vec<(String, String)>, response_headers: Vec<(String, String)> },
    /// The middleware's own response (redirect, JSON, external rewrite result...)
    Respond(Response),
}

fn pairs(v: &Value) -> Vec<(String, String)> {
    v.as_array()
        .map(|a| a.iter().filter_map(|p| Some((p[0].as_str()?.to_string(), p[1].as_str()?.to_string()))).collect())
        .unwrap_or_default()
}

/// Runs the middleware for a request when it exists and matches; `None` = not involved
pub async fn run(root: &Path, method: &str, uri: &axum::http::Uri, headers: &HeaderMap) -> Option<Outcome> {
    let path = uri.path();
    if is_internal(path) || !crate::module_graph::rsc_enabled() {
        return None;
    }
    let file = find(root)?;
    let root = crate::rpc::canonicalize(&root).unwrap_or_else(|_| root.to_path_buf());
    let specifier = crate::module_graph::server_module_specifier(&root, &file)?;

    // The matcher of this version of the middleware (re-read after edits: new specifier)
    let compiled = cache().lock().ok()?.get(&specifier).cloned();
    let compiled = match compiled {
        Some(c) => c,
        None => {
            let reply = crate::worker_pool::call(&root, "middleware_config", json!({ "specifier": specifier }), Vec::new()).await.ok()?;
            crate::ssr::worker::print_logs(reply.stdout(), reply.stderr());
            let compiled = match Compiled::from_config(reply.output()) {
                Ok(c) => std::sync::Arc::new(c),
                Err(e) => {
                    crate::term::error("Invalid middleware matcher", Some(&e), None);
                    std::sync::Arc::new(Compiled { matchers: vec![] })
                }
            };
            cache().lock().ok()?.insert(specifier.clone(), compiled.clone());
            compiled
        }
    };
    let info = RequestInfo { path, headers, query: uri.query().unwrap_or("") };
    if !compiled.applies(&info) {
        return None;
    }

    let header_list: Vec<(String, String)> = headers.iter().filter_map(|(k, v)| v.to_str().ok().map(|v| (k.as_str().to_string(), v.to_string()))).collect();
    let host = headers.get(header::HOST).and_then(|v| v.to_str().ok()).unwrap_or("localhost");
    let input = json!({
        "specifier": specifier,
        "url": format!("http://{host}{}", uri.path_and_query().map(|p| p.as_str()).unwrap_or(path)),
        "method": method,
        "headers": header_list,
    });
    let reply = match crate::worker_pool::call(&root, "middleware", input, Vec::new()).await {
        Ok(reply) => reply,
        Err(e) => {
            crate::term::error("Middleware failed", Some(&e.to_string()), None);
            return Some(Outcome::Respond((StatusCode::INTERNAL_SERVER_ERROR, format!("Middleware failed: {e}")).into_response()));
        }
    };
    crate::ssr::worker::print_logs(reply.stdout(), reply.stderr());
    if let Some(err) = reply.error() {
        crate::term::error("Error in middleware", Some(err), None);
        return Some(Outcome::Respond((StatusCode::INTERNAL_SERVER_ERROR, format!("Error in middleware: {err}\n\n{}", reply.stderr())).into_response()));
    }
    let out = reply.output();
    let request_headers = pairs(&out["requestHeaders"]);
    let response_headers = pairs(&out["responseHeaders"]);
    Some(match out["kind"].as_str().unwrap_or("next") {
        "rewrite" => Outcome::Rewrite { path_and_query: out["rewrite"].as_str().unwrap_or("/").to_string(), request_headers, response_headers },
        "respond" => {
            let status = StatusCode::from_u16(out["status"].as_u64().unwrap_or(200) as u16).unwrap_or(StatusCode::OK);
            let mut response = Response::builder().status(status);
            for (k, v) in &response_headers {
                if let (Ok(name), Ok(value)) = (header::HeaderName::from_bytes(k.as_bytes()), HeaderValue::from_str(v)) {
                    response = response.header(name, value);
                }
            }
            Outcome::Respond(response.body(axum::body::Body::from(Bytes::from(reply.body.clone()))).unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response()))
        }
        _ => Outcome::Next { request_headers, response_headers },
    })
}

/// Adds the middleware's response headers (and cookies) to the final response
pub fn apply_response_headers(response: &mut Response, headers: &[(String, String)]) {
    for (k, v) in headers {
        if let (Ok(name), Ok(value)) = (header::HeaderName::from_bytes(k.as_bytes()), HeaderValue::from_str(v)) {
            if name == header::SET_COOKIE {
                response.headers_mut().append(name, value);
            } else {
                response.headers_mut().insert(name, value);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn matches(config: Value, path: &str) -> bool {
        let compiled = Compiled::from_config(&config).unwrap();
        let headers = HeaderMap::new();
        compiled.applies(&RequestInfo { path, headers: &headers, query: "" })
    }

    #[test]
    fn path_to_regexp_sources() {
        assert!(matches(json!({ "matcher": "/about" }), "/about"));
        assert!(matches(json!({ "matcher": "/about" }), "/about/"));
        assert!(!matches(json!({ "matcher": "/about" }), "/about/team"));
        assert!(matches(json!({ "matcher": "/dashboard/:path*" }), "/dashboard"));
        assert!(matches(json!({ "matcher": "/dashboard/:path*" }), "/dashboard/a/b"));
        assert!(!matches(json!({ "matcher": "/dashboard/:path+" }), "/dashboard"));
        assert!(matches(json!({ "matcher": "/dashboard/:path+" }), "/dashboard/x"));
        assert!(matches(json!({ "matcher": "/blog/:slug" }), "/blog/hello"));
        assert!(!matches(json!({ "matcher": "/blog/:slug" }), "/blog/hello/x"));
        assert!(matches(json!({ "matcher": "/post/:id(\\d+)" }), "/post/42"));
        assert!(!matches(json!({ "matcher": "/post/:id(\\d+)" }), "/post/abc"));
        assert!(matches(json!({ "matcher": ["/a", "/b/:p*"] }), "/b/c"));
        // No matcher: every path
        assert!(matches(json!({}), "/anything"));
    }

    #[test]
    fn negative_lookahead_excludes_prefixes() {
        let config = json!({ "matcher": ["/((?!api|_next/static|_next/image|favicon.ico).*)"] });
        assert!(matches(config.clone(), "/"));
        assert!(matches(config.clone(), "/dashboard/x"));
        assert!(!matches(config.clone(), "/api/users"));
        assert!(!matches(config.clone(), "/favicon.ico"));
        assert!(!matches(config, "/_next/static/chunk.js"));
    }

    #[test]
    fn has_and_missing_conditions() {
        let config = json!({ "matcher": [{ "source": "/:path*", "has": [{ "type": "header", "key": "x-flag", "value": "on|yes" }], "missing": [{ "type": "cookie", "key": "skip" }] }] });
        let compiled = Compiled::from_config(&config).unwrap();
        let mut headers = HeaderMap::new();
        assert!(!compiled.applies(&RequestInfo { path: "/x", headers: &headers, query: "" }));
        headers.insert("x-flag", HeaderValue::from_static("yes"));
        assert!(compiled.applies(&RequestInfo { path: "/x", headers: &headers, query: "" }));
        headers.insert(header::COOKIE, HeaderValue::from_static("a=1; skip=1"));
        assert!(!compiled.applies(&RequestInfo { path: "/x", headers: &headers, query: "" }));
    }

    #[test]
    fn engine_endpoints_are_never_matched() {
        assert!(is_internal("/_nata/router.js"));
        assert!(is_internal("/_bundle/app/page.tsx"));
        assert!(!is_internal("/dashboard"));
    }
}
