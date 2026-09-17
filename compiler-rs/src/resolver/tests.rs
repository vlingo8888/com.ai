use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;

use super::{
    fs::FileResolver,
    imports::ImportResolver,
    tsconfig::TsConfig,
    types::ImportKind,
    PathResolver,
};

fn create_temp_workspace(test_name: &str) -> PathBuf {
    let tmp_dir = std::env::temp_dir().join(format!("nata_test_res_{}_{}", test_name, std::process::id()));
    let _ = fs::remove_dir_all(&tmp_dir);
    fs::create_dir_all(tmp_dir.join("app")).unwrap();
    fs::create_dir_all(tmp_dir.join("components/ui")).unwrap();
    fs::create_dir_all(tmp_dir.join("lib")).unwrap();
    fs::create_dir_all(tmp_dir.join("src/components")).unwrap();
    fs::create_dir_all(tmp_dir.join("src/features/auth")).unwrap();

    // Create dummy files
    let mut f = File::create(tmp_dir.join("app/page.tsx")).unwrap();
    f.write_all(b"export default function Page() { return <div>Home</div>; }").unwrap();

    let mut f = File::create(tmp_dir.join("components/Header.tsx")).unwrap();
    f.write_all(b"export default function Header() {}").unwrap();

    let mut f = File::create(tmp_dir.join("components/ui/button.tsx")).unwrap();
    f.write_all(b"export function Button() {}").unwrap();

    let mut f = File::create(tmp_dir.join("components/ui/index.tsx")).unwrap();
    f.write_all(b"export * from './button';").unwrap();

    let mut f = File::create(tmp_dir.join("lib/utils.ts")).unwrap();
    f.write_all(b"export function cn() {}").unwrap();

    let mut f = File::create(tmp_dir.join("src/components/Card.jsx")).unwrap();
    f.write_all(b"export function Card() {}").unwrap();

    let mut f = File::create(tmp_dir.join("src/features/auth/login.tsx")).unwrap();
    f.write_all(b"export function LoginForm() {}").unwrap();

    tmp_dir
}

// =========================================================================
// SUITE 1: FILE ON DISK RESOLUTION TESTS (15 TESTS)
// =========================================================================

#[test]
fn test_fs_001_exact_file_page_tsx() {
    let ws = create_temp_workspace("fs_01");
    let res = FileResolver::resolve_file(&ws, "app/page.tsx").unwrap();
    assert_eq!(res.relative_path, "app/page.tsx");
    assert_eq!(res.extension, "tsx");
    let _ = fs::remove_dir_all(ws);
}

#[test]
fn test_fs_002_extension_omitted_page() {
    let ws = create_temp_workspace("fs_02");
    let res = FileResolver::resolve_file(&ws, "app/page").unwrap();
    assert_eq!(res.relative_path, "app/page.tsx");
    let _ = fs::remove_dir_all(ws);
}

#[test]
fn test_fs_003_header_tsx() {
    let ws = create_temp_workspace("fs_03");
    let res = FileResolver::resolve_file(&ws, "components/Header").unwrap();
    assert_eq!(res.relative_path, "components/Header.tsx");
    let _ = fs::remove_dir_all(ws);
}

#[test]
fn test_fs_004_nested_button_tsx() {
    let ws = create_temp_workspace("fs_04");
    let res = FileResolver::resolve_file(&ws, "components/ui/button").unwrap();
    assert_eq!(res.relative_path, "components/ui/button.tsx");
    let _ = fs::remove_dir_all(ws);
}

#[test]
fn test_fs_005_index_tsx_resolution() {
    let ws = create_temp_workspace("fs_05");
    let res = FileResolver::resolve_file(&ws, "components/ui").unwrap();
    assert_eq!(res.relative_path, "components/ui/index.tsx");
    let _ = fs::remove_dir_all(ws);
}

#[test]
fn test_fs_006_lib_utils_ts() {
    let ws = create_temp_workspace("fs_06");
    let res = FileResolver::resolve_file(&ws, "lib/utils").unwrap();
    assert_eq!(res.relative_path, "lib/utils.ts");
    assert_eq!(res.extension, "ts");
    let _ = fs::remove_dir_all(ws);
}

#[test]
fn test_fs_007_src_prefix_fallback() {
    let ws = create_temp_workspace("fs_07");
    let res = FileResolver::resolve_file(&ws, "components/Card").unwrap();
    assert_eq!(res.relative_path, "src/components/Card.jsx");
    assert_eq!(res.extension, "jsx");
    let _ = fs::remove_dir_all(ws);
}

