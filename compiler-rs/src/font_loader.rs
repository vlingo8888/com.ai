//! next/font (ROADMAP P2): turns the font requests the compiler recorded (`swc_compiler/fonts.rs`)
//! into CSS, like Next.js:
//! - Google fonts are downloaded once (CSS from fonts.googleapis.com, woff2 files) into
//!   `.nata/fonts/` and self-hosted at `/_nata/fonts/<file>`; only the requested `subsets` are kept.
//!   Nothing is fetched from Google at page load.
//! - Local fonts (`src` relative to the importing module) are served at `/_nata/fonts/local/...`.
//! - A fallback face (`local("Arial")` etc.) gets `size-adjust` / `ascent-override` /
//!   `descent-override` / `line-gap-override` from the real font's metrics (`ttf-parser`), so text
//!   does not shift when the web font arrives.
//! - Each request gets `.__className_<hash>` (font-family, weight, style) and, with `variable`,
//!   `.__variable_<hash>` defining the CSS variable.
//! Offline, the CSS still has the classes and the fallback face.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use crate::swc_compiler::fonts::FontRequest;

const MODERN_UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0 Safari/537.36";

/// Metrics of a font, in font units
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Metrics {
    pub units_per_em: f64,
    pub ascent: f64,
    pub descent: f64,
    pub line_gap: f64,
    pub avg_width: f64,
}

impl Metrics {
    pub fn from_font(data: &[u8]) -> Option<Self> {
        let face = ttf_parser::Face::parse(data, 0).ok()?;
        // Average width of text, weighted by English letter frequency (what capsize, and so Next.js,
        // calls xWidthAvg; the fallback metrics below use the same measure)
        const FREQUENCIES: [(char, f64); 27] = [
            ('a', 0.0668), ('b', 0.0122), ('c', 0.0228), ('d', 0.0348), ('e', 0.1039), ('f', 0.0182), ('g', 0.0165), ('h', 0.0499),
            ('i', 0.057), ('j', 0.0013), ('k', 0.0063), ('l', 0.0329), ('m', 0.0197), ('n', 0.0552), ('o', 0.0614), ('p', 0.0158),
            ('q', 0.0008), ('r', 0.049), ('s', 0.0518), ('t', 0.0741), ('u', 0.0226), ('v', 0.008), ('w', 0.0193), ('x', 0.0012),
            ('y', 0.0162), ('z', 0.0006), (' ', 0.1818),
        ];
        let (mut sum, mut weight) = (0.0, 0.0);
        for (c, f) in FREQUENCIES {
            if let Some(advance) = face.glyph_index(c).and_then(|g| face.glyph_hor_advance(g)) {
                sum += advance as f64 * f;
                weight += f;
            }
        }
        let avg_width = if weight > 0.0 { sum / weight } else { face.units_per_em() as f64 * 0.5 };
        Some(Self {
            units_per_em: face.units_per_em() as f64,
            ascent: face.ascender() as f64,
            descent: face.descender() as f64,
            line_gap: face.line_gap() as f64,
            avg_width,
        })
    }
}

/// System fonts used for the size-adjusted fallback (metrics as published for these fonts)
const ARIAL: (&str, Metrics) = ("Arial", Metrics { units_per_em: 2048.0, ascent: 1854.0, descent: -434.0, line_gap: 67.0, avg_width: 904.0 });
const TIMES: (&str, Metrics) = ("Times New Roman", Metrics { units_per_em: 2048.0, ascent: 1825.0, descent: -443.0, line_gap: 87.0, avg_width: 821.0 });
const COURIER: (&str, Metrics) = ("Courier New", Metrics { units_per_em: 2048.0, ascent: 1705.0, descent: -615.0, line_gap: 0.0, avg_width: 1229.0 });

fn fallback_font(request: &FontRequest) -> (&'static str, Metrics) {
    let name = request.family.to_lowercase();
    let adjust = request.options["adjustFontFallback"].as_str().unwrap_or("");
    if adjust.contains("Times") || (name.contains("serif") && !name.contains("sans")) || name.contains("garamond") || name.contains("playfair") || name.contains("merriweather") {
        TIMES
    } else if name.contains("mono") || name.contains("code") {
        COURIER
    } else {
        ARIAL
    }
}

