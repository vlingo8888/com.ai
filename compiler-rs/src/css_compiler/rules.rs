use std::collections::HashSet;

/// Core Tailwind utility rules generator and CSS preflight builder
pub struct RuleGenerator;

impl RuleGenerator {
    /// Generates modern CSS Reset & Preflight styles
    pub fn generate_preflight() -> &'static str {
        r#"
*, ::before, ::after {
  box-sizing: border-box;
  border-width: 0;
  border-style: solid;
  border-color: #e5e7eb;
}
html {
  line-height: 1.5;
  -webkit-text-size-adjust: 100%;
  -moz-tab-size: 4;
  tab-size: 4;
  font-family: ui-sans-serif, system-ui, sans-serif, "Apple Color Emoji", "Segoe UI Emoji", "Segoe UI Symbol", "Noto Color Emoji";
  font-feature-settings: normal;
  font-variation-settings: normal;
  -webkit-tap-highlight-color: transparent;
}
body {
  margin: 0;
  line-height: inherit;
}
hr {
  height: 0;
  color: inherit;
  border-top-width: 1px;
}
abbr:where([title]) {
  text-decoration: underline dotted;
}
h1, h2, h3, h4, h5, h6 {
  font-size: inherit;
  font-weight: inherit;
  margin: 0;
}
a {
  color: inherit;
  text-decoration: inherit;
}
b, strong {
  font-weight: bolder;
}
code, kbd, samp, pre {
  font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", "Courier New", monospace;
  font-feature-settings: normal;
  font-variation-settings: normal;
  font-size: 1em;
}
small {
  font-size: 80%;
}
sub, sup {
  font-size: 75%;
  line-height: 0;
  position: relative;
  vertical-align: baseline;
}
sub { bottom: -0.25em; }
sup { top: -0.5em; }
table {
  text-indent: 0;
  border-color: inherit;
  border-collapse: collapse;
}
button, input, optgroup, select, textarea {
  font-family: inherit;
  font-feature-settings: inherit;
  font-variation-settings: inherit;
  font-size: 100%;
  font-weight: inherit;
  line-height: inherit;
  letter-spacing: inherit;
  color: inherit;
  margin: 0;
  padding: 0;
}
button, select {
  text-transform: none;
}
button, input:where([type='button']), input:where([type='reset']), input:where([type='submit']) {
  -webkit-appearance: button;
  background-color: transparent;
  background-image: none;
}
:-moz-focusring { outline: auto; }
:-moz-ui-invalid { box-shadow: none; }
progress { vertical-align: baseline; }
::-webkit-inner-spin-button, ::-webkit-outer-spin-button { height: auto; }
[type='search'] { -webkit-appearance: textfield; outline-offset: -2px; }
::-webkit-search-decoration { -webkit-appearance: none; }
::-webkit-file-upload-button { -webkit-appearance: button; font: inherit; }
summary { display: list-item; }
blockquote, dl, dd, h1, h2, h3, h4, h5, h6, hr, figure, p, pre { margin: 0; }
fieldset { margin: 0; padding: 0; }
legend { padding: 0; }
ol, ul, menu { list-style: none; margin: 0; padding: 0; }
dialog { padding: 0; }
textarea { resize: vertical; }
input::placeholder, textarea::placeholder { opacity: 1; color: #9ca3af; }
button, [role="button"] { cursor: pointer; }
:disabled { cursor: default; }
img, svg, video, canvas, audio, iframe, embed, object { display: block; vertical-align: middle; }
img, video { max-width: 100%; height: auto; }
[hidden] { display: none; }
"#
    }

