//! Exports of `com-compiler.wasm` (see compiler-rs/src/browser.rs for what each one does).
//!
//! Memory: the host writes an input with `com_alloc(len)` and passes `(ptr, len)`; the function frees
//! it. An output is a pointer to `[u32 LE length][bytes]` (0 = none), which the host frees with
//! `com_free(ptr, 4 + length)` once read.

use compiler_rs::browser;

fn input(ptr: *mut u8, len: usize) -> Vec<u8> {
    if ptr.is_null() || len == 0 {
        return Vec::new();
    }
    unsafe { Vec::from_raw_parts(ptr, len, len) }
}

fn output(bytes: Vec<u8>) -> *mut u8 {
    let mut out = Vec::with_capacity(4 + bytes.len());
    out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    out.extend_from_slice(&bytes);
    let mut out = out.into_boxed_slice();
    let ptr = out.as_mut_ptr();
    std::mem::forget(out);
    ptr
}

#[no_mangle]
pub extern "C" fn com_alloc(len: usize) -> *mut u8 {
    let mut buf = vec![0u8; len].into_boxed_slice();
    let ptr = buf.as_mut_ptr();
    std::mem::forget(buf);
    ptr
}

#[no_mangle]
pub extern "C" fn com_free(ptr: *mut u8, len: usize) {
    if !ptr.is_null() {
        drop(unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(ptr, len)) });
    }
}

#[no_mangle]
pub extern "C" fn com_es5(ptr: *mut u8, len: usize) -> *mut u8 {
    output(browser::es5(&input(ptr, len)))
}

#[no_mangle]
pub extern "C" fn com_tailwind(ptr: *mut u8, len: usize) -> *mut u8 {
    output(browser::tailwind(&input(ptr, len)))
}

#[no_mangle]
pub extern "C" fn com_routes(ptr: *mut u8, len: usize) -> *mut u8 {
    output(browser::routes(&input(ptr, len)))
}

#[no_mangle]
pub extern "C" fn com_dev_start(ptr: *mut u8, len: usize) -> u32 {
    browser::dev_start(&input(ptr, len))
}

#[no_mangle]
pub extern "C" fn com_request(ptr: *mut u8, len: usize) -> u32 {
    browser::request(&input(ptr, len))
}

#[no_mangle]
pub extern "C" fn com_tick() -> u32 {
    browser::tick()
}

#[no_mangle]
pub extern "C" fn com_job_take(id: u32) -> *mut u8 {
    browser::job_take(id).map_or(std::ptr::null_mut(), output)
}

#[no_mangle]
pub extern "C" fn com_hmr_take() -> *mut u8 {
    output(browser::hmr_take())
}

#[no_mangle]
pub extern "C" fn com_file_changed(ptr: *mut u8, len: usize) {
    browser::file_changed(&input(ptr, len))
}

#[no_mangle]
pub extern "C" fn com_wake(pid: i32) {
    browser::wake(pid)
}
