# tailwind-rs

Native Tailwind CSS v4 compiler used by ws4/ws6 (`/v2`, and as the fallback when the Bun worker is unavailable).

`src/tw/` is a port of **tailwindcss@4.3.3** (the version bundled by `@tailwindcss/node`, which `worker/engine.ts` uses). It is fed the same base stylesheet as the worker, so the unminified output is byte-for-byte identical to the worker's `compiler.build()`.

## Layout

| Path | Upstream |
| --- | --- |
| `src/tw/css_parser.rs`, `ast.rs`, `value_parser.rs` | `css-parser.ts`, `ast.ts`, `value-parser.ts`, `walk.ts` |
| `src/tw/index.rs` | `index.ts` (`compile` / `build`, `@import "tailwindcss"` from embedded `assets/`, `@theme`, `@utility`, `@custom-variant`, `@variant`, `@source inline`, `@media important/reference/theme()/prefix()`) |
| `src/tw/candidate.rs`, `compile.rs`, `design_system.rs` | `candidate.ts`, `compile.ts`, `design-system.ts` |
| `src/tw/utilities/` | `utilities.ts` (registration order matters), `createCssUtility` |
| `src/tw/variants.rs` | `variants.ts` |
| `src/tw/theme.rs`, `optimize.rs` | `theme.ts`, `optimizeAst` (tree-shaking, `@property` and `color-mix` fallbacks) |
| `src/tw/nesting.rs`, `selector_parser.rs`, `attribute_selector.rs` | `handleNesting` (flattens nested rules, hoists `@media`/`@supports`/…), `selector-parser.ts`, `attribute-selector-parser.ts` |
| `src/tw/apply.rs`, `css_functions.rs`, `compat*.rs` | `apply.ts`, `css-functions.ts` (`--theme()`, `--spacing()`, `--alpha()`, legacy `theme(colors.red.500)`), compat legacy utilities |
| `src/tw/lower.rs` | not upstream: flattens CSS nesting + minifies; only a fallback when Lightning CSS fails |
| `src/tw/plugins/` | `@plugin` with the plugin API of `compat/plugin-api.ts` (`addBase`, `addVariant`, `addUtilities`/`addComponents`, `objectToAst`) and native ports: `@tailwindcss/typography` 0.5.20 (calls recorded by `worker/gen-plugins.ts` into `assets/plugins/typography.json`; `className`, `target`) and `@tailwindcss/forms` 0.5.11 (`strategy`, reads `theme('colors.…')`) |
| `src/engine/optimize.rs` | `optimize` of `@tailwindcss/node` (Lightning CSS `transform` twice with the worker's targets, `lightningcss` crate `=1.0.0-alpha.71` = npm `lightningcss@1.32.0`): minified output is byte-for-byte the worker's |
| `src/engine/mod.rs` | public API: `compile_tokens(tokens, custom_css, minify)` with a per-thread compiler cache |
| `src/scan/` | `@tailwindcss/oxide@4.3.3` `crates/oxide/src/extractor` (candidate machines + pre-processors for pug, haml, slim, vue, svelte, erb, twig, clojure, razor, markdown, json, elixir, rust, template-toolkit). Not upstream: memoized machine results on long lines (minified bundles, JSON blobs), where upstream is quadratic; the output is unchanged |
| `src/scanner.rs` | `scan_files_parallel`: candidates of template files (oxide's ignore rules for `node_modules`, lockfiles, binaries…) + concatenated CSS files |
| `src/css_resolve.rs` | not upstream: project CSS → custom CSS like a build sees it: entry stylesheets, inlined `@import "./x.css"` (`layer()`/`supports()`/media), bundled `tw-animate-css` (`src/tw/assets/vendor/`), `@source` / `@source not` / `source(none \| "dir")` for the scanner. `@import "tailwindcss" …` options are kept and moved into the base stylesheet's import by both engines (`hoist_tailwind_imports`, `hoistTailwindImports` in `worker/engine.ts`) |

## Verifying against real Tailwind

```sh
cargo test                                   # unit tests + recorded fixtures (tests/parity.rs)

cargo build --example tw_cli
bun worker/parity.ts                         # every IntelliSense class (~29k), one build each
bun worker/parity.ts --batch 200             # many classes per build (sorting)
bun worker/parity.ts --batch 200 --strict-minify   # minified output as shipped vs the worker's optimize()
bun worker/parity.ts --custom                # @theme/@utility/@apply/@custom-variant/theme() cases
bun worker/parity.ts --plugin-cases --plugins <dir>   # @plugin vs the JS plugins installed in <dir>
```

Candidate extraction against `@tailwindcss/oxide` (`Scanner#scanFiles`):

```sh
cargo build --example tw_cli
bun worker/scan-parity.ts                    # every template file of the repo, classes that produce CSS
bun worker/scan-parity.ts --raw              # every extracted candidate
bun worker/scan-parity.ts --fixtures --raw   # tests/fixtures/scan (one file per template language)
```

Regenerate fixtures after upgrading Tailwind: see the headers of `tests/parity.rs` and `tests/scan_parity.rs`.

Benchmarks: `cargo run --release --example tw_bench -- <classes.txt>`, `cargo run --release --example tw_scan_bench -- <dir>...` (slowest files: `tw_scan_prof`).

## Not supported

- `@config` (JavaScript configs) and `@plugin`s other than `@tailwindcss/typography` and `@tailwindcss/forms` are dropped with a warning (the Bun worker would load them from `node_modules`).
- `@import` of packages other than `tailwindcss` and `tw-animate-css` (and of project files that aren't in the scanned file list) is left in the output as-is, with a warning.
