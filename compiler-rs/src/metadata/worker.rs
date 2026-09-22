use super::types::{MetadataFileKind, MetadataFileRequest, MetadataFileResponse};
use std::path::Path;
use tokio::process::Command;

const META_DELIM_START: &str = "__NATA_META_OUT_START__";
const META_DELIM_END: &str = "__NATA_META_OUT_END__";

pub struct MetadataWorker;

impl MetadataWorker {
    pub async fn execute<P: AsRef<Path>>(
        project_dir: P,
        request: &MetadataFileRequest,
    ) -> Result<MetadataFileResponse, String> {
        let root = project_dir.as_ref();
        let root_canon = crate::rpc::dunce_canonicalize(root);
        let root = &root_canon;
        crate::rpc::RpcExecutor::ensure_runtime_env(root);

        let target_file_path = crate::rpc::clean_path(if request.file_path.is_absolute() {
            request.file_path.clone()
        } else {
            root.join(&request.file_path)
        });

        let runtime_content = include_str!("runtime.ts");
        let nata_dir = root.join(".nata");
        let _ = std::fs::create_dir_all(&nata_dir);
        let _ = std::fs::write(nata_dir.join("metadata_runtime.ts"), runtime_content);

        let kind_str = match request.kind {
            MetadataFileKind::Robots => "Robots",
            MetadataFileKind::Sitemap => "Sitemap",
            MetadataFileKind::Manifest => "Manifest",
        };

        let file_path_json = serde_json::to_string(&target_file_path).unwrap_or_else(|_| "\"\"".to_string());
        let kind_json = serde_json::to_string(kind_str).unwrap_or_else(|_| "\"Robots\"".to_string());
        let base_url_json = match &request.base_url {
            Some(u) => serde_json::to_string(u).unwrap_or_else(|_| "null".to_string()),
            None => "null".to_string(),
        };

        let runtime_path = crate::rpc::clean_path(nata_dir.join("metadata_runtime.ts"));

        let runner_script = format!(
            r#"
import {{ runMetadataHandler }} from "{runtime_path}";

await runMetadataHandler({{
  filePath: {file_path},
  kind: {kind},
  baseUrl: {base_url}
}});
"#,
            runtime_path = runtime_path,
            file_path = file_path_json,
            kind = kind_json,
            base_url = base_url_json,
        );

        let bun_bin = crate::rpc::find_bun_bin();
        let mut cmd = Command::new(&bun_bin);
        cmd.arg("-e").arg(&runner_script);
        cmd.current_dir(root);

        if let Ok(curr_path) = std::env::var("PATH") {
            let mut paths = vec![];
            if let Some(userprofile) = std::env::var_os("USERPROFILE") {
                paths.push(std::path::PathBuf::from(userprofile).join(".bun/bin").to_string_lossy().to_string());
            }
            if let Some(home) = std::env::var_os("HOME") {
                paths.push(std::path::PathBuf::from(home).join(".bun/bin").to_string_lossy().to_string());
            }
            #[cfg(unix)]
            {
                paths.push("/opt/homebrew/bin".to_string());
                paths.push("/usr/local/bin".to_string());
            }
            paths.push(curr_path);
            let sep = if cfg!(windows) { ";" } else { ":" };
            cmd.env("PATH", paths.join(sep));
        }

        let output = cmd.output().await.map_err(|e| format!("Failed to spawn metadata runner: {}", e))?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        if let (Some(start), Some(end)) = (stdout.find(META_DELIM_START), stdout.find(META_DELIM_END)) {
            let json_slice = &stdout[start + META_DELIM_START.len()..end];
            match serde_json::from_str::<MetadataFileResponse>(json_slice) {
                Ok(resp) => return Ok(resp),
                Err(e) => return Err(format!("Failed to parse metadata runner JSON: {} (raw: {})", e, json_slice)),
            }
        }

        if !output.status.success() {
            return Err(format!("Metadata runner failed: {}\nStderr: {}", stdout, stderr));
        }

        Err(format!("Metadata runner did not produce expected output delimiter. Stdout: {}", stdout))
    }
}
