use super::types::{MetadataFileKind, MetadataFileRequest};
use super::worker::MetadataWorker;
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use std::path::{Path, PathBuf};

pub struct MetadataEngine;

impl MetadataEngine {
    /// Dispatches GET /robots.txt
    pub async fn handle_robots(root: &Path, base_url: Option<&str>) -> (StatusCode, HeaderMap, Vec<u8>) {
        let mut headers = HeaderMap::new();
        headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("text/plain; charset=utf-8"));
        headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("public, max-age=3600"));

        let ts_candidates = [
            "app/robots.ts",
            "app/robots.js",
            "src/app/robots.ts",
            "src/app/robots.js",
        ];

        for cand in &ts_candidates {
            let p = root.join(cand);
            if p.exists() && p.is_file() {
                let req = MetadataFileRequest {
                    file_path: p,
                    kind: MetadataFileKind::Robots,
                    base_url: base_url.map(|s| s.to_string()),
                };
                if let Ok(resp) = MetadataWorker::execute(root, &req).await {
                    if let Ok(ct) = HeaderValue::from_str(&resp.content_type) {
                        headers.insert(header::CONTENT_TYPE, ct);
                    }
                    let status = StatusCode::from_u16(resp.status).unwrap_or(StatusCode::OK);
                    return (status, headers, resp.content.into_bytes());
                }
            }
        }

        let static_candidates = [
            "app/robots.txt",
            "public/robots.txt",
            "robots.txt",
        ];

        for cand in &static_candidates {
            let p = root.join(cand);
            if p.exists() && p.is_file() {
                if let Ok(content) = tokio::fs::read(&p).await {
                    return (StatusCode::OK, headers, content);
                }
            }
        }

        let default_robots = "User-agent: *\nAllow: /\n";
        (StatusCode::OK, headers, default_robots.as_bytes().to_vec())
    }

    /// Dispatches GET /sitemap.xml
    pub async fn handle_sitemap(root: &Path, base_url: Option<&str>) -> (StatusCode, HeaderMap, Vec<u8>) {
        let mut headers = HeaderMap::new();
        headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("application/xml; charset=utf-8"));
        headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("public, max-age=3600"));

        let ts_candidates = [
            "app/sitemap.ts",
            "app/sitemap.js",
            "src/app/sitemap.ts",
            "src/app/sitemap.js",
        ];

        for cand in &ts_candidates {
            let p = root.join(cand);
            if p.exists() && p.is_file() {
                let req = MetadataFileRequest {
                    file_path: p,
                    kind: MetadataFileKind::Sitemap,
                    base_url: base_url.map(|s| s.to_string()),
                };
                if let Ok(resp) = MetadataWorker::execute(root, &req).await {
                    if let Ok(ct) = HeaderValue::from_str(&resp.content_type) {
                        headers.insert(header::CONTENT_TYPE, ct);
                    }
                    let status = StatusCode::from_u16(resp.status).unwrap_or(StatusCode::OK);
                    return (status, headers, resp.content.into_bytes());
                }
            }
        }

        let static_candidates = [
            "app/sitemap.xml",
            "public/sitemap.xml",
            "sitemap.xml",
        ];

        for cand in &static_candidates {
            let p = root.join(cand);
            if p.exists() && p.is_file() {
                if let Ok(content) = tokio::fs::read(&p).await {
                    return (StatusCode::OK, headers, content);
                }
            }
        }

        (StatusCode::NOT_FOUND, headers, b"Sitemap not found".to_vec())
    }

    /// Dispatches GET /manifest.webmanifest or /manifest.json
    pub async fn handle_manifest(root: &Path) -> (StatusCode, HeaderMap, Vec<u8>) {
        let mut headers = HeaderMap::new();
        headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("application/manifest+json; charset=utf-8"));
        headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("public, max-age=3600"));

        let ts_candidates = [
            "app/manifest.ts",
            "app/manifest.js",
            "src/app/manifest.ts",
            "src/app/manifest.js",
        ];

        for cand in &ts_candidates {
            let p = root.join(cand);
            if p.exists() && p.is_file() {
                let req = MetadataFileRequest {
                    file_path: p,
                    kind: MetadataFileKind::Manifest,
                    base_url: None,
                };
                if let Ok(resp) = MetadataWorker::execute(root, &req).await {
                    if let Ok(ct) = HeaderValue::from_str(&resp.content_type) {
                        headers.insert(header::CONTENT_TYPE, ct);
                    }
                    let status = StatusCode::from_u16(resp.status).unwrap_or(StatusCode::OK);
                    return (status, headers, resp.content.into_bytes());
                }
            }
        }

        let static_candidates = [
            "app/manifest.json",
            "app/manifest.webmanifest",
            "public/manifest.json",
            "public/manifest.webmanifest",
            "manifest.json",
        ];

        for cand in &static_candidates {
            let p = root.join(cand);
            if p.exists() && p.is_file() {
                if let Ok(content) = tokio::fs::read(&p).await {
                    return (StatusCode::OK, headers, content);
                }
            }
        }

        (StatusCode::NOT_FOUND, headers, b"Manifest not found".to_vec())
    }

    /// Locates route asset file (icon, apple-icon, opengraph-image, twitter-image)
    pub fn find_route_asset(root: &Path, asset_name: &str) -> Option<(PathBuf, &'static str)> {
        let extensions = [
            (".png", "image/png"),
            (".jpg", "image/jpeg"),
            (".jpeg", "image/jpeg"),
            (".svg", "image/svg+xml"),
            (".ico", "image/x-icon"),
            (".webp", "image/webp"),
            (".gif", "image/gif"),
        ];

        let base_dirs = ["app", "src/app", "public", ""];

        for dir in &base_dirs {
            let folder = if dir.is_empty() { root.to_path_buf() } else { root.join(dir) };
            for (ext, mime) in &extensions {
                let candidate = folder.join(format!("{}{}", asset_name, ext));
                if candidate.exists() && candidate.is_file() {
                    return Some((candidate, mime));
                }
            }
        }

        None
    }
}
