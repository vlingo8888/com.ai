use super::types::{SsrMode, SsrOutput, SsrRequest};
use super::worker::SsrWorker;
use std::path::Path;

/// Core Server-Side Rendering Engine Orchestrator
pub struct SsrEngine;

impl SsrEngine {
    /// Renders an App Router route on the server into HTML markup, with automatic benchmarking and graceful CSR fallback
    pub async fn render<P: AsRef<Path>>(
        project_dir: P,
        request: SsrRequest,
    ) -> SsrOutput {
        let root = project_dir.as_ref();
        let path = request.path.clone();

        match SsrWorker::execute(root, &request).await {
            Ok(output) => {
                // Successful renders are covered by the request log line (` GET /path 200 in Xms`)
                if output.mode != SsrMode::Full || output.html.is_empty() {
                    if let Some(ref err) = output.error {
                        report_client_fallback(&path, &request, err);
                    }
                }
                output
            }
            Err(err) => {
                report_client_fallback(&path, &request, &err);
                SsrOutput {
                    error: Some(err),
                    ..SsrOutput::client_only(&request)
                }
            }
        }
    }
}

/// Missing React and async pages are known limitations reported once per page; errors thrown by user code are
/// printed on every request, like Next.js does for server render errors.
fn report_client_fallback(path: &str, request: &SsrRequest, err: &str) {
    if err.contains("could not be loaded for SSR") {
        let key = format!("{}|{}", request.page_file.display(), err);
        crate::term::warn_once(&key, &format!("Rendered {} on the client: {}", path, err));
    } else if err.contains("suspended while responding to synchronous input") || err.contains("NATA_ASYNC_SERVER_COMPONENT") {
        // An `async` page/layout: rendering it on the server needs streaming SSR + Server Components
        // (roadmap P1); until then the client renders it and fetches its data through Server Actions
        let key = format!("{}|async", request.page_file.display());
        crate::term::warn_once(&key, &format!("Rendered {} on the client: async Server Components are not server-rendered yet", path));
    } else {
        crate::term::error(&format!("Error rendering {} on the server (falling back to client rendering)", path), Some(err), None);
    }
}
