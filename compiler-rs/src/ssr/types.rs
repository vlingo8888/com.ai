use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Execution mode for Server-Side Rendering
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SsrMode {
    /// Full SSR with Client Hydration
    Full,
    /// Fast static pre-rendering
    PreRender,
    /// Fallback to Client-Side Rendering
    ClientOnly,
}

impl Default for SsrMode {
    fn default() -> Self {
        Self::Full
    }
}

/// Request payload sent to the SSR Engine
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SsrRequest {
    /// Matched route path (e.g., "/dashboard/classes")
    pub path: String,
    /// Absolute or relative path to the target page file
    pub page_file: PathBuf,
    /// List of cascade layout files from root to leaf
    pub layout_files: Vec<PathBuf>,
    /// Hierarchical segment special files from root to leaf
    #[serde(default)]
    pub segments_files: Vec<crate::router::SegmentFiles>,
    /// Root level global error file (if any)
    #[serde(default)]
    pub global_error_file: Option<PathBuf>,
    /// Extracted dynamic route parameters (e.g., {"id": "105"})
    pub params: serde_json::Value,
    /// URL search query parameters (e.g., {"tab": "overview"})
    pub search_params: serde_json::Value,
    /// Incoming raw cookie string (e.g., "token=xxx; theme=dark")
    pub cookies: Option<String>,
    /// Incoming HTTP request headers as key-value pairs
    pub headers: Option<serde_json::Value>,
}

/// Result produced by the SSR Engine
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SsrOutput {
    /// Rendered HTML markup inside `<div id="root">`
    pub html: String,
    /// Initial serialized state/props for client hydration
    #[serde(default)]
    pub initial_state: serde_json::Value,
    /// Page title extracted from React Helmet / metadata (if any)
    pub title: Option<String>,
    /// Any cookies set during SSR execution (e.g. from cookies().set())
    #[serde(default)]
    pub set_cookies: Vec<crate::rpc::SetCookieInfo>,
    /// HTTP status code (200, 404, 500, or 307 for redirect)
    pub status_code: u16,
    /// Optional redirect URL if `redirect()` was called during SSR
    pub redirect_url: Option<String>,
    /// Total SSR render duration in milliseconds
    pub render_time_ms: f64,
    /// Execution mode used
    pub mode: SsrMode,
    /// Optional error message if SSR encountered issues and fell back
    pub error: Option<String>,
}

impl Default for SsrOutput {
    fn default() -> Self {
        Self {
            html: String::new(),
            initial_state: serde_json::json!({}),
            title: None,
            set_cookies: Vec::new(),
            status_code: 200,
            redirect_url: None,
            render_time_ms: 0.0,
            mode: SsrMode::ClientOnly,
            error: None,
        }
    }
}
