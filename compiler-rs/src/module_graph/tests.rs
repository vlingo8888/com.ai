use std::path::PathBuf;

use super::ModuleGraph;
use crate::resolver::Layer;
use crate::swc_compiler::CompiledModule;

fn module(imports: &[&str], actions: &[(&str, &str)]) -> CompiledModule {
    CompiledModule {
        imports: imports.iter().map(PathBuf::from).collect(),
        actions: actions.iter().map(|(id, name)| (id.to_string(), name.to_string())).collect(),
        ..Default::default()
    }
}

fn p(s: &str) -> PathBuf {
    PathBuf::from(s)
}

#[test]
fn graph_001_invalidation_bumps_the_file_and_its_server_importers() {
    let mut g = ModuleGraph::default();
    g.upsert(&p("/app/page.tsx"), Layer::Rsc, [0; 32], &module(&["/lib/posts.ts", "/app/ui.tsx"], &[]));
    g.upsert(&p("/lib/posts.ts"), Layer::Rsc, [0; 32], &module(&["/lib/db.ts"], &[]));
    g.upsert(&p("/lib/db.ts"), Layer::Rsc, [0; 32], &module(&[], &[]));
    g.upsert(&p("/app/ui.tsx"), Layer::Rsc, [0; 32], &module(&[], &[]));

    let affected = g.invalidate(&p("/lib/db.ts"));
    let files: Vec<_> = affected.iter().map(|(f, _)| f.to_string_lossy().to_string()).collect();
    assert_eq!(files, vec!["/lib/db.ts", "/lib/posts.ts", "/app/page.tsx"]);
    assert_eq!(g.version(&p("/lib/db.ts"), Layer::Rsc), 1);
    assert_eq!(g.version(&p("/app/page.tsx"), Layer::Rsc), 1);
    // Untouched sibling keeps its version (and its module instance in the worker)
    assert_eq!(g.version(&p("/app/ui.tsx"), Layer::Rsc), 0);
    assert_eq!(g.specifier(&p("/app/page.tsx"), Layer::Rsc), "rsc:/app/page.tsx?v=1");
    assert_eq!(g.specifier(&p("/app/ui.tsx"), Layer::Rsc), "rsc:/app/ui.tsx");
}

#[test]
fn graph_002_browser_modules_do_not_bump_importers() {
    let mut g = ModuleGraph::default();
    g.upsert(&p("/app/form.tsx"), Layer::Client, [0; 32], &module(&["/app/input.tsx"], &[]));
    g.upsert(&p("/app/input.tsx"), Layer::Client, [0; 32], &module(&[], &[]));
    let affected = g.invalidate(&p("/app/input.tsx"));
    assert_eq!(affected, vec![(p("/app/input.tsx"), Layer::Client)]);
    assert_eq!(g.version(&p("/app/form.tsx"), Layer::Client), 0);
}

#[test]
fn graph_003_versioned_code_points_imports_at_current_versions() {
    let mut g = ModuleGraph::default();
    g.upsert(&p("/app/page.tsx"), Layer::Rsc, [0; 32], &module(&["/lib/posts.ts"], &[]));
    g.upsert(&p("/lib/posts.ts"), Layer::Rsc, [0; 32], &module(&[], &[]));
    g.invalidate(&p("/lib/posts.ts"));
    g.invalidate(&p("/lib/posts.ts"));
    let code = "import { getPosts } from \"rsc:/lib/posts.ts\";";
    assert_eq!(
        g.versioned_code(code, Layer::Rsc, &[p("/lib/posts.ts")]),
        "import { getPosts } from \"rsc:/lib/posts.ts?v=2\";"
    );
}

#[test]
fn graph_004_action_manifest_follows_recompiles() {
    let mut g = ModuleGraph::default();
    g.upsert(&p("/app/actions.ts"), Layer::Rsc, [0; 32], &module(&[], &[("a1", "addTodo"), ("a2", "greet")]));
    assert_eq!(g.action("a1"), Some(&(p("/app/actions.ts"), "addTodo".to_string())));
    // `greet` was removed from the file
    g.upsert(&p("/app/actions.ts"), Layer::Rsc, [1; 32], &module(&[], &[("a1", "addTodo")]));
    assert!(g.action("a2").is_none());
    assert!(g.action("a1").is_some());
}

#[test]
fn graph_005_removed_import_edges_stop_propagating() {
    let mut g = ModuleGraph::default();
    g.upsert(&p("/app/page.tsx"), Layer::Rsc, [0; 32], &module(&["/lib/old.ts"], &[]));
    g.upsert(&p("/app/page.tsx"), Layer::Rsc, [1; 32], &module(&[], &[]));
    g.upsert(&p("/lib/old.ts"), Layer::Rsc, [0; 32], &module(&[], &[]));
    g.invalidate(&p("/lib/old.ts"));
    assert_eq!(g.version(&p("/app/page.tsx"), Layer::Rsc), 0);
}

#[test]
fn graph_006_react_18_projects_keep_the_client_rendered_pipeline() {
    let dir = std::env::temp_dir().join(format!("nata_react18_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("package.json"), r#"{"dependencies":{"react":"^18.3.1"}}"#).unwrap();
    assert_eq!(super::react_major(&dir), Some(18));
    std::fs::write(dir.join("package.json"), r#"{"dependencies":{"react":"19.3.0"}}"#).unwrap();
    assert_eq!(super::react_major(&dir), Some(19));
    // The installed version wins over the declared range
    std::fs::create_dir_all(dir.join("node_modules/react")).unwrap();
    std::fs::write(dir.join("node_modules/react/package.json"), r#"{"version":"18.2.0"}"#).unwrap();
    assert_eq!(super::react_major(&dir), Some(18));
    let _ = std::fs::remove_dir_all(&dir);
}
