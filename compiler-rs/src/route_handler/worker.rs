use super::types::{RouteHandlerRequest, RouteHandlerResponse};
use std::path::Path;
use tokio::process::Command;

const ROUTE_DELIM_START: &str = "__NATA_ROUTE_OUT_START__";
const ROUTE_DELIM_END: &str = "__NATA_ROUTE_OUT_END__";

pub struct RouteHandlerWorker;

impl RouteHandlerWorker {
    /// Executes a Next.js App Router route handler via Bun/Node runner
    pub async fn execute<P: AsRef<Path>>(
        project_dir: P,
        request: &RouteHandlerRequest,
    ) -> Result<RouteHandlerResponse, String> {
        let root = project_dir.as_ref();
        let start_instant = std::time::Instant::now();

        // 1. Ensure runtime environment exists
        let root_canon = crate::rpc::dunce_canonicalize(root);
        let root = &root_canon;
        crate::rpc::RpcExecutor::ensure_runtime_env(root);

        // 2. Prepare paths
        let handler_file_path = crate::rpc::clean_path(if request.file_path.is_absolute() {
            request.file_path.clone()
        } else {
            root.join(&request.file_path)
        });

        // 3. Write route_handler_runtime.ts into .nata/
        let runtime_content = include_str!("runtime.ts");
        let nata_dir = root.join(".nata");
        let _ = std::fs::create_dir_all(&nata_dir);
        let _ = std::fs::write(nata_dir.join("route_handler_runtime.ts"), runtime_content);

        // 4. Serialize input payload
        let file_path_json = serde_json::to_string(&handler_file_path).unwrap_or_else(|_| "\"\"".to_string());
        let url_json = serde_json::to_string(&request.url).unwrap_or_else(|_| "\"\"".to_string());
        let method_json = serde_json::to_string(&request.method).unwrap_or_else(|_| "\"GET\"".to_string());
        let params_json = serde_json::to_string(&request.params).unwrap_or_else(|_| "{}".to_string());
        let search_params_json = serde_json::to_string(&request.search_params).unwrap_or_else(|_| "{}".to_string());
        let headers_json = serde_json::to_string(&request.headers).unwrap_or_else(|_| "{}".to_string());
        let cookies_json = serde_json::to_string(&request.cookies.clone().unwrap_or_default()).unwrap_or_else(|_| "\"\"".to_string());
        let body_json = match &request.body_base64 {
            Some(b) => serde_json::to_string(b).unwrap_or_else(|_| "null".to_string()),
            None => "null".to_string(),
        };

        let runtime_path = crate::rpc::clean_path(nata_dir.join("route_handler_runtime.ts"));

        let runner_script = format!(
            r#"
import {{ runRouteHandler }} from "{runtime_path}";

await runRouteHandler({{
  filePath: {file_path},
  url: {url},
  method: {method},
  params: {params},
  searchParams: {search_params},
  headers: {headers},
  cookiesStr: {cookies},
  bodyBase64: {body}
}});
"#,
            runtime_path = runtime_path,
            file_path = file_path_json,
            url = url_json,
            method = method_json,
            params = params_json,
            search_params = search_params_json,
            headers = headers_json,
            cookies = cookies_json,
            body = body_json,
        );

        // 5. Execute via Bun or Node runner
        let bun_bin = crate::rpc::find_bun_bin();
        let mut cmd = Command::new(&bun_bin);
        cmd.arg("-e").arg(&runner_script);
        cmd.current_dir(root);

        // Inject full PATH
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

        // Inject project .env variables
        for env_file in &[".env", ".env.development", ".env.local"] {
            let p = root.join(env_file);
            if p.exists() {
                if let Ok(content) = std::fs::read_to_string(&p) {
                    for line in content.lines() {
                        let trimmed = line.trim();
                        if trimmed.is_empty() || trimmed.starts_with('#') || !trimmed.contains('=') {
                            continue;
                        }
                        if let Some((k, v)) = trimmed.split_once('=') {
                            let clean_val = v.trim().trim_matches('"').trim_matches('\'');
                            cmd.env(k.trim(), clean_val);
                        }
                    }
                }
            }
        }

        cmd.env("NODE_ENV", "development");

        let output = match cmd.output().await {
            Ok(o) => o,
            Err(_) => {
                // Fallback to node
                match Command::new("node")
                    .arg("--input-type=module")
                    .arg("-e")
                    .arg(&runner_script)
                    .current_dir(root)
                    .output()
                    .await
                {
                    Ok(o) => o,
                    Err(e) => {
                        let elapsed = start_instant.elapsed().as_secs_f64() * 1000.0;
                        return Ok(RouteHandlerResponse {
                            status: 500,
                            status_text: "Internal Server Error".to_string(),
                            headers: std::collections::HashMap::new(),
                            cookies: Vec::new(),
                            body_base64: String::new(),
                            duration_ms: elapsed,
                            error: Some(format!("Route runner process failed: {}", e)),
                        });
                    }
                }
            }
        };

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        // 6. Parse delimited output
        if let Some(start_idx) = stdout.find(ROUTE_DELIM_START) {
            let rest = &stdout[start_idx + ROUTE_DELIM_START.len()..];
            if let Some(end_idx) = rest.find(ROUTE_DELIM_END) {
                let json_str = &rest[..end_idx];
                if let Ok(mut parsed_output) = serde_json::from_str::<RouteHandlerResponse>(json_str) {
                    parsed_output.duration_ms = start_instant.elapsed().as_secs_f64() * 1000.0;
                    return Ok(parsed_output);
                }
            }
        }

        let elapsed = start_instant.elapsed().as_secs_f64() * 1000.0;
        let err_msg = if !stderr.is_empty() {
            stderr.to_string()
        } else {
            "Route handler output delimiter not found".to_string()
        };

        Ok(RouteHandlerResponse {
            status: 500,
            status_text: "Internal Server Error".to_string(),
            headers: std::collections::HashMap::new(),
            cookies: Vec::new(),
            body_base64: String::new(),
            duration_ms: elapsed,
            error: Some(err_msg),
        })
    }
}