    /// Generates dynamic CSS rules for extracted utility class tokens
    pub fn generate_rules(classes: &HashSet<String>) -> String {
        let mut css = String::new();
        let mut sm_rules = Vec::new();
        let mut md_rules = Vec::new();
        let mut lg_rules = Vec::new();
        let mut xl_rules = Vec::new();
        let mut dark_rules = Vec::new();

        for class in classes {
            if let Some((selector, declaration)) = Self::resolve_utility(class) {
                if class.starts_with("sm:") {
                    sm_rules.push(format!("  {} {{ {} }}", selector, declaration));
                } else if class.starts_with("md:") {
                    md_rules.push(format!("  {} {{ {} }}", selector, declaration));
                } else if class.starts_with("lg:") {
                    lg_rules.push(format!("  {} {{ {} }}", selector, declaration));
                } else if class.starts_with("xl:") {
                    xl_rules.push(format!("  {} {{ {} }}", selector, declaration));
                } else if class.starts_with("dark:") {
                    dark_rules.push(format!("html.dark {} {{ {} }}", selector, declaration));
                } else {
                    css.push_str(&format!("{} {{ {} }}\n", selector, declaration));
                }
            }
        }

        if !dark_rules.is_empty() {
            css.push_str(&dark_rules.join("\n"));
            css.push('\n');
        }

        if !sm_rules.is_empty() {
            css.push_str("@media (min-width: 640px) {\n");
            css.push_str(&sm_rules.join("\n"));
            css.push_str("\n}\n");
        }

        if !md_rules.is_empty() {
            css.push_str("@media (min-width: 768px) {\n");
            css.push_str(&md_rules.join("\n"));
            css.push_str("\n}\n");
        }

        if !lg_rules.is_empty() {
            css.push_str("@media (min-width: 1024px) {\n");
            css.push_str(&lg_rules.join("\n"));
            css.push_str("\n}\n");
        }

        if !xl_rules.is_empty() {
            css.push_str("@media (min-width: 1280px) {\n");
            css.push_str(&xl_rules.join("\n"));
            css.push_str("\n}\n");
        }

        css
    }

    /// Escapes a Tailwind class string for use in CSS selectors
    pub fn escape_selector(class: &str) -> String {
        let mut escaped = String::from(".");
        for c in class.chars() {
            match c {
                ':' => escaped.push_str("\\:"),
                '/' => escaped.push_str("\\/"),
                '[' => escaped.push_str("\\["),
                ']' => escaped.push_str("\\]"),
                '#' => escaped.push_str("\\#"),
                '%' => escaped.push_str("\\%"),
                '.' => escaped.push_str("\\."),
                '(' => escaped.push_str("\\("),
                ')' => escaped.push_str("\\)"),
                ',' => escaped.push_str("\\,"),
                '!' => escaped.push_str("\\!"),
                _ => escaped.push(c),
            }
        }
        escaped
    }

    /// Resolves a single utility class into `(CSS_Selector, CSS_Declaration)`
    pub fn resolve_utility(class: &str) -> Option<(String, String)> {
        let clean_class = class
            .trim_start_matches("sm:")
            .trim_start_matches("md:")
            .trim_start_matches("lg:")
            .trim_start_matches("xl:")
            .trim_start_matches("2xl:")
            .trim_start_matches("dark:");

        let is_hover = clean_class.starts_with("hover:");
        let is_focus = clean_class.starts_with("focus:");
        let is_active = clean_class.starts_with("active:");

        let base_class = if is_hover {
            clean_class.trim_start_matches("hover:")
        } else if is_focus {
            clean_class.trim_start_matches("focus:")
        } else if is_active {
            clean_class.trim_start_matches("active:")
        } else {
            clean_class
        };

        let mut selector = Self::escape_selector(class);
        if is_hover {
            selector.push_str(":hover");
        } else if is_focus {
            selector.push_str(":focus");
        } else if is_active {
            selector.push_str(":active");
        }

        let decl = Self::match_declaration(base_class)?;
        Some((selector, decl))
    }

