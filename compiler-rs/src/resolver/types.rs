use std::path::PathBuf;
use serde::{Deserialize, Serialize};

/// Type of an imported module specifier in JavaScript/TypeScript
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImportKind {
    /// Server Action or Backend RPC module (e.g. `modules/news/use-cases/get-articles`)
    ServerAction(String),
    /// Path alias prefixed with `@/` or `~/` (e.g. `@/components/Button`)
    PathAlias(String),
    /// Relative path (e.g. `./Header`, `../ui/card`)
    Relative(String),
    /// Third-party NPM package (e.g. `lucide-react`, `framer-motion`, `@radix-ui/react-dialog`)
    NpmPackage(String),
    /// Direct stylesheet import (e.g. `./globals.css`, `@/styles/theme.css`)
    Css(String),
    /// Absolute HTTP/HTTPS or internal URL (e.g. `https://esm.sh/react`, `/_bundle/app/page`)
    Absolute(String),
}

/// Metadata and resolved path of a file located on disk
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedFile {
    /// Full absolute path to the file on disk
    pub absolute_path: PathBuf,
    /// Relative path from the project root
    pub relative_path: String,
    /// File extension without dot (e.g. `tsx`, `ts`, `jsx`, `js`)
    pub extension: String,
}
