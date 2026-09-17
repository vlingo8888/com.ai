pub mod fs;
pub mod imports;
pub mod tsconfig;
pub mod types;

#[cfg(test)]
mod tests;

pub use fs::FileResolver;
pub use imports::ImportResolver;
pub use tsconfig::TsConfig;
pub use types::{ImportKind, ResolvedFile};

use std::path::Path;

/// Unified facade for File on Disk & Import Specifier resolution
pub struct PathResolver;

impl PathResolver {
    /// Resolves a requested bundle path to a valid file on disk (auto-detecting tsconfig.json)
    pub fn resolve_file<P: AsRef<Path>>(root_dir: P, requested_path: &str) -> Option<ResolvedFile> {
        FileResolver::resolve_file(root_dir, requested_path)
    }

    /// Resolves a requested bundle path using an explicitly provided TsConfig
    pub fn resolve_file_with_config<P: AsRef<Path>>(
        root_dir: P,
        requested_path: &str,
        tsconfig: &TsConfig,
    ) -> Option<ResolvedFile> {
        FileResolver::resolve_file_with_config(root_dir, requested_path, tsconfig)
    }

    /// Transforms TypeScript/TSX code imports into browser-executable ESM specifiers
    pub fn transform_source(code: &str, file_rel_path: &str) -> String {
        ImportResolver::transform_source(code, file_rel_path)
    }

    /// Transforms TypeScript/TSX code imports using an explicitly provided TsConfig
    pub fn transform_source_with_config(
        code: &str,
        file_rel_path: &str,
        tsconfig: &TsConfig,
    ) -> String {
        ImportResolver::transform_source_with_config(code, file_rel_path, tsconfig)
    }

    /// Classifies an import string into its corresponding `ImportKind`
    pub fn classify_import(specifier: &str) -> ImportKind {
        ImportResolver::classify(specifier)
    }
}
