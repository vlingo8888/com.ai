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

/// The module graph layer a file is compiled for: the browser, server-side rendering of client
/// components, or React Server Components (`react-server` condition)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Layer {
    Client,
    Ssr,
    Rsc,
}

impl Layer {
    pub fn as_str(self) -> &'static str {
        match self {
            Layer::Client => "client",
            Layer::Ssr => "ssr",
            Layer::Rsc => "rsc",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "client" => Some(Layer::Client),
            "ssr" => Some(Layer::Ssr),
            "rsc" => Some(Layer::Rsc),
            _ => None,
        }
    }
}
