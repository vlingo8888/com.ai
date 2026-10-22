//! Native Tailwind CSS v4 engine (public façade).
//!
//! The compiler itself is a port of tailwindcss@4.3.3 living in [`crate::tw`].
//! It is fed the same base stylesheet as the Bun worker (`worker/engine.ts`),
//! so the unminified output matches `compiler.build()` of the worker exactly.
//! The minified output goes through the same Lightning CSS `optimize` as the
//! worker's ([`optimize`]: vendor prefixes, lowered nesting and media ranges,
//! minification), so it matches the worker's minified output too.

pub mod optimize;

use crate::tw::index::{compile, worker_base_css, Compiler};
use crate::tw::lower::{lower, to_minified_css};
use lru::LruCache;
use std::cell::RefCell;
use std::num::NonZeroUsize;
use std::rc::Rc;

thread_local! {
    /// Parsing the theme/preflight and registering utilities is the expensive
    /// part, so compilers are cached per custom CSS (per thread, because the
    /// design system is not `Send`).
    static COMPILERS: RefCell<LruCache<u64, Rc<Compiler>>> =
        RefCell::new(LruCache::new(NonZeroUsize::new(32).unwrap()));
}

fn compiler_for(custom_css: &str) -> Result<Rc<Compiler>, String> {
    let key = xxhash_rust::xxh3::xxh3_64(custom_css.as_bytes());
    if let Some(c) = COMPILERS.with(|cache| cache.borrow_mut().get(&key).cloned()) {
        return Ok(c);
    }
    let compiler = Rc::new(compile(&worker_base_css(custom_css))?);
    COMPILERS.with(|cache| cache.borrow_mut().put(key, compiler.clone()));
    Ok(compiler)
}

thread_local! {
    /// Compilers of whole stylesheets ([`compile_stylesheet`]), by stylesheet
    static STYLESHEETS: RefCell<LruCache<u64, Rc<Compiler>>> = RefCell::new(LruCache::new(NonZeroUsize::new(4).unwrap()));
}

/// Builds a project stylesheet that imports Tailwind itself (`@import "tailwindcss";`), like
/// `compile(css).build(tokens)` then `optimize` of `@tailwindcss/node`, without the worker's base
/// stylesheet. Added for com.ai.vn (Next.js projects have no default theme beyond Tailwind's).
pub fn compile_stylesheet(css: &str, tokens: &[String], minify: bool) -> Result<String, String> {
    let key = xxhash_rust::xxh3::xxh3_64(css.as_bytes());
    let compiler = match STYLESHEETS.with(|cache| cache.borrow_mut().get(&key).cloned()) {
        Some(c) => c,
        None => {
            let compiler = Rc::new(compile(&crate::tw::index::hoist_tailwind_imports(css))?);
            STYLESHEETS.with(|cache| cache.borrow_mut().put(key, compiler.clone()));
            compiler
        }
    };
    let built = compiler.build(tokens);
    optimize::optimize(&built, minify).or_else(|err| {
        tracing::warn!("tailwind: Lightning CSS failed to optimize ({err}); using native lowering");
        Ok(to_minified_css(&lower(compiler.build_ast(tokens), None)))
    })
}

/// Compile `tokens` (candidate class names) into CSS.
///
/// Errors in `custom_css` (invalid `@apply`, `@utility`, …) are reported.
pub fn try_compile_tokens(tokens: &[String], custom_css: &str, minify: bool) -> Result<String, String> {
    let compiler = compiler_for(custom_css)?;
    Ok(render(&compiler, tokens, minify))
}

fn render(compiler: &Compiler, tokens: &[String], minify: bool) -> String {
    if !minify {
        return compiler.build(tokens);
    }
    let css = compiler.build(tokens);
    match optimize::optimize(&css, true) {
        Ok(min) => min,
        Err(err) => {
            // Lightning CSS recovers from almost everything; keep the native
            // lowering as a safety net.
            tracing::warn!("tailwind: Lightning CSS failed to optimize ({err}); using native minifier");
            to_minified_css(&lower(compiler.build_ast(tokens), None))
        }
    }
}

/// Compile `tokens` into CSS. Never fails: when `custom_css` cannot be
/// compiled, the utilities are generated without it and the custom CSS is
/// appended verbatim.
pub fn compile_tokens(tokens: &[String], custom_css: &str, minify: bool) -> String {
    match try_compile_tokens(tokens, custom_css, minify) {
        Ok(css) => css,
        Err(err) => {
            tracing::warn!("tailwind: custom CSS failed to compile ({err}); falling back to base theme");
            let mut css = match compiler_for("") {
                Ok(c) => render(&c, tokens, minify),
                Err(_) => String::new(),
            };
            css.push('\n');
            css.push_str(custom_css);
            css
        }
    }
}

