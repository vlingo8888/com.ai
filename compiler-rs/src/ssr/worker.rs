use super::types::{SsrOutput, SsrRequest};
use std::path::Path;

const SSR_DELIM_START: &str = "__NATA_SSR_OUT_START__";
const SSR_DELIM_END: &str = "__NATA_SSR_OUT_END__";

pub struct SsrWorker;

impl SsrWorker {
    /// Renders a matched App Router route server-side into HTML markup and state
    pub async fn execute<P: AsRef<Path>>(
        project_dir: P,
        request: &SsrRequest,
    ) -> Result<SsrOutput, String> {
        let root = project_dir.as_ref();
        let start_instant = std::time::Instant::now();

        // 1. Ensure runtime environment exists
        let root_canon = crate::rpc::dunce_canonicalize(root);
        let root = &root_canon;
        crate::rpc::RpcExecutor::ensure_runtime_env(root);

        // 2. Prepare path parameters and JSON strings
        let page_path = crate::rpc::clean_path(if request.page_file.is_absolute() {
            request.page_file.clone()
        } else {
            root.join(&request.page_file)
        });

        let layout_paths_vec: Vec<String> = request
            .layout_files
            .iter()
            .map(|p| {
                crate::rpc::clean_path(if p.is_absolute() {
                    p.clone()
                } else {
                    root.join(p)
                })
            })
            .collect();

        let segments_files_clean: Vec<serde_json::Value> = request
            .segments_files
            .iter()
            .map(|s| {
                let clean_opt = |opt: &Option<std::path::PathBuf>| -> serde_json::Value {
                    match opt {
                        Some(p) => serde_json::Value::String(crate::rpc::clean_path(if p.is_absolute() { p.clone() } else { root.join(p) })),
                        None => serde_json::Value::Null,
                    }
                };
                serde_json::json!({
                    "folder": crate::rpc::clean_path(if s.folder.is_absolute() { s.folder.clone() } else { root.join(&s.folder) }),
                    "layout": clean_opt(&s.layout),
                    "template": clean_opt(&s.template),
                    "error": clean_opt(&s.error),
                    "loading": clean_opt(&s.loading),
                    "not_found": clean_opt(&s.not_found),
                })
            })
            .collect();
        let global_error_path = request
            .global_error_file
            .as_ref()
            .map(|p| crate::rpc::clean_path(if p.is_absolute() { p.clone() } else { root.join(p) }));
        let input = serde_json::json!({
            "pagePath": page_path,
            "layoutPaths": layout_paths_vec,
            "segmentsFiles": segments_files_clean,
            "globalErrorPath": global_error_path,
            "params": request.params,
            "searchParams": request.search_params,
            "cookiesStr": request.cookies.clone().unwrap_or_default(),
            "headers": request.headers.clone().unwrap_or_else(|| serde_json::json!({})),
            "urlPath": request.path,
        });
        let failed = |error: String| SsrOutput {
            render_time_ms: start_instant.elapsed().as_secs_f64() * 1000.0,
            error: Some(error),
            ..SsrOutput::client_only(request)
        };

        // 3. Render in the persistent Bun worker
        match crate::worker_pool::call(root, "ssr", input.clone(), Vec::new()).await {
            Ok(reply) => {
                print_logs(reply.stdout(), reply.stderr());
                if let Some(err) = reply.error() {
                    return Ok(failed(err.to_string()));
                }
                return Ok(match serde_json::from_value::<SsrOutput>(reply.output().clone()) {
                    Ok(mut output) => {
                        output.render_time_ms = start_instant.elapsed().as_secs_f64() * 1000.0;
                        output
                    }
                    Err(e) => failed(format!("Invalid SSR output: {e}")),
                });
            }
            Err(crate::worker_pool::WorkerError::Failed(e)) => return Ok(failed(e)),
            Err(crate::worker_pool::WorkerError::Unavailable(_)) => {}
        }

        // 4. Fallback: one-off process printing the result between delimiters
        let runner_script = format!(
            "import {{ runSsr }} from {runtime};\nawait runSsr({input});\n",
            runtime = serde_json::to_string(&crate::rpc::clean_path(root.join(".nata/ssr_runtime.ts"))).unwrap_or_default(),
            input = input,
        );
        let output = match crate::worker_pool::spawn_once(root, &runner_script).await {
            Ok(o) => o,
            Err(e) => return Ok(failed(format!("SSR runner process execution failed: {}", e))),
        };

        let stdout = output.stdout;
        let stderr = output.stderr;
        print_server_logs(&stdout, SSR_DELIM_START, SSR_DELIM_END);

        if let Some(start_idx) = stdout.find(SSR_DELIM_START) {
            let rest = &stdout[start_idx + SSR_DELIM_START.len()..];
            if let Some(end_idx) = rest.find(SSR_DELIM_END) {
                if let Ok(mut parsed_output) = serde_json::from_str::<SsrOutput>(&rest[..end_idx]) {
                    parsed_output.render_time_ms = start_instant.elapsed().as_secs_f64() * 1000.0;
                    return Ok(parsed_output);
                }
            }
        }

        Ok(failed(if !stderr.is_empty() { stderr } else { "SSR delimiter not found".to_string() }))
    }
}

/// Prints what server code logged while handling a request, like `console.log` in Next.js server code
pub(crate) fn print_logs(stdout: &str, stderr: &str) {
    let out = stdout.trim_end();
    if !out.trim().is_empty() {
        println!("{out}");
    }
    let err = stderr.trim_end();
    if !err.trim().is_empty() {
        eprintln!("{err}");
    }
}

/// Prints what server code logged during the render (everything but the delimited result), like the
/// terminal output of `console.log` in a Next.js server component
pub(crate) fn print_server_logs(stdout: &str, start: &str, end: &str) {
    let logs = match (stdout.find(start), stdout.find(end)) {
        (Some(s), Some(e)) if e > s => format!("{}{}", &stdout[..s], &stdout[e + end.len()..]),
        _ => stdout.to_string(),
    };
    let logs = logs.trim_end();
    if !logs.trim().is_empty() {
        println!("{logs}");
    }
}
