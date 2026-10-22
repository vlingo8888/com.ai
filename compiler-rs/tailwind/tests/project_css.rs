//! Project CSS end to end: files → `scanner::scan_files_parallel` → compiled
//! CSS (`@import` resolution, `@import "tailwindcss"` options, `@source`).

use tailwind_rs::scanner::scan_files_parallel;
use tailwind_rs::FileItem;

fn file(path: &str, content: &str) -> FileItem {
    FileItem { path: path.into(), content: content.into() }
}

fn build(files: &[FileItem]) -> (Vec<String>, String) {
    let scan = scan_files_parallel(files);
    let css = tailwind_rs::engine::try_compile_tokens(&scan.tokens, &scan.custom_css, false).unwrap();
    (scan.tokens, css)
}

#[test]
fn import_options_reach_the_compiler() {
    let (_, css) = build(&[
        file("src/index.css", "@import \"tailwindcss\" prefix(tw);"),
        file("src/App.tsx", r#"export const App = () => <div className="tw:flex tw:p-4 hidden" />;"#),
    ]);
    assert!(css.contains(".tw\\:flex {"), "{css}");
    assert!(css.contains("--tw-spacing:"), "{css}");
    // Without the prefix, `hidden` is not a utility any more
    assert!(!css.contains(".hidden {"), "{css}");
}

#[test]
fn important_option() {
    let (_, css) = build(&[file("index.css", "@import 'tailwindcss' important;"), file("index.html", r#"<p class="p-4">"#)]);
    assert!(css.contains("padding: calc(var(--spacing) * 4) !important;"), "{css}");
}

#[test]
fn project_imports_are_inlined_once() {
    let (_, css) = build(&[
        file("src/index.css", "@import \"tailwindcss\";\n@import \"./theme.css\";\n.card { @apply p-2 bg-brand; }"),
        file("src/theme.css", "@theme { --color-brand: #123456; }"),
        file("src/App.tsx", r#"<div className="text-brand" />"#),
    ]);
    assert!(css.contains("--color-brand: #123456;"), "{css}");
    assert!(css.contains(".text-brand {"), "{css}");
    assert!(css.contains("background-color: var(--color-brand);"), "{css}");
    assert!(!css.contains("@import"), "{css}");
}

#[test]
fn tw_animate_css_is_bundled() {
    let (_, css) = build(&[
        file("app/globals.css", "@import \"tailwindcss\";\n@import \"tw-animate-css\";"),
        file("app/dialog.tsx", r#"<div className="data-[state=open]:animate-in data-[state=open]:fade-in-0 zoom-in-95" />"#),
    ]);
    assert!(css.contains(".data-\\[state\\=open\\]\\:animate-in[data-state=\"open\"] {"), "{css}");
    assert!(css.contains(".zoom-in-95 {"), "{css}");
    assert!(!css.contains("tw-animate-css"), "{css}");
}

#[test]
fn source_none_and_explicit_sources() {
    let files = [
        file("src/styles/app.css", "@import \"tailwindcss\" source(none);\n@source \"../components\";\n@source not \"../components/legacy\";\n@source \"../../node_modules/ui-kit/dist/**/*.js\";"),
        file("src/components/Button.tsx", r#"<button className="px-4" />"#),
        file("src/components/legacy/Old.tsx", r#"<div className="mt-8" />"#),
        file("src/pages/Home.tsx", r#"<div className="mb-6" />"#),
        file("node_modules/ui-kit/dist/card.js", r#"const c = "rounded-xl";"#),
    ];
    let (tokens, css) = build(&files);
    assert!(tokens.contains(&"px-4".to_string()) && tokens.contains(&"rounded-xl".to_string()), "{tokens:?}");
    assert!(!tokens.contains(&"mt-8".to_string()), "@source not");
    assert!(!tokens.contains(&"mb-6".to_string()), "source(none)");
    assert!(css.contains(".px-4 {") && css.contains(".rounded-xl {"));
}

#[test]
fn source_dir_limits_auto_detection() {
    let (tokens, _) = build(&[
        file("web/app.css", "@import \"tailwindcss\" source(\"./src\");"),
        file("web/src/a.tsx", r#"<a className="underline" />"#),
        file("web/other/b.tsx", r#"<b className="italic" />"#),
        file("node_modules/x/index.js", r#""ring-2""#),
    ]);
    assert!(tokens.contains(&"underline".to_string()));
    assert!(!tokens.contains(&"italic".to_string()) && !tokens.contains(&"ring-2".to_string()), "{tokens:?}");
}

#[test]
fn auto_detection_by_default() {
    let (tokens, _) = build(&[
        file("index.css", "@import \"tailwindcss\";"),
        file("a/b.html", r#"<i class="flex">"#),
        file("node_modules/pkg/index.js", r#""grid""#),
    ]);
    assert!(tokens.contains(&"flex".to_string()));
    assert!(!tokens.contains(&"grid".to_string()), "node_modules is ignored by default");
}
