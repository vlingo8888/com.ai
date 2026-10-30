//! `tokio::process` for wasm: the part of its API the engine uses, with the process run by the host
//! (`host.rs`). Stdin and stdout are always pipes; stderr is a pipe too, and the host also shows it
//! in the console, so `Stdio::inherit()` keeps its meaning.

use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::io;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::task::{Context, Poll};

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, ReadBuf};

use super::host;

fn lossy(s: impl AsRef<OsStr>) -> String {
    s.as_ref().to_string_lossy().into_owned()
}

fn errno(code: i32, what: &str) -> io::Error {
    io::Error::other(format!("{what}: host error {}", -code))
}

pub struct Command {
    argv: Vec<String>,
    env: BTreeMap<String, Option<String>>,
    cwd: Option<PathBuf>,
    kill_on_drop: bool,
}

impl Command {
    pub fn new(program: impl AsRef<OsStr>) -> Self {
        Command { argv: vec![lossy(program)], env: BTreeMap::new(), cwd: None, kill_on_drop: false }
    }

    pub fn arg(&mut self, arg: impl AsRef<OsStr>) -> &mut Self {
        self.argv.push(lossy(arg));
        self
    }

    pub fn args<I, S>(&mut self, args: I) -> &mut Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.argv.extend(args.into_iter().map(lossy));
        self
    }

    pub fn env(&mut self, key: impl AsRef<OsStr>, value: impl AsRef<OsStr>) -> &mut Self {
        self.env.insert(lossy(key), Some(lossy(value)));
        self
    }

    pub fn envs<I, K, V>(&mut self, vars: I) -> &mut Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: AsRef<OsStr>,
        V: AsRef<OsStr>,
    {
        for (k, v) in vars {
            self.env(k, v);
        }
        self
    }

    pub fn env_remove(&mut self, key: impl AsRef<OsStr>) -> &mut Self {
        self.env.insert(lossy(key), None);
        self
    }

    pub fn current_dir(&mut self, dir: impl AsRef<Path>) -> &mut Self {
        self.cwd = Some(dir.as_ref().to_path_buf());
        self
    }

    pub fn stdin(&mut self, _cfg: impl Into<std::process::Stdio>) -> &mut Self {
        self
    }

    pub fn stdout(&mut self, _cfg: impl Into<std::process::Stdio>) -> &mut Self {
        self
    }

    pub fn stderr(&mut self, _cfg: impl Into<std::process::Stdio>) -> &mut Self {
        self
    }

    pub fn kill_on_drop(&mut self, kill: bool) -> &mut Self {
        self.kill_on_drop = kill;
        self
    }

    pub fn spawn(&mut self) -> io::Result<Child> {
        let request = serde_json::json!({
            "argv": self.argv,
            "env": self.env,
            "cwd": self.cwd.as_ref().map(|p| p.to_string_lossy()),
        });
        let pid = host::spawn(request.to_string().as_bytes());
        if pid < 0 {
            return Err(errno(pid, &format!("could not start {}", self.argv[0])));
        }
        Ok(Child {
            pid,
            stdin: Some(ChildStdin { pid }),
            stdout: Some(ChildStdout { pid, fd: 1 }),
            stderr: Some(ChildStdout { pid, fd: 2 }),
            exit: None,
            kill_on_drop: self.kill_on_drop,
        })
    }

    pub async fn output(&mut self) -> io::Result<Output> {
        let mut child = self.spawn()?;
        child.stdin.take().map(|s| host::close_stdin(s.pid));
        let (mut out, mut err) = (child.stdout.take().unwrap(), child.stderr.take().unwrap());
        let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
        let (a, b) = tokio::join!(out.read_to_end(&mut stdout), err.read_to_end(&mut stderr));
        a?;
        b?;
        let status = child.wait().await?;
        Ok(Output { status, stdout, stderr })
    }

    pub async fn status(&mut self) -> io::Result<ExitStatus> {
        Ok(self.output().await?.status)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExitStatus(i32);

impl ExitStatus {
    pub fn success(&self) -> bool {
        self.0 == 0
    }

    pub fn code(&self) -> Option<i32> {
        Some(self.0)
    }
}

impl std::fmt::Display for ExitStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "exit status: {}", self.0)
    }
}

#[derive(Debug)]
pub struct Output {
    pub status: ExitStatus,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

pub struct Child {
    pid: i32,
    pub stdin: Option<ChildStdin>,
    pub stdout: Option<ChildStdout>,
    pub stderr: Option<ChildStderr>,
    exit: Option<ExitStatus>,
    kill_on_drop: bool,
}

impl Child {
    pub fn id(&self) -> Option<u32> {
        self.exit.is_none().then_some(self.pid as u32)
    }

    pub async fn wait(&mut self) -> io::Result<ExitStatus> {
        if let Some(status) = self.exit {
            return Ok(status);
        }
        let pid = self.pid;
        let code = std::future::poll_fn(|cx| match host::wait(pid) {
            host::PENDING => {
                host::park(pid, 0, cx.waker());
                Poll::Pending
            }
            code => Poll::Ready(code),
        })
        .await;
        let status = ExitStatus(code);
        self.exit = Some(status);
        Ok(status)
    }

    pub fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        if self.exit.is_none() {
            let code = host::wait(self.pid);
            if code != host::PENDING {
                self.exit = Some(ExitStatus(code));
            }
        }
        Ok(self.exit)
    }

    pub fn start_kill(&mut self) -> io::Result<()> {
        host::kill(self.pid);
        Ok(())
    }

    pub async fn kill(&mut self) -> io::Result<()> {
        self.start_kill()?;
        self.wait().await.map(|_| ())
    }
}

impl Drop for Child {
    fn drop(&mut self) {
        if self.kill_on_drop && self.exit.is_none() {
            host::kill(self.pid);
        }
    }
}

pub struct ChildStdin {
    pid: i32,
}

impl AsyncWrite for ChildStdin {
    fn poll_write(self: Pin<&mut Self>, _cx: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> {
        // The host buffers whatever it is given
        let n = host::write(self.pid, buf);
        Poll::Ready(if n < 0 { Err(io::ErrorKind::BrokenPipe.into()) } else { Ok(n as usize) })
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        host::close_stdin(self.pid);
        Poll::Ready(Ok(()))
    }
}

/// Stdout (fd 1) or stderr (fd 2) of a child
pub struct ChildStdout {
    pid: i32,
    fd: i32,
}

pub type ChildStderr = ChildStdout;

impl AsyncRead for ChildStdout {
    fn poll_read(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<io::Result<()>> {
        let n = host::read(self.pid, self.fd, buf.initialize_unfilled());
        match n {
            host::PENDING => {
                host::park(self.pid, self.fd, cx.waker());
                Poll::Pending
            }
            n if n < 0 => Poll::Ready(Err(errno(n, "read from child process"))),
            n => {
                buf.advance(n as usize);
                Poll::Ready(Ok(()))
            }
        }
    }
}
