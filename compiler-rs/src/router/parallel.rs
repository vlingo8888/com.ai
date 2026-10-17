//! Parallel routes (`@slot` folders + `default.tsx`) and intercepting routes (`(.)x`, `(..)x`,
//! `(..)(..)x`, `(...)x`), resolved per request (dev: folders are read when needed).
//!
//! - A layout folder with `@name` subfolders gets a `name` prop: the slot's page matching the rest of
//!   the URL below the layout, else the slot's `default.tsx`, else nothing.
//! - An intercepting folder renders its page instead of the route it targets when the client router
//!   navigates there from inside the folder's layout (soft navigation); loading the URL directly
//!   renders the target route. Inside a slot (the modal pattern) the current page stays as
//!   `children` and the slot shows the intercepting page.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::matcher::RouteMatcher;
use super::scanner::RouteScanner;
use super::segment::SegmentParser;
use super::types::{SegmentFiles, SegmentType};

const PAGE_NAMES: [&str; 4] = ["page.tsx", "page.jsx", "page.ts", "page.js"];

pub fn is_intercepting(name: &str) -> bool {
    name.starts_with("(.)") || name.starts_with("(..)") || name.starts_with("(...)")
}

/// Special files of one folder (`dir` absolute), paths relative to `root`
pub fn segment_files(root: &Path, dir: &Path) -> SegmentFiles {
    SegmentFiles {
        folder: dir.strip_prefix(root).unwrap_or(dir).to_path_buf(),
        layout: RouteScanner::find_special_file(dir, root, "layout"),
        template: RouteScanner::find_special_file(dir, root, "template"),
        error: RouteScanner::find_special_file(dir, root, "error"),
        loading: RouteScanner::find_special_file(dir, root, "loading"),
        not_found: RouteScanner::find_special_file(dir, root, "not-found"),
        forbidden: RouteScanner::find_special_file(dir, root, "forbidden"),
        unauthorized: RouteScanner::find_special_file(dir, root, "unauthorized"),
    }
}

/// A slot or intercepting page resolved for a request
#[derive(Debug, Clone, PartialEq)]
pub struct SubRoute {
    /// Folders from the slot (or intercepting) folder down to the page's folder
    pub segments: Vec<SegmentFiles>,
    /// The page, or `default.tsx`
    pub page: PathBuf,
    pub params: HashMap<String, String>,
}

/// Pages below `dir` with the folder names leading to them (`[["photos", "[id]"], …]`)
fn pages_below(dir: &Path) -> Vec<(Vec<String>, PathBuf)> {
    let mut out = Vec::new();
    for entry in walkdir::WalkDir::new(dir).into_iter().filter_map(|e| e.ok()).filter(|e| e.file_type().is_file()) {
        let name = entry.file_name().to_str().unwrap_or("");
        if !PAGE_NAMES.contains(&name) {
            continue;
        }
        let rel = entry.path().parent().unwrap().strip_prefix(dir).unwrap_or(Path::new(""));
        let parts: Vec<String> = rel.iter().filter_map(|s| s.to_str()).map(String::from).collect();
        // Nested slots and interceptions are not part of this slot's own routes
        if parts.iter().any(|p| p.starts_with('@') || is_intercepting(p)) {
            continue;
        }
        out.push((parts, entry.path().to_path_buf()));
    }
    out
}

/// Matches `path` against folder names (route conventions); returns the params
fn match_folders(folders: &[String], path: &str) -> Option<(i32, HashMap<String, String>)> {
    let refs: Vec<&str> = folders.iter().map(String::as_str).collect();
    let segments = SegmentParser::parse_segments(&refs);
    let (_, regex, names, score) = RouteMatcher::compile_segments(&segments);
    let caps = regex::Regex::new(&regex).ok()?.captures(path)?;
    let params = names
        .iter()
        .enumerate()
        .filter_map(|(i, n)| caps.get(i + 1).filter(|m| !m.as_str().is_empty()).map(|m| (n.clone(), m.as_str().to_string())))
        .collect();
    Some((score, params))
}

