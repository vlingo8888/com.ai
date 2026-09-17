use regex::Regex;
use std::path::{Component, Path, PathBuf};

use super::{tsconfig::TsConfig, types::ImportKind};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportBinding {
    pub local_name: String,
    pub remote_action: String,
}

pub struct ImportResolver;

impl ImportResolver {
    /// Classifies an import specifier string into its corresponding `ImportKind`
    pub fn classify(specifier: &str) -> ImportKind {
        Self::classify_with_config(specifier, &TsConfig::default())
    }

    /// Classifies an import specifier with custom TsConfig path aliases support
    pub fn classify_with_config(specifier: &str, tsconfig: &TsConfig) -> ImportKind {
        let trimmed = specifier.trim();

        if trimmed.starts_with("http://")
            || trimmed.starts_with("https://")
            || trimmed.starts_with("/_bundle/")
        {
            return ImportKind::Absolute(trimmed.to_string());
        }

        if trimmed.ends_with(".css") || trimmed.contains(".css?") {
            return ImportKind::Css(trimmed.to_string());
        }

        if trimmed.starts_with("modules/")
            || trimmed.starts_with("@/modules/")
            || trimmed.starts_with("~/modules/")
            || trimmed.starts_with("./modules/")
            || trimmed.starts_with("../modules/")
        {
            return ImportKind::ServerAction(trimmed.to_string());
        }

        if trimmed.starts_with("@/") || trimmed.starts_with("~/") {
            return ImportKind::PathAlias(trimmed.to_string());
        }

        if trimmed.starts_with("./") || trimmed.starts_with("../") {
            return ImportKind::Relative(trimmed.to_string());
        }

        if trimmed.starts_with("next/") {
            return ImportKind::Absolute(format!("/_nata/shims/{}", trimmed));
        }

        if trimmed == "core" {
            return ImportKind::Absolute("/_nata/shims/core".to_string());
        }

        // Check if custom tsconfig has an alias matching this specifier
        if tsconfig.resolve_path_alias(trimmed).is_some() {
            return ImportKind::PathAlias(trimmed.to_string());
        }

        ImportKind::NpmPackage(trimmed.to_string())
    }

    /// Resolves an import specifier to its browser-executable URL
    pub fn resolve_specifier(specifier: &str, parent_dir: &str) -> String {
        Self::resolve_specifier_with_config(specifier, parent_dir, &TsConfig::default())
    }

    /// Resolves an import specifier with custom TsConfig path aliases
    pub fn resolve_specifier_with_config(
        specifier: &str,
        parent_dir: &str,
        tsconfig: &TsConfig,
    ) -> String {
        match Self::classify_with_config(specifier, tsconfig) {
            ImportKind::Absolute(url) => url,
            ImportKind::Css(_) => "data:text/javascript,/* css import */".to_string(),
            ImportKind::ServerAction(path) => {
                let clean = path
                    .trim_start_matches("@/")
                    .trim_start_matches("~/");
                format!("/_bundle/{}", clean)
            }
            ImportKind::PathAlias(alias) => {
                if let Some(target) = tsconfig.resolve_primary_alias(&alias) {
                    let clean = target.trim_start_matches("./");
                    format!("/_bundle/{}", clean)
                } else if alias.starts_with("@/") || alias.starts_with("~/") {
                    let inner = &alias[2..];
                    format!("/_bundle/{}", inner)
                } else {
                    format!("/_bundle/{}", alias)
                }
            }
            ImportKind::Relative(rel) => {
                let normalized = Self::normalize_relative_path(parent_dir, &rel);
                format!("/_bundle/{}", normalized)
            }
            ImportKind::NpmPackage(pkg) => Self::resolve_npm_cdn(&pkg),
        }
    }