/// The size-adjusted fallback face (Next.js computes it the same way)
pub fn fallback_face(request: &FontRequest, metrics: Option<Metrics>) -> String {
    let (local, fallback) = fallback_font(request);
    let Some(m) = metrics.filter(|_| request.options["adjustFontFallback"] != serde_json::Value::Bool(false)) else {
        return format!("@font-face {{ font-family: '{}'; src: local(\"{local}\"); }}\n", request.fallback_name());
    };
    let size_adjust = (m.avg_width / m.units_per_em) / (fallback.avg_width / fallback.units_per_em);
    let pct = |v: f64| format!("{:.2}%", v * 100.0);
    format!(
        "@font-face {{ font-family: '{}'; src: local(\"{local}\"); ascent-override: {}; descent-override: {}; line-gap-override: {}; size-adjust: {}; }}\n",
        request.fallback_name(),
        pct(m.ascent / (m.units_per_em * size_adjust)),
        pct(m.descent.abs() / (m.units_per_em * size_adjust)),
        pct(m.line_gap / (m.units_per_em * size_adjust)),
        pct(size_adjust),
    )
}

fn list(value: &serde_json::Value) -> Vec<String> {
    match value {
        serde_json::Value::String(s) => vec![s.clone()],
        serde_json::Value::Array(a) => a.iter().filter_map(|v| v.as_str().map(String::from)).collect(),
        _ => vec![],
    }
}

/// css2 query for a Google font: `Inter:ital,wght@0,400;1,700`, or the variable range when no
/// weight is given
pub fn google_query(request: &FontRequest, variable_range: bool) -> String {
    let family = request.family.replace(' ', "+");
    let weights: Vec<String> = list(&request.options["weight"]).into_iter().filter(|w| w != "variable").collect();
    let styles = list(&request.options["style"]);
    let italic = styles.iter().any(|s| s == "italic");
    let normal = styles.is_empty() || styles.iter().any(|s| s == "normal");
    let weight_values: Vec<String> = if weights.is_empty() {
        if variable_range { vec!["100..900".into()] } else { vec![] }
    } else {
        weights
    };
    if weight_values.is_empty() {
        return if italic { format!("{family}:ital@{}", if normal { "0;1" } else { "1" }) } else { family };
    }
    if italic {
        let mut tuples = Vec::new();
        for (flag, wanted) in [("0", normal), ("1", true)] {
            if wanted {
                tuples.extend(weight_values.iter().map(|w| format!("{flag},{w}")));
            }
        }
        format!("{family}:ital,wght@{}", tuples.join(";"))
    } else {
        format!("{family}:wght@{}", weight_values.join(";"))
    }
}

/// Keeps the `@font-face` blocks of the wanted subsets (Google labels each with `/* latin */`)
pub fn filter_subsets(css: &str, subsets: &[String]) -> String {
    if subsets.is_empty() {
        return css.to_string();
    }
    let mut out = String::new();
    let mut current: Option<String> = None;
    for block in css.split("@font-face") {
        let trimmed = block.trim();
        if trimmed.is_empty() {
            continue;
        }
        // The comment naming the subset ends the previous block
        let (body, next_label) = match trimmed.rfind("/*") {
            Some(idx) if trimmed[idx..].contains("*/") && trimmed[..idx].contains('}') => {
                let label = trimmed[idx + 2..trimmed[idx..].find("*/").unwrap() + idx].trim().to_string();
                (trimmed[..idx].trim(), Some(label))
            }
            _ => (trimmed, None),
        };
        if body.starts_with('{') && current.as_ref().is_none_or(|label| subsets.iter().any(|s| s == label)) {
            out.push_str("@font-face ");
            out.push_str(body);
            out.push('\n');
        }
        if let Some(label) = next_label {
            current = Some(label);
        }
        if body.starts_with("/*") {
            current = body.trim_start_matches("/*").split("*/").next().map(|s| s.trim().to_string());
        }
    }
    out
}

