use super::types::{RouteHandlerRequest, RouteHandlerResponse};
use super::worker::RouteHandlerWorker;
use std::path::Path;

/// Core Route Handler Engine Orchestrator
pub struct RouteHandlerEngine;

impl RouteHandlerEngine {
    /// Executes an App Router API endpoint (`route.ts` / `route.js`) with benchmarking and telemetry logging
    pub async fn execute<P: AsRef<Path>>(
        project_dir: P,
        request: RouteHandlerRequest,
    ) -> RouteHandlerResponse {
        let root = project_dir.as_ref();
        let method = request.method.clone();
        let path = request.path.clone();

        match RouteHandlerWorker::execute(root, &request).await {
            Ok(output) => {
                let status = output.status;
                if status < 400 {
                    println!(
                        "  \x1b[1;32m⚡ [API]\x1b[0m \x1b[1m{}\x1b[0m \x1b[1;36m{}\x1b[0m -> \x1b[1;32m{} {}\x1b[0m in \x1b[1;33m{:.2}ms\x1b[0m",
                        method, path, status, output.status_text, output.duration_ms
                    );
                } else if status < 500 {
                    println!(
                        "  \x1b[1;33m▲ [API]\x1b[0m \x1b[1m{}\x1b[0m \x1b[1;36m{}\x1b[0m -> \x1b[1;33m{} {}\x1b[0m in \x1b[1;33m{:.2}ms\x1b[0m",
                        method, path, status, output.status_text, output.duration_ms
                    );
                } else {
                    println!(
                        "  \x1b[1;31m✖ [API Error]\x1b[0m \x1b[1m{}\x1b[0m \x1b[1;36m{}\x1b[0m -> \x1b[1;31m{} {}\x1b[0m ({}ms): {}",
                        method, path, status, output.status_text, output.duration_ms, output.error.as_deref().unwrap_or("Unknown error")
                    );
                }
                output
            }
            Err(err) => {
                println!(
                    "  \x1b[1;31m✖ [API Execution Error]\x1b[0m \x1b[1m{}\x1b[0m \x1b[1;36m{}\x1b[0m: {}",
                    method, path, err
                );
                RouteHandlerResponse {
                    status: 500,
                    status_text: "Internal Server Error".to_string(),
                    headers: std::collections::HashMap::new(),
                    cookies: Vec::new(),
                    body_base64: String::new(),
                    duration_ms: 0.0,
                    error: Some(err),
                }
            }
        }
    }
}