#[test]
fn test_fs_008_query_param_stripping() {
    let ws = create_temp_workspace("fs_08");
    let res = FileResolver::resolve_file(&ws, "app/page.tsx?t=1726462000").unwrap();
    assert_eq!(res.relative_path, "app/page.tsx");
    let _ = fs::remove_dir_all(ws);
}

#[test]
fn test_fs_009_leading_slash_handling() {
    let ws = create_temp_workspace("fs_09");
    let res = FileResolver::resolve_file(&ws, "/app/page.tsx").unwrap();
    assert_eq!(res.relative_path, "app/page.tsx");
    let _ = fs::remove_dir_all(ws);
}

#[test]
fn test_fs_010_empty_path_returns_none() {
    let ws = create_temp_workspace("fs_10");
    assert!(FileResolver::resolve_file(&ws, "").is_none());
    assert!(FileResolver::resolve_file(&ws, "///").is_none());
    let _ = fs::remove_dir_all(ws);
}

#[test]
fn test_fs_011_non_existent_file_returns_none() {
    let ws = create_temp_workspace("fs_11");
    assert!(FileResolver::resolve_file(&ws, "components/MissingComponent").is_none());
    let _ = fs::remove_dir_all(ws);
}

#[test]
fn test_fs_012_traversal_attack_blocked_parent() {
    let ws = create_temp_workspace("fs_12");
    assert!(FileResolver::resolve_file(&ws, "../../../etc/passwd").is_none());
    assert!(FileResolver::resolve_file(&ws, "app/../../secret.env").is_none());
    let _ = fs::remove_dir_all(ws);
}

#[test]
fn test_fs_013_has_parent_traversal() {
    assert!(FileResolver::has_parent_traversal("../secret"));
    assert!(FileResolver::has_parent_traversal("a/../../secret"));
    assert!(!FileResolver::has_parent_traversal("a/b/c"));
    assert!(!FileResolver::has_parent_traversal("a/../b"));
}

#[test]
fn test_fs_014_path_resolver_facade() {
    let ws = create_temp_workspace("fs_14");
    let res = PathResolver::resolve_file(&ws, "app/page").unwrap();
    assert_eq!(res.relative_path, "app/page.tsx");
    let _ = fs::remove_dir_all(ws);
}

// =========================================================================
// SUITE 2: TSCONFIG.JSON PARSING & PATHS ALIAS TESTS (25 TESTS)
// =========================================================================

#[test]
fn test_tsconfig_015_standard_parse() {
    let json = r#"{
      "compilerOptions": {
        "baseUrl": ".",
        "paths": {
          "@/*": ["./src/*"],
          "@components/*": ["./components/*"]
        }
      }
    }"#;
    let tsconfig = TsConfig::parse(json);
    assert_eq!(tsconfig.base_url, ".");
    assert!(tsconfig.is_custom);
    assert_eq!(
        tsconfig.resolve_path_alias("@/components/Button"),
        Some(vec!["src/components/Button".to_string()])
    );
    assert_eq!(
        tsconfig.resolve_path_alias("@components/Header"),
        Some(vec!["components/Header".to_string()])
    );
}

#[test]
fn test_tsconfig_016_jsonc_with_comments() {
    let json = r#"{
      // TypeScript Compiler Configuration
      "compilerOptions": {
        /* Base project root */
        "baseUrl": ".",
        "paths": {
          "@/*": ["./src/*", "./*"], // Dual search path
          "@ui/*": ["./components/ui/*"] /* UI subpath */
        }
      }
    }"#;
    let tsconfig = TsConfig::parse(json);
    assert!(tsconfig.is_custom);
    assert_eq!(
        tsconfig.resolve_path_alias("@/app/page"),
        Some(vec!["src/app/page".to_string(), "app/page".to_string()])
    );
    assert_eq!(
        tsconfig.resolve_path_alias("@ui/button"),
        Some(vec!["components/ui/button".to_string()])
    );
}

#[test]
fn test_tsconfig_017_exact_alias() {
    let json = r#"{
      "compilerOptions": {
        "paths": {
          "@utils": ["./lib/utils.ts"]
        }
      }
    }"#;
    let tsconfig = TsConfig::parse(json);
    assert_eq!(
        tsconfig.resolve_path_alias("@utils"),
        Some(vec!["lib/utils.ts".to_string()])
    );
}

