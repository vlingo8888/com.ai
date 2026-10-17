//! next/image support (ROADMAP P2):
//! - `/_next/image?url=&w=&q=`: resizes a local image (`public/`, or a statically imported one) or an
//!   allowed remote one to an allowed width, re-encodes it (lossy WebP when the browser accepts it,
//!   else the source format) and caches the result in `.nata/cache/images/`. Widths and qualities are
//!   validated like Next.js, so a page cannot ask for arbitrary sizes.
//! - Static imports (`import logo from "./logo.png"`) become `{ src, width, height, blurDataURL }`,
//!   the same object in every layer; the file itself is served at `/_nata/media/<path>`.

use std::io::Cursor;
use std::path::{Path, PathBuf};

use axum::body::Body;
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};

pub const DEVICE_SIZES: [u32; 8] = [640, 750, 828, 1080, 1200, 1920, 2048, 3840];
pub const IMAGE_SIZES: [u32; 8] = [16, 32, 48, 64, 96, 128, 256, 384];

pub const IMAGE_EXTENSIONS: [&str; 9] = ["png", "jpg", "jpeg", "gif", "webp", "avif", "svg", "ico", "bmp"];

pub fn is_image(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| IMAGE_EXTENSIONS.contains(&e.to_lowercase().as_str()))
}

fn content_type(ext: &str) -> &'static str {
    match ext {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        "bmp" => "image/bmp",
        _ => "application/octet-stream",
    }
}

/// Images config from `next.config` (`images`), with Next.js defaults
#[derive(Debug, Clone)]
pub struct ImagesConfig {
    pub device_sizes: Vec<u32>,
    pub image_sizes: Vec<u32>,
    pub qualities: Option<Vec<u32>>,
    pub remote_patterns: Vec<regex::Regex>,
    pub unoptimized: bool,
}

impl Default for ImagesConfig {
    fn default() -> Self {
        Self { device_sizes: DEVICE_SIZES.to_vec(), image_sizes: IMAGE_SIZES.to_vec(), qualities: None, remote_patterns: vec![], unoptimized: false }
    }
}

impl ImagesConfig {
    /// From the `images` object of next.config
    pub fn from_json(v: &serde_json::Value) -> Self {
        let nums = |k: &str| v[k].as_array().map(|a| a.iter().filter_map(|x| x.as_u64().map(|n| n as u32)).collect::<Vec<_>>());
        let mut patterns = Vec::new();
        for p in v["remotePatterns"].as_array().into_iter().flatten() {
            let host = p["hostname"].as_str().unwrap_or("*");
            let host_re = regex::escape(host).replace("\\*\\*", ".+").replace("\\*", "[^.]+");
            let protocol = p["protocol"].as_str().unwrap_or("https?").trim_end_matches(':');
            let path = p["pathname"].as_str().unwrap_or("/**");
            let path_re = regex::escape(path).replace("\\*\\*", ".*").replace("\\*", "[^/]*");
            if let Ok(re) = regex::Regex::new(&format!("^{protocol}://{host_re}(:\\d+)?{path_re}")) {
                patterns.push(re);
            }
        }
        for domain in v["domains"].as_array().into_iter().flatten().filter_map(|d| d.as_str()) {
            if let Ok(re) = regex::Regex::new(&format!("^https?://{}(:\\d+)?/", regex::escape(domain))) {
                patterns.push(re);
            }
        }
        Self {
            device_sizes: nums("deviceSizes").unwrap_or_else(|| DEVICE_SIZES.to_vec()),
            image_sizes: nums("imageSizes").unwrap_or_else(|| IMAGE_SIZES.to_vec()),
            qualities: nums("qualities"),
            remote_patterns: patterns,
            unoptimized: v["unoptimized"].as_bool().unwrap_or(false),
        }
    }

    fn allows_width(&self, w: u32) -> bool {
        self.device_sizes.contains(&w) || self.image_sizes.contains(&w)
    }

    fn allows_quality(&self, q: u32) -> bool {
        (1..=100).contains(&q) && self.qualities.as_ref().is_none_or(|list| list.contains(&q))
    }
}

fn bad_request(message: String) -> Response {
    (StatusCode::BAD_REQUEST, message).into_response()
}

fn query_param(query: &str, name: &str) -> Option<String> {
    query.split('&').find_map(|pair| {
        let (k, v) = pair.split_once('=')?;
        (k == name).then(|| crate::router::types::percent_decode(&v.replace('+', " ")))
    })
}