    /// Resolves a third-party npm package name to an ESM CDN URL or inline shim
    pub fn resolve_npm_cdn(pkg: &str) -> String {
        match pkg {
            "core" => "/_nata/shims/core".to_string(),
            "next/headers" => "/_nata/shims/next/headers".to_string(),
            "react" => "https://esm.sh/react@18.3.1".to_string(),
            "react/jsx-runtime" => "https://esm.sh/react@18.3.1/jsx-runtime".to_string(),
            "react-dom" => "https://esm.sh/react-dom@18.3.1?external=react".to_string(),
            "react-dom/client" => "https://esm.sh/react-dom@18.3.1/client?external=react".to_string(),
            "lucide-react" => "https://esm.sh/lucide-react@0.460.0?external=react,react-dom".to_string(),
            "framer-motion" => "https://esm.sh/framer-motion@11.11.17?external=react,react-dom".to_string(),
            "clsx" => "https://esm.sh/clsx@2.1.1".to_string(),
            "tailwind-merge" => "https://esm.sh/tailwind-merge@2.5.4".to_string(),
            "axios" => "https://esm.sh/axios@1.7.7".to_string(),
            "date-fns" => "https://esm.sh/date-fns@4.1.0".to_string(),
            "canvas-confetti" => "https://esm.sh/canvas-confetti@1.9.4".to_string(),
            "@tanstack/react-query" => "https://esm.sh/@tanstack/react-query@5.59.0?external=react,react-dom".to_string(),
            "@tanstack/react-table" => "https://esm.sh/@tanstack/react-table@8.20.5?external=react,react-dom".to_string(),
            "zustand" => "https://esm.sh/zustand@5.0.0?external=react,react-dom".to_string(),
            "jotai" => "https://esm.sh/jotai@2.10.1?external=react,react-dom".to_string(),
            "swr" => "https://esm.sh/swr@2.2.5?external=react,react-dom".to_string(),
            "zod" => "https://esm.sh/zod@3.23.8".to_string(),
            "react-hook-form" => "https://esm.sh/react-hook-form@7.53.0?external=react,react-dom".to_string(),
            "@hookform/resolvers" => "https://esm.sh/@hookform/resolvers@3.9.0?external=react,react-dom".to_string(),
            "@hookform/resolvers/zod" => "https://esm.sh/@hookform/resolvers@3.9.0/zod?external=react,react-dom,zod".to_string(),
            "sonner" => "https://esm.sh/sonner@1.5.0?external=react,react-dom".to_string(),
            "react-hot-toast" => "https://esm.sh/react-hot-toast@2.4.1?external=react,react-dom".to_string(),
            "next-themes" => "https://esm.sh/next-themes@0.3.0?external=react,react-dom".to_string(),
            "next/link" => "/_nata/shims/next/link".to_string(),
            "next/image" => "/_nata/shims/next/image".to_string(),
            "next/navigation" => "/_nata/shims/next/navigation".to_string(),
            "next/router" => "/_nata/shims/next/router".to_string(),
            "next/head" => "/_nata/shims/next/head".to_string(),
            _ => {
                if pkg.starts_with("next/") {
                    format!("/_nata/shims/{}", pkg)
                } else if pkg.contains('?') {
                    format!("https://esm.sh/{}&external=react,react-dom", pkg)
                } else {
                    format!("https://esm.sh/{}?external=react,react-dom", pkg)
                }
            }
        }
    }

    /// Computes clean canonical relative path without parent backtracking
    pub fn normalize_relative_path(parent_dir: &str, rel_target: &str) -> String {
        let combined = PathBuf::from(parent_dir).join(rel_target);
        let mut components = Vec::new();

        for comp in combined.components() {
            match comp {
                Component::Normal(c) => components.push(c.to_string_lossy().to_string()),
                Component::ParentDir => {
                    components.pop();
                }
                _ => {}
            }
        }

        components.join("/")
    }

    /// Checks if an import specifier corresponds to a backend server action module
    pub fn extract_server_action_module(
        specifier: &str,
        parent_dir: &str,
        tsconfig: &TsConfig,
    ) -> Option<String> {
        let trimmed = specifier.trim();

        if trimmed.starts_with("modules/") {
            return Some(trimmed.to_string());
        }
        if trimmed.starts_with("@/modules/") || trimmed.starts_with("~/modules/") {
            return Some(trimmed[2..].to_string());
        }
        if trimmed.starts_with("./") || trimmed.starts_with("../") {
            let normalized = Self::normalize_relative_path(parent_dir, trimmed);
            if normalized.starts_with("modules/") {
                return Some(normalized);
            }
        }
        if let Some(target) = tsconfig.resolve_primary_alias(trimmed) {
            let clean = target.trim_start_matches("./");
            if clean.starts_with("modules/") {
                return Some(clean.to_string());
            }
        }
        None
    }

