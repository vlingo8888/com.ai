//! Persistent cache of compiled client modules (`/_bundle/*`), so a restarted `com dev` serves
//! already-seen files without re-running the resolver transforms and SWC.
//!
//! Key = blake3(engine build, file path, source, tsconfig); value = the JavaScript sent to the
//! browser. Stored with redb in `.nata/cache/compile.redb`. The file is dropped and recreated when it
//! grows past `COM_CACHE_MAX_MB` (default 256), so the cache stays bounded. If the database cannot be
//! opened (e.g. another `com dev` holds it), the cache is simply disabled.
//!
//! Writes skip fsync: a durable redb commit is an `F_FULLFSYNC` on macOS (~4ms), and a cold page
//! compiles hundreds of modules one commit each. A background thread makes them durable once writes
//! pause (or every `FLUSH_EVERY` writes); a crash loses at most those entries, which recompile.

use redb::{Database, Durability, ReadableTableMetadata, TableDefinition};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock, Weak};
use std::time::{Duration, Instant};

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

/// Non-durable writes made durable at once
const FLUSH_EVERY: u64 = 64;
/// How long writes must pause before the background flush
const FLUSH_IDLE: Duration = Duration::from_millis(500);

pub struct CompileCache {
    db: Database,
    /// Writes committed without fsync since the last durable commit
    pending: AtomicU64,
    last_write: Mutex<Instant>,
    /// Wakes the flusher after a write, so it sleeps for good while nothing is written
    wrote: Condvar,
}

fn caches() -> &'static Mutex<HashMap<PathBuf, Option<Arc<CompileCache>>>> {
    static CACHES: OnceLock<Mutex<HashMap<PathBuf, Option<Arc<CompileCache>>>>> = OnceLock::new();
    CACHES.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Opens (or creates) a redb file with a small page cache. redb caches up to 1 GiB by default, which
/// a long `com dev` session slowly fills with pages it never reads again; the OS page cache already
/// keeps the hot part of the file in memory.
pub fn open_database(path: &Path) -> Option<Database> {
    const PAGE_CACHE: usize = 4 * 1024 * 1024;
    redb::Builder::new().set_cache_size(PAGE_CACHE).create(path).ok()
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
        map.entry(root.to_path_buf())
            .or_insert_with(|| {
                let cache = Arc::new(Self::open(root)?);
                spawn_flusher(Arc::downgrade(&cache));
                Some(cache)
            })
            .clone()
    }

    fn open(root: &Path) -> Option<CompileCache> {
        let dir = root.join(".nata").join("cache");
        std::fs::create_dir_all(&dir).ok()?;
        let path = dir.join("compile.redb");
        if std::fs::metadata(&path).map(|m| m.len() > max_bytes()).unwrap_or(false) {
            let _ = std::fs::remove_file(&path);
        }
        let db = open_database(&path)?;
        Some(CompileCache { db, pending: AtomicU64::new(0), last_write: Mutex::new(Instant::now()), wrote: Condvar::new() })
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
        let Ok(mut txn) = self.db.begin_write() else { return };
        txn.set_durability(Durability::None);
        {
            let Ok(mut table) = txn.open_table(TABLE) else { return };
            let _ = table.insert(key.as_slice(), compiled.as_bytes());
        }
        if txn.commit().is_err() {
            return;
        }
        // Counted before the flusher can look (under `last_write`), so its wakeup is never lost
        let pending = self.pending.fetch_add(1, Ordering::SeqCst) + 1;
        *self.last_write.lock().unwrap_or_else(|e| e.into_inner()) = Instant::now();
        self.wrote.notify_one();
        if pending >= FLUSH_EVERY {
            self.flush();
        }
    }

    /// Makes every write so far durable (an empty commit persists the non-durable ones before it)
    pub fn flush(&self) {
        let count = self.pending.swap(0, Ordering::SeqCst);
        if count == 0 {
            return;
        }
        let committed = self.db.begin_write().map(|txn| txn.commit().is_ok()).unwrap_or(false);
        if !committed {
            self.pending.fetch_add(count, Ordering::SeqCst);
        }
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

/// Flushes `cache` whenever writes pause, until it is dropped. Blocks while nothing is pending
/// instead of polling, so an idle `com dev` has no thread waking up several times a second.
fn spawn_flusher(cache: Weak<CompileCache>) {
    /// Wait between checks that the cache still exists, when no write comes
    const IDLE_CHECK: Duration = Duration::from_secs(60);
    let _ = std::thread::Builder::new().name("compile-cache-flush".into()).spawn(move || loop {
        let Some(cache) = cache.upgrade() else { return };
        let last_write = cache.last_write.lock().unwrap_or_else(|e| e.into_inner());
        if cache.pending.load(Ordering::SeqCst) == 0 {
            drop(cache.wrote.wait_timeout(last_write, IDLE_CHECK));
            continue;
        }
        let quiet = last_write.elapsed();
        drop(last_write);
        if quiet >= FLUSH_IDLE {
            cache.flush();
        } else {
            drop(cache);
            std::thread::sleep(FLUSH_IDLE - quiet);
        }
    });
}

/// Fingerprint of the project's tsconfig (path aliases change how imports are rewritten)
pub fn tsconfig_fingerprint(root: &Path) -> String {
    let content = std::fs::read(root.join("tsconfig.json")).unwrap_or_default();
    blake3::hash(&content).to_hex().to_string()
}

/// Files `resolution_fingerprint` depends on, by name in the root
const RESOLUTION_FILES: [&str; 3] = ["tsconfig.json", "jsconfig.json", "package.json"];
const LOCKFILES: [&str; 5] = ["bun.lock", "bun.lockb", "package-lock.json", "pnpm-lock.yaml", "yarn.lock"];
const NEXT_CONFIGS: [&str; 5] = ["next.config.js", "next.config.mjs", "next.config.ts", "next.config.cjs", "next.config.mts"];

/// Size and mtime of every file the fingerprint reads, plus the loaded next.config `env`: the same
/// signature means the same fingerprint. About 20 `stat` calls instead of reading and parsing the
/// configs and `.env` files for every compiled module.
fn resolution_signature(root: &Path) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    let files = RESOLUTION_FILES.iter().chain(&LOCKFILES).chain(&NEXT_CONFIGS).chain(&crate::env::FILES);
    for name in files {
        hasher.update(name.as_bytes());
        if let Ok(meta) = std::fs::metadata(root.join(name)) {
            let mtime = meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_nanos());
            hasher.update(&meta.len().to_le_bytes());
            hasher.update(&mtime.to_le_bytes());
        }
    }
    hasher.update(&[crate::framework_react::installed() as u8]);
    if let Some(config) = crate::next_config::for_root(root) {
        hasher.update(config["env"].to_string().as_bytes());
    }
    *hasher.finalize().as_bytes()
}

