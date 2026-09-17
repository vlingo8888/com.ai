pub mod matcher;
pub mod scanner;
pub mod segment;
pub mod types;

#[cfg(test)]
mod tests;

pub use matcher::RouteMatcher;
pub use scanner::RouteScanner;
pub use segment::SegmentParser;
pub use types::{RouteEntry, RouteKind, RouteMatch, RouteSegment, SegmentType};

use std::{collections::HashMap, path::Path};

/// App Router Engine responsible for route discovery, indexing, and runtime dispatching
#[derive(Debug, Clone, Default)]
pub struct AppRouter {
    pub routes: Vec<RouteEntry>,
}

impl AppRouter {
    /// Creates an empty App Router instance
    pub fn new() -> Self {
        Self { routes: Vec::new() }
    }

    /// Scans project workspace directory and indexes all App Router pages and API routes
    pub fn scan<P: AsRef<Path>>(root_dir: P) -> Self {
        let routes = RouteScanner::scan(root_dir);
        Self { routes }
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
    }

    /// Matches a request path or URL and returns the matched route and dynamic parameter map
    pub fn match_route(&self, path: &str) -> Option<(&RouteEntry, HashMap<String, String>)> {
        // Fast path through matcher
        let (clean_path, _) = RouteMatcher::normalize_url(path);

        for route in &self.routes {
            if let Ok(re) = regex::Regex::new(&route.regex) {
                if let Some(caps) = re.captures(&clean_path) {
                    let mut params = HashMap::new();
                    for (i, name) in route.param_names.iter().enumerate() {
                        if let Some(val) = caps.get(i + 1) {
                            let raw_val = val.as_str();
                            if !raw_val.is_empty() {
                                params.insert(name.clone(), raw_val.to_string());
                            }
                        }
                    }
                    return Some((route, params));
                }
            }
        }
        None
    }

    /// Detailed match returning route, parameters, query string, and normalized path
    pub fn match_request(&self, path_or_url: &str) -> Option<RouteMatch> {
        RouteMatcher::match_route(&self.routes, path_or_url)
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
