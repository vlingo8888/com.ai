//! The data cache behind `unstable_cache`, `"use cache"` and `fetch(..., { next })` (ROADMAP P2):
//! entries live in the engine, so they survive worker restarts, and are persisted in
//! `.nata/cache/data.redb`. Memory is bounded (`COM_DATA_CACHE_MB`, default 64): the least recently
//! used entries are dropped first.
//!
//! An entry carries its tags (explicit `cacheTag` / `next.tags`, plus `_N_T_<path>` of the page that
//! wrote it) and its revalidate time; a stale entry is still returned (marked `stale`) so the worker
//! serves it while recomputing, like Next.js.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use redb::{Database, ReadableTable, TableDefinition};
use serde::{Deserialize, Serialize};

const TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("data_cache");

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Entry {
    pub value: String,
    pub tags: Vec<String>,
    /// Seconds the entry stays fresh; `None` = until revalidated
    pub revalidate: Option<f64>,
    /// Unix time (seconds) it was written
    pub written: f64,
}

impl Entry {
    pub fn is_stale(&self, now: f64) -> bool {
        self.revalidate.is_some_and(|r| now - self.written >= r)
    }
}

pub struct DataCache {
    /// Each entry with the tick of its last use
    entries: HashMap<String, (Entry, u64)>,
    /// Keys by last use, oldest first: O(log n) per access instead of scanning a queue
    order: BTreeMap<u64, String>,
    tick: u64,
    bytes: usize,
    max_bytes: usize,
    db: Option<Database>,
}

fn now() -> f64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0)
}

pub(crate) fn tag_matches(pattern: &str, tag: &str) -> bool {
    // `_N_T_/blog*` (revalidatePath(path, "layout")) purges everything below the path
    match pattern.strip_suffix('*') {
        Some(prefix) => tag.starts_with(prefix),
        None => tag == pattern,
    }
}

impl DataCache {
    pub fn new(db_path: Option<&Path>, max_bytes: usize) -> Self {
        let db = db_path.and_then(|p| {
            std::fs::create_dir_all(p.parent()?).ok()?;
            crate::compile_cache::open_database(p)
        });
        let mut cache = Self { entries: HashMap::new(), order: BTreeMap::new(), tick: 0, bytes: 0, max_bytes, db };
        cache.load();
        cache
    }

    fn load(&mut self) {
        let Some(db) = &self.db else { return };
        let Ok(txn) = db.begin_read() else { return };
        let Ok(table) = txn.open_table(TABLE) else { return };
        let Ok(iter) = table.iter() else { return };
        let mut loaded = Vec::new();
        for item in iter.flatten() {
            if let Ok(entry) = serde_json::from_slice::<Entry>(item.1.value()) {
                loaded.push((item.0.value().to_string(), entry));
            }
        }
        for (key, entry) in loaded {
            self.insert_memory(key, entry);
        }
    }

    fn next_tick(&mut self) -> u64 {
        self.tick += 1;
        self.tick
    }

    fn insert_memory(&mut self, key: String, entry: Entry) {
        if let Some((old, used)) = self.entries.remove(&key) {
            self.bytes -= old.value.len() + key.len();
            self.order.remove(&used);
        }
        self.bytes += entry.value.len() + key.len();
        let tick = self.next_tick();
        self.order.insert(tick, key.clone());
        self.entries.insert(key, (entry, tick));
        while self.bytes > self.max_bytes {
            let Some((_, oldest)) = self.order.pop_first() else { break };
            if let Some((old, _)) = self.entries.remove(&oldest) {
                self.bytes -= old.value.len() + oldest.len();
                self.persist_delete(&[oldest]);
            }
        }
    }

    fn persist_delete(&self, keys: &[String]) {
        let Some(db) = &self.db else { return };
        let Ok(txn) = db.begin_write() else { return };
        {
            let Ok(mut table) = txn.open_table(TABLE) else { return };
            for key in keys {
                let _ = table.remove(key.as_str());
            }
        }
        let _ = txn.commit();
    }

    pub fn get(&mut self, key: &str) -> Option<(Entry, bool)> {
        let tick = self.next_tick();
        let (entry, used) = self.entries.get_mut(key)?;
        // Recently used: keep it longer
        let previous = std::mem::replace(used, tick);
        let entry = entry.clone();
        if let Some(k) = self.order.remove(&previous) {
            self.order.insert(tick, k);
        }
        let stale = entry.is_stale(now());
        Some((entry, stale))
    }

    pub fn set(&mut self, key: String, value: String, tags: Vec<String>, revalidate: Option<f64>) {
        let entry = Entry { value, tags, revalidate, written: now() };
        if let Some(db) = &self.db {
            if let (Ok(txn), Ok(bytes)) = (db.begin_write(), serde_json::to_vec(&entry)) {
                if let Ok(mut table) = txn.open_table(TABLE) {
                    let _ = table.insert(key.as_str(), bytes.as_slice());
                }
                let _ = txn.commit();
            }
        }
        self.insert_memory(key, entry);
    }

    /// Drops every entry carrying one of `tags`; returns how many
    pub fn invalidate_tags(&mut self, tags: &[String]) -> usize {
        let keys: Vec<String> = self
            .entries
            .iter()
            .filter(|(_, (e, _))| e.tags.iter().any(|t| tags.iter().any(|p| tag_matches(p, t))))
            .map(|(k, _)| k.clone())
            .collect();
        self.remove(&keys)
    }

    /// Server code changed: results of cached functions may be stale (`fetch` responses are kept)
    pub fn invalidate_code(&mut self) -> usize {
        let keys: Vec<String> = self.entries.keys().filter(|k| !k.starts_with("fetch:")).cloned().collect();
        self.remove(&keys)
    }

