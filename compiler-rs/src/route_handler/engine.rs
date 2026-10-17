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
                // The request line itself is printed by the dev server's request log middleware
                if output.status >= 500 {
                    crate::term::error(
                        &format!("{} {} failed with {} {}", method, path, output.status, output.status_text),
                        output.error.as_deref(),
                        None,
                    );
                }
                output
            }
            Err(err) => {
                crate::term::error(&format!("Route handler {} {} could not run", method, path), Some(&err), None);
                RouteHandlerResponse {
                    status: 500,
                    status_text: "Internal Server Error".to_string(),
                    error: Some(err),
                    ..Default::default()
                }
            }
        }
    }
}
