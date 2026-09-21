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
        custom_css: &str,
    ) -> String {
        Self::render_ssr_html_shell(
            title,
            page_entry,
            layout_entries,
            custom_css,
            "",
            &serde_json::json!({}),
            "web",
        )
    }

    /// Generates the Full SSR HTML shell with pre-rendered markup and client hydration support
    pub fn render_ssr_html_shell(
        title: &str,
        page_entry: &Path,
        layout_entries: &[PathBuf],
        custom_css: &str,
        ssr_html: &str,
        ssr_state: &serde_json::Value,
        target: &str,
    ) -> String {
        let entry_str = page_entry.to_string_lossy().replace('\\', "/");
        let layouts_json = serde_json::to_string(
            &layout_entries
                .iter()
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .collect::<Vec<_>>(),
        )
        .unwrap_or_else(|_| "[]".to_string());

        let zmp_script = if target == "zalo" {
            format!("<script>{}</script>", crate::zmp::ZmpMockBridge::get_browser_mock_script())
        } else {
            String::new()
        };
        let zmp_header_html = "";

        let ssr_state_json = serde_json::to_string(ssr_state).unwrap_or_else(|_| "{}".to_string());
        let root_content = if !ssr_html.is_empty() {
            format!(
                r#"{}<div id="root" data-nata-ssr="true">{}</div><script id="__NATA_SSR_DATA__" type="application/json">{}</script>"#,
                zmp_header_html,
                ssr_html,
                ssr_state_json.replace("</script>", "<\\/script>")
            )
        } else {
            format!(
                r#"{}<div id="root">
    <div style="display:flex;align-items:center;justify-content:center;height:100vh;flex-direction:column;gap:12px;">
      <div style="width:36px;height:36px;border:3px solid #38bdf8;border-top-color:transparent;border-radius:50%;animation:spin 0.8s linear infinite;"></div>
      <h2 style="font-weight:600;font-size:18px;color:#e2e8f0;">Mounting App Router...</h2>
      <p id="_mount_status" style="color:#64748b;font-size:13px;">Instant native execution via Rust SWC Engine</p>
    </div>
    <style>@keyframes spin {{ to {{ transform: rotate(360deg); }} }}</style>
  </div>"#,
                zmp_header_html
            )
        };
        let backend_url = std::env::var("COM_BACKEND_URL").unwrap_or_default();

        format!(
            r####"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>{title}</title>
  <script>window.__COM_BACKEND_URL__ = "{backend_url}";</script>
  {zmp_script}
  
  <!-- Preconnect to CDN endpoints for fast parallel downloading -->
  <link rel="preconnect" href="https://esm.sh" crossorigin>
  <link rel="preconnect" href="https://cdn.jsdelivr.net" crossorigin>
  <link rel="dns-prefetch" href="https://esm.sh">
  <link rel="dns-prefetch" href="https://cdn.jsdelivr.net">

  <!-- Tailwind CSS v4 Engine -->
  <script src="https://cdn.jsdelivr.net/npm/@tailwindcss/browser@4" defer></script>
  <style id="_nata_custom_css" type="text/tailwindcss">
    @custom-variant dark (&:is(.dark *));

    @theme inline {{
      --color-brand-50: #f0f9ff;
      --color-brand-500: #0ea5e9;
      --color-brand-600: #0284c7;
      --color-brand-700: #0369a1;

      --color-background: var(--background);
      --color-foreground: var(--foreground);
      --color-card: var(--card);
      --color-card-foreground: var(--card-foreground);
      --color-popover: var(--popover);
      --color-popover-foreground: var(--popover-foreground);
      --color-primary: var(--primary);
      --color-primary-foreground: var(--primary-foreground);
      --color-secondary: var(--secondary);
      --color-secondary-foreground: var(--secondary-foreground);
      --color-muted: var(--muted);
      --color-muted-foreground: var(--muted-foreground);
      --color-accent: var(--accent);
      --color-accent-foreground: var(--accent-foreground);
      --color-destructive: var(--destructive);
      --color-destructive-foreground: var(--destructive-foreground);
      --color-border: var(--border);
      --color-input: var(--input);
      --color-ring: var(--ring);
      --color-sidebar: var(--sidebar-background);
      --color-sidebar-foreground: var(--sidebar-foreground);
      --color-sidebar-primary: var(--sidebar-primary);
      --color-sidebar-primary-foreground: var(--sidebar-primary-foreground);
      --color-sidebar-accent: var(--sidebar-accent);
      --color-sidebar-accent-foreground: var(--sidebar-accent-foreground);
      --color-sidebar-border: var(--sidebar-border);
      --color-sidebar-ring: var(--sidebar-ring);
      --radius-lg: var(--radius);
      --radius-md: calc(var(--radius) - 2px);
      --radius-sm: calc(var(--radius) - 4px);
    }}

    :root {{
      --background: oklch(1 0 0);
      --foreground: oklch(0.145 0 0);
      --card: oklch(1 0 0);
      --card-foreground: oklch(0.145 0 0);
      --popover: oklch(1 0 0);
      --popover-foreground: oklch(0.145 0 0);
      --primary: oklch(0.205 0 0);
      --primary-foreground: oklch(0.985 0 0);
      --secondary: oklch(0.97 0 0);
      --secondary-foreground: oklch(0.205 0 0);
      --muted: oklch(0.97 0 0);
      --muted-foreground: oklch(0.556 0 0);
      --accent: oklch(0.97 0 0);
      --accent-foreground: oklch(0.205 0 0);
      --destructive: oklch(0.577 0.245 27.325);
      --destructive-foreground: oklch(0.577 0.245 27.325);
      --border: oklch(0.922 0 0);
      --input: oklch(0.922 0 0);
      --ring: oklch(0.708 0 0);
      --chart-1: oklch(0.646 0.222 41.116);
      --chart-2: oklch(0.6 0.118 184.704);
      --chart-3: oklch(0.398 0.07 227.392);
      --chart-4: oklch(0.828 0.189 84.429);
      --chart-5: oklch(0.769 0.188 70.08);
      --radius: 0.625rem;
      --sidebar-background: oklch(0.985 0 0);
      --sidebar-foreground: oklch(0.145 0 0);
      --sidebar-primary: oklch(0.205 0 0);
      --sidebar-primary-foreground: oklch(0.985 0 0);
      --sidebar-accent: oklch(0.97 0 0);
      --sidebar-accent-foreground: oklch(0.205 0 0);
      --sidebar-border: oklch(0.922 0 0);
      --sidebar-ring: oklch(0.708 0 0);
    }}

    .dark {{
      --background: oklch(0.145 0 0);
      --foreground: oklch(0.985 0 0);
      --card: oklch(0.17 0 0);
      --card-foreground: oklch(0.985 0 0);
      --popover: oklch(0.17 0 0);
      --popover-foreground: oklch(0.985 0 0);
      --primary: oklch(0.985 0 0);
      --primary-foreground: oklch(0.205 0 0);
      --secondary: oklch(0.269 0 0);
      --secondary-foreground: oklch(0.985 0 0);
      --muted: oklch(0.269 0 0);
      --muted-foreground: oklch(0.708 0 0);
      --accent: oklch(0.269 0 0);
      --accent-foreground: oklch(0.985 0 0);
      --destructive: oklch(0.396 0.141 25.723);
      --destructive-foreground: oklch(0.637 0.237 25.331);
      --border: oklch(0.269 0 0);
      --input: oklch(0.269 0 0);
      --ring: oklch(0.439 0 0);
      --chart-1: oklch(0.488 0.243 264.376);
      --chart-2: oklch(0.696 0.17 162.48);
      --chart-3: oklch(0.769 0.188 70.08);
      --chart-4: oklch(0.627 0.265 303.9);
      --chart-5: oklch(0.645 0.246 16.439);
      --sidebar-background: oklch(0.12 0 0);
      --sidebar-foreground: oklch(0.985 0 0);
      --sidebar-primary: oklch(0.488 0.243 264.376);
      --sidebar-primary-foreground: oklch(0.985 0 0);
      --sidebar-accent: oklch(0.2 0 0);
      --sidebar-accent-foreground: oklch(0.985 0 0);
      --sidebar-border: oklch(0.22 0 0);
      --sidebar-ring: oklch(0.439 0 0);
    }}

    {custom_css}
  </style>

  <!-- ESM Import Maps for Core Framework Packages & React Singleton -->
  <script type="importmap">
  {{
    "imports": {{
      "react": "https://esm.sh/react@18.3.1",
      "react/jsx-runtime": "https://esm.sh/react@18.3.1/jsx-runtime",
      "react-dom": "https://esm.sh/react-dom@18.3.1?external=react",
      "react-dom/client": "https://esm.sh/react-dom@18.3.1/client?external=react",
      "next/link": "/_nata/shims/next/link",
      "next/image": "/_nata/shims/next/image",
      "next/navigation": "/_nata/shims/next/navigation",
      "next/router": "/_nata/shims/next/router",
      "next/head": "/_nata/shims/next/head",
      "next/headers": "/_nata/shims/next/headers",
      "lucide-react": "https://esm.sh/lucide-react@0.460.0?external=react,react-dom",
      "framer-motion": "https://esm.sh/framer-motion@11.11.17?external=react,react-dom",
      "clsx": "https://esm.sh/clsx@2.1.1",
      "tailwind-merge": "https://esm.sh/tailwind-merge@2.5.4",
      "axios": "https://esm.sh/axios@1.7.7",
      "date-fns": "https://esm.sh/date-fns@4.1.0",
      "canvas-confetti": "https://esm.sh/canvas-confetti@1.9.4",
      "@tanstack/react-query": "https://esm.sh/@tanstack/react-query@5.59.0?external=react,react-dom",
      "@tanstack/react-table": "https://esm.sh/@tanstack/react-table@8.20.5?external=react,react-dom",
      "zustand": "https://esm.sh/zustand@5.0.0?external=react,react-dom",
      "jotai": "https://esm.sh/jotai@2.10.1?external=react,react-dom",
      "swr": "https://esm.sh/swr@2.2.5?external=react,react-dom",
      "zod": "https://esm.sh/zod@3.23.8",
      "react-hook-form": "https://esm.sh/react-hook-form@7.53.0?external=react,react-dom",
      "@hookform/resolvers": "https://esm.sh/@hookform/resolvers@3.9.0?external=react,react-dom",
      "@hookform/resolvers/zod": "https://esm.sh/@hookform/resolvers@3.9.0/zod?external=react,react-dom,zod",
      "sonner": "https://esm.sh/sonner@1.5.0?external=react,react-dom",
      "react-hot-toast": "https://esm.sh/react-hot-toast@2.4.1?external=react,react-dom",
      "next-themes": "https://esm.sh/next-themes@0.3.0?external=react,react-dom"
    }}
  }}
  </script>

  <style>
    * {{
      border-color: var(--border);
    }}
    body {{
      margin: 0;
      padding: 0;
      background-color: var(--background);
      color: var(--foreground);
      font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Helvetica Neue", sans-serif;
      padding-bottom: max(env(safe-area-inset-bottom, 0px), 24px);
      box-sizing: border-box;
    }}
    #_dev_bar {{
      position: fixed; bottom: 12px; right: 12px;
      background: rgba(15, 23, 42, 0.85); backdrop-filter: blur(12px);
      border: 1px solid rgba(255, 255, 255, 0.15); padding: 6px 14px;
      border-radius: 9999px; font-size: 12px; display: flex; align-items: center; gap: 8px;
      z-index: 99999; box-shadow: 0 10px 25px rgba(0,0,0,0.5); color: #94a3b8; user-select: none;
    }}
    #_dev_status {{ width: 8px; height: 8px; border-radius: 50%; background: #10b981; }}
    #_error_overlay {{
      display: none; position: fixed; top: 0; left: 0; right: 0; bottom: 0;
      background: rgba(15, 23, 42, 0.95); z-index: 999999; padding: 32px;
      font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
      color: #fca5a5; overflow: auto;
    }}
  </style>
