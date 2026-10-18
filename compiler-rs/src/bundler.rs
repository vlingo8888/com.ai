use std::path::{Path, PathBuf};

pub struct ClientTransformer;

impl ClientTransformer {
    /// Transforms TypeScript/TSX code using the modular PathResolver
    pub fn transform_for_client(code: &str, file_rel_path: &str) -> String {
        crate::resolver::PathResolver::transform_source(code, file_rel_path)
    }

    /// Generates the HTML shell with Tailwind 4 JIT Engine and SWC-compiled ESM module runtime
    pub fn render_html_shell(
        title: &str,
        page_entry: &Path,
        layout_entries: &[PathBuf],
        stylesheet_tags: &str,
    ) -> String {
        let initial_route = Self::route_info_json(page_entry, layout_entries, &[], None, &serde_json::json!({}));
        Self::render_ssr_html_shell(
            title,
            &initial_route,
            stylesheet_tags,
            "",
            "",
            &serde_json::json!({}),
            "web",
            "",
        )
    }

    /// Serializes a matched route into the shape returned by `/_nata/route_info`, which is also the
    /// `initial` route the client runtime mounts from the HTML shell.
    pub fn route_info_json(
        page_file: &Path,
        layout_files: &[PathBuf],
        segments_files: &[crate::router::types::SegmentFiles],
        global_error_file: Option<&PathBuf>,
        params: &serde_json::Value,
    ) -> serde_json::Value {
        let slash = |p: &Path| p.to_string_lossy().replace('\\', "/");
        let opt = |p: &Option<PathBuf>| p.as_deref().map(slash);
        serde_json::json!({
            "found": true,
            "page_file": slash(page_file),
            "layout_files": layout_files.iter().map(|p| slash(p)).collect::<Vec<_>>(),
            "segments_files": segments_files
                .iter()
                .map(|s| serde_json::json!({
                    "folder": slash(&s.folder),
                    "layout": opt(&s.layout),
                    "template": opt(&s.template),
                    "error": opt(&s.error),
                    "loading": opt(&s.loading),
                    "not_found": opt(&s.not_found),
                }))
                .collect::<Vec<_>>(),
            "global_error_file": global_error_file.map(|p| slash(p)),
            "params": params,
        })
    }

    /// ESM import map shared by the web shell and the Zalo bootstrap. Everything points at the dev
    /// server (prefixed with `origin`, empty for same-origin web pages): React from `/_nata/deps/`
    /// (bundled from the project's `node_modules`), framework shims from `/_nata/shims/`. Other packages
    /// need no entry: compiled modules import them as `/_nata/deps/<name>` directly.
    pub fn import_map(origin: &str) -> serde_json::Value {
        const SHIMS: &[&str] = &[
            "next/link",
            "next/image",
            "next/navigation",
            "next/router",
            "next/head",
            "next/headers",
            "next-intl/server",
        ];
        const DEPS: &[&str] = &["react", "react/jsx-runtime", "react/jsx-dev-runtime", "react-dom", "react-dom/client"];
        let origin = origin.trim_end_matches('/');
        let mut imports = serde_json::Map::new();
        for spec in DEPS {
            imports.insert(spec.to_string(), serde_json::Value::String(format!("{}/_nata/deps/{}", origin, spec)));
        }
        for spec in SHIMS {
            imports.insert(spec.to_string(), serde_json::Value::String(format!("{}/_nata/shims/{}", origin, spec)));
        }
        serde_json::json!({ "imports": imports })
    }

