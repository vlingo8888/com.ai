#[cfg(test)]
mod tests {
    use super::super::types::{SsrMode, SsrOutput, SsrRequest};
    use super::super::hydration::HydrationGenerator;
    use std::path::PathBuf;

    #[test]
    fn test_ssr_types_default() {
        let default_output = SsrOutput::default();
        assert_eq!(default_output.status_code, 200);
        assert_eq!(default_output.mode, SsrMode::ClientOnly);
        assert!(default_output.html.is_empty());
    }

    #[test]
    fn test_hydration_serialize_initial_state() {
        let state = serde_json::json!({
            "params": { "id": "105" },
            "searchParams": { "tab": "settings" }
        });
        let script = HydrationGenerator::serialize_initial_state(&state);
        assert!(script.contains("id=\"__NATA_SSR_DATA__\""));
        assert!(script.contains("\"id\":\"105\""));
    }

    #[test]
    fn test_ssr_request_creation() {
        let req = SsrRequest {
            path: "/dashboard/users".to_string(),
            page_file: PathBuf::from("app/dashboard/users/page.tsx"),
            layout_files: vec![
                PathBuf::from("app/layout.tsx"),
                PathBuf::from("app/dashboard/layout.tsx"),
            ],
            params: serde_json::json!({}),
            search_params: serde_json::json!({ "page": 1 }),
            cookies: Some("token=abc".to_string()),
            headers: Some(serde_json::json!({ "user-agent": "test" })),
        };

        assert_eq!(req.path, "/dashboard/users");
        assert_eq!(req.layout_files.len(), 2);
    }
}
