//! Zalo Mini App app-config.json generator and validator

use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::Path,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZmpAppMeta {
    #[serde(default = "default_title")]
    pub title: String,
    #[serde(rename = "headerTitle", default = "default_title")]
    pub header_title: String,
    #[serde(rename = "headerColor", default = "default_header_color")]
    pub header_color: String,
    #[serde(rename = "textColor", default = "default_text_color")]
    pub text_color: String,
    #[serde(rename = "statusBar", default = "default_status_bar")]
    pub status_bar: String,
    #[serde(rename = "leftAction", default = "default_left_action")]
    pub left_action: String,
}

fn default_title() -> String {
    "Zalo Mini App".to_string()
}
fn default_header_color() -> String {
    "#0068FF".to_string()
}
fn default_text_color() -> String {
    "white".to_string()
}
fn default_status_bar() -> String {
    "transparent".to_string()
}
fn default_left_action() -> String {
    "onlyBack".to_string()
}

impl Default for ZmpAppMeta {
    fn default() -> Self {
        Self {
            title: default_title(),
            header_title: default_title(),
            header_color: default_header_color(),
            text_color: default_text_color(),
            status_bar: default_status_bar(),
            left_action: default_left_action(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZmpAppConfig {
    pub app: ZmpAppMeta,
    pub pages: Vec<String>,
    #[serde(rename = "listCSS", default)]
    pub list_css: Vec<String>,
    #[serde(rename = "listSyncJS", default)]
    pub list_sync_js: Vec<String>,
    #[serde(rename = "listAsyncJS", default)]
    pub list_async_js: Vec<String>,
}

impl Default for ZmpAppConfig {
    fn default() -> Self {
        Self {
            app: ZmpAppMeta::default(),
            pages: vec!["pages/index/index".to_string()],
            list_css: vec!["assets/app.css".to_string()],
            list_sync_js: vec!["assets/app.js".to_string()],
            list_async_js: Vec::new(),
        }
    }
}

pub struct ZmpConfigGenerator;

impl ZmpConfigGenerator {
    /// Detects whether the workspace is a Zalo Mini App project
    pub fn is_zalo_mini_app<P: AsRef<Path>>(root_dir: P) -> bool {
        let root = root_dir.as_ref();
        if root.join("app-config.json").exists() || root.join("zmp.json").exists() {
            return true;
        }
        let pkg_path = root.join("package.json");
        if pkg_path.exists() {
            if let Ok(content) = fs::read_to_string(&pkg_path) {
                if content.contains("zmp-sdk") || content.contains("zmp-ui") || content.contains("zmp-framework") {
                    return true;
                }
            }
        }
        false
    }

    /// Loads or generates a standard app-config.json file for the project
    pub fn ensure_app_config<P: AsRef<Path>>(root_dir: P) -> Result<ZmpAppConfig, String> {
        let root = root_dir.as_ref();
        let config_path = root.join("app-config.json");

        if config_path.exists() {
            if let Ok(content) = fs::read_to_string(&config_path) {
                if let Ok(cfg) = serde_json::from_str::<ZmpAppConfig>(&content) {
                    return Ok(cfg);
                }
            }
        }

        // Try extracting title from package.json
        let mut app_title = "Zalo Mini App".to_string();
        let pkg_path = root.join("package.json");
        if pkg_path.exists() {
            if let Ok(content) = fs::read_to_string(&pkg_path) {
                if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(name) = json_val.get("name").and_then(|n| n.as_str()) {
                        app_title = name.to_string();
                    }
                }
            }
        }

        let mut config = ZmpAppConfig::default();
        config.app.title = app_title.clone();
        config.app.header_title = app_title;

        // Write config
        if let Ok(json_str) = serde_json::to_string_pretty(&config) {
            let _ = fs::write(&config_path, json_str);
        }

        Ok(config)
    }
}
