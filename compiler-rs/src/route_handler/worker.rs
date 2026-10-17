use super::types::{RouteHandlerRequest, RouteHandlerResponse};
use std::path::Path;

const ROUTE_DELIM_START: &str = "__NATA_ROUTE_OUT_START__";
const ROUTE_DELIM_END: &str = "__NATA_ROUTE_OUT_END__";

pub struct RouteHandlerWorker;

impl RouteHandlerWorker {
    /// Input of the worker's `route` command (`root` canonical)
    pub fn input_json(root: &Path, request: &RouteHandlerRequest) -> serde_json::Value {
        let handler_file_path = crate::rpc::clean_path(if request.file_path.is_absolute() {
            request.file_path.clone()
        } else {
            root.join(&request.file_path)
        });
        serde_json::json!({
            "filePath": handler_file_path,
            "moduleSpecifier": crate::module_graph::server_module_specifier(root, &handler_file_path),
            "url": request.url,
            "method": request.method,
            "params": request.params,
            "searchParams": request.search_params,
            "headers": request.headers,
            "cookiesStr": request.cookies.clone().unwrap_or_default(),
        })
    }

    /// Runs the route handler and streams its response (React Server Components mode)
    pub async fn stream(root: &Path, request: RouteHandlerRequest) -> axum::response::Response {
        let root = crate::rpc::dunce_canonicalize(root);
        crate::rpc::RpcExecutor::ensure_runtime_env(&root);
        let mut input = Self::input_json(&root, &request);
        input["stream"] = serde_json::Value::Bool(true);
        crate::rsc::stream_response(&root, "route", input, request.body).await
    }

    /// Executes a Next.js App Router route handler via Bun/Node runner
    pub async fn execute<P: AsRef<Path>>(
        project_dir: P,
        request: &RouteHandlerRequest,
    ) -> Result<RouteHandlerResponse, String> {
        let root = project_dir.as_ref();
        let start_instant = std::time::Instant::now();

        // 1. Ensure runtime environment exists
        let root_canon = crate::rpc::dunce_canonicalize(root);
        let root = &root_canon;
        crate::rpc::RpcExecutor::ensure_runtime_env(root);

        // 2. Input of the worker's `route` command
        let input = Self::input_json(root, request);
        let failed = |error: String| RouteHandlerResponse {
            status: 500,
            status_text: "Internal Server Error".to_string(),
            duration_ms: start_instant.elapsed().as_secs_f64() * 1000.0,
            error: Some(error),
            ..Default::default()
        };

        // 3. Run in the persistent Bun worker; bodies travel as raw bytes both ways
        match crate::worker_pool::call(root, "route", input.clone(), request.body.clone()).await {
            Ok(reply) => {
                crate::ssr::worker::print_logs(reply.stdout(), reply.stderr());
                if let Some(err) = reply.error() {
                    return Ok(failed(err.to_string()));
                }
                return Ok(match serde_json::from_value::<RouteHandlerResponse>(reply.output().clone()) {
                    Ok(mut output) => {
                        output.body = reply.body;
                        output.duration_ms = start_instant.elapsed().as_secs_f64() * 1000.0;
                        output
                    }
                    Err(e) => failed(format!("Invalid route handler output: {e}")),
                });
            }
            Err(crate::worker_pool::WorkerError::Failed(e)) => return Ok(failed(e)),
            Err(crate::worker_pool::WorkerError::Unavailable(_)) => {}
        }

        // 4. Fallback: one-off process, body as base64 inside the delimited JSON
        let mut fallback_input = input;
        if !request.body.is_empty() {
            fallback_input["bodyBase64"] = serde_json::Value::String(base64::Engine::encode(
                &base64::engine::general_purpose::STANDARD,
                &request.body,
            ));
        }
        let runner_script = format!(
            "import {{ runRouteHandler }} from {runtime};\nawait runRouteHandler({input});\n",
            runtime = serde_json::to_string(&crate::rpc::clean_path(root.join(".nata/route_handler_runtime.ts"))).unwrap_or_default(),
            input = fallback_input,
        );
        let output = match crate::worker_pool::spawn_once(root, &runner_script).await {
            Ok(o) => o,
            Err(e) => return Ok(failed(format!("Route runner process failed: {}", e))),
        };

        let stdout = output.stdout;
        let stderr = output.stderr;
        crate::ssr::worker::print_server_logs(&stdout, ROUTE_DELIM_START, ROUTE_DELIM_END);

        if let Some(start_idx) = stdout.find(ROUTE_DELIM_START) {
            let rest = &stdout[start_idx + ROUTE_DELIM_START.len()..];
            if let Some(end_idx) = rest.find(ROUTE_DELIM_END) {
                if let Ok(mut parsed_output) = serde_json::from_str::<RouteHandlerResponse>(&rest[..end_idx]) {
                    parsed_output.body = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &parsed_output.body_base64)
                        .unwrap_or_default();
                    parsed_output.body_base64.clear();
                    parsed_output.duration_ms = start_instant.elapsed().as_secs_f64() * 1000.0;
                    return Ok(parsed_output);
                }
            }
        }

        Ok(failed(if !stderr.is_empty() { stderr } else { "Route handler output delimiter not found".to_string() }))
    }
}
