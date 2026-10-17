pub mod matcher;
pub mod parallel;
pub mod scanner;
pub mod segment;
pub mod types;

#[cfg(test)]
mod tests;

pub use matcher::RouteMatcher;
pub use scanner::RouteScanner;
pub use segment::SegmentParser;
pub use types::{RouteEntry, RouteKind, RouteMatch, RouteSegment, SegmentFiles, SegmentType};

use std::{collections::HashMap, path::Path, sync::OnceLock};

/// Radix tree over the route table (`matchit`, the router axum uses): matching costs O(path length)
/// whatever the number of routes. Routes the tree cannot hold (two dynamic segments with different
/// names at the same position, e.g. from two route groups) are matched by their regex instead.
#[derive(Debug, Clone, Default)]
struct RouteIndex {
    tree: matchit::Router<usize>,
    fallback: Vec<usize>,
}

impl RouteIndex {
    fn build(routes: &[RouteEntry]) -> Self {
        let mut index = RouteIndex::default();
        for (idx, route) in routes.iter().enumerate() {
            let mut inserted = true;
            for path in Self::tree_paths(route) {
                if index.tree.insert(path, idx).is_err() {
                    inserted = false;
                }
            }
            if !inserted {
                index.fallback.push(idx);
            }
        }
        index
    }

    /// `/blog/[slug]` → `/blog/{slug}`; `[...slug]` → `{*slug}`; `[[...slug]]` → with and without
    fn tree_paths(route: &RouteEntry) -> Vec<String> {
        let escape = |s: &str| s.replace('{', "{{").replace('}', "}}");
        let mut paths = vec![String::new()];
        for seg in route.segments.iter().filter(|s| s.is_url_segment()) {
            match &seg.segment_type {
                SegmentType::Static(s) => paths.iter_mut().for_each(|p| p.push_str(&format!("/{}", escape(s)))),
                SegmentType::Dynamic(name) => paths.iter_mut().for_each(|p| p.push_str(&format!("/{{{}}}", name))),
                SegmentType::CatchAll(name) => paths.iter_mut().for_each(|p| p.push_str(&format!("/{{*{}}}", name))),
                SegmentType::OptionalCatchAll(name) => {
                    let without: Vec<String> = paths.clone();
                    paths.iter_mut().for_each(|p| p.push_str(&format!("/{{*{}}}", name)));
                    paths.extend(without);
                }
                _ => {}
            }
        }
        paths.into_iter().map(|p| if p.is_empty() { "/".to_string() } else { p }).collect()
    }
}

/// App Router Engine responsible for route discovery, indexing, and runtime dispatching
#[derive(Debug, Clone, Default)]
pub struct AppRouter {
    pub routes: Vec<RouteEntry>,
    index: OnceLock<RouteIndex>,
}

impl AppRouter {
    /// Creates an empty App Router instance
    pub fn new() -> Self {
        Self::default()
    }

    /// Scans project workspace directory and indexes all App Router pages and API routes
    pub fn scan<P: AsRef<Path>>(root_dir: P) -> Self {
        let routes = RouteScanner::scan(root_dir);
        Self { routes, index: OnceLock::new() }
    }

    /// Appends a route entry and re-sorts by priority score
    pub fn add_route(&mut self, route: RouteEntry) {
        self.routes.push(route);
        self.routes.sort_by(|a, b| {
            b.score
                .cmp(&a.score)
                .then_with(|| b.pattern.len().cmp(&a.pattern.len()))
                .then_with(|| a.pattern.cmp(&b.pattern))
        });
        self.index = OnceLock::new();
    }

    fn index(&self) -> &RouteIndex {
        self.index.get_or_init(|| RouteIndex::build(&self.routes))
    }

    fn regex_params(route: &RouteEntry, path: &str) -> Option<HashMap<String, String>> {
        let caps = route.compiled_regex()?.captures(path)?;
        let mut params = HashMap::new();
        for (i, name) in route.param_names.iter().enumerate() {
            if let Some(val) = caps.get(i + 1) {
                if !val.as_str().is_empty() {
                    params.insert(name.clone(), val.as_str().to_string());
                }
            }
        }
        Some(params)
    }

    /// Matches an already-normalized path: the tree first, then the few regex-only routes when one of
    /// them has a higher priority than the tree's match
    fn match_clean(&self, clean_path: &str) -> Option<(usize, HashMap<String, String>)> {
        let index = self.index();
        let mut best: Option<(usize, HashMap<String, String>)> = index.tree.at(clean_path).ok().map(|m| {
            let params = m.params.iter().filter(|(_, v)| !v.is_empty()).map(|(k, v)| (k.to_string(), v.to_string())).collect();
            (*m.value, params)
        });
        for &idx in &index.fallback {
            if best.as_ref().is_some_and(|(b, _)| *b < idx) {
                break;
            }
            if let Some(params) = Self::regex_params(&self.routes[idx], clean_path) {
                best = Some((idx, params));
                break;
            }
        }
        best
    }

    /// Matches a request path or URL and returns the matched route and dynamic parameter map
    pub fn match_route(&self, path: &str) -> Option<(&RouteEntry, HashMap<String, String>)> {
        let (clean_path, _) = RouteMatcher::normalize_url(path);
        self.match_clean(&clean_path).map(|(idx, params)| (&self.routes[idx], params))
    }

    /// Detailed match returning route, parameters, query string, and normalized path
    pub fn match_request(&self, path_or_url: &str) -> Option<RouteMatch> {
        let (clean_path, query) = RouteMatcher::normalize_url(path_or_url);
        let (idx, params) = self.match_clean(&clean_path)?;
        Some(RouteMatch { route: self.routes[idx].clone(), params, query, matched_path: clean_path })
    }

    /// Returns the total number of indexed routes
    pub fn len(&self) -> usize {
        self.routes.len()
    }

    /// Returns true if no routes were discovered
    pub fn is_empty(&self) -> bool {
        self.routes.is_empty()
    }
}
