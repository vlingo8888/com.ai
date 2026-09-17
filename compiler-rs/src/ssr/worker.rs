use super::types::{SsrOutput, SsrRequest};
use std::path::Path;
use tokio::process::Command;

const SSR_DELIM_START: &str = "__NATA_SSR_OUT_START__";
const SSR_DELIM_END: &str = "__NATA_SSR_OUT_END__";

pub struct SsrWorker;

impl SsrWorker {
    /// Renders a matched App Router route server-side into HTML markup and state
    pub async fn execute<P: AsRef<Path>>(
        project_dir: P,
        request: &SsrRequest,
    ) -> Result<SsrOutput, String> {
        let root = project_dir.as_ref();
        let start_instant = std::time::Instant::now();

        // 1. Ensure runtime environment exists
        let root_canon = crate::rpc::dunce_canonicalize(root);
        let root = &root_canon;
        crate::rpc::RpcExecutor::ensure_runtime_env(root);

        // 2. Prepare path parameters and JSON strings
        let page_path = crate::rpc::clean_path(if request.page_file.is_absolute() {
            request.page_file.clone()
        } else {
            root.join(&request.page_file)
        });

        let layout_paths_vec: Vec<String> = request
            .layout_files
            .iter()
            .map(|p| {
                crate::rpc::clean_path(if p.is_absolute() {
                    p.clone()
                } else {
                    root.join(p)
                })
            })
            .collect();
        let layout_paths_json = serde_json::to_string(&layout_paths_vec).unwrap_or_else(|_| "[]".to_string());

        let params_json = serde_json::to_string(&request.params).unwrap_or_else(|_| "{}".to_string());
        let search_params_json = serde_json::to_string(&request.search_params).unwrap_or_else(|_| "{}".to_string());
        let cookies_str_json = serde_json::to_string(&request.cookies.clone().unwrap_or_default()).unwrap_or_else(|_| "\"\"".to_string());
        let headers_json = serde_json::to_string(&request.headers.clone().unwrap_or_else(|| serde_json::json!({}))).unwrap_or_else(|_| "{}".to_string());
        let url_path_json = serde_json::to_string(&request.path).unwrap_or_else(|_| "\"/\"".to_string());

        // 3. Construct in-memory invocation script
        let runner_script = format!(
            r#"
import {{ runSsr }} from "{runtime_path}";

await runSsr({{
  pagePath: "{page_path}",
  layoutPaths: {layout_paths},
  params: {params},
  searchParams: {search_params},
  cookiesStr: {cookies_str},
  headers: {headers},
  urlPath: {url_path}
}});
"#,
            runtime_path = crate::rpc::clean_path(root.join(".nata/ssr_runtime.ts")),
            page_path = page_path,
            layout_paths = layout_paths_json,
            params = params_json,
            search_params = search_params_json,
            cookies_str = cookies_str_json,
            headers = headers_json,
            url_path = url_path_json
        );

        // Write ssr_runtime.ts into .nata/
        let ssr_runtime_content = include_str!("runtime.ts");
        let nata_dir = root.join(".nata");
        let _ = std::fs::create_dir_all(&nata_dir);
        let _ = std::fs::write(nata_dir.join("ssr_runtime.ts"), ssr_runtime_content);

        // 4. Execute via Bun or Node runner
        let bun_bin = crate::rpc::find_bun_bin();
        let mut cmd = Command::new(&bun_bin);
        cmd.arg("-e").arg(&runner_script);
        cmd.current_dir(root);

        // Inject full PATH including ~/.bun/bin, /opt/homebrew/bin, /usr/local/bin
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
            Err(e) => {
                return Ok(SsrOutput {
                    html: String::new(),
                    initial_state: serde_json::json!({ "params": request.params, "searchParams": request.search_params }),
                    title: None,
                    set_cookies: Vec::new(),
                    status_code: 200,
                    redirect_url: None,
                    render_time_ms: start_instant.elapsed().as_secs_f64() * 1000.0,
                    mode: super::types::SsrMode::ClientOnly,
                    error: Some(format!("SSR runner process execution failed: {}", e)),
                });
            }
        };

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        // 5. Parse delimiter output
        if let Some(start_idx) = stdout.find(SSR_DELIM_START) {
            let rest = &stdout[start_idx + SSR_DELIM_START.len()..];
            if let Some(end_idx) = rest.find(SSR_DELIM_END) {
                let json_str = &rest[..end_idx];
                if let Ok(mut parsed_output) = serde_json::from_str::<SsrOutput>(json_str) {
                    parsed_output.render_time_ms = start_instant.elapsed().as_secs_f64() * 1000.0;
                    return Ok(parsed_output);
                }
            }
        }

        // Fallback if parsing failed
        Ok(SsrOutput {
            html: String::new(),
            initial_state: serde_json::json!({ "params": request.params, "searchParams": request.search_params }),
            title: None,
            set_cookies: Vec::new(),
            status_code: 200,
            redirect_url: None,
            render_time_ms: start_instant.elapsed().as_secs_f64() * 1000.0,
            mode: super::types::SsrMode::ClientOnly,
            error: Some(if !stderr.is_empty() { stderr.to_string() } else { "SSR delimiter not found".to_string() }),
        })
    }
}
