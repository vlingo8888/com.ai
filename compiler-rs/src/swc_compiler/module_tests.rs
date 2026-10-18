use std::path::{Path, PathBuf};

use super::module::{compile_module, CompiledModule, Directive, ModuleCompileOptions};
use super::server_actions::action_id;
use crate::resolver::oxc_tests::workspace;
use crate::resolver::Layer;

fn project(name: &str, extra: &[(&str, &str)]) -> PathBuf {
    let mut files: Vec<(&str, &str)> = vec![
        // Its own React 19 set (otherwise the App Router runs on the framework's React, see framework_react.rs)
        ("package.json", r#"{"name":"app","dependencies":{"react":"19.3.0","react-dom":"19.3.0","react-server-dom-webpack":"19.3.0"}}"#),
        ("node_modules/react/package.json", r#"{"name":"react","exports":{".":{"react-server":"./react.react-server.js","default":"./index.js"},"./jsx-dev-runtime":{"react-server":"./jsx-dev-runtime.react-server.js","default":"./jsx-dev-runtime.js"},"./jsx-runtime":{"react-server":"./jsx-runtime.react-server.js","default":"./jsx-runtime.js"}}}"#),
        ("node_modules/react/index.js", ""),
        ("node_modules/react/react.react-server.js", ""),
        ("node_modules/react/jsx-dev-runtime.js", ""),
        ("node_modules/react/jsx-dev-runtime.react-server.js", ""),
        ("node_modules/react/jsx-runtime.js", ""),
        ("node_modules/react/jsx-runtime.react-server.js", ""),
        ("node_modules/react-server-dom-webpack/package.json", r#"{"name":"react-server-dom-webpack","exports":{"./server.edge":"./server.edge.js","./client.edge":"./client.edge.js","./client.browser":"./client.browser.js"}}"#),
        ("node_modules/react-server-dom-webpack/server.edge.js", ""),
        ("node_modules/react-server-dom-webpack/client.edge.js", ""),
        ("node_modules/lucide-react/package.json", r#"{"name":"lucide-react","module":"./dist/esm/index.js","main":"./dist/cjs/index.js"}"#),
        ("node_modules/lucide-react/dist/esm/index.js", "export const Star = () => null;"),
        ("node_modules/lucide-react/dist/cjs/index.js", ""),
        ("lib/posts.ts", "export async function getPosts() { return []; }"),
        ("app/globals.css", "body {}"),
        ("app/button.tsx", "\"use client\";\nexport function Button() { return <button />; }"),
    ];
    files.extend_from_slice(extra);
    workspace(name, &files)
}

fn compile(root: &Path, rel: &str, layer: Layer) -> CompiledModule {
    let file = root.join(rel);
    let source = std::fs::read_to_string(&file).unwrap();
    compile_module(
        &source,
        &ModuleCompileOptions { root, file: &file, layer, development: true, refresh: false, package_specifier: None },
    )
    .unwrap_or_else(|e| panic!("{}", e))
}

const PAGE: &str = r#"import { getPosts } from "@/lib/posts";
import { Button } from "./button";
import { Star } from "lucide-react";
import Link from "next/link";
import "./globals.css";
import type { Metadata } from "next";

export const metadata: Metadata = { title: "Home" };

export default async function Page() {
  const posts = await getPosts();
  return <main><Button /><Star /><Link href="/a">a</Link>{posts.length}</main>;
}
"#;

#[test]
fn mod_001_browser_imports_become_dev_server_urls() {
    let root = project("client_urls", &[("app/page.tsx", PAGE)]);
    let out = compile(&root, "app/page.tsx", Layer::Client);
    assert!(out.code.contains("\"/_bundle/lib/posts.ts\""), "{}", out.code);
    assert!(out.code.contains("\"/_bundle/app/button.tsx\""), "{}", out.code);
    assert!(out.code.contains("\"/_nata/deps/lucide-react\""), "{}", out.code);
    assert!(out.code.contains("\"/_nata/shims/next/link\""), "{}", out.code);
    assert!(out.code.contains("\"/_nata/css/app/globals.css\""), "{}", out.code);
    assert!(out.code.contains("\"/_nata/deps/react/jsx-dev-runtime\""), "automatic JSX runtime: {}", out.code);
    assert!(!out.code.contains("Metadata"), "type-only import is dropped: {}", out.code);
    assert_eq!(out.directive, None);
    assert_eq!(out.exports, vec!["metadata".to_string(), "default".to_string()]);
    assert!(out.imports.contains(&root.join("lib/posts.ts")));
}

#[test]
fn mod_002_react_server_layer_loads_through_the_rsc_namespace() {
    let root = project("rsc_urls", &[("app/page.tsx", PAGE)]);
    let out = compile(&root, "app/page.tsx", Layer::Rsc);
    let ns = |rel: &str| format!("\"rsc:{}\"", root.join(rel).to_string_lossy());
    assert!(out.code.contains(&ns("lib/posts.ts")), "{}", out.code);
    assert!(out.code.contains(&ns("app/button.tsx")), "{}", out.code);
    assert!(out.code.contains(&ns("node_modules/lucide-react/dist/esm/index.js")), "{}", out.code);
    assert!(out.code.contains(&ns("node_modules/react/jsx-dev-runtime.react-server.js")), "{}", out.code);
    assert!(out.code.contains("\"rsc:/__nata_client_shim/next/link\""), "{}", out.code);
    // CSS is handled natively, identical in both layers
    assert!(out.code.contains(&format!("\"{}\"", root.join("app/globals.css").to_string_lossy())), "{}", out.code);
    assert!(out.package_imports.iter().any(|(spec, _)| spec == "lucide-react"));
}

#[test]
fn mod_003_ssr_layer_shares_installed_packages() {
    let root = project("ssr_urls", &[("app/page.tsx", PAGE)]);
    let out = compile(&root, "app/page.tsx", Layer::Ssr);
    assert!(out.code.contains("from \"lucide-react\""), "{}", out.code);
    assert!(out.code.contains("\"react/jsx-dev-runtime\""), "{}", out.code);
    assert!(out.code.contains(&format!("\"ssr:{}\"", root.join("lib/posts.ts").to_string_lossy())), "{}", out.code);
    assert!(out.code.contains("from \"next/link\""), "{}", out.code);
}

#[test]
fn mod_004_use_client_is_a_client_reference_on_the_server() {
    let root = project("client_ref", &[]);
    let out = compile(&root, "app/button.tsx", Layer::Rsc);
    assert_eq!(out.directive, Some(Directive::UseClient));
    assert!(out.is_reference_stub);
    assert!(out.code.contains("registerClientReference"), "{}", out.code);
    assert!(out.code.contains("\"/_bundle/app/button.tsx\", \"Button\""), "{}", out.code);
    assert!(!out.code.contains("<button"), "{}", out.code);
    // The browser and SSR still get the component itself
    let client = compile(&root, "app/button.tsx", Layer::Client);
    assert!(!client.is_reference_stub);
    assert!(client.code.contains("\"button\""), "{}", client.code);
}

const ACTIONS: &str = r#""use server";
import { getPosts } from "@/lib/posts";

let todos: string[] = [];

export async function addTodo(prev: unknown, formData: FormData) {
  todos.push(String(formData.get("title")));
  return { todos, where: typeof window === "undefined" ? "server" : "browser" };
}

export const greet = async (name: string) => `Hello, ${name}`;

export default async function reset() { todos = []; await getPosts(); }
"#;

#[test]
fn mod_005_use_server_module_registers_actions_on_the_server() {
    let root = project("actions_rsc", &[("app/actions.ts", ACTIONS)]);
    let out = compile(&root, "app/actions.ts", Layer::Rsc);
    assert_eq!(out.directive, Some(Directive::UseServer));
    let names: Vec<&str> = out.actions.iter().map(|(_, n)| n.as_str()).collect();
    assert_eq!(names, vec!["addTodo", "greet", "default"]);
    assert_eq!(out.actions[0].0, action_id("app/actions.ts", "addTodo"));
    assert!(out.code.contains(&format!("__nata_action(addTodo, \"{}\")", action_id("app/actions.ts", "addTodo"))), "{}", out.code);
    assert!(out.code.contains("todos.push"), "action body stays on the server: {}", out.code);
    assert!(out.code.contains(&format!("rsc:{}", root.join("node_modules/react-server-dom-webpack/server.edge.js").to_string_lossy())), "{}", out.code);
}

#[test]
fn mod_006_use_server_module_is_only_references_in_the_browser() {
    let root = project("actions_client", &[("app/actions.ts", ACTIONS)]);
    for layer in [Layer::Client, Layer::Ssr] {
        let out = compile(&root, "app/actions.ts", layer);
        assert!(out.is_reference_stub);
        assert!(!out.code.contains("todos"), "server code leaked into {:?}: {}", layer, out.code);
        assert!(!out.code.contains("getPosts"), "{}", out.code);
        assert!(out.code.contains(&format!("export const addTodo = __nata_server_ref(\"{}\"", action_id("app/actions.ts", "addTodo"))), "{}", out.code);
        assert!(out.code.contains("export default __nata_server_ref("), "{}", out.code);
        assert_eq!(out.actions.len(), 3);
    }
    let browser = compile(&root, "app/actions.ts", Layer::Client);
    assert!(browser.code.contains("/_nata/deps/react-server-dom-webpack/client.browser"), "{}", browser.code);
}

#[test]
fn mod_007_inline_server_actions_are_hoisted_with_their_closure() {
    let page = r#"import { db } from "core";
export default async function Page({ id }: { id: string }) {
  const prefix = "item-";
  async function save(formData: FormData) {
    "use server";
    await db.insertInto("items").values({ id: prefix + id, title: formData.get("title") }).execute();
  }
  const remove = async () => { "use server"; console.log("remove"); };
  return <form action={save}><button formAction={remove}>x</button></form>;
}
"#;
    let root = project("inline_actions", &[("app/page.tsx", page)]);
    let out = compile(&root, "app/page.tsx", Layer::Rsc);
    assert_eq!(out.actions.len(), 2, "{}", out.code);
    assert_eq!(out.actions[0].1, "$$ACTION_0");
    assert!(out.code.contains("const save = $$ACTION_0.bind(null, prefix, id)"), "{}", out.code);
    assert!(out.code.contains("const remove = $$ACTION_1;"), "{}", out.code);
    assert!(out.code.contains("export async function $$ACTION_0(prefix, id, formData)"), "{}", out.code);
    assert!(out.code.contains("from \"core\""), "core stays shared: {}", out.code);
    assert!(out.code.contains(&format!("__nata_action($$ACTION_0, \"{}\")", action_id("app/page.tsx", "$$ACTION_0"))), "{}", out.code);
}

#[test]
fn mod_008_type_exports_are_not_exports() {
    let src = "\"use client\";\nexport type Props = { a: string };\nexport interface X {}\nexport enum Color { Red }\nexport { a as b, type Props as P };\nconst a = 1;\n";
    let root = project("type_exports", &[("app/x.tsx", src)]);
    let out = compile(&root, "app/x.tsx", Layer::Client);
    assert_eq!(out.exports, vec!["Color".to_string(), "b".to_string()]);
}


#[test]
fn mod_009_use_cache_functions_run_through_the_data_cache() {
    let src = r#"import { cacheLife } from "next/cache";
export async function getPosts(limit: number) {
  "use cache";
  cacheLife("hours");
  return [limit];
}
export const getUser = async (id: string) => {
  "use cache";
  return { id };
};
export default async function Page({ tenant }: { tenant: string }) {
  async function load(n: number) {
    "use cache";
    return tenant + n;
  }
  return await load(1);
}
export async function notCached() { return 1; }
"#;
    let root = project("use_cache", &[("app/page.tsx", src), (".nata/next/cache-runtime.ts", "export function cacheCall() {}")]);
    let out = compile(&root, "app/page.tsx", Layer::Rsc);
    let code = &out.code;
    assert!(code.contains("import { cacheCall as __nata_cache_call } from \"rsc:"), "{code}");
    assert!(code.contains("__nata_cache_call(\"app/page.tsx#getPosts\", [\n"), "{code}");
    assert!(code.contains("export async function getPosts(...$$nata_args)"), "keeps the name and hoisting: {code}");
    assert!(code.contains("\"app/page.tsx#getUser\""), "{code}");
    // Closed-over variables are part of the key
    assert!(code.contains("\"app/page.tsx#load\""), "{code}");
    assert!(code.contains("tenant\n"), "captured tenant: {code}");
    assert!(!code.contains("\"use cache\""), "{code}");
    assert!(code.contains("export async function notCached() {\n    return 1;"), "{code}");
    // Browser code is never wrapped
    let client = compile(&root, "app/page.tsx", Layer::Client);
    assert!(!client.code.contains("__nata_cache_call"));
}

#[test]
fn mod_010_file_level_use_cache_wraps_every_async_export() {
    let src = "\"use cache\";\nexport async function a() { return 1; }\nexport async function b(x: number) { return x; }\nexport function sync() { return 2; }\n";
    let root = project("use_cache_file", &[("lib/data.ts", src)]);
    let out = compile(&root, "lib/data.ts", Layer::Rsc);
    assert!(out.code.contains("\"lib/data.ts#a\""), "{}", out.code);
    assert!(out.code.contains("\"lib/data.ts#b\""), "{}", out.code);
    assert!(out.code.contains("export function sync() {\n    return 2;"), "{}", out.code);
}

#[test]
fn mod_011_public_env_is_inlined_in_the_browser_only() {
    let src = "export const a = process.env.NEXT_PUBLIC_API;\nexport const b = process.env.SECRET_KEY;\nexport const c = process.env.NODE_ENV;\nexport const d = process.env[\"NEXT_PUBLIC_API\"];\n";
    let root = project("env_inline", &[("app/config.ts", src), (".env", "NEXT_PUBLIC_API=https://api.example.com\nSECRET_KEY=s3cret\n")]);
    let client = compile(&root, "app/config.ts", Layer::Client);
    assert!(client.code.contains("export const a = \"https://api.example.com\""), "{}", client.code);
    assert!(client.code.contains("export const b = undefined"), "{}", client.code);
    assert!(client.code.contains("export const c = \"development\""), "{}", client.code);
    assert!(client.code.contains("export const d = \"https://api.example.com\""), "{}", client.code);
    assert!(!client.code.contains("s3cret"));
    // The server reads its environment at runtime
    let server = compile(&root, "app/config.ts", Layer::Rsc);
    assert!(server.code.contains("process.env.SECRET_KEY"), "{}", server.code);
}

#[test]
fn mod_client_only_react_import_in_server_component_is_a_compile_error() {
    let root = project(
        "server_react_imports",
        &[
            ("components/counter.tsx", "import { useMemo, useState } from \"react\";\nexport function Counter() { const [n] = useState(0); return <b>{n}</b>; }\n"),
            ("components/client.tsx", "\"use client\";\nimport { useState } from \"react\";\nexport function C() { useState(0); return null; }\n"),
            ("components/types.tsx", "import type { Dispatch } from \"react\";\nimport { type SetStateAction, useMemo, cache } from \"react\";\nexport const f = cache(() => 1);\n"),
            ("node_modules/lib/index.js", "import { useState } from \"react\";\nexport const x = 1;\n"),
        ],
    );
    let compile_err = |rel: &str, layer: Layer| {
        let file = root.join(rel);
        let source = std::fs::read_to_string(&file).unwrap();
        compile_module(&source, &ModuleCompileOptions { root: &root, file: &file, layer, development: true, refresh: false, package_specifier: None }).err()
    };

    let err = compile_err("components/counter.tsx", Layer::Rsc).expect("useState in a Server Component is rejected");
    assert!(err.message.starts_with("You're importing a component that needs `useState`. This React Hook only works in a Client Component."), "{}", err.message);
    assert!(err.message.contains("https://nextjs.org/docs/app/api-reference/directives/use-client"), "{}", err.message);
    assert_eq!((err.file.as_str(), err.line, err.column), ("components/counter.tsx", Some(1), Some(19)));
    assert!(err.frame.contains("import { useMemo, useState }"), "{}", err.frame);

    // The same file is fine where React has every API, and so are client modules, type imports,
    // server-safe APIs and packages
    assert!(compile_err("components/counter.tsx", Layer::Ssr).is_none());
    assert!(compile_err("components/client.tsx", Layer::Rsc).is_none());
    assert!(compile_err("components/types.tsx", Layer::Rsc).is_none());
    assert!(compile_err("node_modules/lib/index.js", Layer::Rsc).is_none());
}