fn chain(root: &Path, from: &Path, to: &Path) -> Vec<SegmentFiles> {
    let mut dirs = vec![to.to_path_buf()];
    let mut current = to;
    while current != from {
        let Some(parent) = current.parent() else { break };
        dirs.insert(0, parent.to_path_buf());
        current = parent;
    }
    dirs.iter().map(|d| segment_files(root, d)).collect()
}

fn default_file(dir: &Path) -> Option<PathBuf> {
    ["default.tsx", "default.jsx", "default.ts", "default.js"].iter().map(|n| dir.join(n)).find(|p| p.is_file())
}

/// The slots of the layout folder `folder` (absolute) for the URL segments below it
pub fn match_slots(root: &Path, folder: &Path, remaining: &[&str]) -> Vec<(String, SubRoute)> {
    let Ok(entries) = std::fs::read_dir(folder) else { return vec![] };
    let mut slots: Vec<(String, SubRoute)> = Vec::new();
    let path = format!("/{}", remaining.join("/"));
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with('@') || name == "@children" || !entry.path().is_dir() {
            continue;
        }
        let slot_dir = entry.path();
        let best = pages_below(&slot_dir)
            .into_iter()
            .filter_map(|(folders, page)| match_folders(&folders, &path).map(|(score, params)| (score, folders.len(), params, page)))
            .max_by_key(|(score, depth, _, _)| (*score, *depth));
        let sub = match best {
            Some((_, _, params, page)) => SubRoute { segments: chain(root, &slot_dir, page.parent().unwrap()), page: rel(root, &page), params },
            None => match default_file(&slot_dir) {
                Some(default) => SubRoute { segments: vec![segment_files(root, &slot_dir)], page: rel(root, &default), params: HashMap::new() },
                None => continue,
            },
        };
        slots.push((name[1..].to_string(), sub));
    }
    slots.sort_by(|a, b| a.0.cmp(&b.0));
    slots
}

fn rel(root: &Path, p: &Path) -> PathBuf {
    p.strip_prefix(root).unwrap_or(p).to_path_buf()
}

/// URL folder names of `dir` below the app folder (route groups and slots do not count)
fn url_folders(app_dir: &Path, dir: &Path) -> Vec<String> {
    dir.strip_prefix(app_dir)
        .unwrap_or(Path::new(""))
        .iter()
        .filter_map(|s| s.to_str())
        .filter(|s| {
            let seg = SegmentParser::parse(s);
            !matches!(seg.segment_type, SegmentType::RouteGroup(_) | SegmentType::ParallelSlot(_))
        })
        .map(String::from)
        .collect()
}

/// An interception of a soft navigation
#[derive(Debug, Clone, PartialEq)]
pub struct Interception {
    /// The layout folder (relative) whose slot shows the page; `None` when the intercepting folder is
    /// not in a slot (its page replaces the target route)
    pub slot: Option<(PathBuf, String)>,
    pub sub: SubRoute,
}