/// Simple whitespace/comment minifier for already generated CSS.
pub fn minify_css(css: &str) -> String {
    let ast = match crate::tw::css_parser::parse(css) {
        Ok(ast) => ast,
        Err(_) => return css.to_string(),
    };
    to_minified_css(&ast)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compiles_a_project_stylesheet_without_the_worker_theme() {
        let css = "@import \"tailwindcss\";\n@theme { --color-brand: #123456; }\n.card { @apply p-4; }";
        let out = compile_stylesheet(css, &tokens(&["bg-brand", "flex", "bg-primary-500"]), false).unwrap();
        assert!(out.contains(".bg-brand"), "{out}");
        assert!(out.contains(".flex"));
        assert!(out.contains(".card"));
        // `primary` is a color of the worker's base stylesheet, not of Tailwind
        assert!(!out.contains("bg-primary-500"));
        let min = compile_stylesheet(css, &tokens(&["flex"]), true).unwrap();
        assert!(min.contains(".flex{display:flex}"), "{min}");
        assert!(compile_stylesheet("@import \"tailwindcss\"; .x { @apply not-a-class; }", &[], false).is_err());
    }

    fn tokens(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn test_user_snippet_classes() {
        let list = tokens(&[
            "flex", "flex-col", "w-[390px]", "min-h-[844px]", "bg-[#F4F1EC]", "text-[#12272B]", "font-sans", "p-0",
            "gap-0", "overflow-hidden", "items-center", "text-center", "pt-10", "pb-2", "uppercase", "mb-1",
            "text-[#8B806B]", "w-8", "h-px", "bg-[#BBAD91]", "mt-4", "opacity-60", "w-full", "max-w-[390px]", "px-5",
            "py-8", "bg-white", "border", "border-[#DDD8D0]", "rounded-[16px]", "p-6", "justify-center", "border-b",
            "mb-8", "pb-3", "px-6", "border-b-2", "text-[#5B2823]", "border-[#5B2823]", "text-[#8B806B]/60",
            "border-transparent", "gap-5", "-mt-16", "w-[68px]", "h-[68px]", "rounded-full", "leading-[1.4]",
            "flex-shrink-0", "flex-grow", "mt-2.5", "italic", "truncate", "inline-flex",
        ]);
        let css = compile_tokens(&list, "", false);
        assert!(css.contains(".text-\\[\\#8B806B\\]\\/60 {\n    color: color-mix(in oklab, #8B806B 60%, transparent);\n  }"), "{css}");
        assert!(css.contains(".-mt-16 {\n    margin-top: calc(var(--spacing) * -16);\n  }"));
        assert!(css.contains(".leading-\\[1\\.4\\] {\n    --tw-leading: 1.4;\n    line-height: 1.4;\n  }"));
        assert!(css.contains(".flex-shrink-0 {\n    flex-shrink: 0;\n  }"));
        assert!(css.contains(".flex-grow {\n    flex-grow: 1;\n  }"));
        assert!(css.contains(".rounded-\\[16px\\] {\n    border-radius: 16px;\n  }"));
        assert!(css.contains("--spacing: 0.25rem;"));

        let min = compile_tokens(&list, "", true);
        assert!(min.contains(".-mt-16{margin-top:calc(var(--spacing) * -16)}"), "{min}");
        assert!(!min.contains('&'));
    }

    #[test]
    fn variants_are_lowered_when_minified() {
        let css = compile_tokens(&tokens(&["hover:bg-red-500", "dark:text-primary", "md:flex"]), "", true);
        assert!(css.contains("@media (hover:hover){.hover\\:bg-red-500:hover{background-color:var(--color-red-500)}}"), "{css}");
        assert!(css.contains(".dark\\:text-primary:where(.dark,.dark *){color:var(--primary)}"), "{css}");
        // Media ranges are lowered for the worker's browser targets
        assert!(css.contains("@media (min-width:48rem){.md\\:flex{display:flex}}"), "{css}");
    }

    #[test]
    fn minified_output_has_vendor_prefixes() {
        let css = compile_tokens(&tokens(&["bg-clip-text", "mask-linear-from-20%", "backdrop-blur-md", "select-none"]), "", true);
        assert!(css.contains(".bg-clip-text{-webkit-background-clip:text;background-clip:text}"), "{css}");
        assert!(css.contains("-webkit-mask-composite:source-in;mask-composite:intersect"), "{css}");
        assert!(css.contains("-webkit-backdrop-filter:"), "{css}");
        assert!(css.contains("-webkit-user-select:none;user-select:none"), "{css}");
    }

    #[test]
    fn custom_css_directives() {
        let custom = "@theme { --color-brand: #123456; }\n@utility tab-* { tab-size: --value(integer); }\n.btn { @apply px-4 py-2 bg-brand; }";
        let css = try_compile_tokens(&tokens(&["bg-brand", "tab-4"]), custom, false).unwrap();
        assert!(css.contains("--color-brand: #123456;"), "{css}");
        assert!(css.contains(".tab-4 {\n    tab-size: 4;\n  }"), "{css}");
        assert!(css.contains(".btn {\n  background-color: var(--color-brand);\n  padding-inline: calc(var(--spacing) * 4);"), "{css}");
    }

    #[test]
    fn broken_custom_css_falls_back() {
        let css = compile_tokens(&tokens(&["p-4"]), ".x { @apply does-not-exist; }", true);
        assert!(css.contains(".p-4{padding:calc(var(--spacing) * 4)}"), "{css}");
    }
}