    fn match_declaration(cls: &str) -> Option<String> {
        // 1. Static Layout & Display
        match cls {
            "flex" => return Some("display: flex;".into()),
            "inline-flex" => return Some("display: inline-flex;".into()),
            "grid" => return Some("display: grid;".into()),
            "block" => return Some("display: block;".into()),
            "inline-block" => return Some("display: inline-block;".into()),
            "inline" => return Some("display: inline;".into()),
            "hidden" => return Some("display: none;".into()),

            // Flexbox
            "flex-col" => return Some("flex-direction: column;".into()),
            "flex-row" => return Some("flex-direction: row;".into()),
            "flex-wrap" => return Some("flex-wrap: wrap;".into()),
            "flex-1" => return Some("flex: 1 1 0%;".into()),
            "items-center" => return Some("align-items: center;".into()),
            "items-start" => return Some("align-items: flex-start;".into()),
            "items-end" => return Some("align-items: flex-end;".into()),
            "justify-center" => return Some("justify-content: center;".into()),
            "justify-between" => return Some("justify-content: space-between;".into()),
            "justify-start" => return Some("justify-content: flex-start;".into()),
            "justify-end" => return Some("justify-content: flex-end;".into()),

            // Positioning
            "relative" => return Some("position: relative;".into()),
            "absolute" => return Some("position: absolute;".into()),
            "fixed" => return Some("position: fixed;".into()),
            "sticky" => return Some("position: sticky;".into()),
            "top-0" => return Some("top: 0px;".into()),
            "bottom-0" => return Some("bottom: 0px;".into()),
            "left-0" => return Some("left: 0px;".into()),
            "right-0" => return Some("right: 0px;".into()),
            "inset-0" => return Some("inset: 0px;".into()),
            "z-10" => return Some("z-index: 10;".into()),
            "z-20" => return Some("z-index: 20;".into()),
            "z-30" => return Some("z-index: 30;".into()),
            "z-40" => return Some("z-index: 40;".into()),
            "z-50" => return Some("z-index: 50;".into()),

            // Sizing
            "w-full" => return Some("width: 100%;".into()),
            "w-screen" => return Some("width: 100vw;".into()),
            "h-full" => return Some("height: 100%;".into()),
            "h-screen" => return Some("height: 100vh;".into()),
            "min-h-screen" => return Some("min-height: 100vh;".into()),
            "min-w-0" => return Some("min-width: 0px;".into()),
            "container" => return Some("width: 100%;".into()),
            "mx-auto" => return Some("margin-left: auto; margin-right: auto;".into()),

            // Typography & Colors
            "text-white" => return Some("color: #ffffff;".into()),
            "text-black" => return Some("color: #000000;".into()),
            "bg-white" => return Some("background-color: #ffffff;".into()),
            "bg-transparent" => return Some("background-color: transparent;".into()),
            "border" => return Some("border-width: 1px;".into()),
            "border-b" => return Some("border-bottom-width: 1px;".into()),
            "border-t" => return Some("border-top-width: 1px;".into()),
            "rounded" => return Some("border-radius: 0.25rem;".into()),
            "rounded-md" => return Some("border-radius: 0.375rem;".into()),
            "rounded-lg" => return Some("border-radius: 0.5rem;".into()),
            "rounded-xl" => return Some("border-radius: 0.75rem;".into()),
            "rounded-2xl" => return Some("border-radius: 1rem;".into()),
            "rounded-full" => return Some("border-radius: 9999px;".into()),
            "shadow-sm" => return Some("box-shadow: 0 1px 2px 0 rgba(0,0,0,0.05);".into()),
            "shadow-md" => return Some("box-shadow: 0 4px 6px -1px rgba(0,0,0,0.1), 0 2px 4px -2px rgba(0,0,0,0.1);".into()),
            "shadow-lg" => return Some("box-shadow: 0 10px 15px -3px rgba(0,0,0,0.1), 0 4px 6px -4px rgba(0,0,0,0.1);".into()),
            "shadow-xl" => return Some("box-shadow: 0 20px 25px -5px rgba(0,0,0,0.1), 0 8px 10px -6px rgba(0,0,0,0.1);".into()),
            "backdrop-blur-sm" => return Some("backdrop-filter: blur(4px); -webkit-backdrop-filter: blur(4px);".into()),
            "backdrop-blur-md" => return Some("backdrop-filter: blur(12px); -webkit-backdrop-filter: blur(12px);".into()),
            "backdrop-blur-lg" => return Some("backdrop-filter: blur(16px); -webkit-backdrop-filter: blur(16px);".into()),
            "overflow-hidden" => return Some("overflow: hidden;".into()),
            "overflow-auto" => return Some("overflow: auto;".into()),
            "cursor-pointer" => return Some("cursor: pointer;".into()),
            "transition-all" => return Some("transition-property: all; transition-timing-function: cubic-bezier(0.4, 0, 0.2, 1); transition-duration: 150ms;".into()),
            "leading-tight" => return Some("line-height: 1.25;".into()),
            "leading-none" => return Some("line-height: 1;".into()),
            "tracking-tight" => return Some("letter-spacing: -0.025em;".into()),
            "tracking-wider" => return Some("letter-spacing: 0.05em;".into()),
            _ => {}
        }

        // 2. Pattern Matchers: Spacing, Sizing, Font Size, Colors, Arbitrary Values
        if let Some(val) = cls.strip_prefix("p-") {
            return Some(format!("padding: {};", Self::scale_rem(val)));
        }
        if let Some(val) = cls.strip_prefix("px-") {
            let s = Self::scale_rem(val);
            return Some(format!("padding-left: {s}; padding-right: {s};"));
        }
        if let Some(val) = cls.strip_prefix("py-") {
            let s = Self::scale_rem(val);
            return Some(format!("padding-top: {s}; padding-bottom: {s};"));
        }
        if let Some(val) = cls.strip_prefix("pt-") {
            return Some(format!("padding-top: {};", Self::scale_rem(val)));
        }
        if let Some(val) = cls.strip_prefix("pb-") {
            return Some(format!("padding-bottom: {};", Self::scale_rem(val)));
        }
        if let Some(val) = cls.strip_prefix("pl-") {
            return Some(format!("padding-left: {};", Self::scale_rem(val)));
        }
        if let Some(val) = cls.strip_prefix("pr-") {
            return Some(format!("padding-right: {};", Self::scale_rem(val)));
        }
        if let Some(val) = cls.strip_prefix("m-") {
            return Some(format!("margin: {};", Self::scale_rem(val)));
        }
        if let Some(val) = cls.strip_prefix("gap-") {
            return Some(format!("gap: {};", Self::scale_rem(val)));
        }
        if let Some(val) = cls.strip_prefix("w-") {
            return Some(format!("width: {};", Self::scale_rem(val)));
        }
        if let Some(val) = cls.strip_prefix("h-") {
            return Some(format!("height: {};", Self::scale_rem(val)));
        }
        if let Some(val) = cls.strip_prefix("max-w-") {
            return Some(format!("max-width: {};", Self::max_w_val(val)));
        }
        if let Some(val) = cls.strip_prefix("text-") {
            if let Some(fs) = Self::font_size(val) {
                return Some(fs);
            }
            if let Some(fw) = Self::font_weight(val) {
                return Some(fw);
            }
            if let Some(col) = Self::color_val(val) {
                return Some(format!("color: {col};"));
            }
        }
        if let Some(val) = cls.strip_prefix("font-") {
            if let Some(fw) = Self::font_weight(val) {
                return Some(fw);
            }
        }
        if let Some(val) = cls.strip_prefix("bg-") {
            if let Some(col) = Self::color_val(val) {
                return Some(format!("background-color: {col};"));
            }
        }
        if let Some(val) = cls.strip_prefix("border-") {
            if let Some(col) = Self::color_val(val) {
                return Some(format!("border-color: {col};"));
            }
        }

        None
    }

