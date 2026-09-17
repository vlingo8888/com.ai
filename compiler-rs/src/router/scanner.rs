use std::path::Path;
use walkdir::WalkDir;

use super::{
    matcher::RouteMatcher,
    segment::SegmentParser,
    types::{RouteEntry, RouteKind},
};

pub struct RouteScanner;

impl RouteScanner {
    /// Scans a project root directory and builds the list of all active App Router routes
    pub fn scan<P: AsRef<Path>>(root_dir: P) -> Vec<RouteEntry> {
        let root = root_dir.as_ref();
        let app_dir = root.join("app");

        if !app_dir.exists() {
            let src_app = root.join("src").join("app");
            if src_app.exists() {
                return Self::build_from_dir(&src_app, root);
            }
            return Vec::new();
        }

        Self::build_from_dir(&app_dir, root)
    }

    /// Recursively scans an app directory and constructs RouteEntry instances
    pub fn build_from_dir(app_dir: &Path, root: &Path) -> Vec<RouteEntry> {
        let mut routes = Vec::new();

        for entry in WalkDir::new(app_dir)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
        {
            let path = entry.path();
            let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

            let (kind, is_api) = if file_name == "page.tsx"
                || file_name == "page.jsx"
                || file_name == "page.js"
                || file_name == "page.ts"
            {
                (RouteKind::Page, false)
            } else if file_name == "route.ts" || file_name == "route.js" {
                (RouteKind::Api, true)
            } else {
                continue;
            };

            let rel = path.strip_prefix(app_dir).unwrap_or(path);
            let parent = rel.parent().unwrap_or(Path::new(""));
            let segment_strs: Vec<&str> = parent
                .iter()
                .filter_map(|s| s.to_str())
                .filter(|s| !s.is_empty())
                .collect();

            let segments = SegmentParser::parse_segments(&segment_strs);
            let (pattern, regex_str, param_names, score) =
                RouteMatcher::compile_segments(&segments);

            // Find all cascading layouts from root app dir to current folder
            let mut layout_files = Vec::new();
            let mut current_search = path.parent();
            while let Some(dir) = current_search {
                for ext in &["tsx", "jsx", "js", "ts"] {
                    let layout_cand = dir.join(format!("layout.{}", ext));
                    if layout_cand.exists() {
                        if let Ok(rel_path) = layout_cand.strip_prefix(root) {
                            layout_files.insert(0, rel_path.to_path_buf());
                        }
                        break;
                    }
                }
                if dir == app_dir {
                    break;
                }
                current_search = dir.parent();
            }

            let rel_page = path.strip_prefix(root).unwrap_or(path).to_path_buf();

            routes.push(RouteEntry {
                pattern,
                regex: regex_str,
                page_file: rel_page,
                layout_files,
                param_names,
                is_api,
                kind,
                score,
                segments,
            });
        }

        // Deterministic priority sort:
        // 1. Highest score first (static > dynamic > optional catch-all > catch-all)
        // 2. Exact matches before wildcard matches
        // 3. More specific paths before shorter generic paths
        routes.sort_by(|a, b| {
            b.score
                .cmp(&a.score)
                .then_with(|| b.pattern.len().cmp(&a.pattern.len()))
                .then_with(|| a.pattern.cmp(&b.pattern))
        });

        routes
    }
}