    /// Parses an import clause into local and remote binding identifiers
    pub fn parse_import_bindings(clause: &str) -> Vec<ImportBinding> {
        let mut bindings = Vec::new();
        let trimmed = clause.trim();

        if trimmed.is_empty() || trimmed.starts_with("type ") {
            return bindings;
        }

        let remaining = trimmed;

        if let (Some(start_idx), Some(end_idx)) = (remaining.find('{'), remaining.rfind('}')) {
            let before_braces = remaining[..start_idx].trim().trim_end_matches(',').trim();
            let inside_braces = &remaining[start_idx + 1..end_idx];

            if !before_braces.is_empty() {
                for part in before_braces.split(',') {
                    let part = part.trim();
                    if !part.is_empty() && !part.starts_with("type ") {
                        if let Some(rest) = part.strip_prefix("* as ") {
                            bindings.push(ImportBinding {
                                local_name: rest.trim().to_string(),
                                remote_action: "default".to_string(),
                            });
                        } else {
                            bindings.push(ImportBinding {
                                local_name: part.to_string(),
                                remote_action: "default".to_string(),
                            });
                        }
                    }
                }
            }

            for item in inside_braces.split(',') {
                let item = item.trim();
                if item.is_empty() || item.starts_with("type ") {
                    continue;
                }

                if let Some((remote, local)) = item.split_once(" as ") {
                    let remote_clean = remote.trim();
                    let local_clean = local.trim();
                    if !remote_clean.is_empty() && !local_clean.is_empty() {
                        bindings.push(ImportBinding {
                            local_name: local_clean.to_string(),
                            remote_action: remote_clean.to_string(),
                        });
                    }
                } else {
                    bindings.push(ImportBinding {
                        local_name: item.to_string(),
                        remote_action: item.to_string(),
                    });
                }
            }
        } else {
            for part in remaining.split(',') {
                let part = part.trim();
                if part.is_empty() || part.starts_with("type ") {
                    continue;
                }
                if let Some(rest) = part.strip_prefix("* as ") {
                    bindings.push(ImportBinding {
                        local_name: rest.trim().to_string(),
                        remote_action: "default".to_string(),
                    });
                } else {
                    bindings.push(ImportBinding {
                        local_name: part.to_string(),
                        remote_action: "default".to_string(),
                    });
                }
            }
        }

        bindings
    }