/// Finds an intercepting route for a client navigation from `current` to `target`
pub fn find_interception(root: &Path, app_dir: &Path, target: &str, current: &str) -> Option<Interception> {
    let mut best: Option<(i32, Interception)> = None;
    for entry in walkdir::WalkDir::new(app_dir).into_iter().filter_map(|e| e.ok()).filter(|e| e.file_type().is_dir()) {
        let name = entry.file_name().to_string_lossy().to_string();
        if !is_intercepting(&name) {
            continue;
        }
        let intercept_dir = entry.path();
        let parent = intercept_dir.parent()?;
        let (levels_up, stripped) = if let Some(rest) = name.strip_prefix("(...)") {
            (usize::MAX, rest)
        } else if let Some(rest) = name.strip_prefix("(..)(..)") {
            (2, rest)
        } else if let Some(rest) = name.strip_prefix("(..)") {
            (1, rest)
        } else {
            (0, name.strip_prefix("(.)").unwrap_or(&name))
        };
        let base = url_folders(app_dir, parent);
        let base: Vec<String> = if levels_up == usize::MAX { vec![] } else { base[..base.len().saturating_sub(levels_up)].to_vec() };
        // Where the interception applies: the layout owning the slot, or the parent folder
        let (anchor_dir, slot) = match parent.file_name().and_then(|n| n.to_str()).filter(|n| n.starts_with('@')) {
            Some(slot_name) => (parent.parent()?.to_path_buf(), Some(slot_name[1..].to_string())),
            None => (parent.to_path_buf(), None),
        };
        let anchor = url_folders(app_dir, &anchor_dir);
        let anchor_refs: Vec<&str> = anchor.iter().map(String::as_str).collect();
        let (_, anchor_regex, _, _) = RouteMatcher::compile_segments(&SegmentParser::parse_segments(&anchor_refs));
        let prefix = anchor_regex.trim_end_matches("/?$").trim_end_matches('$');
        let applies = regex::Regex::new(&format!("{prefix}(?:/.*)?$")).ok()?.is_match(current) || anchor.is_empty();
        if !applies {
            continue;
        }
        for (folders, page) in pages_below(intercept_dir) {
            let mut target_folders = base.clone();
            if !stripped.is_empty() {
                target_folders.push(stripped.to_string());
            }
            target_folders.extend(folders.iter().cloned());
            let Some((score, params)) = match_folders(&target_folders, target) else { continue };
            let sub = SubRoute { segments: chain(root, intercept_dir, page.parent().unwrap()), page: rel(root, &page), params };
            let interception = Interception { slot: slot.as_ref().map(|s| (rel(root, &anchor_dir), s.clone())), sub };
            if best.as_ref().is_none_or(|(b, _)| score > *b) {
                best = Some((score, interception));
            }
        }
    }
    best.map(|(_, i)| i)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(name: &str, files: &[&str]) -> (PathBuf, PathBuf) {
        let root = std::env::temp_dir().join(format!("nata_parallel_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for f in files {
            let p = root.join(f);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, "export default function X() { return null; }").unwrap();
        }
        let root = root.canonicalize().unwrap();
        (root.clone(), root.join("app"))
    }

    #[test]
    fn slots_match_the_rest_of_the_url_or_fall_back_to_default() {
        let (root, app) = project(
            "slots",
            &["app/layout.tsx", "app/page.tsx", "app/@team/page.tsx", "app/@team/settings/page.tsx", "app/@team/default.tsx", "app/@analytics/page.tsx", "app/settings/page.tsx"],
        );
        let at_root = match_slots(&root, &app, &[]);
        assert_eq!(at_root.len(), 2);
        assert_eq!(at_root[1].0, "team");
        assert_eq!(at_root[1].1.page, PathBuf::from("app/@team/page.tsx"));
        let at_settings = match_slots(&root, &app, &["settings"]);
        let team = at_settings.iter().find(|(n, _)| n == "team").unwrap();
        assert_eq!(team.1.page, PathBuf::from("app/@team/settings/page.tsx"));
        // @analytics has no page for /settings and no default: left out
        assert!(at_settings.iter().all(|(n, _)| n != "analytics"));
        let elsewhere = match_slots(&root, &app, &["other"]);
        assert_eq!(elsewhere.iter().find(|(n, _)| n == "team").unwrap().1.page, PathBuf::from("app/@team/default.tsx"));
    }

    #[test]
    fn modal_interception_from_inside_the_layout() {
        let (root, app) = project(
            "intercept",
            &["app/layout.tsx", "app/page.tsx", "app/@modal/default.tsx", "app/@modal/(.)photos/[id]/page.tsx", "app/photos/[id]/page.tsx"],
        );
        let found = find_interception(&root, &app, "/photos/7", "/").expect("intercepted");
        assert_eq!(found.slot, Some((PathBuf::from("app"), "modal".to_string())));
        assert_eq!(found.sub.page, PathBuf::from("app/@modal/(.)photos/[id]/page.tsx"));
        assert_eq!(found.sub.params.get("id").map(String::as_str), Some("7"));
        assert!(find_interception(&root, &app, "/about", "/").is_none());
    }

    #[test]
    fn interception_levels_follow_route_segments() {
        let (root, app) = project("levels", &["app/feed/page.tsx", "app/feed/(..)photo/[id]/page.tsx", "app/photo/[id]/page.tsx"]);
        let found = find_interception(&root, &app, "/photo/3", "/feed").expect("(..) goes one segment up");
        assert_eq!(found.slot, None);
        // Only from inside /feed
        assert!(find_interception(&root, &app, "/photo/3", "/elsewhere").is_none());
    }
}