</head>
<body>
  {root_content}

  <div id="_error_overlay" style="display: none; position: fixed; inset: 0; background: rgba(5, 8, 16, 0.94); backdrop-filter: blur(16px); z-index: 999999; padding: 36px 20px; overflow: auto; font-family: ui-sans-serif, system-ui, -apple-system, sans-serif;">
    <div style="max-width: 900px; margin: 0 auto; background: #0c111d; border: 1px solid rgba(239, 68, 68, 0.35); border-radius: 16px; box-shadow: 0 25px 50px -12px rgba(0, 0, 0, 0.7); overflow: hidden;">
      <div style="padding: 18px 24px; border-bottom: 1px solid rgba(255,255,255,0.08); display: flex; align-items: center; justify-content: space-between; background: rgba(239, 68, 68, 0.08);">
        <div style="display: flex; align-items: center; gap: 10px; flex-wrap: wrap;">
          <span id="_err_badge" style="background: #ef4444; color: #fff; font-size: 11px; font-weight: 700; padding: 4px 10px; border-radius: 6px; text-transform: uppercase; letter-spacing: 0.5px;">Error</span>
          <span id="_err_file" style="font-family: ui-monospace, monospace; font-size: 13px; color: #fca5a5; background: rgba(0,0,0,0.4); padding: 4px 10px; border-radius: 6px; border: 1px solid rgba(239,68,68,0.2);"></span>
        </div>
        <div style="display: flex; gap: 8px;">
          <button onclick="navigator.clipboard.writeText(document.getElementById('_err_raw').textContent); this.textContent='Copied!';" style="background: rgba(255,255,255,0.1); border: 1px solid rgba(255,255,255,0.15); color: #e2e8f0; padding: 6px 12px; border-radius: 8px; font-size: 12px; cursor: pointer; font-weight: 500;">Copy Error</button>
          <button onclick="document.getElementById('_error_overlay').style.display='none';" style="background: transparent; border: none; color: #94a3b8; font-size: 18px; cursor: pointer; padding: 0 6px;">✕</button>
        </div>
      </div>

      <div style="padding: 24px;">
        <h1 id="_err_title" style="font-size: 20px; font-weight: 700; color: #f87171; margin: 0 0 16px 0; line-height: 1.4;"></h1>

        <div id="_err_solution_box" style="background: rgba(14, 165, 233, 0.08); border: 1px solid rgba(14, 165, 233, 0.25); border-radius: 10px; padding: 16px; margin-bottom: 20px; color: #bae6fd; font-size: 14px; line-height: 1.6;">
          <div style="font-weight: 600; color: #38bdf8; display: flex; align-items: center; gap: 6px; margin-bottom: 6px;">
            <span>💡 Suggested Solution</span>
          </div>
          <div id="_err_solution"></div>
        </div>

        <div style="font-weight: 600; font-size: 12px; color: #94a3b8; margin-bottom: 8px; text-transform: uppercase; letter-spacing: 0.5px;">Error Details & Stack Trace</div>
        <pre id="_err_raw" style="background: #030712; padding: 16px; border-radius: 10px; border: 1px solid #1e293b; font-size: 13px; line-height: 1.6; white-space: pre-wrap; word-break: break-all; color: #fca5a5; font-family: ui-monospace, SFMono-Regular, Menlo, monospace; margin: 0; max-height: 350px; overflow: auto;"></pre>
      </div>
    </div>
  </div>

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

  <script type="module">
    import React from "react";
    import ReactDOM from "react-dom/client";

    // Bind React and ReactDOM to global scope for SWC classic runtime
    window.React = React;
    window.ReactDOM = ReactDOM;

    function extractFileFromStack(str) {{
      if (!str) return "";
      const match = str.match(/(?:_bundle\/|\/)([a-zA-Z0-9_\-\/]+\.(?:tsx|ts|jsx|js|css))(?::(\d+)(?::(\d+))?)?/);
      if (match) {{
        return match[1] + (match[2] ? `:${{match[2]}}` : '') + (match[3] ? `:${{match[3]}}` : '');
      }}
      return "";
    }}

    function parseErrorDetails(rawErr) {{
      const msg = rawErr?.message || String(rawErr || "");
      const stack = rawErr?.stack || "";
      const fullText = stack ? `${{msg}}\n${{stack}}` : msg;

      // 1. Missing Named Export
      const exportMatch = fullText.match(/The requested module '([^']+)' does not provide an export named '([^']+)'/i);
      if (exportMatch) {{
        let modPath = exportMatch[1].replace(/^\/_bundle\//, "");
        const missingExport = exportMatch[2];
        return {{
          badge: "Missing Named Export",
          title: `Module does not export '${{missingExport}}'`,
          file: modPath,
          solution: `File <code>${{modPath}}</code> không có <code>export function ${{missingExport}}</code> (hoặc <code>export const ${{missingExport}}</code>).<br/>👉 <b>Cách sửa:</b> Hãy kiểm tra xem component trong <code>${{modPath}}</code> được đặt tên là gì (ví dụ: <code>DatePicker</code> thay vì <code>DateTimePicker</code>), hoặc thêm <code>export {{ DatePicker as ${{missingExport}} }}</code>. Nếu là export mặc định, hãy dùng <code>import ${{missingExport}} from "${{modPath}}"</code>.`,
          raw: fullText
        }};
      }}

      // 2. Module Not Found
      const notFoundMatch = fullText.match(/(?:Module not found|Failed to load module|Cannot find module):\s*([^\s,]+)/i);
      if (notFoundMatch) {{
        let modPath = notFoundMatch[1].replace(/^\/_bundle\//, "");
        return {{
          badge: "Module Not Found (404)",
          title: `Cannot find module '${{modPath}}'`,
          file: modPath,
          solution: `Không tìm thấy file <code>${{modPath}}</code>.<br/>👉 <b>Cách sửa:</b> Kiểm tra xem file <code>${{modPath}}.tsx</code> hoặc <code>${{modPath}}/index.tsx</code> đã tồn tại chưa, hoặc kiểm tra đường dẫn import/alias <code>@/</code>.`,
          raw: fullText
        }};
      }}

      // 3. Null / Undefined property access
      const nullMatch = fullText.match(/Cannot read properties of (null|undefined) \(reading '([^']+)'\)/i);
      if (nullMatch) {{
        const nullType = nullMatch[1];
        const propName = nullMatch[2];
        const file = extractFileFromStack(stack);
        return {{
          badge: "Null Reference Error",
          title: `Cannot read properties of ${{nullType}} (reading '${{propName}}')`,
          file: file || "Component",
          solution: `Dữ liệu đang bị <code>${{nullType}}</code> khi cố gắng truy cập <code>.${{propName}}</code>.<br/>👉 <b>Cách sửa:</b> Dùng optional chaining <code>item?.${{propName}}</code> hoặc giá trị mặc định <code>(item || "").${{propName}}</code>.`,
          raw: fullText
        }};
      }}

      // 4. SWC / Syntax Error
      if (fullText.includes("SWC Compilation Error") || fullText.includes("SyntaxError")) {{
        const file = extractFileFromStack(fullText);
        return {{
          badge: "Syntax / Compile Error",
          title: "TypeScript / JSX Syntax Error",
          file: file || "Syntax Error",
          solution: `Có lỗi cú pháp hoặc thẻ JSX chưa đóng trong file.<br/>👉 <b>Cách sửa:</b> Kiểm tra dòng và cột được chỉ ra trong thông báo lỗi bên dưới.`,
          raw: fullText
        }};
      }}

      return {{
        badge: "Runtime Error",
        title: rawErr?.name || "Component Mount Failed",
        file: extractFileFromStack(stack) || "App Router",
        solution: "Kiểm tra lại dữ liệu truyền vào props của component hoặc thứ tự gọi React Hooks.",
        raw: fullText
      }};
    }}

    function showError(err) {{
      const overlay = document.getElementById('_error_overlay');
      if (overlay) {{
        const parsed = parseErrorDetails(err);
        document.getElementById('_err_badge').textContent = parsed.badge;
        document.getElementById('_err_file').textContent = parsed.file ? "📁 " + parsed.file : "";
        document.getElementById('_err_file').style.display = parsed.file ? "inline-block" : "none";
        document.getElementById('_err_title').textContent = parsed.title;
        document.getElementById('_err_solution').innerHTML = parsed.solution;
        document.getElementById('_err_raw').textContent = parsed.raw;
        overlay.style.display = 'block';
      }}
    }}

    window.addEventListener('error', (e) => showError(e.error || e.message));
    window.addEventListener('unhandledrejection', (e) => showError(e.reason));

    const moduleCache = new Map();

    function updateMountStatus(msg) {{
      const el = document.getElementById('_mount_status');
      if (el) el.textContent = msg;
    }}

    async function loadModule(specifier, timestamp = null) {{
      let clean = specifier.trim();
      if (clean.startsWith("/_bundle/")) {{
        clean = clean.slice(9);
      }}
      const normKey = clean.replace(/^\/+/, '').split('?')[0];

      if (!timestamp && moduleCache.has(normKey)) {{
        return moduleCache.get(normKey);
      }}

      updateMountStatus("Loading " + normKey + "...");
      const targetUrl = "/_bundle/" + normKey + (timestamp ? "?t=" + timestamp : "");
      try {{
        const mod = await import(targetUrl);
        moduleCache.set(normKey, mod);
        return mod;
      }} catch (err) {{
        console.error("Failed to load module:", normKey, err);
        throw err;
      }}
    }}

    let currentRoot = null;
    let currentRenderId = 0;

    function unwrapHtmlBody(node) {{
      if (!node) return node;
      if (Array.isArray(node)) {{
        const unwrapped = node.map(unwrapHtmlBody).filter(Boolean);
        if (unwrapped.length === 0) return null;
        if (unwrapped.length === 1) return unwrapped[0];
        return React.createElement(React.Fragment, null, ...unwrapped);
      }}
      if (React.isValidElement(node)) {{
        const type = node.type;
        const props = node.props || {{}};
        if (type === 'html' || (typeof type === 'string' && type.toLowerCase() === 'html')) {{
          if (props.lang && typeof document !== 'undefined' && document.documentElement) {{
            document.documentElement.lang = props.lang;
          }}
          if (props.className && typeof document !== 'undefined' && document.documentElement) {{
            document.documentElement.className = props.className;
          }}
          return unwrapHtmlBody(props.children);
        }}
        if (type === 'body' || (typeof type === 'string' && type.toLowerCase() === 'body')) {{
          if (props.className && typeof document !== 'undefined' && document.body) {{
            document.body.className = props.className;
          }}
          return unwrapHtmlBody(props.children);
        }}
        if (type === 'head' || (typeof type === 'string' && type.toLowerCase() === 'head')) {{
          return null;
        }}
        return node;
      }}
      return node;
    }}

    function makeParams(paramsObj) {{
      const p = {{ ...(paramsObj || {{}}) }};
      Object.defineProperty(p, 'then', {{
        value: (resolve) => Promise.resolve(p).then(resolve),
        enumerable: false,
        writable: true,
        configurable: true,
      }});
      return p;
    }}

    function makeSearchParams(searchParamsObj) {{
      const sp = {{ ...(searchParamsObj || {{}}) }};
      Object.defineProperty(sp, 'then', {{
        value: (resolve) => Promise.resolve(sp).then(resolve),
        enumerable: false,
        writable: true,
        configurable: true,
      }});
      return sp;
    }}

    function makeComponent(Comp) {{
      if (!Comp) return () => null;
      const isAsync = Comp.constructor && (Comp.constructor.name === 'AsyncFunction' || Comp[Symbol.toStringTag] === 'AsyncFunction');
      return function DynamicComponentWrapper(props = {{}}) {{
        const safeProps = props || {{}};
        if (isAsync) {{
          const [content, setContent] = React.useState(null);
          const [err, setErr] = React.useState(null);
          React.useEffect(() => {{
            let active = true;
            Promise.resolve(Comp(safeProps)).then(res => {{
              if (active) setContent(unwrapHtmlBody(res));
            }}).catch(e => {{
              if (active) setErr(e);
            }});
            return () => {{ active = false; }};
          }}, [safeProps.children, JSON.stringify(safeProps.params)]);

          if (err) throw err;
          return content;
        }}
        try {{
          const res = Comp(safeProps);
          if (res instanceof Promise) {{
            const [content, setContent] = React.useState(null);
            const [err, setErr] = React.useState(null);
            React.useEffect(() => {{
              let active = true;
              res.then(r => {{ if (active) setContent(unwrapHtmlBody(r)); }}).catch(e => {{ if (active) setErr(e); }});
              return () => {{ active = false; }};
            }}, []);
            if (err) throw err;
            return content;
          }}
          return unwrapHtmlBody(res);
        }} catch (e) {{
          throw e;
        }}
      }};
    }}

    class ErrorBoundary extends React.Component {{
      constructor(props) {{
        super(props);
        this.state = {{ hasError: false, error: null }};
      }}
      static getDerivedStateFromError(error) {{
        return {{ hasError: true, error }};
      }}
      componentDidCatch(error, info) {{
        console.error("[Route ErrorBoundary]", error, info);
      }}
      render() {{
        if (this.state.hasError) {{
          if (this.props.fallback) {{
            const Fallback = this.props.fallback;
            return React.createElement(Fallback, {{
              error: this.state.error,
              reset: () => this.setState({{ hasError: false, error: null }})
            }});
          }}
          return React.createElement("div", {{
            style: {{ padding: "2rem", color: "#ef4444", fontFamily: "system-ui" }}
          }}, React.createElement("h2", {{ style: {{ fontWeight: 600 }} }}, "Application Error"),
             React.createElement("pre", {{ style: {{ marginTop: "0.5rem", fontSize: "0.875rem" }} }}, this.state.error?.message || String(this.state.error)),
             React.createElement("button", {{
               onClick: () => this.setState({{ hasError: false, error: null }}),
               style: {{ marginTop: "1rem", padding: "0.5rem 1rem", background: "#ef4444", color: "white", borderRadius: "0.375rem", border: "none", cursor: "pointer" }}
             }}, "Try again")
          );
        }}
        return this.props.children;
      }}
    }}

    class NotFoundBoundary extends React.Component {{
      constructor(props) {{
        super(props);
        this.state = {{ isNotFound: false }};
      }}
      static getDerivedStateFromError(error) {{
        if (error?.digest === "NEXT_NOT_FOUND" || error?.message === "NEXT_NOT_FOUND" || (error?.message && error.message.includes("404 Not Found"))) {{
          return {{ isNotFound: true }};
        }}
        throw error;
      }}
      render() {{
        if (this.state.isNotFound) {{
          if (this.props.fallback) {{
            const Fallback = this.props.fallback;
            return React.createElement(Fallback, {{}});
          }}
          return React.createElement("div", {{
            style: {{ minHeight: "60vh", display: "flex", alignItems: "center", justifyContent: "center", flexDirection: "column" }}
          }}, React.createElement("h1", {{ style: {{ fontSize: "2rem", fontWeight: 700 }} }}, "404 - Page Not Found"));
        }}
        return this.props.children;
      }}
    }}

    async function renderRoute(entryStr, layoutFiles, params = {{}}, timestamp = null, segmentFiles = [], globalErrorFile = null) {{
      const renderId = ++currentRenderId;
      window.__NATA_PARAMS__ = params || {{}};

      const searchParamsObj = typeof window !== 'undefined'
        ? Object.fromEntries(new URLSearchParams(window.location.search))
        : {{}};

      const safeParams = makeParams(params);
      const safeSearchParams = makeSearchParams(searchParamsObj);
      const routeProps = {{
        params: safeParams,
        searchParams: safeSearchParams
      }};

      window.dispatchEvent(new CustomEvent("_nata_navigate", {{ detail: {{ params: window.__NATA_PARAMS__, searchParams: searchParamsObj }} }}));

      try {{
        // Parallel preload all segment modules
        const allModules = [entryStr];
        if (Array.isArray(segmentFiles)) {{
          for (const s of segmentFiles) {{
            if (s.layout) allModules.push(s.layout);
            if (s.template) allModules.push(s.template);
            if (s.error) allModules.push(s.error);
            if (s.loading) allModules.push(s.loading);
            if (s.not_found) allModules.push(s.not_found);
          }}
        }} else if (Array.isArray(layoutFiles)) {{
          allModules.push(...layoutFiles);
        }}
        if (globalErrorFile) allModules.push(globalErrorFile);

        await Promise.all(allModules.map(f => loadModule(f, timestamp).catch(() => null)));

        const pageMod = await loadModule(entryStr, timestamp);
        const Page = pageMod.default || Object.values(pageMod).find(v => typeof v === 'function');

        if (!Page) {{
          throw new Error("No default export or React component found in " + entryStr);
        }}

        let RootComponent = makeComponent(Page);

        if (Array.isArray(segmentFiles) && segmentFiles.length > 0) {{
          for (let i = segmentFiles.length - 1; i >= 0; i--) {{
            const seg = segmentFiles[i];

            if (seg.not_found) {{
              try {{
                const nfMod = await loadModule(seg.not_found, timestamp);
                const NF = nfMod.default || Object.values(nfMod).find(v => typeof v === 'function');
                if (NF) {{
                  const CurrentChild = RootComponent;
                  const WrappedNF = makeComponent(NF);
                  RootComponent = (props = {{}}) => React.createElement(NotFoundBoundary, {{ fallback: WrappedNF }}, React.createElement(CurrentChild, props));
                }}
              }} catch (e) {{}}
            }}

            if (seg.loading) {{
              try {{
                const loadMod = await loadModule(seg.loading, timestamp);
                const Loading = loadMod.default || Object.values(loadMod).find(v => typeof v === 'function');
                if (Loading) {{
                  const CurrentChild = RootComponent;
                  const WrappedLoading = makeComponent(Loading);
                  RootComponent = (props = {{}}) => React.createElement(React.Suspense, {{ fallback: React.createElement(WrappedLoading, props) }}, React.createElement(CurrentChild, props));
                }}
              }} catch (e) {{}}
            }}

            if (seg.error) {{
              try {{
                const errMod = await loadModule(seg.error, timestamp);
                const ErrComp = errMod.default || Object.values(errMod).find(v => typeof v === 'function');
                if (ErrComp) {{
                  const CurrentChild = RootComponent;
                  const WrappedErr = makeComponent(ErrComp);
                  RootComponent = (props = {{}}) => React.createElement(ErrorBoundary, {{ fallback: WrappedErr }}, React.createElement(CurrentChild, props));
                }}
              }} catch (e) {{}}
            }}

            if (seg.template) {{
              try {{
                const tplMod = await loadModule(seg.template, timestamp);
                const Tpl = tplMod.default || Object.values(tplMod).find(v => typeof v === 'function');
                if (Tpl) {{
                  const CurrentChild = RootComponent;
                  const WrappedTpl = makeComponent(Tpl);
                  RootComponent = (props = {{}}) => React.createElement(WrappedTpl, {{ ...routeProps, ...(props || {{}}), key: window.location.pathname, children: React.createElement(CurrentChild, props) }});
                }}
              }} catch (e) {{}}
            }}

            if (seg.layout) {{
              try {{
                const layoutMod = await loadModule(seg.layout, timestamp);
                let Layout = layoutMod.default;
                if (!Layout) {{
                  const entry = Object.entries(layoutMod).find(([k, v]) => typeof v === 'function' && (/^[A-Z]/.test(k) || k.endsWith('Layout')));
                  Layout = entry ? entry[1] : undefined;
                }}
                if (!Layout) {{
                  Layout = Object.values(layoutMod).find(v => typeof v === 'function');
                }}
                if (Layout) {{
                  const CurrentChild = RootComponent;
                  const WrappedLayout = makeComponent(Layout);
                  RootComponent = (props = {{}}) => React.createElement(WrappedLayout, {{ ...routeProps, ...(props || {{}}), children: React.createElement(CurrentChild, props) }});
                }}
              }} catch (layoutErr) {{
                console.warn("Could not wrap layout:", seg.layout, layoutErr);
              }}
            }}
          }}
        }} else {{
          // Fallback legacy layout wrapping
          for (let i = layoutFiles.length - 1; i >= 0; i--) {{
            try {{
              const layoutMod = await loadModule(layoutFiles[i], timestamp);
              let Layout = layoutMod.default;
              if (!Layout) {{
                const entry = Object.entries(layoutMod).find(([k, v]) => typeof v === 'function' && (/^[A-Z]/.test(k) || k.endsWith('Layout')));
                Layout = entry ? entry[1] : undefined;
              }}
              if (!Layout) {{
                Layout = Object.values(layoutMod).find(v => typeof v === 'function');
              }}
              if (Layout) {{
                const CurrentChild = RootComponent;
                const WrappedLayout = makeComponent(Layout);
                RootComponent = (props = {{}}) => React.createElement(WrappedLayout, {{ ...routeProps, ...(props || {{}}), children: React.createElement(CurrentChild, props) }});
              }}
            }} catch (layoutErr) {{
              console.warn("Could not wrap layout:", layoutFiles[i], layoutErr);
            }}
          }}
        }}

        if (globalErrorFile) {{
          try {{
            const geMod = await loadModule(globalErrorFile, timestamp);
            const GE = geMod.default || Object.values(geMod).find(v => typeof v === 'function');
            if (GE) {{
              const CurrentChild = RootComponent;
              const WrappedGE = makeComponent(GE);
              RootComponent = (props = {{}}) => React.createElement(ErrorBoundary, {{ fallback: WrappedGE }}, React.createElement(CurrentChild, props));
            }}
          }} catch (e) {{}}
        }}

        if (renderId === currentRenderId) {{
          const rootEl = document.getElementById("root");
          const isSsr = rootEl && rootEl.getAttribute("data-nata-ssr") === "true";

          if (!currentRoot) {{
            if (isSsr && typeof ReactDOM.hydrateRoot === "function") {{
              try {{
                currentRoot = ReactDOM.hydrateRoot(rootEl, React.createElement(RootComponent, routeProps), {{
                  onRecoverableError(error) {{
                    console.warn("[NATA Hydration Info]", error);
                  }}
                }});
                rootEl.removeAttribute("data-nata-ssr");
              }} catch (hydrateErr) {{
                console.warn("[NATA SSR] Hydration mismatch/error, falling back to createRoot:", hydrateErr);
                rootEl.innerHTML = "";
                currentRoot = ReactDOM.createRoot(rootEl);
                currentRoot.render(React.createElement(RootComponent, routeProps));
              }}
            }} else {{
              currentRoot = ReactDOM.createRoot(rootEl);
              currentRoot.render(React.createElement(RootComponent, routeProps));
            }}
          }} else {{
            currentRoot.render(React.createElement(RootComponent, routeProps));
          }}
          const overlay = document.getElementById('_error_overlay');
          if (overlay) overlay.style.display = 'none';
        }}
      }} catch (err) {{
        console.error("Mount error:", err);
        showError(err);
      }}
    }}

    const routeInfoCache = new Map();

    async function navigateTo(targetUrl, options = {{}}) {{
      try {{
        const urlObj = new URL(targetUrl, window.location.origin);
        const targetPath = urlObj.pathname;

        if (!options.fromPopState) {{
          if (options.replace) {{
            window.history.replaceState({{}}, "", urlObj.pathname + urlObj.search + urlObj.hash);
          }} else {{
            window.history.pushState({{}}, "", urlObj.pathname + urlObj.search + urlObj.hash);
          }}
        }}

        window.dispatchEvent(new CustomEvent("_nata_navigate", {{ detail: {{ url: targetUrl, pathname: targetPath }} }}));

        let info = routeInfoCache.get(targetPath);
        if (!info) {{
          const res = await fetch(`/_nata/route_info?path=${{encodeURIComponent(targetPath)}}`);
          if (!res.ok) {{
            window.location.href = targetUrl;
            return;
          }}
          info = await res.json();
          if (info && info.found) {{
            routeInfoCache.set(targetPath, info);
          }}
        }}

        if (!info || !info.found) {{
          if (info && info.not_found_file) {{
            await renderRoute(info.not_found_file, info.layout_files || [], info.params || {{}}, null, info.segments_files || [], info.global_error_file);
            return;
          }}
          window.location.href = targetUrl;
          return;
        }}

        await renderRoute(info.page_file, info.layout_files || [], info.params || {{}}, null, info.segments_files || [], info.global_error_file);
        window.scrollTo(0, 0);
      }} catch (err) {{
        console.warn("SPA navigation fallback:", err);
        window.location.href = targetUrl;
      }}
    }}

    window.__NATA_NAVIGATE__ = navigateTo;

    window.addEventListener('popstate', () => {{
      const currentPath = window.location.pathname + window.location.search;
      navigateTo(currentPath, {{ fromPopState: true }});
    }});

    // Neutral Dark Query & Action Debugger Engine
    window.__NATA_LOGS__ = [];
    window.__NATA_CURRENT_TAB__ = "all";
    window.__NATA_EXPANDED_ITEMS__ = new Set();
    window.__NATA_IS_DRAWER_OPEN__ = false;
    window.__NATA_IS_MAXIMIZED__ = false;

    function escapeHtml(str) {{
      if (str === null || str === undefined) return '';
      return String(str)
        .replace(/&/g, '&amp;')
        .replace(/</g, '&lt;')
        .replace(/>/g, '&gt;')
        .replace(/"/g, '&quot;')
        .replace(/'/g, '&#039;');
    }}

    function highlightSql(sql) {{
      if (!sql) return '';
      const safe = escapeHtml(sql);
      const sqlRegex = /('(?:[^'\\]|\\.)*'|"(?:[^"\\]|\\.)*")|(\$\d+)|\b(SELECT|FROM|WHERE|AND|OR|NOT|IN|LIKE|ILIKE|BETWEEN|IS|NULL|JOIN|INNER\s+JOIN|LEFT\s+JOIN|RIGHT\s+JOIN|FULL\s+JOIN|CROSS\s+JOIN|ON|GROUP\s+BY|ORDER\s+BY|ASC|DESC|LIMIT|OFFSET|HAVING|UNION|ALL|AS|DISTINCT|CASE|WHEN|THEN|ELSE|END|INSERT\s+INTO|VALUES|UPDATE|SET|DELETE\s+FROM|DELETE|RETURNING|CREATE\s+TABLE|ALTER\s+TABLE|DROP\s+TABLE|BEGIN|COMMIT|ROLLBACK|COUNT|SUM|AVG|MIN|MAX|EXISTS|COALESCE)\b|\b(\d+(?:\.\d+)?)\b/gi;
      return safe.replace(sqlRegex, function (match, str, param, kw, num) {{
        if (str) return '<span style="color:#34d399;">' + str + '</span>';
        if (param) return '<span style="color:#c084fc;font-weight:600;">' + param + '</span>';
        if (kw) return '<span style="color:#38bdf8;font-weight:600;">' + kw + '</span>';
        if (num) return '<span style="color:#fbbf24;">' + num + '</span>';
        return match;
      }});
    }}

    function syntaxHighlightJson(json) {{
      if (json === undefined || json === null) return '<span style="color:#71717a;">null</span>';
      let str = '';
      try {{
        str = JSON.stringify(json, null, 2);
      }} catch (e) {{
        str = String(json);
      }}
      const safe = escapeHtml(str);
      return safe.replace(/("(\\u[a-zA-Z0-9]{{4}}|\\[^u]|[^\\"])*"(\s*:)?|\b(true|false|null)\b|-?\d+(?:\.\d*)?(?:[eE][+\-]?\d+)?)/g, function (match) {{
        let cls = 'color:#fbbf24;';
        if (/^"/.test(match)) {{
          if (/:$/.test(match)) {{
            cls = 'color:#93c5fd;font-weight:500;';
          }} else {{
            cls = 'color:#34d399;';
          }}
        }} else if (/true|false/.test(match)) {{
          cls = 'color:#c084fc;font-weight:600;';
        }} else if (/null/.test(match)) {{
          cls = 'color:#71717a;';
        }}
        return '<span style="' + cls + '">' + match + '</span>';
      }});
    }}

    window.__NATA_COPY__ = function(text, btnId) {{
      navigator.clipboard.writeText(text);
      const btn = document.getElementById(btnId);
      if (btn) {{
        const originalText = btn.innerHTML;
        btn.innerHTML = '<span style="color:#34d399;">✓ Copied!</span>';
        setTimeout(() => {{ btn.innerHTML = originalText; }}, 1400);
      }}
    }};

    window.__NATA_TOGGLE_ITEM__ = function(id) {{
      if (window.__NATA_EXPANDED_ITEMS__.has(id)) {{
        window.__NATA_EXPANDED_ITEMS__.delete(id);
      }} else {{
        window.__NATA_EXPANDED_ITEMS__.add(id);
      }}
      renderLogs();
    }};

    window.__NATA_TOGGLE_DEBUGGER__ = function() {{
      const drawer = document.getElementById('_nata_debug_drawer');
      if (!drawer) return;
      window.__NATA_IS_DRAWER_OPEN__ = !window.__NATA_IS_DRAWER_OPEN__;
      drawer.style.display = window.__NATA_IS_DRAWER_OPEN__ ? 'flex' : 'none';
      if (window.__NATA_IS_DRAWER_OPEN__) {{
        renderLogs();
        setTimeout(() => {{
          const searchInput = document.getElementById('_dbg_search');
          if (searchInput) searchInput.focus();
        }}, 50);
      }}
    }};

    window.__NATA_TOGGLE_EXPAND__ = function() {{
      const drawer = document.getElementById('_nata_debug_drawer');
      const btn = document.getElementById('_dbg_expand_btn');
      if (!drawer) return;
      window.__NATA_IS_MAXIMIZED__ = !window.__NATA_IS_MAXIMIZED__;
      drawer.style.height = window.__NATA_IS_MAXIMIZED__ ? '85vh' : '460px';
      if (btn) btn.textContent = window.__NATA_IS_MAXIMIZED__ ? '⤓' : '⤢';
    }};

    window.__NATA_SET_TAB__ = function(tabName) {{
      window.__NATA_CURRENT_TAB__ = tabName;
      ['all', 'sql', 'rpc', 'error'].forEach(t => {{
        const el = document.getElementById('_tab_btn_' + t);
        if (el) {{
          if (t === tabName) {{
            el.style.background = '#27272a';
            el.style.color = '#fafafa';
          }} else {{
            el.style.background = 'transparent';
            el.style.color = '#a1a1aa';
          }}
        }}
      }});
      renderLogs();
    }};

    window.__NATA_FILTER_LOGS__ = function() {{
      renderLogs();
    }};

    window.__NATA_CLEAR_LOGS__ = async function() {{
      window.__NATA_LOGS__ = [];
      window.__NATA_EXPANDED_ITEMS__.clear();
      try {{
        await fetch('/_nata/logs', {{ method: 'DELETE' }});
      }} catch (e) {{}}
      renderLogs();
    }};

    window.__NATA_EXPORT_LOGS__ = function() {{
      const dataStr = 'data:text/json;charset=utf-8,' + encodeURIComponent(JSON.stringify(window.__NATA_LOGS__, null, 2));
      const downloadAnchor = document.createElement('a');
      downloadAnchor.setAttribute('href', dataStr);
      downloadAnchor.setAttribute('download', 'nata-query-logs-' + Date.now() + '.json');
      document.body.appendChild(downloadAnchor);
      downloadAnchor.click();
      downloadAnchor.remove();
    }};

    function updatePillBadges() {{
      const total = window.__NATA_LOGS__.length;
      const errors = window.__NATA_LOGS__.filter(l => l.status === 'error').length;
      const pillStats = document.getElementById('_nata_pill_stats');
      const pillErr = document.getElementById('_nata_pill_err');
      if (pillStats) {{
        pillStats.textContent = total + ' ' + (total === 1 ? 'log' : 'logs');
      }}
      if (pillErr) {{
        if (errors > 0) {{
          pillErr.style.display = 'inline-block';
          pillErr.textContent = errors + ' err';
        }} else {{
          pillErr.style.display = 'none';
        }}
      }}
    }}

    function renderLogs() {{
      const container = document.getElementById('_dbg_logs_container');
      if (!container) return;

      const searchInput = document.getElementById('_dbg_search');
      const query = (searchInput ? searchInput.value : '').trim().toLowerCase();
      const tab = window.__NATA_CURRENT_TAB__ || 'all';

      // Update tab counter numbers
      const allCount = window.__NATA_LOGS__.length;
      const sqlCount = window.__NATA_LOGS__.filter(l => l.type === 'sql').length;
      const rpcCount = window.__NATA_LOGS__.filter(l => l.type === 'rpc').length;
      const errCount = window.__NATA_LOGS__.filter(l => l.status === 'error').length;

      const tabCntAll = document.getElementById('_tab_cnt_all');
      const tabCntSql = document.getElementById('_tab_cnt_sql');
      const tabCntRpc = document.getElementById('_tab_cnt_rpc');
      const tabCntErr = document.getElementById('_tab_cnt_err');
      if (tabCntAll) tabCntAll.textContent = '(' + allCount + ')';
      if (tabCntSql) tabCntSql.textContent = '(' + sqlCount + ')';
      if (tabCntRpc) tabCntRpc.textContent = '(' + rpcCount + ')';
      if (tabCntErr) tabCntErr.textContent = '(' + errCount + ')';

      updatePillBadges();

      // Filter by tab
      let filtered = window.__NATA_LOGS__.filter(item => {{
        if (tab === 'sql') return item.type === 'sql';
        if (tab === 'rpc') return item.type === 'rpc';
        if (tab === 'error') return item.status === 'error';
        return true;
      }});

      // Filter by search query
      if (query) {{
        filtered = filtered.filter(item => {{
          const str = JSON.stringify(item).toLowerCase();
          return str.includes(query);
        }});
      }}

      if (filtered.length === 0) {{
        container.innerHTML = '<div style="display:flex;flex-direction:column;align-items:center;justify-content:center;height:100%;min-height:220px;color:#71717a;gap:10px;user-select:none;">' +
          '<div style="font-size:28px;opacity:0.6;">⚡</div>' +
          '<div style="font-size:14px;font-weight:500;color:#a1a1aa;">No logs to display</div>' +
          '<div style="font-size:12px;color:#71717a;">Execute queries or server actions in your app to inspect them live.</div>' +
        '</div>';
        return;
      }}

      let html = '';
      for (let i = filtered.length - 1; i >= 0; i--) {{
        const item = filtered[i];
        const isExpanded = window.__NATA_EXPANDED_ITEMS__.has(item.id);
        const isError = item.status === 'error';
        const isSql = item.type === 'sql';
        const isRpc = item.type === 'rpc';
        const duration = Number(item.duration_ms || 0);

        // Latency color styling
        let latColor = '#34d399';
        let latBg = 'rgba(16,185,129,0.12)';
        let latBorder = 'rgba(16,185,129,0.25)';
        if (duration >= 200 || isError) {{
          latColor = '#f43f5e';
          latBg = 'rgba(244,63,94,0.12)';
          latBorder = 'rgba(244,63,94,0.25)';
        }} else if (duration >= 50) {{
          latColor = '#fbbf24';
          latBg = 'rgba(245,158,11,0.12)';
          latBorder = 'rgba(245,158,11,0.25)';
        }}

        // Type Badge
        let typeBadge = '';
        if (isError) {{
          typeBadge = '<span style="background:rgba(244,63,94,0.15);color:#fda4af;border:1px solid rgba(244,63,94,0.3);padding:2px 7px;border-radius:6px;font-size:10px;font-weight:700;">ERROR</span>';
        }} else if (isSql) {{
          typeBadge = '<span style="background:rgba(56,189,248,0.12);color:#38bdf8;border:1px solid rgba(56,189,248,0.25);padding:2px 7px;border-radius:6px;font-size:10px;font-weight:700;">SQL</span>';
        }} else {{
          typeBadge = '<span style="background:rgba(168,85,247,0.12);color:#c084fc;border:1px solid rgba(168,85,247,0.25);padding:2px 7px;border-radius:6px;font-size:10px;font-weight:700;">RPC</span>';
        }}

        // Summary Title
        let summaryTitle = '';
        if (isSql) {{
          const cleanSql = (item.sql || '').trim().replace(/\s+/g, ' ');
          summaryTitle = highlightSql(cleanSql.length > 120 ? cleanSql.slice(0, 120) + '...' : cleanSql);
        }} else if (isRpc) {{
          summaryTitle = '<span style="color:#e4e4e7;font-weight:600;">' + escapeHtml(item.module) + '</span> ' +
            '<span style="color:#71717a;">›</span> ' +
            '<span style="color:#38bdf8;font-weight:600;">' + escapeHtml(item.action) + '()</span>';
        }}

        // Meta info (row count or nested queries)
        let metaInfo = '';
        if (isSql) {{
          metaInfo = '<span style="color:#a1a1aa;font-size:11px;">' + (item.rowCount !== undefined ? item.rowCount + ' row(s)' : 'query') + '</span>';
        }} else if (isRpc) {{
          const qLen = Array.isArray(item.queries) ? item.queries.length : 0;
          metaInfo = '<span style="color:#a1a1aa;font-size:11px;">' + (qLen > 0 ? qLen + ' SQL query(s)' : 'action') + '</span>';
        }}

        const timeStr = item.timestamp ? new Date(item.timestamp).toLocaleTimeString() : '';

        // Card header
        html += '<div style="background:#121215;border:1px solid ' + (isExpanded ? '#3f3f46' : '#27272a') + ';border-radius:10px;overflow:hidden;flex-shrink:0;transition:border-color 0.15s ease;">';
        html += '<div onclick="window.__NATA_TOGGLE_ITEM__(\'' + item.id + '\')" style="padding:10px 14px;cursor:pointer;display:flex;align-items:center;justify-content:space-between;gap:12px;user-select:none;min-height:42px;box-sizing:border-box;background:' + (isExpanded ? '#18181b' : 'transparent') + ';">';
        html += '<div style="display:flex;align-items:center;gap:10px;flex:1;overflow:hidden;min-width:0;">';
        html += '<span style="font-size:10px;color:#71717a;transform:rotate(' + (isExpanded ? '90deg' : '0deg') + ');transition:transform 0.15s ease;flex-shrink:0;">▶</span>';
        html += typeBadge;
        html += '<div style="font-family:ui-monospace,SFMono-Regular,Menlo,monospace;font-size:12px;color:#f4f4f5;white-space:nowrap;overflow:hidden;text-overflow:ellipsis;flex:1;min-width:0;">' + summaryTitle + '</div>';
        html += '</div>';
        html += '<div style="display:flex;align-items:center;gap:10px;flex-shrink:0;">';
        html += metaInfo;
        html += '<span style="background:' + latBg + ';color:' + latColor + ';border:1px solid ' + latBorder + ';padding:2px 8px;border-radius:6px;font-size:11px;font-family:ui-monospace,monospace;font-weight:600;">' + duration + 'ms</span>';
        html += '<span style="color:#71717a;font-size:11px;font-family:ui-monospace,monospace;">' + timeStr + '</span>';
        html += '</div>';
        html += '</div>';

        // Card expanded body
        if (isExpanded) {{
          html += '<div style="padding:14px;border-top:1px solid #27272a;display:flex;flex-direction:column;gap:12px;background:#09090b;">';

          // 1. SQL Query Box
          if (isSql && item.sql) {{
            const copyBtnId = '_btn_copy_sql_' + item.id;
            const rawSqlEscaped = escapeHtml(item.sql).replace(/'/g, "\\'");
            html += '<div style="display:flex;flex-direction:column;gap:6px;">';
            html += '<div style="display:flex;align-items:center;justify-content:space-between;">';
            html += '<span style="font-size:11px;font-weight:600;color:#94a3b8;text-transform:uppercase;letter-spacing:0.5px;">SQL Statement</span>';
            html += '<button id="' + copyBtnId + '" onclick="window.__NATA_COPY__(\'' + rawSqlEscaped + '\', \'' + copyBtnId + '\')" style="background:#18181b;border:1px solid #27272a;color:#e4e4e7;padding:3px 8px;border-radius:6px;font-size:11px;cursor:pointer;">Copy SQL</button>';
            html += '</div>';
            html += '<pre style="margin:0;background:#121215;padding:12px;border-radius:8px;border:1px solid #27272a;font-family:ui-monospace,SFMono-Regular,Menlo,monospace;font-size:12px;line-height:1.5;color:#f4f4f5;white-space:pre-wrap;word-break:break-all;overflow-x:auto;">' + highlightSql(item.sql) + '</pre>';
            html += '</div>';
          }}

          // 2. Parameters / Arguments Box
          const params = isSql ? item.parameters : item.parameters;
          if (params !== undefined && params !== null && (Array.isArray(params) ? params.length > 0 : Object.keys(params).length > 0)) {{
            const copyBtnId = '_btn_copy_param_' + item.id;
            const rawParamsEscaped = escapeHtml(JSON.stringify(params, null, 2)).replace(/'/g, "\\'");
            html += '<div style="display:flex;flex-direction:column;gap:6px;">';
            html += '<div style="display:flex;align-items:center;justify-content:space-between;">';
            html += '<span style="font-size:11px;font-weight:600;color:#94a3b8;text-transform:uppercase;letter-spacing:0.5px;">Parameters / Arguments</span>';
            html += '<button id="' + copyBtnId + '" onclick="window.__NATA_COPY__(\'' + rawParamsEscaped + '\', \'' + copyBtnId + '\')" style="background:#18181b;border:1px solid #27272a;color:#e4e4e7;padding:3px 8px;border-radius:6px;font-size:11px;cursor:pointer;">Copy Params</button>';
            html += '</div>';
            html += '<pre style="margin:0;background:#121215;padding:12px;border-radius:8px;border:1px solid #27272a;font-family:ui-monospace,SFMono-Regular,Menlo,monospace;font-size:12px;line-height:1.5;color:#f4f4f5;white-space:pre-wrap;word-break:break-all;overflow-x:auto;">' + syntaxHighlightJson(params) + '</pre>';
            html += '</div>';
          }}

          // 3. Response / Result Payload Box
          if (item.response !== undefined && item.response !== null) {{
            const copyBtnId = '_btn_copy_res_' + item.id;
            const rawResEscaped = escapeHtml(JSON.stringify(item.response, null, 2)).replace(/'/g, "\\'");
            const isArr = Array.isArray(item.response);
            const countLabel = isArr ? ' (' + item.response.length + ' rows)' : '';
            html += '<div style="display:flex;flex-direction:column;gap:6px;">';
            html += '<div style="display:flex;align-items:center;justify-content:space-between;">';
            html += '<span style="font-size:11px;font-weight:600;color:#94a3b8;text-transform:uppercase;letter-spacing:0.5px;">Response / Result' + countLabel + '</span>';
            html += '<button id="' + copyBtnId + '" onclick="window.__NATA_COPY__(\'' + rawResEscaped + '\', \'' + copyBtnId + '\')" style="background:#18181b;border:1px solid #27272a;color:#e4e4e7;padding:3px 8px;border-radius:6px;font-size:11px;cursor:pointer;">Copy Response</button>';
            html += '</div>';
            html += '<pre style="margin:0;background:#121215;padding:12px;border-radius:8px;border:1px solid #27272a;font-family:ui-monospace,SFMono-Regular,Menlo,monospace;font-size:12px;line-height:1.5;color:#f4f4f5;white-space:pre-wrap;word-break:break-all;max-height:260px;overflow-y:auto;">' + syntaxHighlightJson(item.response) + '</pre>';
            html += '</div>';
          }}

          // 4. Nested SQL Queries (for RPC actions)
          if (isRpc && Array.isArray(item.queries) && item.queries.length > 0) {{
            html += '<div style="display:flex;flex-direction:column;gap:6px;">';
            html += '<span style="font-size:11px;font-weight:600;color:#38bdf8;text-transform:uppercase;letter-spacing:0.5px;">Executed SQL Queries (' + item.queries.length + ')</span>';
            html += '<div style="display:flex;flex-direction:column;gap:6px;">';
            for (const q of item.queries) {{
              html += '<div style="background:#18181b;padding:8px 12px;border-radius:6px;border:1px solid #27272a;font-family:ui-monospace,SFMono-Regular,Menlo,monospace;font-size:11px;">';
              html += '<div style="display:flex;justify-content:space-between;margin-bottom:4px;">';
              html += '<span style="color:#38bdf8;font-weight:600;">SQL</span>';
              html += '<span style="color:#34d399;font-weight:600;">' + (q.duration_ms || 0) + 'ms</span>';
              html += '</div>';
              html += '<div style="color:#f4f4f5;white-space:pre-wrap;word-break:break-all;">' + highlightSql(q.sql || '') + '</div>';
              if (q.parameters && q.parameters.length > 0) {{
                html += '<div style="color:#a1a1aa;margin-top:4px;font-size:10px;">Params: ' + escapeHtml(JSON.stringify(q.parameters)) + '</div>';
              }}
              html += '</div>';
            }}
            html += '</div>';
            html += '</div>';
          }}

          // 5. Error Info Box (if status === 'error')
          if (isError && item.error) {{
            html += '<div style="background:rgba(244,63,94,0.1);border:1px solid rgba(244,63,94,0.3);border-radius:8px;padding:12px;color:#fda4af;font-size:12px;">';
            html += '<div style="font-weight:700;color:#f43f5e;margin-bottom:4px;">Error Details</div>';
            html += '<pre style="margin:0;font-family:ui-monospace,monospace;white-space:pre-wrap;word-break:break-all;color:#fca5a5;">' + escapeHtml(item.error) + '</pre>';
            html += '</div>';
          }}

          html += '</div>';
        }}

        html += '</div>';
      }}

      container.innerHTML = html;
    }}

    // Global keyboard shortcuts for DevTools
    window.addEventListener('keydown', (e) => {{
      // Ctrl+Shift+D or Cmd+Shift+D to toggle debugger
      if ((e.ctrlKey || e.metaKey) && e.shiftKey && (e.key === 'D' || e.key === 'd')) {{
        e.preventDefault();
        window.__NATA_TOGGLE_DEBUGGER__();
      }}
      // Escape to close debugger
      if (e.key === 'Escape' && window.__NATA_IS_DRAWER_OPEN__) {{
        window.__NATA_TOGGLE_DEBUGGER__();
      }}
      // / or Ctrl+K / Cmd+K to focus search input when open
      if ((e.key === '/' || ((e.ctrlKey || e.metaKey) && (e.key === 'k' || e.key === 'K'))) && window.__NATA_IS_DRAWER_OPEN__) {{
        const searchInput = document.getElementById('_dbg_search');
        if (searchInput && document.activeElement !== searchInput) {{
          e.preventDefault();
          searchInput.focus();
          searchInput.select();
        }}
      }}
    }});

    // Fetch initial logs on mount
    fetch('/_nata/logs').then(r => r.json()).then(data => {{
      if (data && Array.isArray(data.logs)) {{
        window.__NATA_LOGS__ = data.logs;
        renderLogs();
      }}
    }}).catch(() => {{}});

    // Diagnostic notice if CDN network is slow
    const mountDiagnosticTimer = setTimeout(() => {{
      const el = document.getElementById('_mount_status');
      if (el && document.getElementById('root')?.contains(el)) {{
        el.innerHTML = '<span style="color:#f59e0b;">Fetching remote packages from CDN...</span><br/><span style="font-size:11px;color:#94a3b8;">If this takes long, check your internet connection or browser Network tab.</span>';
      }}
    }}, 4000);

    let initialData = {{}};
    try {{
      const ssrDataEl = document.getElementById('__NATA_SSR_DATA__');
      if (ssrDataEl && ssrDataEl.textContent) {{
        initialData = JSON.parse(ssrDataEl.textContent);
      }}
    }} catch (e) {{}}

    const initialParams = (initialData && initialData.params) ? initialData.params : {{}};
    window.__NATA_PARAMS__ = initialParams;

    // Initial mount
    renderRoute("{entry_str}", {layouts_json}, initialParams).finally(() => {{
      clearTimeout(mountDiagnosticTimer);
    }});

    // HMR WebSocket with Smart CSS Hot Reload and Live Component Updates
    const wsProtocol = location.protocol === 'https:' ? 'wss:' : 'ws:';
    const ws = new WebSocket(wsProtocol + '//' + location.host + '/_hmr');
    ws.onmessage = async (e) => {{
      try {{
        const data = JSON.parse(e.data);
        const now = data.timestamp || Date.now();

        if (data.type === 'query_log') {{
          if (data.log) {{
            window.__NATA_LOGS__.push(data.log);
          }}
          if (Array.isArray(data.queries)) {{
            for (const q of data.queries) {{
              if (!window.__NATA_LOGS__.some(l => l.id === q.id)) {{
                window.__NATA_LOGS__.push(q);
              }}
            }}
          }}
          renderLogs();
        }} else if (data.type === 'css') {{
          console.log('[HMR:CSS] Live updating styles:', data.file);
          try {{
            const cssRes = await fetch('/_nata/styles.css?t=' + now);
            if (cssRes.ok) {{
              const newCss = await cssRes.text();
              const styleTag = document.getElementById('_nata_custom_css');
              if (styleTag) {{
                styleTag.textContent = newCss;
              }}
            }}
          }} catch (cssErr) {{
            console.warn('[HMR:CSS] Hot update fallback reload:', cssErr);
            location.reload();
          }}
        }} else if (data.type === 'reload' || data.type === 'RELOAD') {{
          console.log('[HMR:RELOAD] Source modified, hot updating:', data.file);
          moduleCache.clear();
          routeInfoCache.clear();

          const currentPath = window.location.pathname;
          let info = null;
          try {{
            const res = await fetch(`/_nata/route_info?path=${{encodeURIComponent(currentPath)}}&t=${{now}}`);
            if (res.ok) info = await res.json();
          }} catch {{}}

          if (info && info.found) {{
            await renderRoute(info.page_file, info.layout_files || [], info.params || {{}}, now, info.segments_files || [], info.global_error_file);
          }} else {{
            location.reload();
          }}
        }}
      }} catch (err) {{
        console.error('[HMR] Message processing error:', err);
      }}
    }};
    ws.onclose = () => {{
      const statusEl = document.getElementById('_dev_status');
      if (statusEl) statusEl.style.background = '#ef4444';
    }};
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
            "body { color: red; }",
        );

        assert!(html.contains(r#""next/link": "/_nata/shims/next/link""#));
        assert!(html.contains(r#""react": "https://esm.sh/react@18.3.1""#));
        // Ensure no wildcard "/" or "/react@18.3.1/" in importmap that intercepts "/_bundle/..."
        assert!(!html.contains(r#""/":"#) && !html.contains(r#""/": "#));
        assert!(html.contains("renderRoute(\"app/page.tsx\""));
        assert!(html.contains("[\"app/layout.tsx\"]"));
    }
}
