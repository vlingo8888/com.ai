//! Next.js-style terminal output for `com dev`:
//!
//! ```text
//!  ✓ Ready in 812ms
//!  GET /about 200 in 61ms
//!  ⚠ Rendered /about on the client: react-dom/server could not be loaded
//!  ⨯ ./app/page.tsx:5:3
//!  Expected ';', got 'x'
//! ```
//!
//! Every helper builds the whole message first and writes it with a single call, so concurrent
//! requests never interleave their lines. `NO_COLOR=1` disables ANSI colors.

use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

pub struct Palette {
    pub on: bool,
}

impl Palette {
    pub fn from_env() -> Self {
        Self { on: std::env::var_os("NO_COLOR").is_none() }
    }
    pub fn paint(&self, code: &str, s: &str) -> String {
        if self.on { format!("\x1b[{code}m{s}\x1b[0m") } else { s.to_string() }
    }
    pub fn dim(&self, s: &str) -> String { self.paint("90", s) }
    pub fn bold(&self, s: &str) -> String { self.paint("1;37", s) }
    pub fn cyan(&self, s: &str) -> String { self.paint("1;36", s) }
    pub fn green(&self, s: &str) -> String { self.paint("1;32", s) }
    pub fn red(&self, s: &str) -> String { self.paint("1;31", s) }
    pub fn yellow(&self, s: &str) -> String { self.paint("1;33", s) }
    pub fn magenta(&self, s: &str) -> String { self.paint("1;35", s) }
    pub fn blue(&self, s: &str) -> String { self.paint("34", s) }
}

fn process_start() -> &'static Instant {
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now)
}

/// Marks the moment the engine started; `ready()` reports the time elapsed since then.
pub fn mark_start() {
    process_start();
}

pub fn format_ms(ms: f64) -> String {
    if ms >= 1000.0 { format!("{:.1}s", ms / 1000.0) } else { format!("{}ms", ms.round() as u64) }
}

pub fn format_ready(p: &Palette, ms: f64) -> String {
    format!(" {} Ready in {}\n", p.green("✓"), format_ms(ms))
}

/// Milliseconds since `com dev` started: `COM_START_TIME` (epoch ms, set by the CLI wrapper) when
/// present, so the time spent on tunnels/prompts before spawning the engine is included like Next.js.
fn elapsed_since_start_ms() -> f64 {
    let from_cli = std::env::var("COM_START_TIME").ok().and_then(|v| v.trim().parse::<u128>().ok());
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).ok();
    match (from_cli, now) {
        (Some(start), Some(now)) if now >= start => (now - start) as f64,
        _ => process_start().elapsed().as_secs_f64() * 1000.0,
    }
}

pub fn ready() {
    let ms = elapsed_since_start_ms();
    print!("{}", format_ready(&Palette::from_env(), ms));
}

pub fn format_request(p: &Palette, method: &str, path: &str, status: u16, ms: f64) -> String {
    let status_str = status.to_string();
    let status_colored = match status {
        200..=299 => p.green(&status_str),
        300..=399 => p.cyan(&status_str),
        400..=499 => p.yellow(&status_str),
        _ => p.red(&status_str),
    };
    format!(" {} {} {} in {}\n", method, path, status_colored, format_ms(ms))
}

pub fn request(method: &str, path: &str, status: u16, ms: f64) {
    print!("{}", format_request(&Palette::from_env(), method, path, status, ms));
}

pub fn format_success(p: &Palette, msg: &str) -> String {
    format!(" {} {}\n", p.green("✓"), msg)
}

pub fn success(msg: &str) {
    print!("{}", format_success(&Palette::from_env(), msg));
}

pub fn event(msg: &str) {
    let p = Palette::from_env();
    print!(" {} {}\n", p.dim("○"), msg);
}

pub fn info(msg: &str) {
    let p = Palette::from_env();
    print!(" {} {}\n", p.dim("-"), msg);
}

pub fn warn(msg: &str) {
    let p = Palette::from_env();
    print!(" {} {}\n", p.yellow("⚠"), msg);
}

/// Prints a warning only the first time this exact `key` is seen (e.g. SSR fallback per route + reason).
pub fn warn_once(key: &str, msg: &str) {
    static SEEN: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    let seen = SEEN.get_or_init(|| Mutex::new(HashSet::new()));
    if seen.lock().map(|mut s| s.insert(key.to_string())).unwrap_or(true) {
        warn(msg);
    }
}

pub fn format_error(p: &Palette, title: &str, detail: Option<&str>, frame: Option<&str>) -> String {
    let mut out = format!(" {} {}\n", p.red("⨯"), p.bold(title));
    if let Some(d) = detail.filter(|d| !d.trim().is_empty()) {
        for line in d.trim_end().lines() {
            out.push_str(&format!(" {}\n", line));
        }
    }
    if let Some(f) = frame.filter(|f| !f.trim().is_empty()) {
        for line in f.trim_end().lines() {
            let painted = if line.starts_with('>') || line.trim_start().starts_with('|') && line.contains('^') {
                p.red(line)
            } else {
                p.dim(line)
            };
            out.push_str(&format!("  {}\n", painted));
        }
    }
    out
}

pub fn error(title: &str, detail: Option<&str>, frame: Option<&str>) {
    eprint!("{}", format_error(&Palette::from_env(), title, detail, frame));
}

/// Next.js-style code frame (`> 5 | code` + caret line) around a 1-based `line`/`column`.
pub fn code_frame(source: &str, line: usize, column: usize, context: usize) -> String {
    let lines: Vec<&str> = source.lines().collect();
    if line == 0 || line > lines.len() {
        return String::new();
    }
    let start = line.saturating_sub(context + 1);
    let end = (line + context).min(lines.len());
    let width = end.to_string().len();
    let mut out = String::new();
    for (i, text) in lines.iter().enumerate().take(end).skip(start) {
        let n = i + 1;
        let marker = if n == line { '>' } else { ' ' };
        out.push_str(&format!("{} {:>width$} | {}\n", marker, n, text, width = width));
        if n == line {
            let caret_pad = " ".repeat(column.saturating_sub(1));
            out.push_str(&format!("  {:>width$} | {}^\n", "", caret_pad, width = width));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_line_matches_next_style() {
        let p = Palette { on: false };
        assert_eq!(format_request(&p, "GET", "/about", 200, 61.4), " GET /about 200 in 61ms\n");
        assert_eq!(format_ready(&p, 1520.0), " ✓ Ready in 1.5s\n");
    }

    #[test]
    fn colors_can_be_disabled() {
        let p = Palette { on: false };
        assert!(!format_request(&p, "POST", "/api", 500, 3.0).contains('\x1b'));
        let colored = Palette { on: true };
        assert!(format_request(&colored, "POST", "/api", 500, 3.0).contains("\x1b[1;31m500"));
    }

    #[test]
    fn code_frame_points_at_column() {
        let src = "a\nb\nconst x = ;\nd\ne";
        let frame = code_frame(src, 3, 11, 1);
        assert_eq!(frame, "  2 | b\n> 3 | const x = ;\n    |           ^\n  4 | d\n");
    }
}
