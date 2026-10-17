//! Terminal logging for Server Action (RPC) calls during `com dev`.
//!
//! Every RPC is rendered as one self-contained block (header, request args, each SQL
//! query with params/rows/timing, user console output, response or error) and written
//! with a single `print!`, so concurrent calls never interleave their lines.
//!
//! Environment:
//! - `COM_LOG=off|compact|full` (default `compact`)
//! - `COM_LOG_SLOW_MS=<ms>`      slow-query highlight threshold (default 200)
//! - `NO_COLOR=1`                disable ANSI colors

use crate::term::Palette;
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Off,
    Compact,
    Full,
}

#[derive(Debug, Clone)]
pub struct LogConfig {
    pub level: LogLevel,
    pub slow_ms: f64,
    pub color: bool,
}

impl LogConfig {
    pub fn from_env() -> Self {
        let level = match std::env::var("COM_LOG").unwrap_or_default().trim().to_lowercase().as_str() {
            "off" | "0" | "false" | "none" => LogLevel::Off,
            "full" | "verbose" | "debug" | "2" => LogLevel::Full,
            _ => LogLevel::Compact,
        };
        let slow_ms = std::env::var("COM_LOG_SLOW_MS")
            .ok()
            .and_then(|v| v.trim().parse().ok())
            .unwrap_or(200.0);
        let color = std::env::var_os("NO_COLOR").is_none();
        Self { level, slow_ms, color }
    }
}

/// Everything known about one finished RPC call.
pub struct RpcLogEntry<'a> {
    pub module: &'a str,
    pub action: &'a str,
    pub duration_ms: f64,
    pub args: &'a Value,
    pub queries: &'a [Value],
    pub stdout: &'a str,
    pub stderr: &'a str,
    pub outcome: RpcOutcome<'a>,
}

pub enum RpcOutcome<'a> {
    Ok(&'a Value),
    Err(&'a str),
}

const REDACTED: &str = "‹redacted›";
const SENSITIVE_KEYS: &[&str] = &[
    "password", "passwd", "pwd", "secret", "token", "authorization", "cookie", "cookies",
    "apikey", "api_key", "otp", "pin", "private_key", "service_key", "refresh_token",
    "access_token", "accesstoken", "jwt", "session",
];

fn is_sensitive_key(key: &str) -> bool {
    let k = key.to_lowercase().replace('-', "_");
    SENSITIVE_KEYS.iter().any(|s| k == *s || k.ends_with(&format!("_{s}")) || k.ends_with(s) && s.len() > 4)
}

/// Strings that are secrets regardless of where they appear (e.g. SQL params).
fn looks_like_secret(s: &str) -> bool {
    let bcrypt = s.len() == 60 && (s.starts_with("$2a$") || s.starts_with("$2b$") || s.starts_with("$2y$"));
    let jwt_like = s.len() > 40 && s.starts_with("eyJ") && s.contains('.');
    bcrypt || jwt_like
}

/// Deep-copies a JSON value with sensitive keys and secret-looking strings masked.
pub fn redact(value: &Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(k, v)| {
                    let masked = if is_sensitive_key(k) && !v.is_null() { Value::String(REDACTED.into()) } else { redact(v) };
                    (k.clone(), masked)
                })
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(redact).collect()),
        Value::String(s) if looks_like_secret(s) => Value::String(REDACTED.into()),
        other => other.clone(),
    }
}

/// Collapses whitespace so multi-line SQL fits on one line.
pub fn one_line_sql(sql: &str) -> String {
    sql.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Truncates on a char boundary, appending an ellipsis.
pub fn truncate(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        return s.to_string();
    }
    let cut: String = s.chars().take(max_chars.saturating_sub(1)).collect();
    format!("{cut}…")
}

pub fn human_bytes(n: usize) -> String {
    if n < 1024 {
        format!("{n} B")
    } else if n < 1024 * 1024 {
        format!("{:.1} KB", n as f64 / 1024.0)
    } else {
        format!("{:.1} MB", n as f64 / (1024.0 * 1024.0))
    }
}

pub fn human_ms(ms: f64) -> String {
    if ms >= 1000.0 {
        format!("{:.2}s", ms / 1000.0)
    } else if ms >= 10.0 {
        format!("{ms:.0}ms")
    } else {
        format!("{ms:.1}ms")
    }
}