/// The file behind a local image URL: `public/` or a statically imported image
fn local_source(root: &Path, url: &str) -> Option<PathBuf> {
    let path = url.split('?').next().unwrap_or(url);
    let candidate = match path.strip_prefix("/_nata/media/") {
        Some(rel) => root.join(rel),
        None => root.join("public").join(path.trim_start_matches('/')),
    };
    let canonical = candidate.canonicalize().ok()?;
    let root = root.canonicalize().ok()?;
    (canonical.starts_with(&root) && canonical.is_file()).then_some(canonical)
}

pub struct Optimized {
    pub bytes: Vec<u8>,
    pub content_type: &'static str,
}

/// Resizes and re-encodes `source` (never upscales); `None` when it cannot be decoded
pub fn optimize(source: &[u8], source_ext: &str, width: u32, quality: u32, webp: bool) -> Option<Optimized> {
    let img = image::load_from_memory(source).ok()?;
    let (w0, h0) = (img.width(), img.height());
    let width = width.min(w0).max(1);
    let height = ((h0 as f64) * (width as f64 / w0 as f64)).round().max(1.0) as u32;
    let rgba = img.to_rgba8();
    let resized = if width == w0 {
        rgba
    } else {
        let src = fast_image_resize::images::Image::from_vec_u8(w0, h0, rgba.into_raw(), fast_image_resize::PixelType::U8x4).ok()?;
        let mut dst = fast_image_resize::images::Image::new(width, height, fast_image_resize::PixelType::U8x4);
        let mut resizer = fast_image_resize::Resizer::new();
        let options = fast_image_resize::ResizeOptions::new().resize_alg(fast_image_resize::ResizeAlg::Convolution(fast_image_resize::FilterType::Lanczos3));
        resizer.resize(&src, &mut dst, &options).ok()?;
        image::RgbaImage::from_raw(width, height, dst.into_vec())?
    };
    if webp {
        let encoded = webp::Encoder::from_rgba(&resized, width, height).encode(quality as f32);
        return Some(Optimized { bytes: encoded.to_vec(), content_type: "image/webp" });
    }
    let mut out = Cursor::new(Vec::new());
    match source_ext {
        "jpg" | "jpeg" => {
            let rgb = image::DynamicImage::ImageRgba8(resized).to_rgb8();
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, quality.min(100) as u8).encode_image(&rgb).ok()?;
            Some(Optimized { bytes: out.into_inner(), content_type: "image/jpeg" })
        }
        _ => {
            image::DynamicImage::ImageRgba8(resized).write_to(&mut out, image::ImageFormat::Png).ok()?;
            Some(Optimized { bytes: out.into_inner(), content_type: "image/png" })
        }
    }
}

fn images_config(root: &Path) -> ImagesConfig {
    crate::next_config::for_root(root).map(|c| ImagesConfig::from_json(&c["images"])).unwrap_or_default()
}