    pub fn clear(&mut self) -> usize {
        let keys: Vec<String> = self.entries.keys().cloned().collect();
        self.remove(&keys)
    }

    fn remove(&mut self, keys: &[String]) -> usize {
        for key in keys {
            if let Some((old, used)) = self.entries.remove(key) {
                self.bytes -= old.value.len() + key.len();
                self.order.remove(&used);
            }
        }
        self.persist_delete(keys);
        keys.len()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

fn registry() -> &'static Mutex<HashMap<PathBuf, Arc<Mutex<DataCache>>>> {
    static R: OnceLock<Mutex<HashMap<PathBuf, Arc<Mutex<DataCache>>>>> = OnceLock::new();
    R.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The data cache of a project root
pub fn for_root(root: &Path) -> Arc<Mutex<DataCache>> {
    // Looked up by the root as given first: `canonicalize` is a syscall per path component
    if let Some(cache) = registry().lock().unwrap().get(root) {
        return cache.clone();
    }
    let canonical = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    if let Some(cache) = registry().lock().unwrap().get(&canonical).cloned() {
        registry().lock().unwrap().insert(root.to_path_buf(), cache.clone());
        return cache;
    }
    let root = canonical;
    let max = std::env::var("COM_DATA_CACHE_MB").ok().and_then(|v| v.parse::<usize>().ok()).unwrap_or(64) * 1024 * 1024;
    registry()
        .lock()
        .unwrap()
        .entry(root.clone())
        .or_insert_with(|| Arc::new(Mutex::new(DataCache::new(Some(&root.join(".nata/cache/data.redb")), max))))
        .clone()
}

/// CALL handlers used by `next/cache-runtime.ts`
pub fn handle(root: &Path, cmd: &str, input: &serde_json::Value) -> serde_json::Value {
    let cache = for_root(root);
    let mut cache = cache.lock().unwrap_or_else(|e| e.into_inner());
    match cmd {
        "cache_get" => match cache.get(input["key"].as_str().unwrap_or("")) {
            Some((entry, stale)) => serde_json::json!({ "hit": true, "stale": stale, "value": entry.value }),
            None => serde_json::json!({ "hit": false }),
        },
        "cache_set" => {
            let tags = input["tags"].as_array().map(|a| a.iter().filter_map(|t| t.as_str().map(String::from)).collect()).unwrap_or_default();
            cache.set(
                input["key"].as_str().unwrap_or("").to_string(),
                input["value"].as_str().unwrap_or("null").to_string(),
                tags,
                input["revalidate"].as_f64(),
            );
            serde_json::json!({ "ok": true })
        }
        "cache_invalidate" => {
            let tags: Vec<String> = input["tags"].as_array().map(|a| a.iter().filter_map(|t| t.as_str().map(String::from)).collect()).unwrap_or_default();
            // Pages `com start` serves from its page cache carry the same tags
            crate::prod::invalidate_tags(&tags);
            serde_json::json!({ "removed": cache.invalidate_tags(&tags) })
        }
        _ => serde_json::json!({ "error": format!("unknown cache command {cmd}") }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_db(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("nata_data_cache_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join("data.redb")
    }

    #[test]
    fn stores_tags_and_invalidates_by_tag_and_path_prefix() {
        let mut c = DataCache::new(None, 1 << 20);
        c.set("a".into(), "1".into(), vec!["posts".into(), "_N_T_/blog/1".into()], None);
        c.set("b".into(), "2".into(), vec!["_N_T_/blog/2".into()], None);
        c.set("c".into(), "3".into(), vec!["users".into()], None);
        assert_eq!(c.invalidate_tags(&["posts".into()]), 1);
        assert!(c.get("a").is_none());
        assert_eq!(c.invalidate_tags(&["_N_T_/blog*".into()]), 1);
        assert!(c.get("b").is_none());
        assert_eq!(c.get("c").unwrap().0.value, "3");
    }

    #[test]
    fn marks_entries_stale_after_revalidate_seconds() {
        let mut c = DataCache::new(None, 1 << 20);
        c.set("k".into(), "v".into(), vec![], Some(0.0));
        assert!(c.get("k").unwrap().1, "revalidate 0: stale right away");
        c.set("k2".into(), "v".into(), vec![], Some(3600.0));
        assert!(!c.get("k2").unwrap().1);
        c.set("k3".into(), "v".into(), vec![], None);
        assert!(!c.get("k3").unwrap().1);
    }

    #[test]
    fn evicts_least_recently_used_beyond_the_memory_limit() {
        let mut c = DataCache::new(None, 30);
        c.set("a".into(), "x".repeat(10), vec![], None);
        c.set("b".into(), "y".repeat(10), vec![], None);
        c.get("a");
        c.set("c".into(), "z".repeat(10), vec![], None);
        assert!(c.get("b").is_none(), "least recently used goes first");
        assert!(c.get("a").is_some());
        assert!(c.get("c").is_some());
    }

    #[test]
    fn survives_a_restart() {
        let path = temp_db("persist");
        {
            let mut c = DataCache::new(Some(&path), 1 << 20);
            c.set("k".into(), "\"persisted\"".into(), vec!["t".into()], None);
        }
        let mut reopened = DataCache::new(Some(&path), 1 << 20);
        assert_eq!(reopened.get("k").unwrap().0.value, "\"persisted\"");
        reopened.invalidate_tags(&["t".into()]);
        drop(reopened);
        assert!(DataCache::new(Some(&path), 1 << 20).get("k").is_none());
    }
}
