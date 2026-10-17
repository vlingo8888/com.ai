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

        let zmp_script = if target == "zalo" {
            format!("<script>{}</script>", crate::zmp::ZmpMockBridge::get_browser_mock_script())
        } else {
            String::new()
        };

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

  <div id="_nata_debug_pill" onclick="window.__NATA_TOGGLE_DEBUGGER__()" title="Toggle Query & Action Debugger (Ctrl+Shift+D)" style="position: fixed; bottom: 12px; right: 12px; z-index: 999990; display: flex; align-items: center; gap: 8px; padding: 6px 14px; background: #18181b; border: 1px solid #27272a; border-radius: 9999px; font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; font-size: 12px; color: #f4f4f5; cursor: pointer; user-select: none; box-shadow: 0 10px 25px rgba(0,0,0,0.6); transition: all 0.15s ease;">
    <div id="_dev_status" style="width: 8px; height: 8px; border-radius: 50%; background: #10b981; box-shadow: 0 0 8px rgba(16,185,129,0.5);"></div>
    <span style="font-weight: 600; letter-spacing: -0.2px;">⚡ Debug</span>
    <span id="_nata_pill_stats" style="color: #a1a1aa; font-size: 11px; background: #27272a; padding: 2px 8px; border-radius: 9999px;">0 logs</span>
    <span id="_nata_pill_err" style="display: none; color: #fda4af; background: rgba(244,63,94,0.25); border: 1px solid rgba(244,63,94,0.3); padding: 2px 8px; border-radius: 9999px; font-weight: 600;">0 err</span>
  </div>

  <div id="_nata_debug_drawer" style="display: none; position: fixed; bottom: 0; left: 0; right: 0; height: 480px; max-height: 85vh; background: #09090b; border-top: 1px solid #27272a; z-index: 999995; font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; color: #f4f4f5; box-shadow: 0 -12px 40px rgba(0,0,0,0.8); flex-direction: column; border-top-left-radius: 12px; border-top-right-radius: 12px; overflow: hidden;">
    <div style="padding: 10px 16px; border-bottom: 1px solid #27272a; display: flex; align-items: center; justify-content: space-between; background: #121215; gap: 12px; flex-wrap: wrap;">
      <div style="display: flex; align-items: center; gap: 10px;">
        <div style="display: flex; align-items: center; gap: 6px;">
          <span style="font-weight: 700; font-size: 13px; color: #fafafa; letter-spacing: -0.3px;">⚡ NATA Query Inspector</span>
          <span style="font-size: 10px; font-weight: 600; background: rgba(16,185,129,0.15); color: #34d399; border: 1px solid rgba(16,185,129,0.3); padding: 2px 6px; border-radius: 4px; text-transform: uppercase;">Live</span>
        </div>
        <div id="_dbg_tabs" style="display: flex; align-items: center; gap: 4px; background: #18181b; padding: 3px; border-radius: 8px; border: 1px solid #27272a;">
          <button id="_tab_btn_all" onclick="window.__NATA_SET_TAB__('all')" style="background: #27272a; color: #fafafa; border: none; padding: 4px 10px; border-radius: 6px; font-size: 11px; font-weight: 500; cursor: pointer;">All <span id="_tab_cnt_all" style="color: #a1a1aa; margin-left: 2px;">(0)</span></button>
          <button id="_tab_btn_sql" onclick="window.__NATA_SET_TAB__('sql')" style="background: transparent; color: #a1a1aa; border: none; padding: 4px 10px; border-radius: 6px; font-size: 11px; font-weight: 500; cursor: pointer;">SQL Queries <span id="_tab_cnt_sql" style="color: #71717a; margin-left: 2px;">(0)</span></button>
          <button id="_tab_btn_rpc" onclick="window.__NATA_SET_TAB__('rpc')" style="background: transparent; color: #a1a1aa; border: none; padding: 4px 10px; border-radius: 6px; font-size: 11px; font-weight: 500; cursor: pointer;">RPC Actions <span id="_tab_cnt_rpc" style="color: #71717a; margin-left: 2px;">(0)</span></button>
          <button id="_tab_btn_error" onclick="window.__NATA_SET_TAB__('error')" style="background: transparent; color: #a1a1aa; border: none; padding: 4px 10px; border-radius: 6px; font-size: 11px; font-weight: 500; cursor: pointer;">Errors <span id="_tab_cnt_err" style="color: #f43f5e; margin-left: 2px;">(0)</span></button>
        </div>
      </div>
      <div style="display: flex; align-items: center; gap: 8px; flex: 1; max-width: 420px; min-width: 200px;">
        <div style="position: relative; width: 100%;">
          <input id="_dbg_search" oninput="window.__NATA_FILTER_LOGS__()" type="text" placeholder="Search SQL, table, action, params, response... (Ctrl+K)" style="width: 100%; box-sizing: border-box; background: #18181b; border: 1px solid #27272a; border-radius: 8px; padding: 6px 10px 6px 28px; font-size: 12px; color: #f4f4f5; outline: none;" />
          <span style="position: absolute; left: 9px; top: 50%; transform: translateY(-50%); color: #71717a; font-size: 12px;">🔍</span>
        </div>
      </div>
      <div style="display: flex; align-items: center; gap: 6px;">
        <button onclick="window.__NATA_CLEAR_LOGS__()" title="Clear all logs" style="background: #18181b; border: 1px solid #27272a; color: #a1a1aa; padding: 5px 10px; border-radius: 6px; font-size: 11px; cursor: pointer; display: flex; align-items: center; gap: 4px;">🗑 Clear</button>
        <button onclick="window.__NATA_EXPORT_LOGS__()" title="Export logs as JSON" style="background: #18181b; border: 1px solid #27272a; color: #a1a1aa; padding: 5px 10px; border-radius: 6px; font-size: 11px; cursor: pointer; display: flex; align-items: center; gap: 4px;">💾 Export</button>
        <button id="_dbg_expand_btn" onclick="window.__NATA_TOGGLE_EXPAND__()" title="Toggle full height" style="background: #18181b; border: 1px solid #27272a; color: #a1a1aa; padding: 5px 9px; border-radius: 6px; font-size: 11px; cursor: pointer;">⤢</button>
        <button onclick="window.__NATA_TOGGLE_DEBUGGER__()" title="Close Debugger (Esc)" style="background: transparent; border: none; color: #71717a; padding: 4px 8px; font-size: 15px; cursor: pointer;">✕</button>
      </div>
    </div>
    <div id="_dbg_logs_container" style="flex: 1; overflow-y: auto; padding: 12px 16px; display: flex; flex-direction: column; gap: 8px; min-height: 0;"></div>
  </div>

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
