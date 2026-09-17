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
        let zmp_header_html = if target == "zalo" {
            r#"<div id="_zmp_sim_header" style="position:sticky;top:0;left:0;right:0;height:44px;background:#0068ff;color:white;display:flex;align-items:center;justify-content:space-between;padding:0 12px;z-index:99999;font-family:system-ui,-apple-system,sans-serif;font-size:14px;font-weight:600;box-shadow:0 1px 4px rgba(0,0,0,0.15);">
    <div style="display:flex;align-items:center;gap:8px;">
      <span style="cursor:pointer;font-size:18px;" onclick="window.history.back()">❮</span>
      <span>Zalo Mini App</span>
    </div>
    <div style="display:flex;align-items:center;gap:10px;background:rgba(255,255,255,0.25);padding:4px 10px;border-radius:16px;font-size:12px;">
      <span style="cursor:pointer;">⋯</span>
      <span style="opacity:0.6;">|</span>
      <span style="cursor:pointer;" onclick="console.log('Close mini app')">✕</span>
    </div>
  </div>"#
        } else {
            ""
        };

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

        format!(
            r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>{title}</title>
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

  <div id="_dev_bar">
    <div id="_dev_status"></div>
    <span>⚡ SWC RUST ENGINE</span>
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
          }}, [safeProps.children]);

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

    async function renderRoute(entryStr, layoutFiles, params = {{}}, timestamp = null) {{
      const renderId = ++currentRenderId;
      window.__NATA_PARAMS__ = params;

      try {{
        const pageMod = await loadModule(entryStr, timestamp);
        const Page = pageMod.default || Object.values(pageMod).find(v => typeof v === 'function');

        if (!Page) {{
          throw new Error("No default export or React component found in " + entryStr);
        }}

        let RootComponent = makeComponent(Page);

        // Wrap layouts from inside-out (leaf layout to root layout)
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
              RootComponent = (props = {{}}) => React.createElement(WrappedLayout, {{ ...(props || {{}}), children: React.createElement(CurrentChild, props || {{}}) }});
            }}
          }} catch (layoutErr) {{
            console.warn("Could not wrap layout:", layoutFiles[i], layoutErr);
          }}
        }}

        if (renderId === currentRenderId) {{
          const rootEl = document.getElementById("root");
          const isSsr = rootEl && rootEl.getAttribute("data-nata-ssr") === "true";

          if (!currentRoot) {{
            if (isSsr && typeof ReactDOM.hydrateRoot === "function") {{
              try {{
                currentRoot = ReactDOM.hydrateRoot(rootEl, React.createElement(RootComponent, {{}}), {{
                  onRecoverableError(error) {{
                    console.warn("[NATA Hydration Info]", error);
                  }}
                }});
                rootEl.removeAttribute("data-nata-ssr");
              }} catch (hydrateErr) {{
                console.warn("[NATA SSR] Hydration mismatch/error, falling back to createRoot:", hydrateErr);
                rootEl.innerHTML = "";
                currentRoot = ReactDOM.createRoot(rootEl);
                currentRoot.render(React.createElement(RootComponent, {{}}));
              }}
            }} else {{
              currentRoot = ReactDOM.createRoot(rootEl);
              currentRoot.render(React.createElement(RootComponent, {{}}));
            }}
          }} else {{
            currentRoot.render(React.createElement(RootComponent, {{}}));
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
          window.location.href = targetUrl;
          return;
        }}

        await renderRoute(info.page_file, info.layout_files || [], info.params || {{}});
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

    // Diagnostic notice if CDN network is slow
    const mountDiagnosticTimer = setTimeout(() => {{
      const el = document.getElementById('_mount_status');
      if (el && document.getElementById('root')?.contains(el)) {{
        el.innerHTML = '<span style="color:#f59e0b;">Fetching remote packages from CDN...</span><br/><span style="font-size:11px;color:#94a3b8;">If this takes long, check your internet connection or browser Network tab.</span>';
      }}
    }}, 4000);

    // Initial mount
    renderRoute("{entry_str}", {layouts_json}).finally(() => {{
      clearTimeout(mountDiagnosticTimer);
    }});

    // HMR WebSocket with Smart CSS Hot Reload and Live Component Updates
    const wsProtocol = location.protocol === 'https:' ? 'wss:' : 'ws:';
    const ws = new WebSocket(wsProtocol + '//' + location.host + '/_hmr');
    ws.onmessage = async (e) => {{
      try {{
        const data = JSON.parse(e.data);
        const now = data.timestamp || Date.now();

        if (data.type === 'css') {{
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
            await renderRoute(info.page_file, info.layout_files || [], info.params || {{}}, now);
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
</html>"#
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
