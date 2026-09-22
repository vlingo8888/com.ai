use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MetadataFileKind {
    Robots,
    Sitemap,
    Manifest,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetadataFileRequest {
    pub file_path: PathBuf,
    pub kind: MetadataFileKind,
    #[serde(default)]
    pub base_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetadataFileResponse {
    pub content: String,
    pub content_type: String,
    pub status: u16,
    pub error: Option<String>,
}

impl Default for MetadataFileResponse {
    fn default() -> Self {
        Self {
            content: String::new(),
            content_type: "text/plain; charset=utf-8".to_string(),
            status: 200,
            error: None,
        }
    }
}