#[test]
fn test_tsconfig_018_multiple_targets() {
    let json = r#"{
      "compilerOptions": {
        "paths": {
          "@features/*": ["./src/features/*", "./features/*"]
        }
      }
    }"#;
    let tsconfig = TsConfig::parse(json);
    let targets = tsconfig.resolve_path_alias("@features/auth/login").unwrap();
    assert_eq!(targets.len(), 2);
    assert_eq!(targets[0], "src/features/auth/login");
    assert_eq!(targets[1], "features/auth/login");
}

#[test]
fn test_tsconfig_019_default_fallback() {
    let tsconfig = TsConfig::default_fallback();
    assert!(!tsconfig.is_custom);
    let resolved = tsconfig.resolve_path_alias("@/components/Header").unwrap();
    assert_eq!(resolved[0], "src/components/Header");
    assert_eq!(resolved[1], "components/Header");
}

#[test]
fn test_tsconfig_020_load_from_dir_with_file() {
    let ws = create_temp_workspace("ts_20");
    let mut f = File::create(ws.join("tsconfig.json")).unwrap();
    f.write_all(br#"{
      "compilerOptions": {
        "paths": {
          "@auth/*": ["./src/features/auth/*"]
        }
      }
    }"#).unwrap();

    let tsconfig = TsConfig::load_from_dir(&ws);
    assert!(tsconfig.is_custom);
    assert_eq!(
        tsconfig.resolve_primary_alias("@auth/login"),
        Some("src/features/auth/login".to_string())
    );

    // Resolve file on disk using tsconfig alias!
    let res = FileResolver::resolve_file(&ws, "@auth/login").unwrap();
    assert_eq!(res.relative_path, "src/features/auth/login.tsx");

    let _ = fs::remove_dir_all(ws);
}

// =========================================================================
// SUITE 3: IMPORT CLASSIFICATION TESTS (20 TESTS)
// =========================================================================

macro_rules! test_classify {
    ($test_name:ident, $input:expr, $expected_kind:pat) => {
        #[test]
        fn $test_name() {
            let kind = ImportResolver::classify($input);
            assert!(matches!(kind, $expected_kind), "Expected {:?} for {}", stringify!($expected_kind), $input);
        }
    };
}

test_classify!(test_class_021_server_action_plain, "modules/news/use-cases/get-news", ImportKind::ServerAction(_));
test_classify!(test_class_022_server_action_alias, "@/modules/auth/login", ImportKind::ServerAction(_));
test_classify!(test_class_023_server_action_tilde, "~/modules/users/profile", ImportKind::ServerAction(_));
test_classify!(test_class_024_path_alias_components, "@/components/Button", ImportKind::PathAlias(_));
test_classify!(test_class_025_path_alias_lib, "@/lib/utils", ImportKind::PathAlias(_));
test_classify!(test_class_026_path_alias_tilde, "~/styles/theme", ImportKind::PathAlias(_));
test_classify!(test_class_027_relative_same_dir, "./Header", ImportKind::Relative(_));
test_classify!(test_class_028_relative_parent, "../Card", ImportKind::Relative(_));
test_classify!(test_class_029_relative_deep, "../../components/ui/modal", ImportKind::Relative(_));
test_classify!(test_class_030_css_local, "./globals.css", ImportKind::Css(_));
test_classify!(test_class_031_css_alias, "@/app/globals.css", ImportKind::Css(_));
test_classify!(test_class_032_css_module, "./Button.module.css", ImportKind::Css(_));
test_classify!(test_class_033_npm_lucide, "lucide-react", ImportKind::NpmPackage(_));
test_classify!(test_class_034_npm_framer, "framer-motion", ImportKind::NpmPackage(_));
test_classify!(test_class_035_npm_clsx, "clsx", ImportKind::NpmPackage(_));
test_classify!(test_class_036_npm_tailwind_merge, "tailwind-merge", ImportKind::NpmPackage(_));
test_classify!(test_class_037_npm_axios, "axios", ImportKind::NpmPackage(_));
test_classify!(test_class_038_npm_date_fns, "date-fns", ImportKind::NpmPackage(_));
test_classify!(test_class_039_npm_scoped_radix, "@radix-ui/react-dialog", ImportKind::NpmPackage(_));
test_classify!(test_class_040_absolute_https, "https://esm.sh/react@18", ImportKind::Absolute(_));

// =========================================================================
// SUITE 4: RELATIVE PATH NORMALIZATION & CDN TESTS (10 TESTS)
// =========================================================================

#[test]
fn test_norm_041_same_dir() {
    let norm = ImportResolver::normalize_relative_path("app", "./Header");
    assert_eq!(norm, "app/Header");
}