    /// Generates modern async proxy caller for Server Actions with cookie synchronization
    pub fn generate_proxy_code(binding: &ImportBinding, module_path: &str) -> String {
        let local = &binding.local_name;
        let action = &binding.remote_action;

        format!(
            r#"
export const {local} = new Proxy(function() {{}}, {{
  get: (target, prop) => {{
    if (prop === "then" || prop === Symbol.toStringTag || typeof prop === "symbol") {{
      return undefined;
    }}
    return async (...args) => {{
      const cookieHeader = (() => {{
        try {{
          const map = new Map();
          if (typeof document !== "undefined" && document.cookie) {{
            for (const pair of document.cookie.split(";")) {{
              const idx = pair.indexOf("=");
              if (idx !== -1) {{
                const k = decodeURIComponent(pair.slice(0, idx).trim());
                const v = decodeURIComponent(pair.slice(idx + 1).trim());
                if (k) map.set(k, v);
              }}
            }}
          }}
          if (typeof localStorage !== "undefined") {{
            for (const key of ["token", "auth_token", "accessToken", "jwt", "session"]) {{
              const val = localStorage.getItem(key);
              if (val && !map.has(key)) map.set(key, val);
            }}
          }}
          return Array.from(map.entries()).map(([k, v]) => `${{encodeURIComponent(k)}}=${{encodeURIComponent(v)}}`).join("; ");
        }} catch {{ return ""; }}
      }})();

      const res = await fetch("/_nata/rpc", {{
        method: "POST",
        headers: {{
          "Content-Type": "application/json",
          ...(cookieHeader ? {{ "x-nata-cookies": cookieHeader }} : {{}})
        }},
        credentials: "same-origin",
        body: JSON.stringify({{
          module: "{module_path}",
          action: String(prop),
          args: args,
          cookies: cookieHeader || undefined
        }})
      }});

      const setCookiesHeader = res.headers.get("x-nata-set-cookies");
      if (setCookiesHeader && typeof document !== "undefined") {{
        try {{
          const list = JSON.parse(setCookiesHeader);
          if (Array.isArray(list)) {{
            for (const c of list) {{
              if (!c || !c.name) continue;
              if (c.deleted || c.maxAge === 0) {{
                document.cookie = `${{encodeURIComponent(c.name)}}=; Path=/; Max-Age=0; Expires=Thu, 01 Jan 1970 00:00:00 GMT`;
                if (typeof localStorage !== "undefined") {{
                  try {{ localStorage.removeItem(c.name); }} catch {{}}
                }}
              }} else {{
                let cookieStr = `${{encodeURIComponent(c.name)}}=${{encodeURIComponent(c.value || "")}}`;
                cookieStr += `; Path=${{c.path || "/"}}`;
                if (c.maxAge !== undefined && c.maxAge !== null) cookieStr += `; Max-Age=${{c.maxAge}}`;
                if (c.expires) {{
                  const expDate = new Date(c.expires);
                  if (!isNaN(expDate.getTime())) {{
                    cookieStr += `; Expires=${{expDate.toUTCString()}}`;
                  }}
                }}
                if (c.domain) cookieStr += `; Domain=${{c.domain}}`;
                if (c.secure && typeof location !== "undefined" && location.protocol === "https:") cookieStr += "; Secure";
                if (c.sameSite) cookieStr += `; SameSite=${{c.sameSite}}`;
                else cookieStr += "; SameSite=Lax";
                document.cookie = cookieStr;
                if (typeof localStorage !== "undefined" && c.value) {{
                  try {{ localStorage.setItem(c.name, c.value); }} catch {{}}
                }}
              }}
            }}
          }}
        }} catch {{}}
      }}

      if (!res.ok) {{
        const err = await res.json().catch(() => ({{ error: res.statusText }}));
        throw new Error(err.error || "RPC Call failed: " + res.statusText);
      }}
      const jsonRes = await res.json();
      if (jsonRes && typeof jsonRes === "object") {{
        const tok = jsonRes.token || jsonRes.accessToken || jsonRes.jwt;
        if (typeof tok === "string" && tok) {{
          try {{
            if (typeof localStorage !== "undefined") localStorage.setItem("token", tok);
            if (typeof document !== "undefined") {{
              document.cookie = `token=${{encodeURIComponent(tok)}}; Path=/; Max-Age=604800; SameSite=Lax`;
            }}
          }} catch {{}}
        }}
      }}
      return jsonRes;
    }};
  }},
  apply: (target, thisArg, args) => {{
    return (async () => {{
      const cookieHeader = (() => {{
        try {{
          const map = new Map();
          if (typeof document !== "undefined" && document.cookie) {{
            for (const pair of document.cookie.split(";")) {{
              const idx = pair.indexOf("=");
              if (idx !== -1) {{
                const k = decodeURIComponent(pair.slice(0, idx).trim());
                const v = decodeURIComponent(pair.slice(idx + 1).trim());
                if (k) map.set(k, v);
              }}
            }}
          }}
          if (typeof localStorage !== "undefined") {{
            for (const key of ["token", "auth_token", "accessToken", "jwt", "session"]) {{
              const val = localStorage.getItem(key);
              if (val && !map.has(key)) map.set(key, val);
            }}
          }}
          return Array.from(map.entries()).map(([k, v]) => `${{encodeURIComponent(k)}}=${{encodeURIComponent(v)}}`).join("; ");
        }} catch {{ return ""; }}
      }})();

      const res = await fetch("/_nata/rpc", {{
        method: "POST",
        headers: {{
          "Content-Type": "application/json",
          ...(cookieHeader ? {{ "x-nata-cookies": cookieHeader }} : {{}})
        }},
        credentials: "same-origin",
        body: JSON.stringify({{
          module: "{module_path}",
          action: "{action}",
          args: args,
          cookies: cookieHeader || undefined
        }})
      }});

      const setCookiesHeader = res.headers.get("x-nata-set-cookies");
      if (setCookiesHeader && typeof document !== "undefined") {{
        try {{
          const list = JSON.parse(setCookiesHeader);
          if (Array.isArray(list)) {{
            for (const c of list) {{
              if (!c || !c.name) continue;
              if (c.deleted || c.maxAge === 0) {{
                document.cookie = `${{encodeURIComponent(c.name)}}=; Path=/; Max-Age=0; Expires=Thu, 01 Jan 1970 00:00:00 GMT`;
                if (typeof localStorage !== "undefined") {{
                  try {{ localStorage.removeItem(c.name); }} catch {{}}
                }}
              }} else {{
                let cookieStr = `${{encodeURIComponent(c.name)}}=${{encodeURIComponent(c.value || "")}}`;
                cookieStr += `; Path=${{c.path || "/"}}`;
                if (c.maxAge !== undefined && c.maxAge !== null) cookieStr += `; Max-Age=${{c.maxAge}}`;
                if (c.expires) {{
                  const expDate = new Date(c.expires);
                  if (!isNaN(expDate.getTime())) {{
                    cookieStr += `; Expires=${{expDate.toUTCString()}}`;
                  }}
                }}
                if (c.domain) cookieStr += `; Domain=${{c.domain}}`;
                if (c.secure && typeof location !== "undefined" && location.protocol === "https:") cookieStr += "; Secure";
                if (c.sameSite) cookieStr += `; SameSite=${{c.sameSite}}`;
                else cookieStr += "; SameSite=Lax";
                document.cookie = cookieStr;
                if (typeof localStorage !== "undefined" && c.value) {{
                  try {{ localStorage.setItem(c.name, c.value); }} catch {{}}
                }}
              }}
            }}
          }}
        }} catch {{}}
      }}

      if (!res.ok) {{
        const err = await res.json().catch(() => ({{ error: res.statusText }}));
        throw new Error(err.error || "RPC Call failed: " + res.statusText);
      }}
      const jsonRes = await res.json();
      if (jsonRes && typeof jsonRes === "object") {{
        const tok = jsonRes.token || jsonRes.accessToken || jsonRes.jwt;
        if (typeof tok === "string" && tok) {{
          try {{
            if (typeof localStorage !== "undefined") localStorage.setItem("token", tok);
            if (typeof document !== "undefined") {{
              document.cookie = `token=${{encodeURIComponent(tok)}}; Path=/; Max-Age=604800; SameSite=Lax`;
            }}
          }} catch {{}}
        }}
      }}
      return jsonRes;
    }})();
  }}
}});
"#
        )
    }