fn class_rules(request: &FontRequest) -> String {
    let mut decl = format!("font-family: {};", request.font_family());
    let result = request.result();
    if let Some(w) = result["style"]["fontWeight"].as_u64().map(|w| w.to_string()).or_else(|| result["style"]["fontWeight"].as_str().map(String::from)) {
        decl.push_str(&format!(" font-weight: {w};"));
    }
    if let Some(s) = result["style"]["fontStyle"].as_str() {
        decl.push_str(&format!(" font-style: {s};"));
    }
    let mut css = format!(".{} {{ {decl} }}\n", request.class_name());
    if let (Some(class), Some(var)) = (request.variable_class(), request.options["variable"].as_str()) {
        css.push_str(&format!(".{class} {{ {var}: {}; }}\n", request.font_family()));
    }
    css
}

fn generated() -> &'static Mutex<HashMap<String, String>> {
    static G: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
    G.get_or_init(Default::default)
}

/// `/_nata/fonts/local/<hash>-<i>.<ext>` → file
fn local_files() -> &'static Mutex<HashMap<String, PathBuf>> {
    static L: OnceLock<Mutex<HashMap<String, PathBuf>>> = OnceLock::new();
    L.get_or_init(Default::default)
}

async fn fetch(root: &Path, url: &str, user_agent: Option<&str>) -> Option<Vec<u8>> {
    let headers = user_agent.map(|ua| serde_json::json!({ "user-agent": ua })).unwrap_or(serde_json::json!({}));
    let reply = crate::worker_pool::call(root, "fetch_bytes", serde_json::json!({ "url": url, "headers": headers }), Vec::new()).await.ok()?;
    (reply.error().is_none() && reply.output()["ok"].as_bool() == Some(true)).then_some(reply.body)
}

async fn google_css(root: &Path, request: &FontRequest) -> Option<(String, Option<Metrics>)> {
    let display = request.options["display"].as_str().unwrap_or("swap");
    let mut css = None;
    for variable in [true, false] {
        let url = format!("https://fonts.googleapis.com/css2?family={}&display={display}", google_query(request, variable));
        if let Some(bytes) = fetch(root, &url, Some(MODERN_UA)).await {
            let text = String::from_utf8_lossy(&bytes).to_string();
            if text.contains("@font-face") {
                css = Some(text);
                break;
            }
        }
    }
    let css = css?;
    let subsets = list(&request.options["subsets"]);
    let mut css = filter_subsets(&css, &subsets);
    // Self-host every file
    let dir = root.join(".nata/fonts");
    let _ = tokio::fs::create_dir_all(&dir).await;
    let url_re = regex::Regex::new(r"url\((https://[^)]+)\)").unwrap();
    let urls: Vec<String> = url_re.captures_iter(&css).map(|c| c[1].to_string()).collect();
    for url in urls {
        let ext = url.rsplit('.').next().filter(|e| e.len() <= 5).unwrap_or("woff2");
        let file = format!("{}.{}", &blake3::hash(url.as_bytes()).to_hex()[..16], ext);
        let path = dir.join(&file);
        if !path.is_file() {
            let Some(bytes) = fetch(root, &url, Some(MODERN_UA)).await else { continue };
            let _ = tokio::fs::write(&path, bytes).await;
        }
        css = css.replace(&format!("url({url})"), &format!("url(/_nata/fonts/{file})"));
    }
    css = regex::Regex::new(r"font-family:\s*'[^']*';").unwrap().replace_all(&css, format!("font-family: '{}';", request.family_name()).as_str()).to_string();
    // Metrics: Google sends TrueType to a plain user agent
    let ttf_css = fetch(root, &format!("https://fonts.googleapis.com/css2?family={}", google_query(request, false)), None).await;
    let mut metrics = None;
    if let Some(ttf_url) = ttf_css.and_then(|b| url_re.captures(&String::from_utf8_lossy(&b)).map(|c| c[1].to_string())) {
        if let Some(data) = fetch(root, &ttf_url, None).await {
            metrics = Metrics::from_font(&data);
        }
    }
    Some((css, metrics))
}

