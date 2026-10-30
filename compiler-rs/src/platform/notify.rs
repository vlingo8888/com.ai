//! The part of notify's API `watcher.rs` uses, fed by the host instead of the OS: the bun-rust VFS
//! reports every change through the exported `com_file_changed`, which calls `dispatch`. The watcher
//! logic (debouncing, HMR, worker restarts) is the same as on the native engine.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

pub mod event {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum CreateKind {
        Any,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum ModifyKind {
        Any,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum RemoveKind {
        Any,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventKind {
    Any,
    Create(event::CreateKind),
    Modify(event::ModifyKind),
    Remove(event::RemoveKind),
    Other,
}

#[derive(Debug, Clone)]
pub struct Event {
    pub kind: EventKind,
    pub paths: Vec<PathBuf>,
}

#[derive(Debug)]
pub struct Error(String);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, Copy, Default)]
pub struct Config;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecursiveMode {
    Recursive,
    NonRecursive,
}

type Handler = Box<dyn FnMut(Result<Event>) + Send>;

struct Registration {
    id: u64,
    roots: Vec<(PathBuf, RecursiveMode)>,
    handler: Handler,
}

fn registrations() -> &'static Mutex<Vec<Registration>> {
    static REGISTRATIONS: OnceLock<Mutex<Vec<Registration>>> = OnceLock::new();
    REGISTRATIONS.get_or_init(Default::default)
}

pub trait Watcher {
    fn watch(&mut self, path: &Path, mode: RecursiveMode) -> Result<()>;
}

pub struct RecommendedWatcher {
    id: u64,
}

impl RecommendedWatcher {
    pub fn new<F: FnMut(Result<Event>) + Send + 'static>(handler: F, _config: Config) -> Result<Self> {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        registrations().lock().unwrap().push(Registration { id, roots: Vec::new(), handler: Box::new(handler) });
        Ok(RecommendedWatcher { id })
    }
}

impl Watcher for RecommendedWatcher {
    fn watch(&mut self, path: &Path, mode: RecursiveMode) -> Result<()> {
        let mut regs = registrations().lock().unwrap();
        if let Some(reg) = regs.iter_mut().find(|r| r.id == self.id) {
            reg.roots.push((path.to_path_buf(), mode));
        }
        Ok(())
    }
}

impl Drop for RecommendedWatcher {
    fn drop(&mut self) {
        registrations().lock().unwrap().retain(|r| r.id != self.id);
    }
}

fn watched(roots: &[(PathBuf, RecursiveMode)], path: &Path) -> bool {
    roots.iter().any(|(root, mode)| match mode {
        RecursiveMode::Recursive => path.starts_with(root),
        RecursiveMode::NonRecursive => path.parent() == Some(root.as_path()),
    })
}

/// A change in the host's file system, delivered to every watcher of that path
pub fn dispatch(path: PathBuf, kind: EventKind) {
    let mut regs = registrations().lock().unwrap();
    for reg in regs.iter_mut().filter(|r| watched(&r.roots, &path)) {
        (reg.handler)(Ok(Event { kind, paths: vec![path.clone()] }));
    }
}