    fn scale_rem(val: &str) -> String {
        if val.starts_with('[') && val.ends_with(']') {
            return val[1..val.len() - 1].to_string();
        }
        match val {
            "0" => "0px".into(),
            "0.5" => "0.125rem".into(),
            "1" => "0.25rem".into(),
            "1.5" => "0.375rem".into(),
            "2" => "0.5rem".into(),
            "2.5" => "0.625rem".into(),
            "3" => "0.75rem".into(),
            "3.5" => "0.875rem".into(),
            "4" => "1rem".into(),
            "5" => "1.25rem".into(),
            "6" => "1.5rem".into(),
            "7" => "1.75rem".into(),
            "8" => "2rem".into(),
            "9" => "2.25rem".into(),
            "10" => "2.5rem".into(),
            "12" => "3rem".into(),
            "14" => "3.5rem".into(),
            "16" => "4rem".into(),
            "20" => "5rem".into(),
            "24" => "6rem".into(),
            "32" => "8rem".into(),
            _ => format!("{}rem", val),
        }
    }

    fn max_w_val(val: &str) -> String {
        if val.starts_with('[') && val.ends_with(']') {
            return val[1..val.len() - 1].to_string();
        }
        match val {
            "xs" => "20rem".into(),
            "sm" => "24rem".into(),
            "md" => "28rem".into(),
            "lg" => "32rem".into(),
            "xl" => "36rem".into(),
            "2xl" => "42rem".into(),
            "3xl" => "48rem".into(),
            "4xl" => "56rem".into(),
            "5xl" => "64rem".into(),
            "6xl" => "72rem".into(),
            "7xl" => "80rem".into(),
            "full" => "100%".into(),
            _ => val.to_string(),
        }
    }

    fn font_size(val: &str) -> Option<String> {
        if val.starts_with('[') && val.ends_with(']') {
            return Some(format!("font-size: {};", &val[1..val.len() - 1]));
        }
        match val {
            "xs" => Some("font-size: 0.75rem; line-height: 1rem;".into()),
            "sm" => Some("font-size: 0.875rem; line-height: 1.25rem;".into()),
            "base" => Some("font-size: 1rem; line-height: 1.5rem;".into()),
            "lg" => Some("font-size: 1.125rem; line-height: 1.75rem;".into()),
            "xl" => Some("font-size: 1.25rem; line-height: 1.75rem;".into()),
            "2xl" => Some("font-size: 1.5rem; line-height: 2rem;".into()),
            "3xl" => Some("font-size: 1.875rem; line-height: 2.25rem;".into()),
            "4xl" => Some("font-size: 2.25rem; line-height: 2.5rem;".into()),
            _ => None,
        }
    }