    /// Transforms Next.js Font imports (next/font/google, next/font/local) into lightweight mock font declarations
    pub fn transform_next_fonts(code: &str) -> String {
        let font_import_regex = Regex::new(
            r#"(?s)\bimport\s+(?:type\s+)?([^;'"]*?)\s+from\s*['"]next/font/(?:google|local)['"]\s*;?"#,
        )
        .unwrap();

        font_import_regex
            .replace_all(code, |caps: &regex::Captures| {
                let clause = caps.get(1).unwrap().as_str();
                let bindings = Self::parse_import_bindings(clause);

                if bindings.is_empty() {
                    return "/* next/font import */".to_string();
                }

                let mut declarations = String::new();
                for binding in bindings {
                    let name = &binding.local_name;
                    let css_name = name.to_lowercase().replace('_', "-");
                    let font_family = name.replace('_', " ");

                    declarations.push_str(&format!(
                        r#"
export const {name} = (opts = {{}}) => ({{
  className: "font-{css_name}",
  variable: (opts && opts.variable) ? opts.variable : "--font-{css_name}",
  style: {{ fontFamily: "{font_family}, -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif" }}
}});
"#
                    ));
                }
                declarations
            })
            .to_string()
    }

    /// Transforms all TypeScript/TSX imports in source code into browser-compatible ESM imports
    pub fn transform_source(code: &str, file_rel_path: &str) -> String {
        Self::transform_source_with_config(code, file_rel_path, &TsConfig::default())
    }

