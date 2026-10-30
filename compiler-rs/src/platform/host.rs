//! Calls into the JavaScript host of `com-compiler.wasm` (import module `com_host`), and the wakers
//! the host triggers through the exported `com_wake` when a child process has output or has exited.
//!
//! Child processes are Bun processes run by the host (in the browser: bun-rust process workers). Every
//! call returns at once: a read with no data yet returns `PENDING`, the task parks, and the host calls
//! `com_wake(pid)` once there is something to read, then drives the runtime again (`com_tick`).

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::task::Waker;

/// No data yet / still running (WASI `EAGAIN`)
pub const PENDING: i32 = -6;

#[link(wasm_import_module = "com_host")]
extern "C" {
    /// `{argv, env, cwd}` as JSON; the new process id, or a negative errno
    fn process_spawn(request: *const u8, len: usize) -> i32;
    /// Bytes written to stdin, or a negative errno once it is closed
    fn process_write(pid: i32, data: *const u8, len: usize) -> i32;
    fn process_close_stdin(pid: i32);
    /// `fd` 1 (stdout) or 2 (stderr): bytes read, 0 at the end, `PENDING`, or a negative errno
    fn process_read(pid: i32, fd: i32, buf: *mut u8, cap: usize) -> i32;
    /// Exit code, or `PENDING` while it runs
    fn process_wait(pid: i32) -> i32;
    fn process_kill(pid: i32);
}

pub fn spawn(request: &[u8]) -> i32 {
    unsafe { process_spawn(request.as_ptr(), request.len()) }
}

pub fn write(pid: i32, data: &[u8]) -> i32 {
    unsafe { process_write(pid, data.as_ptr(), data.len()) }
}

pub fn close_stdin(pid: i32) {
    unsafe { process_close_stdin(pid) }
}

pub fn read(pid: i32, fd: i32, buf: &mut [u8]) -> i32 {
    unsafe { process_read(pid, fd, buf.as_mut_ptr(), buf.len()) }
}

pub fn wait(pid: i32) -> i32 {
    unsafe { process_wait(pid) }
}

pub fn kill(pid: i32) {
    unsafe { process_kill(pid) }
}

/// Tasks waiting on a process, by (pid, fd); fd 0 waits for the exit
fn wakers() -> &'static Mutex<HashMap<(i32, i32), Waker>> {
    static WAKERS: OnceLock<Mutex<HashMap<(i32, i32), Waker>>> = OnceLock::new();
    WAKERS.get_or_init(Default::default)
}

pub fn park(pid: i32, fd: i32, waker: &Waker) {
    wakers().lock().unwrap().insert((pid, fd), waker.clone());
}

/// Wakes every task waiting on `pid` (`com_wake`)
pub fn wake(pid: i32) {
    let woken: Vec<Waker> = {
        let mut map = wakers().lock().unwrap();
        let keys: Vec<_> = map.keys().filter(|(p, _)| *p == pid).copied().collect();
        keys.into_iter().filter_map(|k| map.remove(&k)).collect()
    };
    woken.into_iter().for_each(Waker::wake);
}
