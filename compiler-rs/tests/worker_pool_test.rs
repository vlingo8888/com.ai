use compiler_rs::worker_pool;
use std::fs;
use std::path::PathBuf;

fn temp_project(name: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let dir = std::env::temp_dir().join(format!("nata_worker_test_{name}_{nanos}"));
    fs::create_dir_all(&dir).unwrap();
    dir.canonicalize().unwrap()
}

fn bun_available() -> bool {
    std::process::Command::new(compiler_rs::rpc::find_bun_bin()).arg("--version").output().is_ok()
}

#[tokio::test]
async fn worker_keeps_state_between_requests_and_captures_output() {
    if !bun_available() {
        return;
    }
    let root = temp_project("state");
    let script = r#"
globalThis.__count = (globalThis.__count || 0) + 1;
console.log("count", globalThis.__count);
console.error("to stderr");
"#;
    let first = worker_pool::run(&root, script).await.unwrap();
    let second = worker_pool::run(&root, script).await.unwrap();

    assert!(first.success && second.success);
    assert_eq!(first.stdout, "count 1\n");
    // Same process: module-level state survives, which is what keeps imports and the DB warm
    assert_eq!(second.stdout, "count 2\n");
    assert_eq!(second.stderr, "to stderr\n");
    let _ = fs::remove_dir_all(&root);
}

#[tokio::test]
async fn process_exit_ends_the_request_not_the_worker() {
    if !bun_available() {
        return;
    }
    let root = temp_project("exit");
    let failed = worker_pool::run(&root, "process.stdout.write('bye'); process.exit(3);").await.unwrap();
    assert!(!failed.success);
    assert_eq!(failed.stdout, "bye");

    let thrown = worker_pool::run(&root, "throw new Error('boom');").await.unwrap();
    assert!(!thrown.success);
    assert!(thrown.stderr.contains("boom"));

    let ok = worker_pool::run(&root, "console.log('still alive');").await.unwrap();
    assert!(ok.success);
    assert_eq!(ok.stdout, "still alive\n");
    let _ = fs::remove_dir_all(&root);
}

#[tokio::test]
async fn restart_drops_module_state() {
    if !bun_available() {
        return;
    }
    let root = temp_project("restart");
    let script = "globalThis.__n = (globalThis.__n || 0) + 1; console.log(globalThis.__n);";
    assert_eq!(worker_pool::run(&root, script).await.unwrap().stdout, "1\n");
    worker_pool::restart(&root).await;
    assert_eq!(worker_pool::run(&root, script).await.unwrap().stdout, "1\n");
    let _ = fs::remove_dir_all(&root);
}

#[tokio::test]
async fn concurrent_requests_run_in_parallel_with_isolated_state() {
    if !bun_available() {
        return;
    }
    let root = temp_project("concurrent");
    // Each request stores its own cookie map, waits, then reads it back
    let script = |name: &str| {
        format!(
            r#"
globalThis.__NATA_COOKIES__ = new Map([["user", "{name}"]]);
await new Promise((r) => setTimeout(r, 300));
console.log(globalThis.__NATA_COOKIES__.get("user"), typeof window);
"#
        )
    };
    // Warm up so the timing below measures requests, not process startup
    worker_pool::run(&root, "1;").await.unwrap();

    let (alice, bob) = (script("alice"), script("bob"));
    let started = std::time::Instant::now();
    let (a, b) = tokio::join!(worker_pool::run(&root, &alice), worker_pool::run(&root, &bob));
    let elapsed = started.elapsed();

    // `window` only exists while rendering (SSR), never in actions or route handlers
    assert_eq!(a.unwrap().stdout, "alice undefined\n");
    assert_eq!(b.unwrap().stdout, "bob undefined\n");
    // Two 300ms requests overlapping, not queued one after the other
    assert!(elapsed < std::time::Duration::from_millis(550), "took {elapsed:?}");
    let _ = fs::remove_dir_all(&root);
}

#[tokio::test]
async fn request_bodies_round_trip_as_raw_bytes() {
    if !bun_available() {
        return;
    }
    let root = temp_project("route");
    fs::write(
        root.join("route.ts"),
        r#"export async function POST(req: Request) {
  const bytes = new Uint8Array(await req.arrayBuffer());
  return new Response(bytes.reverse(), { headers: { "content-type": "application/octet-stream" } });
}
"#,
    )
    .unwrap();
    let input = serde_json::json!({
        "filePath": root.join("route.ts"),
        "url": "http://localhost/api",
        "method": "POST",
        "params": {},
        "searchParams": {},
        "headers": {},
        "cookiesStr": "",
    });
    let body: Vec<u8> = vec![0, 1, 2, 255, 254];
    let reply = worker_pool::call(&root, "route", input, body).await.unwrap();
    assert_eq!(reply.output()["status"], 200);
    assert_eq!(reply.body, vec![254, 255, 2, 1, 0]);
    let _ = fs::remove_dir_all(&root);
}