    fn font_weight(val: &str) -> Option<String> {
        match val {
            "normal" => Some("font-weight: 400;".into()),
            "medium" => Some("font-weight: 500;".into()),
            "semibold" => Some("font-weight: 600;".into()),
            "bold" => Some("font-weight: 700;".into()),
            "extrabold" => Some("font-weight: 800;".into()),
            _ => None,
        }
    }

    fn color_val(val: &str) -> Option<String> {
        if val.starts_with('[') && val.contains(']') {
            let inner = &val[1..val.find(']').unwrap()];
            if let Some(slash_idx) = val.find('/') {
                let opacity = &val[slash_idx + 1..];
                if let Ok(op_num) = opacity.parse::<f32>() {
                    return Some(format!("rgba({}, {})", inner, op_num / 100.0));
                }
            }
            return Some(inner.to_string());
        }

        // Handle opacity syntax like `white/70`, `white/80`
        if let Some((color_name, opacity_str)) = val.split_once('/') {
            let alpha = opacity_str.parse::<f32>().unwrap_or(100.0) / 100.0;
            if color_name == "white" {
                return Some(format!("rgba(255, 255, 255, {})", alpha));
            }
            if color_name == "black" {
                return Some(format!("rgba(0, 0, 0, {})", alpha));
            }
        }

        match val {
            "white" => Some("#ffffff".into()),
            "black" => Some("#000000".into()),
            "transparent" => Some("transparent".into()),
            "current" => Some("currentColor".into()),

            // Zinc / Gray
            "zinc-50" => Some("#fafafa".into()),
            "zinc-100" => Some("#f4f4f5".into()),
            "zinc-200" => Some("#e4e4e7".into()),
            "zinc-300" => Some("#d4d4d8".into()),
            "zinc-400" => Some("#a1a1aa".into()),
            "zinc-500" => Some("#71717a".into()),
            "zinc-600" => Some("#52525b".into()),
            "zinc-700" => Some("#3f3f46".into()),
            "zinc-800" => Some("#27272a".into()),
            "zinc-900" => Some("#18181b".into()),
            "zinc-950" => Some("#09090b".into()),

            // Slate / Gray
            "slate-50" => Some("#f8fafc".into()),
            "slate-100" => Some("#f1f5f9".into()),
            "slate-200" => Some("#e2e8f0".into()),
            "slate-300" => Some("#cbd5e1".into()),
            "slate-400" => Some("#94a3b8".into()),
            "slate-500" => Some("#64748b".into()),
            "slate-600" => Some("#475569".into()),
            "slate-700" => Some("#334155".into()),
            "slate-800" => Some("#1e293b".into()),
            "slate-900" => Some("#0f172a".into()),
            "slate-950" => Some("#020617".into()),

            // Sky / Blue
            "sky-50" => Some("#f0f9ff".into()),
            "sky-100" => Some("#e0f2fe".into()),
            "sky-200" => Some("#bae6fd".into()),
            "sky-400" => Some("#38bdf8".into()),
            "sky-500" => Some("#0ea5e9".into()),
            "sky-600" => Some("#0284c7".into()),
            "sky-700" => Some("#0369a1".into()),

            "blue-50" => Some("#eff6ff".into()),
            "blue-100" => Some("#dbeafe".into()),
            "blue-500" => Some("#3b82f6".into()),
            "blue-600" => Some("#2563eb".into()),
            "blue-700" => Some("#1d4ed8".into()),

            // Emerald / Green
            "emerald-50" => Some("#ecfdf5".into()),
            "emerald-100" => Some("#d1fae5".into()),
            "emerald-200" => Some("#a7f3d0".into()),
            "emerald-500" => Some("#10b981".into()),
            "emerald-600" => Some("#059669".into()),
            "emerald-700" => Some("#047857".into()),

            // Red / Amber
            "red-50" => Some("#fef2f2".into()),
            "red-500" => Some("#ef4444".into()),
            "red-600" => Some("#dc2626".into()),
            "amber-50" => Some("#fffbeb".into()),
            "amber-500" => Some("#f59e0b".into()),

            _ => None,
        }
    }
}
