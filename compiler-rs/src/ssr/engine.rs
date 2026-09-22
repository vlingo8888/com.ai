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
                if output.mode == SsrMode::Full && !output.html.is_empty() {
                    println!(
                        "  \x1b[1;32m⚡ [SSR]\x1b[0m \x1b[1;36m{}\x1b[0m rendered in \x1b[1;33m{:.2}ms\x1b[0m",
                        path, output.render_time_ms
                    );
                } else if let Some(ref err) = output.error {
                    println!(
                        "  \x1b[1;33m▲ [SSR Fallback -> CSR]\x1b[0m \x1b[1;36m{}\x1b[0m ({}ms): {}",
                        path, output.render_time_ms, err
                    );
                }
                output
            }
            Err(err) => {
                println!(
                    "  \x1b[1;31m✖ [SSR Error -> Fallback CSR]\x1b[0m \x1b[1;36m{}\x1b[0m: {}",
                    path, err
                );
                SsrOutput {
                    html: String::new(),
                    initial_state: serde_json::json!({ "params": request.params, "searchParams": request.search_params }),
                    title: None,
                    metadata: None,
                    head_tags: None,
                    set_cookies: Vec::new(),
                    status_code: 200,
                    redirect_url: None,
                    render_time_ms: 0.0,
                    mode: SsrMode::ClientOnly,
                    error: Some(err),
                }
            }
        }
    }
}
