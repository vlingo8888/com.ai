//! Candidate extraction parity with `@tailwindcss/oxide` 4.3.3. Fixtures in
//! `tests/fixtures/scan/` are recorded from real oxide with:
//!
//!   bun worker/scan-parity.ts --write-fixtures tests/fixtures/scan_expected.json
//!
//! For the differential run over every template file of the repo see
//! `worker/scan-parity.ts`.

use serde_json::Value;
use std::collections::BTreeSet;

#[test]
fn scanner_matches_oxide() {
    let dir = format!("{}/tests/fixtures", env!("CARGO_MANIFEST_DIR"));
    let fixtures: Vec<Value> =
        serde_json::from_str(&std::fs::read_to_string(format!("{dir}/scan_expected.json")).unwrap()).unwrap();
    assert!(!fixtures.is_empty());

    let mut failures = vec![];
    for f in &fixtures {
        let file = f["file"].as_str().unwrap();
        let content = std::fs::read_to_string(format!("{dir}/scan/{file}")).unwrap();
        let expected: BTreeSet<&str> = f["candidates"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
        let actual = tailwind_rs::scanner::extract_file_candidates(file, &content);
        let actual: BTreeSet<&str> = actual.iter().map(String::as_str).collect();
        if expected != actual {
            let missing: Vec<_> = expected.difference(&actual).collect();
            let extra: Vec<_> = actual.difference(&expected).collect();
            failures.push(format!("{file}:\n  missing: {missing:?}\n  extra:   {extra:?}"));
        }
    }
    assert!(failures.is_empty(), "{} of {} files differ:\n{}", failures.len(), fixtures.len(), failures.join("\n"));
}

#[test]
fn scan_skips_ignored_files() {
    use tailwind_rs::FileItem;
    let file = |path: &str| FileItem { path: path.into(), content: "flex p-4".into() };
    for path in ["package-lock.json", "a/node_modules/x/index.js", "logo.png", ".env.local", "styles.scss", "yarn.lock"] {
        assert!(tailwind_rs::scanner::scan_files_parallel(&[file(path)]).tokens.is_empty(), "{path} should be skipped");
    }
    for path in ["index.html", "src/App.tsx", "README.md", "data.json", "Makefile"] {
        assert!(!tailwind_rs::scanner::scan_files_parallel(&[file(path)]).tokens.is_empty(), "{path} should be scanned");
    }
}