fn local_css(request: &FontRequest) -> (String, Option<Metrics>) {
    let importer_dir = Path::new(&request.importer).parent().map(Path::to_path_buf).unwrap_or_default();
    let display = request.options["display"].as_str().unwrap_or("swap");
    let sources: Vec<(String, Option<String>, Option<String>)> = match &request.options["src"] {
        serde_json::Value::String(s) => vec![(s.clone(), request.options["weight"].as_str().map(String::from), request.options["style"].as_str().map(String::from))],
        serde_json::Value::Array(a) => a
            .iter()
            .filter_map(|s| Some((s["path"].as_str()?.to_string(), s["weight"].as_str().map(String::from), s["style"].as_str().map(String::from))))
            .collect(),
        _ => vec![],
    };
    let mut css = String::new();
    let mut metrics = None;
    for (i, (src, weight, style)) in sources.iter().enumerate() {
        let file = importer_dir.join(src);
        let ext = file.extension().and_then(|e| e.to_str()).unwrap_or("woff2").to_lowercase();
        let name = format!("{}-{i}.{ext}", request.hash);
        local_files().lock().unwrap().insert(name.clone(), file.clone());
        if metrics.is_none() && matches!(ext.as_str(), "ttf" | "otf") {
            metrics = std::fs::read(&file).ok().and_then(|d| Metrics::from_font(&d));
        }
        let format = match ext.as_str() {
            "woff2" => "woff2",
            "woff" => "woff",
            "otf" => "opentype",
            _ => "truetype",
        };
        css.push_str(&format!(
            "@font-face {{ font-family: '{}'; src: url(/_nata/fonts/local/{name}) format('{format}'); font-display: {display};{}{} }}\n",
            request.family_name(),
            weight.as_ref().map(|w| format!(" font-weight: {w};")).unwrap_or_default(),
            style.as_ref().map(|s| format!(" font-style: {s};")).unwrap_or_default(),
        ));
    }
    (css, metrics)
}

/// The CSS of one font request (cached in memory and in `.nata/fonts/<hash>.css`)
pub async fn font_css(root: &Path, request: &FontRequest) -> String {
    if let Some(css) = generated().lock().unwrap().get(&request.hash).cloned() {
        return css;
    }
    let disk = root.join(".nata/fonts").join(format!("{}.css", request.hash));
    if request.kind == "google" {
        if let Ok(css) = tokio::fs::read_to_string(&disk).await {
            generated().lock().unwrap().insert(request.hash.clone(), css.clone());
            return css;
        }
    }
    let (faces, metrics, complete) = if request.kind == "local" {
        let (css, metrics) = local_css(request);
        (css, metrics, true)
    } else {
        match google_css(root, request).await {
            Some((css, metrics)) => (css, metrics, true),
            None => {
                crate::term::warn_once(&format!("font:{}", request.family), &format!("Could not download the Google font \"{}\" (offline?): using the fallback font", request.family));
                (String::new(), None, false)
            }
        }
    };
    let css = format!("{faces}{}{}", fallback_face(request, metrics), class_rules(request));
    if complete {
        if request.kind == "google" {
            let _ = tokio::fs::create_dir_all(disk.parent().unwrap()).await;
            let _ = tokio::fs::write(&disk, &css).await;
        }
        generated().lock().unwrap().insert(request.hash.clone(), css.clone());
    }
    css
}

/// CSS for every font the project loads, and the version of that set
pub async fn stylesheet(root: &Path) -> (String, u32) {
    let (fonts, version) = {
        let graph = crate::module_graph::for_root(root);
        let graph = graph.read().unwrap_or_else(|e| e.into_inner());
        (graph.fonts(), graph.fonts_version)
    };
    let mut css = String::new();
    for font in fonts {
        css.push_str(&font_css(root, &font).await);
    }
    (css, version)
}

