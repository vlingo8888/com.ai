//! `tokio::fs` for wasm: the same functions over `std::fs`, which WASI routes to the host's VFS. A VFS
//! call returns at once (the files are in memory), so there is nothing to gain from a thread.

use std::io;
use std::path::Path;

pub async fn read(path: impl AsRef<Path>) -> io::Result<Vec<u8>> {
    std::fs::read(path)
}

pub async fn read_to_string(path: impl AsRef<Path>) -> io::Result<String> {
    std::fs::read_to_string(path)
}

pub async fn write(path: impl AsRef<Path>, contents: impl AsRef<[u8]>) -> io::Result<()> {
    std::fs::write(path, contents)
}

pub async fn create_dir_all(path: impl AsRef<Path>) -> io::Result<()> {
    std::fs::create_dir_all(path)
}

pub async fn metadata(path: impl AsRef<Path>) -> io::Result<std::fs::Metadata> {
    std::fs::metadata(path)
}

pub async fn remove_file(path: impl AsRef<Path>) -> io::Result<()> {
    std::fs::remove_file(path)
}

pub async fn rename(from: impl AsRef<Path>, to: impl AsRef<Path>) -> io::Result<()> {
    std::fs::rename(from, to)
}