/// Fingerprint of everything module resolution depends on besides the file itself: tsconfig,
/// `package.json` and the installed packages (lockfile size and mtime, cheap to stat). Remembered per
/// root until one of those files changes.
pub fn resolution_fingerprint(root: &Path) -> String {
    static MEMO: OnceLock<Mutex<HashMap<PathBuf, ([u8; 32], String)>>> = OnceLock::new();
    let signature = resolution_signature(root);
    let memo = MEMO.get_or_init(Default::default);
    if let Some((sig, fingerprint)) = memo.lock().unwrap_or_else(|e| e.into_inner()).get(root) {
        if *sig == signature {
            return fingerprint.clone();
        }
    }
    let fingerprint = compute_resolution_fingerprint(root);
    memo.lock().unwrap_or_else(|e| e.into_inner()).insert(root.to_path_buf(), (signature, fingerprint.clone()));
    fingerprint
}

fn compute_resolution_fingerprint(root: &Path) -> String {
    let mut hasher = blake3::Hasher::new();
    for name in RESOLUTION_FILES {
        hasher.update(&std::fs::read(root.join(name)).unwrap_or_default());
    }
    for name in LOCKFILES {
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
    // Barrel imports of these packages are rewritten in server modules (swc_compiler/barrel.rs)
    if let Some(config) = crate::next_config::for_root(root) {
        hasher.update(config["experimental"]["optimizePackageImports"].to_string().as_bytes());
    }
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
        drop(reopened);

        // Writes skip fsync; a flush makes them survive reopening
        let other = CompileCache::key("app/other.tsx", "export default 3", "ts");
        {
            let cache = CompileCache::open(&root).unwrap();
            cache.put(&other, "other");
            assert_eq!(cache.pending.load(Ordering::SeqCst), 1);
            cache.flush();
            assert_eq!(cache.pending.load(Ordering::SeqCst), 0);
        }
        assert_eq!(CompileCache::open(&root).unwrap().get(&other).as_deref(), Some("other"));
        assert_ne!(key, CompileCache::key("app/page.tsx", "export default 1", "other"));
        let _ = std::fs::remove_dir_all(&root);
    }
}