/// `/_nata/fonts/<file>` (downloaded Google fonts) and `/_nata/fonts/local/<name>`
pub async fn serve(root: &Path, path: &str) -> axum::response::Response {
    use axum::response::IntoResponse;
    let file = match path.strip_prefix("local/") {
        Some(name) => local_files().lock().unwrap().get(name).cloned(),
        None if !path.contains("..") && !path.contains('/') => Some(root.join(".nata/fonts").join(path)),
        None => None,
    };
    let Some(file) = file else { return axum::http::StatusCode::NOT_FOUND.into_response() };
    match tokio::fs::read(&file).await {
        Ok(bytes) => {
            let ext = file.extension().and_then(|e| e.to_str()).unwrap_or("");
            let mime = match ext {
                "woff2" => "font/woff2",
                "woff" => "font/woff",
                "otf" => "font/otf",
                _ => "font/ttf",
            };
            ([(axum::http::header::CONTENT_TYPE, mime), (axum::http::header::CACHE_CONTROL, "public, max-age=31536000, immutable")], bytes).into_response()
        }
        Err(_) => axum::http::StatusCode::NOT_FOUND.into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn request(kind: &str, family: &str, options: serde_json::Value) -> FontRequest {
        FontRequest { hash: "abc12345".into(), kind: kind.into(), family: family.into(), options, importer: "/p/app/layout.tsx".into() }
    }

    #[test]
    fn google_queries_follow_css2() {
        assert_eq!(google_query(&request("google", "Inter", json!({})), true), "Inter:wght@100..900");
        assert_eq!(google_query(&request("google", "Inter", json!({})), false), "Inter");
        assert_eq!(google_query(&request("google", "Roboto Mono", json!({ "weight": ["400", "700"] })), true), "Roboto+Mono:wght@400;700");
        assert_eq!(google_query(&request("google", "Lora", json!({ "weight": "400", "style": ["normal", "italic"] })), true), "Lora:ital,wght@0,400;1,400");
    }

    #[test]
    fn keeps_only_requested_subsets() {
        let css = "/* vietnamese */\n@font-face { font-family: 'Inter'; src: url(https://x/vi.woff2); }\n/* latin */\n@font-face { font-family: 'Inter'; src: url(https://x/latin.woff2); }\n/* cyrillic */\n@font-face { font-family: 'Inter'; src: url(https://x/cy.woff2); }\n";
        let out = filter_subsets(css, &["latin".into(), "vietnamese".into()]);
        assert!(out.contains("vi.woff2") && out.contains("latin.woff2"), "{out}");
        assert!(!out.contains("cy.woff2"), "{out}");
    }

    #[test]
    fn fallback_face_is_size_adjusted() {
        let r = request("google", "Inter", json!({}));
        let m = Metrics { units_per_em: 2048.0, ascent: 1984.0, descent: -494.0, line_gap: 0.0, avg_width: 1100.0 };
        let face = fallback_face(&r, Some(m));
        assert!(face.contains("font-family: '__Inter_Fallback_abc12345'"), "{face}");
        assert!(face.contains("src: local(\"Arial\")"), "{face}");
        assert!(face.contains("size-adjust: 121.68%"), "{face}");
        assert!(face.contains("ascent-override:"), "{face}");
        // Monospace families fall back to Courier New
        assert!(fallback_face(&request("google", "Roboto Mono", json!({})), None).contains("Courier New"));
    }

    #[test]
    fn classes_and_variables() {
        let r = request("google", "Inter", json!({ "variable": "--font-inter", "weight": "700" }));
        let css = class_rules(&r);
        assert!(css.contains(".__className_abc12345 { font-family: '__Inter_abc12345', '__Inter_Fallback_abc12345'; font-weight: 700; }"), "{css}");
        assert!(css.contains(".__variable_abc12345 { --font-inter: '__Inter_abc12345', '__Inter_Fallback_abc12345'; }"), "{css}");
        assert_eq!(r.result()["style"]["fontWeight"], 700);
    }
}