    /// Transforms source code using custom TsConfig path aliases
    pub fn transform_source_with_config(
        code: &str,
        file_rel_path: &str,
        tsconfig: &TsConfig,
    ) -> String {
        // Step 1: Replace Server Actions with RPC proxy callers
        let mut transformed = Self::transform_server_actions_with_config(code, file_rel_path, tsconfig);

        // Step 2: Transform next/font (google and local) into mock font functions
        transformed = Self::transform_next_fonts(&transformed);

        // Step 3: Comment out direct CSS imports
        let css_import_regex = Regex::new(
            r#"(?m)^\s*import\s+['"][^'"]+\.css(?:\?[^'"]*)?['"];?"#,
        )
        .unwrap();
        transformed = css_import_regex.replace_all(&transformed, "/* css import */").to_string();

        // Step 4: Match and rewrite static imports and exports: `import ... from "..."` and `export ... from "..."`
        let import_export_regex = Regex::new(
            r#"(?s)\b((?:import|export)\s+(?:type\s+)?[^;'"]*?\s+from\s*['"])([^'"]+)(['"])"#,
        )
        .unwrap();

        let parent_dir = Path::new(file_rel_path)
            .parent()
            .unwrap_or(Path::new(""))
            .to_string_lossy()
            .replace('\\', "/");

        transformed = import_export_regex
            .replace_all(&transformed, |caps: &regex::Captures| {
                let prefix = caps.get(1).unwrap().as_str();
                let specifier = caps.get(2).unwrap().as_str();
                let suffix = caps.get(3).unwrap().as_str();

                let resolved = Self::resolve_specifier_with_config(specifier, &parent_dir, tsconfig);
                format!("{}{}{}", prefix, resolved, suffix)
            })
            .to_string();

        // Step 5: Rewrite dynamic imports: `import("...")`
        let dynamic_import_regex = Regex::new(
            r#"import\s*\(\s*['"]([^'"]+)['"]\s*\)"#,
        )
        .unwrap();

        transformed = dynamic_import_regex
            .replace_all(&transformed, |caps: &regex::Captures| {
                let specifier = caps.get(1).unwrap().as_str();
                let resolved = Self::resolve_specifier_with_config(specifier, &parent_dir, tsconfig);
                format!("import(\"{}\")", resolved)
            })
            .to_string();

        transformed
    }

    /// Transforms backend server action imports into typed async RPC client proxy calls
    pub fn transform_server_actions(code: &str) -> String {
        Self::transform_server_actions_with_config(code, "", &TsConfig::default())
    }

    /// Transforms backend server action imports with context path and tsconfig
    pub fn transform_server_actions_with_config(
        code: &str,
        file_rel_path: &str,
        tsconfig: &TsConfig,
    ) -> String {
        let parent_dir = Path::new(file_rel_path)
            .parent()
            .unwrap_or(Path::new(""))
            .to_string_lossy()
            .replace('\\', "/");

        let import_regex = Regex::new(
            r#"(?s)\bimport\s+(?:type\s+)?([^;'"]*?)\s+from\s*['"]([^'"]+)['"]\s*;?"#,
        )
        .unwrap();

        let mut transformed = import_regex
            .replace_all(code, |caps: &regex::Captures| {
                let full_match = caps.get(0).unwrap().as_str();
                let clause = caps.get(1).unwrap().as_str();
                let specifier = caps.get(2).unwrap().as_str();

                if let Some(module_path) =
                    Self::extract_server_action_module(specifier, &parent_dir, tsconfig)
                {
                    let bindings = Self::parse_import_bindings(clause);
                    if bindings.is_empty() {
                        return "/* server action type import */".to_string();
                    }

                    let mut proxy_code = String::new();
                    for binding in &bindings {
                        proxy_code.push_str(&Self::generate_proxy_code(binding, &module_path));
                    }
                    proxy_code
                } else {
                    full_match.to_string()
                }
            })
            .to_string();

        // Also handle side-effect server action imports: `import "modules/...";`
        let side_effect_regex = Regex::new(
            r#"(?m)^\s*import\s+['"]([^'"]+)['"]\s*;?"#,
        )
        .unwrap();

        transformed = side_effect_regex
            .replace_all(&transformed, |caps: &regex::Captures| {
                let full_match = caps.get(0).unwrap().as_str();
                let specifier = caps.get(1).unwrap().as_str();
                if Self::extract_server_action_module(specifier, &parent_dir, tsconfig).is_some() {
                    "/* server action import */".to_string()
                } else {
                    full_match.to_string()
                }
            })
            .to_string();

        transformed
    }
}