/// `GET /_next/image?url=&w=&q=`
pub async fn handle(root: &Path, query: &str, accept: &str) -> Response {
    let config = images_config(root);
    let Some(url) = query_param(query, "url") else { return bad_request("\"url\" parameter is required".into()) };
    let Some(width) = query_param(query, "w").and_then(|w| w.parse::<u32>().ok()) else {
        return bad_request("\"w\" parameter (width) is required".into());
    };
    let quality = query_param(query, "q").and_then(|q| q.parse::<u32>().ok()).unwrap_or(75);
    if !config.allows_width(width) {
        return bad_request(format!("\"w\" parameter (width) of {width} is not allowed"));
    }
    if !config.allows_quality(quality) {
        return bad_request(format!("\"q\" parameter (quality) of {quality} is not allowed"));
    }

    let (source, ext): (Vec<u8>, String) = if url.starts_with('/') && !url.starts_with("//") {
        let Some(file) = local_source(root, &url) else { return (StatusCode::NOT_FOUND, "The requested resource isn't a valid image.").into_response() };
        let ext = file.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
        match tokio::fs::read(&file).await {
            Ok(bytes) => (bytes, ext),
            Err(_) => return (StatusCode::NOT_FOUND, "The requested resource isn't a valid image.").into_response(),
        }
    } else if url.starts_with("http://") || url.starts_with("https://") {
        if !config.remote_patterns.iter().any(|p| p.is_match(&url)) {
            return bad_request(format!("\"url\" parameter is not allowed: configure images.remotePatterns in next.config for {url}"));
        }
        match fetch_remote(root, &url).await {
            Some(bytes) => {
                let ext = url.split('?').next().unwrap_or("").rsplit('.').next().unwrap_or("").to_lowercase();
                (bytes, ext)
            }
            None => return (StatusCode::BAD_GATEWAY, format!("Could not fetch {url}")).into_response(),
        }
    } else {
        return bad_request("\"url\" parameter is invalid".into());
    };

    let original = |bytes: Vec<u8>, ext: &str| {
        let mut response = Response::new(Body::from(bytes));
        response.headers_mut().insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type(ext)));
        response.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("public, max-age=60, must-revalidate"));
        response
    };
    // Vector, animated and icon formats are served as they are (like Next.js)
    if config.unoptimized || matches!(ext.as_str(), "svg" | "gif" | "ico" | "avif") {
        return original(source, &ext);
    }

    let webp = accept.contains("image/webp");
    let key = blake3::hash(format!("{url}|{width}|{quality}|{webp}|{}", blake3::hash(&source).to_hex()).as_bytes()).to_hex().to_string();
    let cache_dir = root.join(".nata/cache/images");
    let cache_file = cache_dir.join(format!("{}.{}", &key[..32], if webp { "webp" } else if matches!(ext.as_str(), "jpg" | "jpeg") { "jpg" } else { "png" }));
    let optimized = match tokio::fs::read(&cache_file).await {
        Ok(bytes) => Some(Optimized { bytes, content_type: content_type(cache_file.extension().and_then(|e| e.to_str()).unwrap_or("png")) }),
        Err(_) => {
            let ext_for_encode = ext.clone();
            let result = tokio::task::spawn_blocking(move || optimize(&source, &ext_for_encode, width, quality, webp)).await.ok().flatten();
            if let Some(o) = &result {
                let _ = tokio::fs::create_dir_all(&cache_dir).await;
                let _ = tokio::fs::write(&cache_file, &o.bytes).await;
            }
            result
        }
    };
    let Some(optimized) = optimized else { return (StatusCode::UNSUPPORTED_MEDIA_TYPE, "The requested resource isn't a valid image.").into_response() };
    let etag = format!("\"{}\"", &key[..20]);
    let mut response = Response::new(Body::from(optimized.bytes));
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(optimized.content_type));
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("public, max-age=60, must-revalidate"));
    headers.insert(header::VARY, HeaderValue::from_static("Accept"));
    if let Ok(v) = HeaderValue::from_str(&etag) {
        headers.insert(header::ETAG, v);
    }
    response
}

/// Remote images are fetched by the worker (the engine has no HTTP client)
async fn fetch_remote(root: &Path, url: &str) -> Option<Vec<u8>> {
    let reply = crate::worker_pool::call(root, "fetch_bytes", serde_json::json!({ "url": url }), Vec::new()).await.ok()?;
    (reply.error().is_none() && reply.output()["ok"].as_bool() == Some(true)).then_some(reply.body)
}

/// `/_nata/media/<path>`: a statically imported image, as is
pub async fn media(root: &Path, rel: &str) -> Response {
    let file = root.join(rel);
    let ok = file.canonicalize().ok().zip(root.canonicalize().ok()).is_some_and(|(f, r)| f.starts_with(r)) && is_image(&file);
    match (ok, tokio::fs::read(&file).await) {
        (true, Ok(bytes)) => {
            let ext = file.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
            let mut response = Response::new(Body::from(bytes));
            response.headers_mut().insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type(&ext)));
            response.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
            response
        }
        _ => StatusCode::NOT_FOUND.into_response(),
    }
}

