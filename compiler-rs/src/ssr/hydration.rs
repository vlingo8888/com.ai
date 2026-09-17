/// Helper generator for client-side React hydration and initial state serialization
pub struct HydrationGenerator;

impl HydrationGenerator {
    /// Serializes initial server state into a safe JSON script block
    pub fn serialize_initial_state(state: &serde_json::Value) -> String {
        let json_str = serde_json::to_string(state).unwrap_or_else(|_| "{}".to_string());
        format!(
            r#"<script id="__NATA_SSR_DATA__" type="application/json">{}</script>"#,
            json_str.replace("</script>", "<\\/script>")
        )
    }

    /// Generates client hydration script
    pub fn generate_hydration_script() -> &'static str {
        r#"
      let ssrData = {};
      try {
        const dataEl = document.getElementById('__NATA_SSR_DATA__');
        if (dataEl && dataEl.textContent) {
          ssrData = JSON.parse(dataEl.textContent);
        }
      } catch (e) {
        console.warn('Could not parse SSR data:', e);
      }
      window.__INITIAL_DATA__ = ssrData;
        "#
    }
}
