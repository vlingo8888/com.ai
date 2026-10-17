use std::collections::HashMap;
use std::path::Path;

use super::{client_reference_specifier, segment_key};

#[test]
fn rsc_001_segment_keys_carry_param_values() {
    let mut params = HashMap::new();
    params.insert("slug".to_string(), "hello".to_string());
    params.insert("rest".to_string(), "a/b".to_string());
    assert_eq!(segment_key(Path::new("app/blog/[slug]"), &params), "app/blog/hello");
    assert_eq!(segment_key(Path::new("app/(shop)/docs/[...rest]"), &params), "app/(shop)/docs/a/b");
    assert_eq!(segment_key(Path::new("app"), &params), "app");
    // Different params, different key: the layout under it re-renders
    params.insert("slug".to_string(), "other".to_string());
    assert_eq!(segment_key(Path::new("app/blog/[slug]"), &params), "app/blog/other");
}

#[test]
fn rsc_002_client_reference_ids_map_to_ssr_modules() {
    let root = std::env::temp_dir().join(format!("nata_rsc_refs_{}", std::process::id()));
    std::fs::create_dir_all(root.join("app")).unwrap();
    std::fs::write(root.join("app/form.tsx"), "\"use client\";").unwrap();
    // Its own React 19 set: packages load natively, not through the framework's React
    std::fs::write(root.join("package.json"), r#"{"dependencies":{"react":"19.3.0","react-dom":"19.3.0","react-server-dom-webpack":"19.3.0"}}"#).unwrap();
    crate::next_shims::write(&root);
    let root = root.canonicalize().unwrap();
    let spec = client_reference_specifier(&root, "/_bundle/app/form.tsx").unwrap();
    assert_eq!(spec, format!("ssr:{}", root.join("app/form.tsx").display()));
    assert_eq!(client_reference_specifier(&root, "/_nata/deps/@radix-ui/react-dialog").unwrap(), "@radix-ui/react-dialog");
    // Framework shims are files of the project's .nata, loaded in the SSR layer
    assert_eq!(client_reference_specifier(&root, "/_nata/shims/next/link").unwrap(), format!("ssr:{}", root.join(".nata/next/link.js").display()));
    assert!(client_reference_specifier(&root, "/_bundle/../../etc/passwd").is_err());
    assert!(client_reference_specifier(&root, "/_bundle/app/missing.tsx").is_err());
}