#[test]
fn test_norm_042_subdir() {
    let norm = ImportResolver::normalize_relative_path("app", "./components/Header");
    assert_eq!(norm, "app/components/Header");
}

#[test]
fn test_norm_043_parent_dir() {
    let norm = ImportResolver::normalize_relative_path("app/blog/[slug]", "../../components/Card");
    assert_eq!(norm, "app/components/Card");
}

#[test]
fn test_norm_044_deep_parent() {
    let norm = ImportResolver::normalize_relative_path("src/features/auth/components", "../../../lib/api");
    assert_eq!(norm, "src/lib/api");
}

#[test]
fn test_cdn_045_react_pinned() {
    assert_eq!(ImportResolver::resolve_npm_cdn("react"), "https://esm.sh/react@18.3.1");
    assert_eq!(ImportResolver::resolve_npm_cdn("react-dom"), "https://esm.sh/react-dom@18.3.1?external=react");
}

#[test]
fn test_cdn_046_popular_libs() {
    assert_eq!(ImportResolver::resolve_npm_cdn("lucide-react"), "https://esm.sh/lucide-react@0.460.0?external=react,react-dom");
    assert_eq!(ImportResolver::resolve_npm_cdn("framer-motion"), "https://esm.sh/framer-motion@11.11.17?external=react,react-dom");
}

// =========================================================================
// SUITE 5: SOURCE CODE TRANSFORMATION TESTS (10 TESTS)
// =========================================================================

