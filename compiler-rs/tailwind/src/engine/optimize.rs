//! `optimize` of `@tailwindcss/node` 4.3.3 (what the Bun worker ships when
//! minifying), on the same Lightning CSS version as the npm package.
//!
//! ```js
//! transform({ minify, drafts: { customMedia: true }, nonStandard: { deepSelectorCombinator: true },
//!   include: Features.Nesting | Features.MediaQueries,
//!   exclude: Features.LogicalProperties | Features.DirSelector | Features.LightDark,
//!   targets: { safari: 16.4, ios_saf: 16.4, firefox: 128, chrome: 111 }, errorRecovery: true })
//! ```
//!
//! run twice, then `@media not (` → `@media not all and (`.

use lightningcss::stylesheet::{MinifyOptions, ParserFlags, ParserOptions, PrinterOptions, StyleSheet};
use lightningcss::targets::{Browsers, Features, Targets};

fn targets() -> Targets {
    Targets {
        browsers: Some(Browsers {
            safari: Some((16 << 16) | (4 << 8)),
            ios_saf: Some((16 << 16) | (4 << 8)),
            firefox: Some(128 << 16),
            chrome: Some(111 << 16),
            ..Browsers::default()
        }),
        include: Features::Nesting | Features::MediaQueries,
        exclude: Features::LogicalProperties | Features::DirSelector | Features::LightDark,
    }
}

/// One Lightning CSS `transform` call.
fn transform(css: &str, minify: bool) -> Result<String, String> {
    let mut stylesheet = StyleSheet::parse(
        css,
        ParserOptions {
            filename: "input.css".into(),
            flags: ParserFlags::CUSTOM_MEDIA | ParserFlags::DEEP_SELECTOR_COMBINATOR,
            error_recovery: true,
            ..ParserOptions::default()
        },
    )
    .map_err(|e| e.to_string())?;

    let targets = targets();
    stylesheet
        .minify(MinifyOptions { targets, ..MinifyOptions::default() })
        .map_err(|e| e.to_string())?;
    let out = stylesheet
        .to_css(PrinterOptions { minify, targets, ..PrinterOptions::default() })
        .map_err(|e| e.to_string())?;
    Ok(out.code)
}

/// Optimize generated CSS exactly like the worker's `optimize(css, { minify })`.
pub fn optimize(css: &str, minify: bool) -> Result<String, String> {
    let once = transform(css, minify)?;
    let twice = transform(&once, minify)?;
    Ok(twice.replace("@media not (", "@media not all and ("))
}
