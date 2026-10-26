//! The machine-wide package store (`src/store` in the CLI, `com pkg`): a project without
//! `node_modules` finds its packages through links in `.nata/modules/node_modules`, which point at
//! entries of `~/.com/store/v1` shared by every project.
//!
//! The same rules as the CLI's `usesStore` / `packagesDir`:
//! - the store is used when the project has no `node_modules` (or only the link `com pkg link` makes),
//!   unless `COM_STORE=0`;
//! - every place that used to read or write `<root>/node_modules` goes through [`packages_dir`];
//! - nothing is ever written through a link into the store ([`is_shared`]): its files are every
//!   project's.

use std::path::{Path, PathBuf};

pub fn enabled() -> bool {
    !matches!(std::env::var("COM_STORE").as_deref(), Ok("0") | Ok("false") | Ok("off"))
}

/// `<root>/.nata/modules/node_modules`
pub fn farm_dir(root: &Path) -> PathBuf {
    root.join(".nata").join("modules").join("node_modules")
}

fn same_file(a: &Path, b: &Path) -> bool {
    matches!((crate::rpc::canonicalize(&a), crate::rpc::canonicalize(&b)), (Ok(a), Ok(b)) if a == b)
}

/// Whether the project gets its packages from the store
pub fn uses_store(root: &Path) -> bool {
    if !enabled() {
        return false;
    }
    let nm = root.join("node_modules");
    match std::fs::symlink_metadata(&nm) {
        Err(_) => true,
        Ok(meta) => meta.file_type().is_symlink() && same_file(&nm, &farm_dir(root)),
    }
}

/// Where the project's packages are: `node_modules`, or the store links
pub fn packages_dir(root: &Path) -> PathBuf {
    if uses_store(root) {
        farm_dir(root)
    } else {
        root.join("node_modules")
    }
}

/// From `<packages dir>/<name>/` to `.nata/`, for files the framework writes into a package it makes
/// (`core`, the stand-in `next`): `../../.nata` in `node_modules`, `../../..` from
/// `.nata/modules/node_modules/<name>`
pub fn nata_from_package(root: &Path) -> &'static str {
    if uses_store(root) {
        "../../.."
    } else {
        "../../.nata"
    }
}

/// A package directory that is a link (into the store, or `bun link`): the framework never writes
/// into it
pub fn is_shared(dir: &Path) -> bool {
    std::fs::symlink_metadata(dir).is_ok_and(|m| m.file_type().is_symlink())
}

/// `<root>/.nata/modules`: resolving a bare specifier from here finds the store links the standard way
/// (`node_modules` of this folder)
pub fn resolve_dir(root: &Path) -> PathBuf {
    root.join(".nata").join("modules")
}

/// Arguments for Bun processes of a project on the store: no auto-install. Without a `node_modules`,
/// Bun would otherwise resolve packages from its own global cache, in whatever version it holds.
pub fn bun_args(root: &Path) -> &'static [&'static str] {
    if uses_store(root) {
        &["--no-install"]
    } else {
        &[]
    }
}

/// Environment for Bun processes that load the project's packages themselves (the worker, the
/// dependency bundler, the client bundler): Bun honours `NODE_PATH`, for imports, `Bun.build` and
/// `Bun.resolveSync`
pub fn env(root: &Path) -> Vec<(String, String)> {
    if !uses_store(root) {
        return Vec::new();
    }
    let farm = farm_dir(root).to_string_lossy().to_string();
    let sep = if cfg!(windows) { ";" } else { ":" };
    let node_path = match std::env::var("NODE_PATH") {
        Ok(existing) if !existing.is_empty() && !existing.split(sep).any(|p| p == farm) => format!("{farm}{sep}{existing}"),
        Ok(existing) if !existing.is_empty() => existing,
        _ => farm.clone(),
    };
    vec![("NODE_PATH".to_string(), node_path), ("COM_PACKAGES_DIR".to_string(), farm)]
}

/// `~/.com/store/v1`
pub fn store_root() -> PathBuf {
    crate::framework_react::com_home().join("store").join("v1")
}

/// Whether `path` (canonical) is a file of the store: package CSS imported by a project is served from it
pub fn in_store(path: &Path) -> bool {
    let root = store_root();
    let root = crate::rpc::canonicalize(&root).unwrap_or(root);
    path.starts_with(&root)
}

/// `com pkg add` for a package a project imports before installing it (the engine installs it like
/// `bun add` used to); `None` when the CLI is unknown (`COM_CLI`, set by `com dev`)
pub fn add_command(root: &Path, specs: &[&str], dev: bool) -> Option<tokio::process::Command> {
    let cli = std::env::var("COM_CLI").ok().filter(|p| Path::new(p).is_file())?;
    let mut cmd = tokio::process::Command::new(crate::rpc::find_bun_bin());
    cmd.arg(cli).arg("pkg").arg("add").args(specs).arg("--dir").arg(root);
    if dev {
        cmd.arg("-D");
    }
    Some(cmd)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(name: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let root = std::env::temp_dir().join(format!("nata_store_{name}_{nanos}"));
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn projects_without_node_modules_use_the_store_links() {
        let root = project("farm");
        assert!(uses_store(&root));
        assert_eq!(packages_dir(&root), root.join(".nata/modules/node_modules"));
        assert_eq!(nata_from_package(&root), "../../..");
        let env = env(&root);
        assert!(env.iter().any(|(k, v)| k == "NODE_PATH" && v.starts_with(&*farm_dir(&root).to_string_lossy())));

        // `com pkg link`: node_modules is a link to the store links, still the store
        std::fs::create_dir_all(farm_dir(&root)).unwrap();
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(farm_dir(&root), root.join("node_modules")).unwrap();
            assert!(uses_store(&root));
            std::fs::remove_file(root.join("node_modules")).unwrap();
        }

        // A real node_modules: as before
        std::fs::create_dir_all(root.join("node_modules")).unwrap();
        assert!(!uses_store(&root));
        assert_eq!(packages_dir(&root), root.join("node_modules"));
        assert_eq!(nata_from_package(&root), "../../.nata");
        assert!(super::env(&root).is_empty());
        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn links_are_shared_and_never_written() {
        let root = project("shared");
        let real = root.join("real");
        std::fs::create_dir_all(&real).unwrap();
        std::os::unix::fs::symlink(&real, root.join("link")).unwrap();
        assert!(is_shared(&root.join("link")));
        assert!(!is_shared(&real));
        assert!(!is_shared(&root.join("missing")));
        let _ = std::fs::remove_dir_all(root);
    }
}