fn timing(p: &Palette, ms: f64, slow_ms: f64) -> String {
    let t = human_ms(ms);
    if ms >= slow_ms * 5.0 {
        p.red(&format!("{t} slow"))
    } else if ms >= slow_ms {
        p.yellow(&format!("{t} slow"))
    } else {
        p.dim(&t)
    }
}

fn json_preview(value: &Value, level: LogLevel, max_chars: usize) -> Vec<String> {
    let redacted = redact(value);
    match level {
        LogLevel::Full => serde_json::to_string_pretty(&redacted)
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect(),
        _ => vec![truncate(&serde_json::to_string(&redacted).unwrap_or_default(), max_chars)],
    }
}

fn rows_preview(rows: &Value, level: LogLevel) -> Option<Vec<String>> {
    let arr = rows.as_array()?;
    if arr.is_empty() {
        return None;
    }
    let shown = if level == LogLevel::Full { 20 } else { 1 };
    let head = Value::Array(arr.iter().take(shown).cloned().collect());
    let mut lines = json_preview(&head, level, 160);
    if arr.len() > shown {
        if let Some(last) = lines.last_mut() {
            last.push_str(&format!("  … +{} more", arr.len() - shown));
        }
    }
    Some(lines)
}

/// Renders the full block for one RPC call. Returns an empty string when logging is off
/// and the call succeeded quietly (callers then print their own one-line summary).
pub fn render_rpc(entry: &RpcLogEntry, cfg: &LogConfig) -> String {
    let p = Palette { on: cfg.color };
    let mut out: Vec<String> = Vec::new();

    let failed_queries = entry.queries.iter().filter(|q| q.get("status").and_then(Value::as_str) == Some("error")).count();
    let q_count = entry.queries.len();
    let (badge, action) = match entry.outcome {
        RpcOutcome::Ok(_) if failed_queries == 0 => (p.green("✓"), p.cyan(entry.action)),
        RpcOutcome::Ok(_) => (p.yellow("⚠"), p.cyan(entry.action)),
        RpcOutcome::Err(_) => (p.red("⨯"), p.red(entry.action)),
    };
    let mut meta = vec![timing(&p, entry.duration_ms, cfg.slow_ms * 2.5)];
    if q_count > 0 {
        let label = if q_count == 1 { "1 query".to_string() } else { format!("{q_count} queries") };
        meta.push(if failed_queries > 0 { p.red(&format!("{label}, {failed_queries} failed")) } else { p.dim(&label) });
    }
    out.push(format!(
        "  {} {} {} {} {} {}",
        p.magenta("⚡"),
        p.bold(entry.module),
        p.dim("›"),
        action,
        badge,
        meta.join(&p.dim(" · "))
    ));

    if cfg.level == LogLevel::Off {
        if let RpcOutcome::Err(e) = entry.outcome {
            out.push(format!("  {}  {} {}", p.dim("└─"), p.red("error"), e));
        }
        return out.join("\n") + "\n";
    }

    let mut body: Vec<(String, Vec<String>)> = Vec::new(); // (first line, continuation lines)

    let has_args = entry.args.as_array().map_or(!entry.args.is_null(), |a| !a.is_empty());
    if has_args {
        let mut lines = json_preview(entry.args, cfg.level, 200);
        let first = format!("{} {}", p.blue("→ args"), lines.remove(0));
        body.push((first, lines));
    }

    for q in entry.queries {
        let sql = q.get("sql").and_then(Value::as_str).unwrap_or("");
        let ms = q.get("duration_ms").and_then(Value::as_f64).unwrap_or(0.0);
        let is_err = q.get("status").and_then(Value::as_str) == Some("error");
        let row_count = q.get("rowCount").and_then(Value::as_u64).unwrap_or(0);

        let sql_text = if cfg.level == LogLevel::Full { one_line_sql(sql) } else { truncate(&one_line_sql(sql), 220) };
        let stats = if is_err {
            format!("{} {}", p.red("⨯"), timing(&p, ms, cfg.slow_ms))
        } else {
            let rows = if row_count == 1 { "1 row".to_string() } else { format!("{row_count} rows") };
            format!("{} {}", timing(&p, ms, cfg.slow_ms), p.dim(&rows))
        };
        let mut cont = Vec::new();
        if let Some(params) = q.get("parameters").and_then(Value::as_array).filter(|a| !a.is_empty()) {
            let rendered = truncate(&serde_json::to_string(&redact(&Value::Array(params.clone()))).unwrap_or_default(), 200);
            cont.push(format!("{} {}", p.dim("params"), rendered));
        }
        if is_err {
            let err = q.get("error").and_then(Value::as_str).unwrap_or("unknown error");
            cont.push(format!("{} {}", p.red("error "), err));
        } else if let Some(rows) = q.get("response").and_then(|r| rows_preview(r, cfg.level)) {
            let mut rows = rows.into_iter();
            if let Some(first) = rows.next() {
                cont.push(format!("{} {}", p.dim("←     "), first));
            }
            cont.extend(rows.map(|l| format!("       {l}")));
        }
        body.push((format!("{} {} {}", p.paint("1;34", "SQL"), stats, sql_text), cont));
    }

    let user_lines: Vec<&str> = entry.stdout.lines().map(str::trim_end).filter(|l| !l.is_empty()).collect();
    for line in user_lines {
        body.push((format!("{} {}", p.dim("log"), line), vec![]));
    }
    // SQL errors are already rendered from the query log; skip their stderr echo.
    let stderr_lines: Vec<&str> = entry
        .stderr
        .lines()
        .map(str::trim_end)
        .filter(|l| !l.is_empty() && !l.contains("[SQL Error]") && !l.trim_start().starts_with("↳ Error:"))
        .collect();
    for line in stderr_lines {
        body.push((format!("{} {}", p.yellow("warn"), line), vec![]));
    }

    match entry.outcome {
        RpcOutcome::Ok(data) => {
            let size = serde_json::to_string(data).map(|s| s.len()).unwrap_or(0);
            let mut lines = json_preview(data, cfg.level, 240);
            let first = format!("{} {} {}", p.green("← response"), p.dim(&format!("({})", human_bytes(size))), lines.remove(0));
            body.push((first, lines));
        }
        RpcOutcome::Err(e) => {
            let mut lines = e.lines().map(str::to_string);
            let first = format!("{} {}", p.red("← error"), lines.next().unwrap_or_default());
            body.push((first, lines.collect()));
        }
    }

    let n = body.len();
    for (i, (first, cont)) in body.into_iter().enumerate() {
        let last = i + 1 == n;
        let branch = if last { "└─" } else { "├─" };
        let rail = if last { "  " } else { "│ " };
        out.push(format!("  {} {}", p.dim(branch), first));
        for line in cont {
            out.push(format!("  {}   {}", p.dim(rail), line));
        }
    }

    out.join("\n") + "\n"
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn cfg(level: LogLevel) -> LogConfig {
        LogConfig { level, slow_ms: 200.0, color: false }
    }

    #[test]
    fn redacts_sensitive_keys_and_secret_strings() {
        let v = json!({
            "phoneNumber": "0935492113",
            "password": "123456",
            "user": { "accessToken": "abc", "name": "A" },
            "list": ["$2b$10$abcdefghijklmnopqrstuabcdefghijklmnopqrstuvwxyz012345", "ok"],
            "token": null
        });
        let r = redact(&v);
        assert_eq!(r["phoneNumber"], "0935492113");
        assert_eq!(r["password"], REDACTED);
        assert_eq!(r["user"]["accessToken"], REDACTED);
        assert_eq!(r["user"]["name"], "A");
        assert_eq!(r["list"][0], REDACTED);
        assert_eq!(r["list"][1], "ok");
        assert!(r["token"].is_null(), "null values are left as-is");
    }

    #[test]
    fn does_not_redact_unrelated_keys() {
        let r = redact(&json!({ "pinned": true, "spinner": 1, "tokens_used": 3, "sessionCount": 2 }));
        assert_eq!(r["pinned"], true);
        assert_eq!(r["spinner"], 1);
    }

    #[test]
    fn compact_block_contains_request_queries_and_response() {
        let args = json!([{ "phoneNumber": "0935492113", "password": "123456" }]);
        let queries = vec![json!({
            "sql": "select \"id\", \"password\"\n  from \"users\"\n  where \"phone_number\" = $1",
            "parameters": ["0935492113"],
            "response": [{ "id": 826, "password": "$2b$10$abcdefghijklmnopqrstuabcdefghijklmnopqrstuvwxyz012345" }],
            "rowCount": 1,
            "duration_ms": 3.2,
            "status": "success"
        })];
        let data = json!({ "success": true, "token": "eyJzdWIiOjgyNn0.signature-part-that-is-long-enough" });
        let out = render_rpc(
            &RpcLogEntry {
                module: "@/modules/auth",
                action: "loginAction",
                duration_ms: 412.0,
                args: &args,
                queries: &queries,
                stdout: "result ok\n",
                stderr: "",
                outcome: RpcOutcome::Ok(&data),
            },
            &cfg(LogLevel::Compact),
        );
        assert!(out.contains("@/modules/auth › loginAction ✓"));
        assert!(out.contains("1 query"));
        assert!(out.contains(r#"→ args [{"phoneNumber":"0935492113","password":"‹redacted›"}]"#));
        assert!(out.contains(r#"SQL 3.2ms 1 row select "id", "password" from "users" where "phone_number" = $1"#));
        assert!(out.contains(r#"params ["0935492113"]"#));
        assert!(out.contains(r#"[{"id":826,"password":"‹redacted›"}]"#));
        assert!(out.contains("log result ok"));
        assert!(out.contains(r#"← response"#) && out.contains(r#""token":"‹redacted›""#));
        assert!(!out.contains("123456"), "password must never be printed");
        assert!(out.trim_end().lines().last().unwrap().contains("└─"));
    }

    #[test]
    fn failed_query_and_rpc_error_are_rendered() {
        let queries = vec![json!({ "sql": "select 1", "parameters": [], "status": "error", "error": "relation does not exist", "duration_ms": 12.0 })];
        let out = render_rpc(
            &RpcLogEntry {
                module: "@/modules/x",
                action: "boom",
                duration_ms: 20.0,
                args: &json!([]),
                queries: &queries,
                stdout: "",
                stderr: "✖ [SQL Error] select 1 (12ms)\n  ↳ Error: relation does not exist\n",
                outcome: RpcOutcome::Err("relation does not exist"),
            },
            &cfg(LogLevel::Compact),
        );
        assert!(out.contains("boom ⨯"));
        assert!(out.contains("1 query, 1 failed"));
        assert!(out.contains("error  relation does not exist"));
        assert!(out.contains("← error relation does not exist"));
        assert!(!out.contains("[SQL Error]"), "duplicate stderr echo of SQL errors is suppressed");
        assert!(!out.contains("→ args"), "empty args are omitted");
    }

    #[test]
    fn rows_are_previewed_compactly_and_fully() {
        let rows: Vec<Value> = (1..=5).map(|i| json!({ "id": i })).collect();
        let queries = vec![json!({ "sql": "select id from t", "response": rows, "rowCount": 5, "duration_ms": 1.0, "status": "success" })];
        let render = |level| {
            render_rpc(
                &RpcLogEntry {
                    module: "m",
                    action: "a",
                    duration_ms: 1.0,
                    args: &json!([]),
                    queries: &queries,
                    stdout: "",
                    stderr: "",
                    outcome: RpcOutcome::Ok(&json!(null)),
                },
                &cfg(level),
            )
        };
        let compact = render(LogLevel::Compact);
        assert!(compact.contains(r#"[{"id":1}]  … +4 more"#));
        let full = render(LogLevel::Full);
        assert!(full.contains(r#""id": 5"#));
        let off = render(LogLevel::Off);
        assert_eq!(off.lines().count(), 1);
    }

    #[test]
    fn slow_queries_are_flagged() {
        let p = Palette { on: false };
        assert_eq!(timing(&p, 50.0, 200.0), "50ms");
        assert_eq!(timing(&p, 250.0, 200.0), "250ms slow");
        assert_eq!(timing(&p, 1500.0, 200.0), "1.50s slow");
    }

    #[test]
    fn truncate_respects_char_boundaries() {
        assert_eq!(truncate("Phan Thị Ánh Tuyết", 8), "Phan Th…");
        assert_eq!(truncate("abc", 8), "abc");
    }
}
