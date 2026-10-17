//! next/og support: the worker renders JSX to SVG with Satori (text as paths), the engine turns it
//! into PNG with resvg, and provides a default font when the image passes none.

use std::path::Path;

/// SVG → PNG (`ImageResponse` body)
pub fn svg_to_png(svg: &str) -> Result<Vec<u8>, String> {
    let tree = resvg::usvg::Tree::from_str(svg, &resvg::usvg::Options::default()).map_err(|e| format!("invalid SVG: {e}"))?;
    let size = tree.size().to_int_size();
    let mut pixmap = resvg::tiny_skia::Pixmap::new(size.width(), size.height()).ok_or("image size is zero")?;
    resvg::render(&tree, resvg::tiny_skia::Transform::default(), &mut pixmap.as_mut());
    pixmap.encode_png().map_err(|e| format!("PNG encoding failed: {e}"))
}

/// A sans-serif TrueType font for Satori: a system font when one exists, else Inter from Google
/// Fonts (downloaded once into `.nata/fonts/`)
pub async fn default_font(root: &Path) -> Result<Vec<u8>, String> {
    const SYSTEM: [&str; 6] = [
        "/System/Library/Fonts/Supplemental/Arial.ttf",
        "/Library/Fonts/Arial.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/TTF/DejaVuSans.ttf",
        "C:\\Windows\\Fonts\\arial.ttf",
    ];
    for path in SYSTEM {
        if let Ok(bytes) = tokio::fs::read(path).await {
            return Ok(bytes);
        }
    }
    let cached = root.join(".nata/fonts/og-default.ttf");
    if let Ok(bytes) = tokio::fs::read(&cached).await {
        return Ok(bytes);
    }
    let fetch = |url: String| async move {
        let reply = crate::worker_pool::call(root, "fetch_bytes", serde_json::json!({ "url": url }), Vec::new()).await.ok()?;
        (reply.output()["ok"].as_bool() == Some(true)).then_some(reply.body)
    };
    // A plain user agent gets TrueType URLs
    let css = fetch("https://fonts.googleapis.com/css2?family=Inter".to_string()).await.ok_or("no system font and Google Fonts unreachable")?;
    let css = String::from_utf8_lossy(&css).to_string();
    let url = regex::Regex::new(r"url\((https://[^)]+)\)").unwrap().captures(&css).map(|c| c[1].to_string()).ok_or("no font URL")?;
    let bytes = fetch(url).await.ok_or("font download failed")?;
    let _ = tokio::fs::create_dir_all(cached.parent().unwrap()).await;
    let _ = tokio::fs::write(&cached, &bytes).await;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    #[test]
    fn rasterizes_svg() {
        let png = super::svg_to_png(r#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10"><rect width="20" height="10" fill="red"/></svg>"#).unwrap();
        let img = image::load_from_memory(&png).unwrap();
        assert_eq!((img.width(), img.height()), (20, 10));
        assert_eq!(img.to_rgba8().get_pixel(5, 5).0, [255, 0, 0, 255]);
    }
}