    /// Generates the HTML shell; when `ssr_html` is non-empty the client runtime hydrates it,
    /// otherwise it mounts `initial_route` client-side.
    pub fn render_ssr_html_shell(
        title: &str,
        initial_route: &serde_json::Value,
        stylesheet_tags: &str,
        ssr_html: &str,
        loading_html: &str,
        ssr_state: &serde_json::Value,
        target: &str,
        head_tags: &str,
    ) -> String {
        let import_map = serde_json::to_string_pretty(&Self::import_map("")).unwrap_or_else(|_| "{}".to_string());
        let nata_config = serde_json::to_string(&serde_json::json!({
            "mode": "web",
            "ssr": !ssr_html.is_empty(),
            "origin": "",
            "initial": initial_route,
        }))
        .unwrap_or_else(|_| "{}".to_string())
        .replace("</", "<\\/");

        let zmp_script = if target == "zalo" { crate::zmp::ZmpMockBridge::script_tag() } else { String::new() };

        let ssr_state_json = serde_json::to_string(ssr_state).unwrap_or_else(|_| "{}".to_string());
        // Server-rendered page, or (when the page renders on the client) the route's loading.tsx rendered on
        // the server as a placeholder until the client mounts, like the Suspense fallback Next.js streams.
        // Without loading.tsx the root stays empty: no framework UI.
        let root_content = if !ssr_html.is_empty() {
            format!(
                r#"<div id="root" data-nata-ssr="true">{}</div><script id="__NATA_SSR_DATA__" type="application/json">{}</script>"#,
                ssr_html,
                ssr_state_json.replace("</script>", "<\\/script>")
            )
        } else {
            format!(r#"<div id="root">{}</div>"#, loading_html)
        };
        let backend_url = std::env::var("COM_BACKEND_URL").unwrap_or_default();

        let title_tag = if head_tags.contains("<title>") {
            String::new()
        } else {
            format!("  <title>{}</title>", title)
        };
        let formatted_head_tags = if head_tags.trim().is_empty() {
            String::new()
        } else {
            format!("  {}", head_tags.trim())
        };

        format!(
            r####"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
{title_tag}
{formatted_head_tags}
  <script>window.__COM_BACKEND_URL__ = "{backend_url}";</script>
  {zmp_script}
  
  <!-- Preconnect to CDN endpoints for fast parallel downloading -->
  <link rel="preconnect" href="https://cdn.jsdelivr.net" crossorigin>
  <link rel="dns-prefetch" href="https://cdn.jsdelivr.net">

  {stylesheet_tags}

  <!-- ESM Import Maps for Core Framework Packages & React Singleton -->
  <script type="importmap">
  {import_map}
  </script>

</head>
<body>
  {root_content}

  <script>window.__NATA_CONFIG__ = {nata_config};</script>
  <script type="module">
    // The overlay loads first so a runtime that cannot load (offline CDN, dev server down) is still reported
    import {{ reportIssue }} from "/_nata/overlay.js";
    import("/_nata/client.js").catch((err) => reportIssue(err, "runtime"));
  </script>
</body>
</html>"####
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_html_shell_has_no_root_slash_in_importmap() {
        let html = ClientTransformer::render_html_shell(
            "Test App",
            Path::new("app/page.tsx"),
            &[PathBuf::from("app/layout.tsx")],
            r#"<style id="_nata_custom_css">body{color:red}</style>"#,
        );

        assert!(html.contains(r#""next/link": "/_nata/shims/next/link""#));
        assert!(html.contains(r#""react": "/_nata/deps/react""#));
        assert!(!html.contains("esm.sh"));
        // Ensure no wildcard "/" or "/react@18.3.1/" in importmap that intercepts "/_bundle/..."
        assert!(!html.contains(r#""/":"#) && !html.contains(r#""/": "#));
        assert!(html.contains(r#"import { reportIssue } from "/_nata/overlay.js";"#));
        assert!(html.contains(r#"import("/_nata/client.js")"#));
        assert!(!html.contains("_error_overlay"));
        assert!(html.contains(r#"<style id="_nata_custom_css">body{color:red}</style>"#));
        assert!(!html.contains("tailwindcss/browser"));
        // No framework loading screen: an empty root when there is neither SSR markup nor loading.tsx
        assert!(html.contains(r#"<div id="root"></div>"#));
        assert!(!html.contains("Mounting App Router"));
        assert!(html.contains(r#""page_file":"app/page.tsx""#));
        assert!(html.contains(r#""layout_files":["app/layout.tsx"]"#));
        assert!(html.contains(r#""ssr":false"#));
    }

    #[test]
    fn test_ssr_shell_passes_segments_for_matching_hydration() {
        let segments = vec![crate::router::types::SegmentFiles {
            folder: PathBuf::from("app"),
            layout: Some(PathBuf::from("app/layout.tsx")),
            loading: Some(PathBuf::from("app/loading.tsx")),
            ..Default::default()
        }];
        let initial = ClientTransformer::route_info_json(
            Path::new("app/page.tsx"),
            &[PathBuf::from("app/layout.tsx")],
            &segments,
            None,
            &serde_json::json!({ "id": "1" }),
        );
        let html = ClientTransformer::render_ssr_html_shell(
            "T",
            &initial,
            "",
            "<main>hi</main>",
            "",
            &serde_json::json!({}),
            "web",
            "",
        );
        assert!(html.contains(r#"data-nata-ssr="true""#));
        assert!(html.contains(r#""ssr":true"#));
        assert!(html.contains(r#""loading":"app/loading.tsx""#));
        assert!(html.contains(r#""params":{"id":"1"}"#));
    }

    #[test]
    fn test_client_rendered_page_shows_server_rendered_loading_ui() {
        let initial = ClientTransformer::route_info_json(Path::new("app/page.tsx"), &[], &[], None, &serde_json::json!({}));
        let html = ClientTransformer::render_ssr_html_shell("T", &initial, "", "", "<p>Loading posts...</p>", &serde_json::json!({}), "web", "");
        assert!(html.contains(r#"<div id="root"><p>Loading posts...</p></div>"#));
        // Not marked as SSR: the client mounts with createRoot and replaces the placeholder
        assert!(!html.contains("data-nata-ssr"));
    }

    #[test]
    fn test_import_map_prefixes_shims_with_origin() {
        let map = ClientTransformer::import_map("https://x.dev/");
        assert_eq!(map["imports"]["next/navigation"], "https://x.dev/_nata/shims/next/navigation");
        assert_eq!(map["imports"]["react"], "https://x.dev/_nata/deps/react");
        assert_eq!(ClientTransformer::import_map("")["imports"]["next/link"], "/_nata/shims/next/link");
    }
}
