use swc_core::common::{
    comments::NoopComments,
    errors::Handler,
    sync::Lrc,
    FileName, Globals, Mark, SourceMap, Spanned, GLOBALS,
};
use swc_core::ecma::{
    ast::Pass,
    codegen::{text_writer::JsWriter, Config as CodegenConfig, Emitter},
    parser::{lexer::Lexer, Parser, StringInput, Syntax, TsSyntax},
    transforms::{
        base::resolver,
        react::{react, Options as ReactOptions, Runtime},
        typescript::typescript,
    },
    visit::VisitMutWith,
};

/// A syntax error with its location, ready to be shown as a Next.js-style build error
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct CompileError {
    pub file: String,
    pub message: String,
    /// 1-based line/column of the first error, when the parser reported a span
    pub line: Option<usize>,
    pub column: Option<usize>,
    /// Code frame around the error (see `term::code_frame`), empty when no location is known
    pub frame: String,
}

impl CompileError {
    fn plain(file: &str, message: String) -> Self {
        Self { file: file.to_string(), message, line: None, column: None, frame: String::new() }
    }

    /// `./app/page.tsx:5:3`, the location header Next.js prints above build errors
    pub fn location(&self) -> String {
        match (self.line, self.column) {
            (Some(l), Some(c)) => format!("./{}:{}:{}", self.file, l, c),
            _ => format!("./{}", self.file),
        }
    }
}

impl std::fmt::Display for CompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.location(), self.message)
    }
}

/// High-performance SWC Native Compiler Engine
pub struct SwcCompiler;

impl SwcCompiler {
    /// Compiles TypeScript / TSX source code into browser-executable JavaScript
    pub fn compile(source_code: &str, file_name: &str) -> Result<String, CompileError> {
        let cm: Lrc<SourceMap> = Default::default();
        // Diagnostics are reported through `CompileError`, so SWC's own rustc-style output is discarded
        let handler = Handler::with_emitter_writer(Box::new(std::io::sink()), Some(cm.clone()));

        let fm = cm.new_source_file(
            FileName::Custom(file_name.to_string()).into(),
            source_code.to_string(),
        );

        let is_tsx = file_name.ends_with(".tsx") || file_name.ends_with(".jsx") || source_code.contains("</") || source_code.contains("/>");

        let syntax = Syntax::Typescript(TsSyntax {
            tsx: true,
            decorators: true,
            dts: false,
            no_early_errors: true,
            ..Default::default()
        });

        let lexer = Lexer::new(
            syntax,
            Default::default(),
            StringInput::from(&*fm),
            None,
        );

        let mut parser = Parser::new_from(lexer);

        for err in parser.take_errors() {
            err.into_diagnostic(&handler).emit();
        }

        let mut program = match parser.parse_program() {
            Ok(p) => p,
            Err(err) => {
                let pos = cm.lookup_char_pos(err.span().lo);
                let message = err.kind().msg().to_string();
                err.into_diagnostic(&handler).emit();
                let line = pos.line;
                let column = pos.col_display + 1;
                return Err(CompileError {
                    file: file_name.to_string(),
                    message,
                    line: Some(line),
                    column: Some(column),
                    frame: crate::term::code_frame(source_code, line, column, 2),
                });
            }
        };

        let globals = Globals::default();
        GLOBALS.set(&globals, || {
            let unresolved_mark = Mark::new();
            let top_level_mark = Mark::new();

            // 1. Resolve identifiers
            program.visit_mut_with(&mut resolver(unresolved_mark, top_level_mark, false));

            // 2. Transform TypeScript syntax
            let mut ts_pass = typescript(
                Default::default(),
                unresolved_mark,
                top_level_mark,
            );
            ts_pass.process(&mut program);

            // 3. Transform React JSX/TSX syntax
            if is_tsx {
                let react_options = ReactOptions {
                    runtime: Some(Runtime::Classic),
                    ..Default::default()
                };
                let mut react_pass = react::<NoopComments>(
                    cm.clone(),
                    None,
                    react_options,
                    top_level_mark,
                    unresolved_mark,
                );
                react_pass.process(&mut program);
            }

            // 4. Emit JavaScript code
            let mut buf = vec![];
            {
                let mut emitter = Emitter {
                    cfg: CodegenConfig::default().with_minify(false),
                    cm: cm.clone(),
                    comments: None,
                    wr: Box::new(JsWriter::new(cm.clone(), "\n", &mut buf, None)),
                };

                if let Err(e) = emitter.emit_program(&program) {
                    return Err(CompileError::plain(file_name, format!("SWC Codegen Error: {:?}", e)));
                }
            }

            String::from_utf8(buf).map_err(|e| CompileError::plain(file_name, format!("UTF-8 conversion error: {:?}", e)))
        })
    }
}
