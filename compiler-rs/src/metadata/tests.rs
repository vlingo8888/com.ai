use super::engine::MetadataEngine;
use std::fs;

fn make_temp_dir() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "nata_meta_test_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = fs::create_dir_all(&dir);
    dir
}

#[tokio::test]
async fn test_robots_default_fallback() {
    let tmp = make_temp_dir();
    let root = &tmp;

    let (status, headers, body) = MetadataEngine::handle_robots(root, Some("http://localhost:3000")).await;
    assert_eq!(status, axum::http::StatusCode::OK);
    assert_eq!(headers.get("content-type").unwrap(), "text/plain; charset=utf-8");
    let content = String::from_utf8_lossy(&body);
    assert!(content.contains("User-agent: *"));
    assert!(content.contains("Allow: /"));
}

#[tokio::test]
async fn test_robots_static_file() {
    let tmp = make_temp_dir();
    let root = &tmp;
    let public_dir = root.join("public");
    fs::create_dir_all(&public_dir).unwrap();
    fs::write(public_dir.join("robots.txt"), "User-agent: Googlebot\nDisallow: /admin\n").unwrap();

    let (status, _headers, body) = MetadataEngine::handle_robots(root, None).await;
    assert_eq!(status, axum::http::StatusCode::OK);
    let content = String::from_utf8_lossy(&body);
    assert!(content.contains("Googlebot"));
    assert!(content.contains("Disallow: /admin"));
}

#[tokio::test]
async fn test_sitemap_static_file() {
    let tmp = make_temp_dir();
    let root = &tmp;
    let app_dir = root.join("app");
    fs::create_dir_all(&app_dir).unwrap();
    fs::write(app_dir.join("sitemap.xml"), "<urlset><url><loc>https://example.com</loc></url></urlset>").unwrap();

    let (status, headers, body) = MetadataEngine::handle_sitemap(root, None).await;
    assert_eq!(status, axum::http::StatusCode::OK);
    assert_eq!(headers.get("content-type").unwrap(), "application/xml; charset=utf-8");
    let content = String::from_utf8_lossy(&body);
    assert!(content.contains("https://example.com"));
}

#[tokio::test]
async fn test_manifest_static_file() {
    let tmp = make_temp_dir();
    let root = &tmp;
    let app_dir = root.join("app");
    fs::create_dir_all(&app_dir).unwrap();
    fs::write(app_dir.join("manifest.json"), r#"{"name":"My App","short_name":"App"}"#).unwrap();

    let (status, headers, body) = MetadataEngine::handle_manifest(root).await;
    assert_eq!(status, axum::http::StatusCode::OK);
    assert_eq!(headers.get("content-type").unwrap(), "application/manifest+json; charset=utf-8");
    let content = String::from_utf8_lossy(&body);
    assert!(content.contains("My App"));
}

#[test]
fn test_find_route_asset() {
    let tmp = make_temp_dir();
    let root = &tmp;
    let app_dir = root.join("app");
    fs::create_dir_all(&app_dir).unwrap();
    fs::write(app_dir.join("icon.png"), b"fake_png").unwrap();
    fs::write(app_dir.join("opengraph-image.jpg"), b"fake_jpg").unwrap();

    let icon_res = MetadataEngine::find_route_asset(root, "icon");
    assert!(icon_res.is_some());
    let (p, mime) = icon_res.unwrap();
    assert_eq!(mime, "image/png");
    assert!(p.ends_with("icon.png"));

    let og_res = MetadataEngine::find_route_asset(root, "opengraph-image");
    assert!(og_res.is_some());
    let (p, mime) = og_res.unwrap();
    assert_eq!(mime, "image/jpeg");
    assert!(p.ends_with("opengraph-image.jpg"));

    let missing = MetadataEngine::find_route_asset(root, "twitter-image");
    assert!(missing.is_none());
}