/// Width and height of an image file (SVG from its attributes / viewBox)
pub fn dimensions(file: &Path) -> Option<(u32, u32)> {
    if file.extension().is_some_and(|e| e == "svg") {
        let text = std::fs::read_to_string(file).ok()?;
        let attr = |name: &str| regex::Regex::new(&format!(r#"<svg[^>]*\s{name}="([\d.]+)"#)).ok()?.captures(&text)?.get(1)?.as_str().parse::<f64>().ok();
        if let (Some(w), Some(h)) = (attr("width"), attr("height")) {
            return Some((w as u32, h as u32));
        }
        let vb = regex::Regex::new(r#"viewBox="[\d.\-]+\s+[\d.\-]+\s+([\d.]+)\s+([\d.]+)""#).ok()?.captures(&text)?;
        return Some((vb.get(1)?.as_str().parse::<f64>().ok()? as u32, vb.get(2)?.as_str().parse::<f64>().ok()? as u32));
    }
    image::image_dimensions(file).ok()
}

/// A tiny blurred preview as a data URL (`placeholder="blur"`), for JPEG, PNG and WebP
fn blur_data_url(file: &Path) -> Option<String> {
    let ext = file.extension()?.to_str()?.to_lowercase();
    if !matches!(ext.as_str(), "jpg" | "jpeg" | "png" | "webp") {
        return None;
    }
    let img = image::open(file).ok()?;
    let small = img.resize(8, 8, image::imageops::FilterType::Triangle);
    let mut out = Cursor::new(Vec::new());
    small.write_to(&mut out, image::ImageFormat::Png).ok()?;
    Some(format!("data:image/png;base64,{}", base64::Engine::encode(&base64::engine::general_purpose::STANDARD, out.into_inner())))
}

/// The ES module a static image import loads: identical in the browser and on the server
pub fn static_import_module(root: &Path, file: &Path) -> String {
    let rel = crate::swc_compiler::module::relative(root, file);
    let (width, height) = dimensions(file).unwrap_or((0, 0));
    let data = serde_json::json!({
        "src": format!("/_nata/media/{rel}"),
        "width": width,
        "height": height,
        "blurDataURL": blur_data_url(file),
        "blurWidth": if width > 0 { 8 } else { 0 },
        "blurHeight": if width > 0 { ((8.0 * height as f64 / width.max(1) as f64).round() as u32).max(1) } else { 0 },
    });
    format!("export default {data};\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(width: u32, height: u32) -> Vec<u8> {
        let img = image::RgbaImage::from_fn(width, height, |x, y| image::Rgba([(x % 255) as u8, (y % 255) as u8, 128, 255]));
        let mut out = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(img).write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    }

    #[test]
    fn resizes_to_the_requested_width_and_keeps_the_ratio() {
        let out = optimize(&png(400, 200), "png", 128, 75, false).unwrap();
        assert_eq!(out.content_type, "image/png");
        let decoded = image::load_from_memory(&out.bytes).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (128, 64));
    }

    #[test]
    fn never_upscales_and_encodes_webp() {
        let out = optimize(&png(100, 50), "png", 640, 75, true).unwrap();
        assert_eq!(out.content_type, "image/webp");
        let decoded = image::load_from_memory(&out.bytes).unwrap();
        assert_eq!(decoded.width(), 100);
    }

    #[test]
    fn validates_widths_and_qualities_like_next() {
        let config = ImagesConfig::default();
        assert!(config.allows_width(640) && config.allows_width(16));
        assert!(!config.allows_width(641));
        assert!(config.allows_quality(75) && !config.allows_quality(0) && !config.allows_quality(101));
        let custom = ImagesConfig::from_json(&serde_json::json!({ "qualities": [50, 80], "remotePatterns": [{ "protocol": "https", "hostname": "**.example.com", "pathname": "/img/**" }] }));
        assert!(custom.allows_quality(80) && !custom.allows_quality(75));
        assert!(custom.remote_patterns[0].is_match("https://cdn.example.com/img/a.png"));
        assert!(!custom.remote_patterns[0].is_match("https://example.org/img/a.png"));
    }

    #[test]
    fn static_import_module_has_dimensions_and_blur() {
        let dir = std::env::temp_dir().join(format!("nata_img_{}", std::process::id()));
        std::fs::create_dir_all(dir.join("app")).unwrap();
        std::fs::write(dir.join("app/logo.png"), png(64, 32)).unwrap();
        std::fs::write(dir.join("app/icon.svg"), r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 12"></svg>"#).unwrap();
        let module = static_import_module(&dir, &dir.join("app/logo.png"));
        assert!(module.contains("\"src\":\"/_nata/media/app/logo.png\""), "{module}");
        assert!(module.contains("\"width\":64") && module.contains("\"height\":32"), "{module}");
        assert!(module.contains("data:image/png;base64,"), "{module}");
        assert_eq!(dimensions(&dir.join("app/icon.svg")), Some((24, 12)));
    }
}
