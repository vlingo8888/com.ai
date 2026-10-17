//! `.env*` files the way Next.js loads them in development: `.env.development.local`, `.env.local`,
//! `.env.development`, `.env` (the first file defining a key wins), and the process environment wins
//! over every file. Values may be quoted (double quotes understand `\n`), lines may start with
//! `export`, unquoted values end at ` #`, and `$VAR` / `${VAR}` expand to other variables.
//!
//! `NEXT_PUBLIC_*` variables (and next.config `env`) are inlined into browser code by the compiler.

use std::collections::BTreeMap;
use std::path::Path;

pub const FILES: [&str; 4] = [".env.development.local", ".env.local", ".env.development", ".env"];

/// Parses one `.env` file
pub fn parse(content: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut lines = content.lines();
    while let Some(line) = lines.next() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("export ").unwrap_or(line);
        let Some((key, rest)) = line.split_once('=') else { continue };
        let key = key.trim().to_string();
        if key.is_empty() || !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '-') {
            continue;
        }
        let rest = rest.trim_start();
        let value = if let Some(quote) = rest.chars().next().filter(|c| *c == '"' || *c == '\'' || *c == '`') {
            // Quoted, possibly over several lines
            let mut text = rest[1..].to_string();
            while !text.contains(quote) {
                match lines.next() {
                    Some(next) => {
                        text.push('\n');
                        text.push_str(next);
                    }
                    None => break,
                }
            }
            let inner = text.split(quote).next().unwrap_or("").to_string();
            if quote == '"' {
                inner.replace("\\n", "\n").replace("\\r", "\r").replace("\\\"", "\"")
            } else {
                inner
            }
        } else {
            rest.split(" #").next().unwrap_or("").trim().to_string()
        };
        out.push((key, value));
    }
    out
}

fn expand(value: &str, vars: &BTreeMap<String, String>) -> String {
    let re = regex::Regex::new(r"\\?\$\{([A-Za-z_][A-Za-z0-9_]*)(?::-([^}]*))?\}|\\?\$([A-Za-z_][A-Za-z0-9_]*)").unwrap();
    re.replace_all(value, |caps: &regex::Captures| {
        let whole = caps.get(0).unwrap().as_str();
        if whole.starts_with('\\') {
            return whole[1..].to_string();
        }
        let name = caps.get(1).or_else(|| caps.get(3)).unwrap().as_str();
        std::env::var(name).ok().or_else(|| vars.get(name).cloned()).filter(|v| !v.is_empty()).unwrap_or_else(|| caps.get(2).map(|d| d.as_str().to_string()).unwrap_or_default())
    })
    .to_string()
}

/// Variables from the `.env*` files (not the process environment), expanded
pub fn load_files(root: &Path) -> BTreeMap<String, String> {
    let mut vars = BTreeMap::new();
    for file in FILES {
        let Ok(content) = std::fs::read_to_string(root.join(file)) else { continue };
        for (k, v) in parse(&content) {
            vars.entry(k).or_insert(v);
        }
    }
    let snapshot = vars.clone();
    vars.into_iter().map(|(k, v)| (k, expand(&v, &snapshot))).collect()
}

/// The value a variable has for the app: process environment first, then the files
pub fn get(root: &Path, key: &str) -> Option<String> {
    std::env::var(key).ok().or_else(|| load_files(root).get(key).cloned())
}

/// What browser code sees: every `NEXT_PUBLIC_*` variable and next.config `env`
pub fn public(root: &Path) -> BTreeMap<String, String> {
    let mut vars: BTreeMap<String, String> = load_files(root).into_iter().filter(|(k, _)| k.starts_with("NEXT_PUBLIC_")).collect();
    for (k, v) in std::env::vars() {
        if k.starts_with("NEXT_PUBLIC_") {
            vars.insert(k, v);
        }
    }
    if let Some(config) = crate::next_config::for_root(root) {
        for (k, v) in config["env"].as_object().into_iter().flatten() {
            if let Some(v) = v.as_str().map(String::from).or_else(|| (!v.is_null()).then(|| v.to_string())) {
                vars.insert(k.clone(), v);
            }
        }
    }
    vars
}

/// Fingerprint of the env files (public values are compiled into browser modules)
pub fn fingerprint(root: &Path) -> String {
    let mut hasher = blake3::Hasher::new();
    for (k, v) in public(root) {
        hasher.update(k.as_bytes());
        hasher.update(b"=");
        hasher.update(v.as_bytes());
        hasher.update(b"\n");
    }
    hasher.finalize().to_hex()[..16].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_like_dotenv() {
        let vars = parse("# comment\nA=1\nexport B=two # trailing\nC=\"line1\\nline2\"\nD='single $NOT'\nE=\"multi\nline\"\n  F = spaced \nbad line\n");
        let get = |k: &str| vars.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone());
        assert_eq!(get("A").as_deref(), Some("1"));
        assert_eq!(get("B").as_deref(), Some("two"));
        assert_eq!(get("C").as_deref(), Some("line1\nline2"));
        assert_eq!(get("D").as_deref(), Some("single $NOT"));
        assert_eq!(get("E").as_deref(), Some("multi\nline"));
        assert_eq!(get("F").as_deref(), Some("spaced"));
    }

    #[test]
    fn next_precedence_and_expansion() {
        let dir = std::env::temp_dir().join(format!("nata_env_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(".env"), "NEXT_PUBLIC_A=env\nHOST=example.com\nURL=https://${HOST}/api\nNEXT_PUBLIC_B=env\n").unwrap();
        std::fs::write(dir.join(".env.development"), "NEXT_PUBLIC_A=development\n").unwrap();
        std::fs::write(dir.join(".env.local"), "NEXT_PUBLIC_A=local\nSECRET=s\n").unwrap();
        let files = load_files(&dir);
        assert_eq!(files["NEXT_PUBLIC_A"], "local");
        assert_eq!(files["URL"], "https://example.com/api");
        let public = public(&dir);
        assert_eq!(public.get("NEXT_PUBLIC_B").map(String::as_str), Some("env"));
        assert!(!public.contains_key("SECRET"), "server-only variables never reach the browser");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
