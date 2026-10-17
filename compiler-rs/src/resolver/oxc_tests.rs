use std::fs;
use std::path::{Path, PathBuf};

use super::oxc::{self, is_builtin, package_name};
use super::Layer;

/// A throwaway project: `files` are `(relative path, contents)`
pub(crate) fn workspace(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("nata_oxc_{}_{}", name, std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    for (rel, contents) in files {
        let path = dir.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }
    oxc::invalidate(&dir);
    dir.canonicalize().unwrap()
}

fn rel(root: &Path, p: &Path) -> String {
    p.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/")
}

const NESTED_EXPORTS: &str = r##"{
  "name": "lib",
  "exports": {
    ".": {
      "react-server": "./server.js",
      "browser": { "import": "./browser.mjs", "default": "./browser.cjs" },
      "node": { "import": "./node.mjs", "require": "./node.cjs" },
      "default": "./index.js"
    },
    "./feature/*": { "import": "./dist/feature/*.mjs", "default": "./dist/feature/*.js" },
    "./package.json": "./package.json"
  },
  "imports": { "#internal": "./internal.js" }
}"##;

fn lib_workspace(name: &str) -> PathBuf {
    workspace(
        name,
        &[
            ("app/page.tsx", "export default function Page() {}"),
            ("node_modules/lib/package.json", NESTED_EXPORTS),
            ("node_modules/lib/server.js", ""),
            ("node_modules/lib/browser.mjs", ""),
            ("node_modules/lib/browser.cjs", ""),
            ("node_modules/lib/node.mjs", ""),
            ("node_modules/lib/node.cjs", ""),
            ("node_modules/lib/index.js", "require('#internal')"),
            ("node_modules/lib/internal.js", ""),
            ("node_modules/lib/dist/feature/a.mjs", ""),
            ("node_modules/lib/dist/feature/a.js", ""),
        ],
    )
}

#[test]
fn oxc_001_nested_exports_pick_the_condition_of_each_layer() {
    let root = lib_workspace("conditions");
    let r = oxc::resolvers(&root);
    let app = root.join("app/page.tsx");
    assert_eq!(rel(&root, &r.resolve(Layer::Rsc, &app, "lib").unwrap().path), "node_modules/lib/server.js");
    assert_eq!(rel(&root, &r.resolve(Layer::Client, &app, "lib").unwrap().path), "node_modules/lib/browser.mjs");
    assert_eq!(rel(&root, &r.resolve(Layer::Ssr, &app, "lib").unwrap().path), "node_modules/lib/node.mjs");
    assert!(r.resolve(Layer::Ssr, &app, "lib").unwrap().in_node_modules);
}

#[test]
fn oxc_002_require_in_the_react_server_layer_never_picks_import() {
    let root = workspace(
        "require",
        &[
            ("node_modules/dual/package.json", r#"{"name":"dual","exports":{".":{"import":"./esm.mjs","require":"./cjs.cjs"}}}"#),
            ("node_modules/dual/esm.mjs", ""),
            ("node_modules/dual/cjs.cjs", ""),
        ],
    );
    let r = oxc::resolvers(&root);
    assert_eq!(rel(&root, &r.resolve_from_root(Layer::Rsc, "dual").unwrap().path), "node_modules/dual/esm.mjs");
    assert_eq!(rel(&root, &r.resolve_rsc_require(&root.join("package.json"), "dual").unwrap().path), "node_modules/dual/cjs.cjs");
}

#[test]
fn oxc_003_subpath_patterns_and_imports_field() {
    let root = lib_workspace("patterns");
    let r = oxc::resolvers(&root);
    let app = root.join("app/page.tsx");
    assert_eq!(rel(&root, &r.resolve(Layer::Client, &app, "lib/feature/a").unwrap().path), "node_modules/lib/dist/feature/a.mjs");
    let lib_dir = root.join("node_modules/lib/index.js");
    assert_eq!(rel(&root, &r.resolve_rsc_require(&lib_dir, "#internal").unwrap().path), "node_modules/lib/internal.js");
    // Not exported: the exports map seals the package
    assert!(r.resolve(Layer::Client, &app, "lib/internal.js").is_none());
}

#[test]
fn oxc_004_tsconfig_paths_with_extends() {
    let root = workspace(
        "tsconfig",
        &[
            ("tsconfig.base.json", r#"{ "compilerOptions": { "baseUrl": ".", "paths": { "@ui/*": ["components/ui/*"] } } }"#),
            ("tsconfig.json", r#"{ // comments are fine
                "extends": "./tsconfig.base.json" }"#),
            ("components/ui/button.tsx", "export function Button() {}"),
            ("app/page.tsx", ""),
        ],
    );
    let r = oxc::resolvers(&root);
    let found = r.resolve(Layer::Client, &root.join("app/page.tsx"), "@ui/button").unwrap();
    assert_eq!(rel(&root, &found.path), "components/ui/button.tsx");
    assert!(!found.in_node_modules);
}

#[test]
fn oxc_005_next_style_aliases_without_tsconfig() {
    let root = workspace(
        "noconfig",
        &[("src/lib/utils.ts", ""), ("lib/db.ts", ""), ("app/page.tsx", "")],
    );
    let r = oxc::resolvers(&root);
    let app = root.join("app/page.tsx");
    assert_eq!(rel(&root, &r.resolve(Layer::Rsc, &app, "@/lib/utils").unwrap().path), "src/lib/utils.ts");
    assert_eq!(rel(&root, &r.resolve(Layer::Rsc, &app, "@/lib/db").unwrap().path), "lib/db.ts");
    assert_eq!(rel(&root, &r.resolve(Layer::Rsc, &app, "~/lib/db").unwrap().path), "lib/db.ts");
}

#[test]
fn oxc_006_extensions_index_files_and_ts_js_extension_alias() {
    let root = workspace(
        "extensions",
        &[("components/ui/index.tsx", ""), ("lib/util.ts", ""), ("app/page.tsx", "")],
    );
    let r = oxc::resolvers(&root);
    let app = root.join("app/page.tsx");
    assert_eq!(rel(&root, &r.resolve(Layer::Client, &app, "../components/ui").unwrap().path), "components/ui/index.tsx");
    assert_eq!(rel(&root, &r.resolve(Layer::Client, &app, "../lib/util.js").unwrap().path), "lib/util.ts");
}

#[test]
fn oxc_007_builtins_and_package_names() {
    assert!(is_builtin("node:fs"));
    assert!(is_builtin("fs/promises"));
    assert!(is_builtin("bun:sqlite"));
    assert!(!is_builtin("react"));
    assert!(!is_builtin("@scope/fs"));
    assert_eq!(package_name("@radix-ui/react-dialog/dist/index.mjs"), "@radix-ui/react-dialog");
    assert_eq!(package_name("react-dom/client"), "react-dom");
    assert_eq!(package_name("lodash"), "lodash");
}