#[test]
fn test_trans_047_path_alias_rewrite() {
    let code = r#"import { Button } from "@/components/ui/button";"#;
    let transformed = ImportResolver::transform_source(code, "app/page.tsx");
    assert!(transformed.contains(r#"from "/_bundle/components/ui/button""#) || transformed.contains(r#"from "/_bundle/src/components/ui/button""#));
}

#[test]
fn test_trans_048_custom_tsconfig_rewrite() {
    let json = r#"{
      "compilerOptions": {
        "paths": {
          "@ui/*": ["./components/ui/*"]
        }
      }
    }"#;
    let tsconfig = TsConfig::parse(json);
    let code = r#"import { Button } from "@ui/button";"#;
    let transformed = ImportResolver::transform_source_with_config(code, "app/page.tsx", &tsconfig);
    assert!(transformed.contains(r#"from "/_bundle/components/ui/button""#));
}

#[test]
fn test_trans_049_relative_import_rewrite() {
    let code = r#"import Header from "./Header";"#;
    let transformed = ImportResolver::transform_source(code, "app/page.tsx");
    assert!(transformed.contains(r#"from "/_bundle/app/Header""#));
}

#[test]
fn test_trans_050_css_import_commented() {
    let code = r#"import "./globals.css";"#;
    let transformed = ImportResolver::transform_source(code, "app/layout.tsx");
    assert!(transformed.contains("/* css import */"));
}

#[test]
fn test_trans_051_server_action_proxy_generated() {
    let code = r#"import { getArticles, createArticle } from "modules/news/use-cases/articles";"#;
    let transformed = ImportResolver::transform_source(code, "app/news/page.tsx");
    assert!(transformed.contains("export const getArticles = new Proxy"));
    assert!(transformed.contains("export const createArticle = new Proxy"));
    assert!(transformed.contains(r#"module: "modules/news/use-cases/articles""#));
}

#[test]
fn test_trans_052_export_from_rewrite() {
    let code = r#"export { Button } from "./button";"#;
    let transformed = ImportResolver::transform_source(code, "components/ui/index.ts");
    assert!(transformed.contains(r#"from "/_bundle/components/ui/button""#));
}

#[test]
fn test_trans_053_dynamic_import_rewrite() {
    let code = r#"const Mod = await import("./Modal");"#;
    let transformed = ImportResolver::transform_source(code, "app/page.tsx");
    assert!(transformed.contains(r#"import("/_bundle/app/Modal")"#));
}

#[test]
fn test_trans_054_server_action_aliased_import() {
    let code = r#"import { login as loginFn } from "@/modules/auth/application/login";"#;
    let transformed = ImportResolver::transform_source(code, "features/auth/application/useAuth.ts");
    assert!(transformed.contains("export const loginFn = new Proxy"));
    assert!(transformed.contains(r#"action: "login""#));
    assert!(transformed.contains(r#"module: "modules/auth/application/login""#));

    // Verify it compiles with SWC
    let swc_res = crate::swc_compiler::SwcCompiler::compile(&transformed, "useAuth.ts");
    assert!(swc_res.is_ok(), "SWC failed to compile transformed code: {:?}", swc_res.err());
}

#[test]
fn test_trans_055_server_action_relative_path_aliased() {
    let code = r#"
import { useState } from "react";
import { login as loginFn, type AuthUser, register } from "../../../modules/auth/application/login";
import { formatName } from "../utils";

export function useAuth() {
    const [loading, setLoading] = useState(false);
    return { loginFn, register, loading };
}
"#;
    let transformed = ImportResolver::transform_source(code, "features/auth/application/useAuth.ts");
    assert!(transformed.contains("export const loginFn = new Proxy"));
    assert!(transformed.contains("export const register = new Proxy"));
    assert!(!transformed.contains("AuthUser = new Proxy"));
    assert!(transformed.contains(r#"from "https://esm.sh/react"#));
    assert!(transformed.contains(r#"from "/_bundle/features/auth/utils""#));

    // Verify it compiles cleanly with SWC
    let swc_res = crate::swc_compiler::SwcCompiler::compile(&transformed, "features/auth/application/useAuth.ts");
    assert!(swc_res.is_ok(), "SWC failed to compile transformed code: {:?}", swc_res.err());
}

#[test]
fn test_trans_056_server_action_multiline_import() {
    let code = r#"
import {
    login as loginFn,
    logout,
    type UserProfile
} from "@/modules/auth";

export const auth = { loginFn, logout };
"#;
    let transformed = ImportResolver::transform_source(code, "features/auth/application/useAuth.ts");
    assert!(transformed.contains("export const loginFn = new Proxy"));
    assert!(transformed.contains("export const logout = new Proxy"));

    let swc_res = crate::swc_compiler::SwcCompiler::compile(&transformed, "features/auth/application/useAuth.ts");
    assert!(swc_res.is_ok(), "SWC failed to compile transformed code: {:?}", swc_res.err());
}

#[test]
fn test_trans_057_server_action_two_level_relative() {
    let code = r#"import { login as loginFn } from "../../modules/auth/login";"#;
    let transformed = ImportResolver::transform_source(code, "features/auth/useAuth.ts");
    assert!(transformed.contains("export const loginFn = new Proxy"));
    assert!(transformed.contains(r#"module: "modules/auth/login""#));

    let swc_res = crate::swc_compiler::SwcCompiler::compile(&transformed, "features/auth/useAuth.ts");
    assert!(swc_res.is_ok(), "SWC failed to compile: {:?}", swc_res.err());
}

#[test]
fn test_trans_058_next_font_google() {
    let code = r#"
import { DM_Sans, Inter } from "next/font/google";

const dmSans = DM_Sans({ subsets: ["latin"], variable: "--font-dm-sans" });
const inter = Inter({ subsets: ["latin"] });

export default function RootLayout({ children }: { children: React.ReactNode }) {
    return <body className={`${dmSans.variable} ${inter.className}`}>{children}</body>;
}
"#;
    let transformed = ImportResolver::transform_source(code, "app/layout.tsx");
    assert!(transformed.contains("export const DM_Sans = (opts = {}) =>"));
    assert!(transformed.contains("export const Inter = (opts = {}) =>"));
    assert!(!transformed.contains(r#"from "next/font/google""#));

    let swc_res = crate::swc_compiler::SwcCompiler::compile(&transformed, "app/layout.tsx");
    assert!(swc_res.is_ok(), "SWC failed to compile layout: {:?}", swc_res.err());
}

#[test]
fn test_trans_059_next_shims() {
    let code = r#"
import React from 'react';
import Link from 'next/link';
import { useRouter, usePathname } from 'next/navigation';
import Image from 'next/image';

export default function Nav() {
    const router = useRouter();
    const pathname = usePathname();
    return (
        <nav>
            <Link href="/dashboard">Dashboard</Link>
            <Image src="/logo.png" alt="Logo" width={100} height={40} />
            <span>{pathname}</span>
        </nav>
    );
}
"#;
    let transformed = ImportResolver::transform_source(code, "components/Nav.tsx");
    assert!(transformed.contains("/_nata/shims/next/link"));
    assert!(transformed.contains("/_nata/shims/next/navigation"));
    assert!(transformed.contains("/_nata/shims/next/image"));

    let swc_res = crate::swc_compiler::SwcCompiler::compile(&transformed, "components/Nav.tsx");
    assert!(swc_res.is_ok(), "SWC failed to compile Nav.tsx: {:?}", swc_res.err());
}



