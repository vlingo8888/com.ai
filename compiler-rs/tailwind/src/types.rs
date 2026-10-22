use serde::{Deserialize, Serialize};

/// A project file given to the scanner: its project-relative path and text
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileItem {
    pub path: String,
    pub content: String,
}
