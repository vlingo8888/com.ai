use std::path::{Component, Path, PathBuf};
use super::{tsconfig::TsConfig, types::ResolvedFile};

pub struct FileResolver;

impl FileResolver {
    /// Resolves a requested bundle path to an actual file on disk with automatic extension matching
    pub fn resolve_file<P: AsRef<Path>>(root_dir: P, requested_path: &str) -> Option<ResolvedFile> {
        let tsconfig = TsConfig::load_from_dir(root_dir.as_ref());
        Self::resolve_file_with_config(root_dir, requested_path, &tsconfig)
    }

    /// Resolves a requested bundle path using custom TsConfig path mappings and fuzzy matching
    pub fn resolve_file_with_config<P: AsRef<Path>>(
        root_dir: P,
        requested_path: &str,
        tsconfig: &TsConfig,
    ) -> Option<ResolvedFile> {
        let root = root_dir.as_ref();

        // Strip query parameters if any (e.g. `page.tsx?t=123456`)
        let path_without_query = match requested_path.split_once('?') {
            Some((p, _)) => p,
            None => requested_path,
        };

        let clean_path = path_without_query.trim_start_matches('/');
        if clean_path.is_empty() {
            return None;
        }

        // Security check: prevent directory traversal attacks (e.g. `../../etc/passwd`)
        if Self::has_parent_traversal(clean_path) {
            return None;
        }

        // Generate candidate relative paths to check
        let mut paths_to_check = vec![clean_path.to_string()];

        // If path looks like an alias or matches tsconfig paths, add mapped targets
        if let Some(mapped_targets) = tsconfig.resolve_path_alias(clean_path) {
            for target in mapped_targets {
                let clean_target = target.trim_start_matches("./").to_string();
                if !paths_to_check.contains(&clean_target) {
                    paths_to_check.push(clean_target);
                }
            }
        }

        // Also check with `@/` prefix alias
        let at_alias = format!("@/{}", clean_path);
        if let Some(mapped_targets) = tsconfig.resolve_path_alias(&at_alias) {
            for target in mapped_targets {
                let clean_target = target.trim_start_matches("./").to_string();
                if !paths_to_check.contains(&clean_target) {
                    paths_to_check.push(clean_target);
                }
            }
        }

        // 1. Direct extension candidates pass
        for path_entry in &paths_to_check {
            let candidates = Self::generate_candidates(root, path_entry);
            for cand in candidates {
                if cand.exists() && cand.is_file() {
                    if Self::is_safe_path(root, &cand) {
                        return Some(Self::to_resolved_file(root, cand));
                    }
                }
            }
        }

        // 2. Fuzzy case / kebab-case search pass
        for path_entry in &paths_to_check {
            let p = Path::new(path_entry);
            if let Some(file_name) = p.file_name().and_then(|n| n.to_str()) {
                let parent_rel = p.parent().unwrap_or(Path::new(""));

                // Candidate parent folders: root/parent, root/src/parent, root/app/parent
                let parent_dirs = vec![
                    root.join(parent_rel),
                    root.join("src").join(parent_rel),
                    root.join("app").join(parent_rel),
                ];

                for p_dir in parent_dirs {
                    if let Some(found_path) = Self::fuzzy_search_in_dir(&p_dir, file_name) {
                        if Self::is_safe_path(root, &found_path) {
                            return Some(Self::to_resolved_file(root, found_path));
                        }
                    }
                }
            }
        }

        None
    }

    fn is_safe_path(root: &Path, cand: &Path) -> bool {
        if let Ok(canon_cand) = cand.canonicalize() {
            if let Ok(canon_root) = root.canonicalize() {
                return canon_cand.starts_with(&canon_root);
            }
        }
        false
    }

    fn to_resolved_file(root: &Path, cand: PathBuf) -> ResolvedFile {
        let ext = cand
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_string();

        let rel_path = cand
            .strip_prefix(root)
            .unwrap_or(&cand)
            .to_string_lossy()
            .replace('\\', "/");

        ResolvedFile {
            absolute_path: cand,
            relative_path: rel_path,
            extension: ext,
        }
    }

    /// Checks if a path attempts to escape parent directories
    pub fn has_parent_traversal(path_str: &str) -> bool {
        let path = Path::new(path_str);
        let mut depth: isize = 0;
        for comp in path.components() {
            match comp {
                Component::Normal(_) => depth += 1,
                Component::ParentDir => {
                    depth -= 1;
                    if depth < 0 {
                        return true;
                    }
                }
                _ => {}
            }
        }
        false
    }

    /// Generates candidate file paths in order of preference
    pub fn generate_candidates(root: &Path, rel_path: &str) -> Vec<PathBuf> {
        let mut list = Vec::with_capacity(36);

        // 1. Direct path
        Self::add_extensions(&mut list, &root.join(rel_path));

        // 2. If starts with src/, also check without src/
        if let Some(stripped) = rel_path.strip_prefix("src/") {
            Self::add_extensions(&mut list, &root.join(stripped));
            Self::add_extensions(&mut list, &root.join("app").join(stripped));
        } else {
            // If doesn't start with src/, check with src/ and app/
            Self::add_extensions(&mut list, &root.join("src").join(rel_path));
            Self::add_extensions(&mut list, &root.join("app").join(rel_path));
        }

        list
    }

    fn add_extensions(list: &mut Vec<PathBuf>, base: &Path) {
        let base_str = base.to_string_lossy();
        list.push(base.to_path_buf());
        list.push(PathBuf::from(format!("{}.tsx", base_str)));
        list.push(PathBuf::from(format!("{}.ts", base_str)));
        list.push(PathBuf::from(format!("{}.jsx", base_str)));
        list.push(PathBuf::from(format!("{}.js", base_str)));
        list.push(base.join("index.tsx"));
        list.push(base.join("index.ts"));
        list.push(base.join("index.jsx"));
        list.push(base.join("index.js"));
    }

    fn fuzzy_search_in_dir(parent_dir: &Path, target_name: &str) -> Option<PathBuf> {
        if !parent_dir.exists() || !parent_dir.is_dir() {
            return None;
        }

        let target_stem = target_name
            .trim_end_matches(".tsx")
            .trim_end_matches(".ts")
            .trim_end_matches(".jsx")
            .trim_end_matches(".js");

        let target_norm: String = target_stem
            .chars()
            .filter(|c| c.is_alphanumeric())
            .flat_map(|c| c.to_lowercase())
            .collect();

        if let Ok(entries) = std::fs::read_dir(parent_dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.is_file() {
                    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                        let stem_norm: String = stem
                            .chars()
                            .filter(|c| c.is_alphanumeric())
                            .flat_map(|c| c.to_lowercase())
                            .collect();

                        if stem_norm == target_norm {
                            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                                if matches!(ext, "tsx" | "ts" | "jsx" | "js") {
                                    return Some(path);
                                }
                            }
                        }
                    }
                }
            }
        }
        None
    }
}
