use rayon::prelude::*;
use swc_core::common::{
    comments::NoopComments,
    sync::Lrc,
    Globals, Mark, SourceMap, GLOBALS,
};
use swc_core::ecma::{
    ast::Pass,
    codegen::{text_writer::JsWriter, Config as CodegenConfig, Emitter},
    transforms::{
        base::{
            fixer::fixer,
            helpers::{inject_helpers, Helpers, HELPERS},
            hygiene::hygiene,
            resolver,
        },
        module::{
            common_js::{common_js, FeatureFlag},
            path::Resolver,
        },
        react::{react, Options as ReactOptions, Runtime},
        typescript::typescript,
    },
    visit::VisitMutWith,
};

use super::engine::CompileError;

/// Files the cloud runtime evaluates; anything else has no `es5`
pub fn is_code_file(path: &str) -> bool {
    [".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs"].iter().any(|ext| path.ends_with(ext))
}

/// The `es5` of a file for the cloud runtime (`new Function` with `require`/`module`/`exports`):
/// CommonJS with classic `React.createElement` JSX and inline helpers, the same output as the web
/// editor's SWC compile.
pub fn compile_cjs(source: &str, file_name: &str) -> Result<String, CompileError> {
    let cm: Lrc<SourceMap> = Default::default();
    let mut program = super::module::parse(&cm, source, file_name)?;

    let globals = Globals::default();
    GLOBALS.set(&globals, || {
        HELPERS.set(&Helpers::new(false), || {
            let unresolved_mark = Mark::new();
            let top_level_mark = Mark::new();

            program.visit_mut_with(&mut resolver(unresolved_mark, top_level_mark, true));
            typescript(Default::default(), unresolved_mark, top_level_mark).process(&mut program);
            react::<NoopComments>(
                cm.clone(),
                None,
                ReactOptions { runtime: Some(Runtime::Classic), ..Default::default() },
                top_level_mark,
                unresolved_mark,
            )
            .process(&mut program);
            common_js(
                Resolver::Default,
                unresolved_mark,
                Default::default(),
                FeatureFlag { support_block_scoping: true, support_arrow: true },
            )
            .process(&mut program);
            inject_helpers(unresolved_mark).process(&mut program);
            hygiene().process(&mut program);
            fixer(None).process(&mut program);

            let mut buf = vec![];
            {
                let mut emitter = Emitter {
                    cfg: CodegenConfig::default().with_minify(false),
                    cm: cm.clone(),
                    comments: None,
                    wr: Box::new(JsWriter::new(cm.clone(), "\n", &mut buf, None)),
                };
                emitter
                    .emit_program(&program)
                    .map_err(|e| CompileError { file: file_name.to_string(), message: format!("SWC Codegen Error: {e:?}"), line: None, column: None, frame: String::new() })?;
            }
            String::from_utf8(buf).map_err(|e| CompileError { file: file_name.to_string(), message: format!("UTF-8 conversion error: {e:?}"), line: None, column: None, frame: String::new() })
        })
    })
}

#[derive(serde::Deserialize)]
pub struct Es5Input {
    pub path: String,
    pub content: String,
}

#[derive(serde::Serialize)]
pub struct Es5Output {
    pub path: String,
    pub es5: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Compiles every file on all cores; a file that fails gets an empty `es5` and its error
pub fn compile_all_cjs(files: Vec<Es5Input>) -> Vec<Es5Output> {
    files
        .into_par_iter()
        .map(|f| {
            if !is_code_file(&f.path) || f.content.is_empty() {
                return Es5Output { path: f.path, es5: String::new(), error: None };
            }
            match compile_cjs(&f.content, &f.path) {
                Ok(es5) => Es5Output { path: f.path, es5, error: None },
                Err(err) => Es5Output { error: Some(err.to_string()), path: f.path, es5: String::new() },
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_becomes_commonjs_without_imports() {
        let es5 = compile_cjs(
            "import type { ReactNode } from \"react\";\nimport \"./globals.css\";\nimport { Providers } from \"@/components/providers\";\nexport default function Layout({ children }: { children: ReactNode }) { return <Providers>{children}</Providers>; }",
            "app/layout.tsx",
        )
        .unwrap();
        assert!(!es5.lines().any(|l| l.trim_start().starts_with("import ")), "{es5}");
        assert!(es5.contains("require(\"./globals.css\")"), "{es5}");
        assert!(es5.contains("require(\"@/components/providers\")"), "{es5}");
        assert!(!es5.contains("require(\"react\")"), "type-only import must be dropped: {es5}");
        assert!(es5.contains("React.createElement"), "{es5}");
        assert!(es5.contains("exports"), "{es5}");
    }

    #[test]
    fn default_import_gets_inline_interop_helper() {
        let es5 = compile_cjs("import x from \"./x\";\nexport const y = x + 1;", "a.ts").unwrap();
        assert!(es5.contains("function _interop_require_default"), "{es5}");
        assert!(!es5.contains("@swc/helpers"), "{es5}");
    }

    #[test]
    fn broken_file_is_reported_and_others_compile() {
        let out = compile_all_cjs(vec![
            Es5Input { path: "ok.ts".into(), content: "export const a: number = 1;".into() },
            Es5Input { path: "bad.tsx".into(), content: "export const x = <div>;".into() },
            Es5Input { path: "app/globals.css".into(), content: "body{}".into() },
        ]);
        assert!(out[0].es5.contains("a = 1") && out[0].error.is_none());
        assert!(out[1].es5.is_empty() && out[1].error.is_some());
        assert!(out[2].es5.is_empty() && out[2].error.is_none());
    }
}
