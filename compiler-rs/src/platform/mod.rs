//! What the engine needs from the operating system, for the two builds of the same code:
//!
//! - Windows, macOS, Linux: tokio itself (`fs`, `process`, `spawn_blocking`), re-exported as is, so
//!   the native engine behaves exactly as before.
//! - `wasm32-wasip1` (`com-compiler.wasm`, run in the browser by the bun-rust kernel): files through
//!   WASI (the kernel's VFS), child processes (the Bun workers) through the host (`host.rs`), and no
//!   threads: blocking work runs as a task on the single-threaded runtime.
//!
//! Code that would otherwise call `tokio::fs`, `tokio::process` or `tokio::task::spawn_blocking`
//! calls these instead; the caches use `redb` from here (in memory on wasm)
//! and the file watcher `notify` (changes reported by the host on wasm).

#[cfg(not(target_family = "wasm"))]
pub use notify;
#[cfg(not(target_family = "wasm"))]
pub use redb;
#[cfg(not(target_family = "wasm"))]
pub use tokio::{fs, process, task::spawn_blocking};

#[cfg(target_family = "wasm")]
pub mod fs;
#[cfg(target_family = "wasm")]
pub mod host;
#[cfg(target_family = "wasm")]
pub mod notify;
#[cfg(target_family = "wasm")]
pub mod process;
#[cfg(target_family = "wasm")]
pub mod redb;

/// No thread pool on wasm: the closure runs as an ordinary task on the current thread, and the caller
/// awaits the same `JoinHandle` it would get from tokio
#[cfg(target_family = "wasm")]
pub fn spawn_blocking<F, R>(f: F) -> tokio::task::JoinHandle<R>
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    tokio::spawn(async move { f() })
}
