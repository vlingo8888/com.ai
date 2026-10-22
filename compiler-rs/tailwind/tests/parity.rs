//! Byte-for-byte parity with tailwindcss@4.3.3 (as configured by
//! `worker/engine.ts`). Fixtures are recorded from real Tailwind with:
//!
//!   bun worker/parity.ts --batch 300 --limit 1200 --write-fixtures tests/fixtures/batches.json
//!   bun worker/parity.ts --custom --write-fixtures tests/fixtures/custom.json
//!   bun worker/parity.ts --batch 300 --limit 1200 --strict-minify --write-fixtures tests/fixtures/minified.json
//!   bun worker/parity.ts --custom --strict-minify --write-fixtures tests/fixtures/custom-minified.json
//!
//! Plugin fixtures need the JavaScript plugins installed in some directory
//! (`bun add @tailwindcss/typography @tailwindcss/forms`):
//!
//!   bun worker/parity.ts --plugin-cases --plugins <dir> --write-fixtures tests/fixtures/plugins.json
//!   bun worker/parity.ts --plugin-cases --plugins <dir> --strict-minify --write-fixtures tests/fixtures/plugins-minified.json
//!
//! For the full differential run (~29k classes) see `worker/parity.ts`.

use serde_json::Value;

/// Minified cases that differ only because this copy minifies with lightningcss 1.0.0-alpha.72 (the
/// engine's version) instead of alpha.71 (npm lightningcss@1.32.0, what the fixtures were recorded
/// with). `apply-mixin` is an invalid top-level `@apply --mixin`: alpha.72 moves it into the next rule.
const LIGHTNINGCSS_72_DIFFERENCES: &[&str] = &["apply-mixin"];

fn run(file: &str) {
    let fixtures: Vec<Value> =
        serde_json::from_str(&std::fs::read_to_string(format!("{}/tests/fixtures/{}", env!("CARGO_MANIFEST_DIR"), file)).unwrap())
            .unwrap();
    assert!(!fixtures.is_empty());

    let mut failures = vec![];
    for f in &fixtures {
        let name = f["name"].as_str().unwrap();
        let tokens: Vec<String> = f["tokens"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect();
        let custom_css = f["customCss"].as_str().unwrap();
        let minify = f["minify"].as_bool().unwrap_or(false);
        if minify && LIGHTNINGCSS_72_DIFFERENCES.contains(&name) {
            continue;
        }
        let actual = tailwind_rs::engine::try_compile_tokens(&tokens, custom_css, minify)
            // Minified CSS is one line: split rules for a readable diff.
            .map(|css| if minify { css.replace('}', "}\n") } else { css });
        let expected = f["expected"].as_str().map(|css| if minify { css.replace('}', "}\n") } else { css.to_string() });

        match (expected.as_deref(), actual) {
            (Some(expected), Ok(actual)) if expected == actual => {}
            (None, Err(_)) => {}
            (expected, actual) => {
                let expected = expected.unwrap_or("<error>");
                let actual = actual.unwrap_or_else(|e| format!("<error: {e}>"));
                let line = expected.lines().zip(actual.lines()).position(|(a, b)| a != b).unwrap_or(0);
                failures.push(format!(
                    "{name}: first difference at line {line}\n  expected: {:?}\n  actual:   {:?}",
                    expected.lines().nth(line),
                    actual.lines().nth(line)
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{} of {} cases differ:\n{}", failures.len(), fixtures.len(), failures.join("\n"));
}

#[test]
fn batches_match_tailwind() {
    run("batches.json");
}

#[test]
fn custom_css_matches_tailwind() {
    run("custom.json");
}

#[test]
fn plugins_match_tailwind() {
    // Real Tailwind running the JavaScript @tailwindcss/typography and
    // @tailwindcss/forms vs the native ports (src/tw/plugins)
    run("plugins.json");
    run("plugins-minified.json");
}

#[test]
fn minified_matches_worker() {
    // The worker ships `optimize(css, { minify: true })` (Lightning CSS)
    run("minified.json");
    run("custom-minified.json");
}

#[test]
fn builds_are_independent() {
    // A cached compiler must not leak state (used theme variables, invalid
    // candidates) from one build into the next.
    let a = tailwind_rs::engine::compile_tokens(&["font-sans".into(), "p-1".into()], "", true);
    let _ = tailwind_rs::engine::compile_tokens(&["text-red-500".into(), "not-a-class".into()], "", true);
    let b = tailwind_rs::engine::compile_tokens(&["font-sans".into(), "p-1".into()], "", true);
    assert_eq!(a, b);
    assert!(!b.contains("--color-red-500"));
}
