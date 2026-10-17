//! Persistent cache of compiled client modules (`/_bundle/*`), so a restarted `com dev` serves
//! already-seen files without re-running the resolver transforms and SWC.
//!
//! Key = blake3(engine build, file path, source, tsconfig); value = the JavaScript sent to the
//! browser. Stored with redb in `.nata/cache/compile.redb`. The file is dropped and recreated when it
//! grows past `COM_CACHE_MAX_MB` (default 256), so the cache stays bounded. If the database cannot be
//! opened (e.g. another `com dev` holds it), the cache is simply disabled.

use redb::{Database, ReadableTableMetadata, TableDefinition};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

const TABLE: TableDefinition<&[u8], &[u8]> = TableDefinition::new("compiled_modules");

/// Identifies the engine build: rebuilding it (new transforms, new SWC) must invalidate cached output,
/// and `CARGO_PKG_VERSION` does not change between development builds
fn engine_fingerprint() -> &'static str {
    static FINGERPRINT: OnceLock<String> = OnceLock::new();
    FINGERPRINT.get_or_init(|| {
        let exe = std::env::current_exe().ok().and_then(|p| std::fs::metadata(p).ok());
        let (len, mtime) = exe
            .map(|m| (m.len(), m.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_nanos())))
            .unwrap_or((0, 0));
        format!("{}:{len}:{mtime}", env!("CARGO_PKG_VERSION"))
    })
}

pub struct CompileCache {
    db: Database,
}

fn caches() -> &'static Mutex<HashMap<PathBuf, Option<Arc<CompileCache>>>> {
    static CACHES: OnceLock<Mutex<HashMap<PathBuf, Option<Arc<CompileCache>>>>> = OnceLock::new();
    CACHES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn max_bytes() -> u64 {
    std::env::var("COM_CACHE_MAX_MB").ok().and_then(|v| v.parse::<u64>().ok()).unwrap_or(256) * 1024 * 1024
}

fn enabled() -> bool {
    !matches!(std::env::var("COM_CACHE").as_deref(), Ok("0") | Ok("false") | Ok("off"))
}

impl CompileCache {
    /// The cache for a project root, opened on first use; `None` when disabled or unavailable
    pub fn for_root(root: &Path) -> Option<Arc<CompileCache>> {
        if !enabled() {
            return None;
        }
        let mut map = caches().lock().unwrap_or_else(|e| e.into_inner());
        map.entry(root.to_path_buf()).or_insert_with(|| Self::open(root).map(Arc::new)).clone()
    }

    fn open(root: &Path) -> Option<CompileCache> {
        let dir = root.join(".nata").join("cache");
        std::fs::create_dir_all(&dir).ok()?;
        let path = dir.join("compile.redb");
        if std::fs::metadata(&path).map(|m| m.len() > max_bytes()).unwrap_or(false) {
            let _ = std::fs::remove_file(&path);
        }
        let db = Database::create(&path).ok()?;
        Some(CompileCache { db })
    }

    /// Cache key for one module; any change to the inputs produces a new key
    pub fn key(relative_path: &str, source: &str, tsconfig_fingerprint: &str) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        for part in [engine_fingerprint(), relative_path, source, tsconfig_fingerprint] {
            hasher.update(&(part.len() as u64).to_le_bytes());
            hasher.update(part.as_bytes());
        }
        *hasher.finalize().as_bytes()
    }

    pub fn get(&self, key: &[u8; 32]) -> Option<String> {
        let txn = self.db.begin_read().ok()?;
        let table = txn.open_table(TABLE).ok()?;
        let value = table.get(key.as_slice()).ok()??;
        String::from_utf8(value.value().to_vec()).ok()
    }

    pub fn put(&self, key: &[u8; 32], compiled: &str) {
        let Ok(txn) = self.db.begin_write() else { return };
        {
            let Ok(mut table) = txn.open_table(TABLE) else { return };
            let _ = table.insert(key.as_slice(), compiled.as_bytes());
        }
        let _ = txn.commit();
    }

    pub fn len(&self) -> u64 {
        self.db
            .begin_read()
            .ok()
            .and_then(|txn| txn.open_table(TABLE).ok())
            .and_then(|table| table.len().ok())
            .unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Fingerprint of the project's tsconfig (path aliases change how imports are rewritten)
pub fn tsconfig_fingerprint(root: &Path) -> String {
    let content = std::fs::read(root.join("tsconfig.json")).unwrap_or_default();
    blake3::hash(&content).to_hex().to_string()
}

/// Fingerprint of everything module resolution depends on besides the file itself: tsconfig,
/// `package.json` and the installed packages (lockfile size and mtime, cheap to stat)
pub fn resolution_fingerprint(root: &Path) -> String {
    let mut hasher = blake3::Hasher::new();
    for name in ["tsconfig.json", "jsconfig.json", "package.json"] {
        hasher.update(&std::fs::read(root.join(name)).unwrap_or_default());
    }
    for name in ["bun.lock", "bun.lockb", "package-lock.json", "pnpm-lock.yaml", "yarn.lock"] {
        if let Ok(meta) = std::fs::metadata(root.join(name)) {
            let mtime = meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_nanos());
            hasher.update(name.as_bytes());
            hasher.update(&meta.len().to_le_bytes());
            hasher.update(&mtime.to_le_bytes());
        }
    }
    // Server modules import React from the framework's copy when the project's is too old
    if crate::framework_react::active_modules_dir(root).is_some() {
        hasher.update(b"framework-react:");
        hasher.update(crate::framework_react::VERSION.as_bytes());
    }
    // NEXT_PUBLIC_* values are compiled into browser modules
    hasher.update(crate::env::fingerprint(root).as_bytes());
    hasher.finalize().to_hex().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stores_and_reloads_compiled_modules() {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let root = std::env::temp_dir().join(format!("nata_cache_test_{nanos}"));
        std::fs::create_dir_all(&root).unwrap();

        let key = CompileCache::key("app/page.tsx", "export default 1", "ts");
        {
            let cache = CompileCache::open(&root).unwrap();
            assert!(cache.get(&key).is_none());
            cache.put(&key, "compiled");
            assert_eq!(cache.get(&key).as_deref(), Some("compiled"));
        }
        // Survives reopening (i.e. a `com dev` restart)
        let reopened = CompileCache::open(&root).unwrap();
        assert_eq!(reopened.get(&key).as_deref(), Some("compiled"));
        assert_eq!(reopened.len(), 1);

        assert_ne!(key, CompileCache::key("app/page.tsx", "export default 2", "ts"));
        assert_ne!(key, CompileCache::key("app/page.tsx", "export default 1", "other"));
        let _ = std::fs::remove_dir_all(&root);
    }
}
