//! `@tailwindcss/typography` 0.5.20 (`@plugin "@tailwindcss/typography" { className: …; target: …; }`).
//!
//! Its styles are static data (built from `tailwindcss/colors`), so the plugin
//! API calls it makes are recorded by `worker/gen-plugins.ts` into
//! `assets/plugins/typography.json`, with `__CLASS__` standing for the
//! `className` option, and replayed here.

use super::{entries_from_json, OptionValue, PluginApi};
use indexmap::IndexMap;
use std::sync::LazyLock;

static DATA: LazyLock<serde_json::Value> =
    LazyLock::new(|| serde_json::from_str(include_str!("../assets/plugins/typography.json")).expect("typography.json"));

pub fn register(api: &mut PluginApi<'_>, options: Option<&IndexMap<String, OptionValue>>) -> Result<(), String> {
    let class_name = options.and_then(|o| o.get("className")).and_then(OptionValue::as_str).unwrap_or("prose").to_string();
    let target = match options.and_then(|o| o.get("target")).and_then(OptionValue::as_str) {
        Some("legacy") => "legacy",
        _ => "modern",
    };
    let fill = |s: &str| s.replace("__CLASS__", &class_name);

    for call in DATA[target].as_array().into_iter().flatten() {
        if let Some(name) = call["variant"].as_str() {
            let selectors: Vec<String> =
                call["selectors"].as_array().into_iter().flatten().filter_map(|s| s.as_str()).map(|s| fill(s)).collect();
            api.add_variant(&fill(name), &selectors);
        } else if call.get("utilities").is_some() {
            let entries: Vec<_> =
                entries_from_json(&call["utilities"]).into_iter().map(|(k, v)| (fill(&k), v.map_strings(&fill))).collect();
            api.add_utilities(&entries)?;
        } else if call.get("base").is_some() {
            let entries: Vec<_> =
                entries_from_json(&call["base"]).into_iter().map(|(k, v)| (fill(&k), v.map_strings(&fill))).collect();
            api.add_base(&entries)?;
        }
    }
    Ok(())
}
