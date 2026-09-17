use super::{ClassScanner, CssCompiler, RuleGenerator};
use std::collections::HashSet;

#[test]
fn test_class_scanner_from_tsx() {
    let tsx_source = r#"
import React from "react";
import { cn } from "@/lib/utils";

export function Hero({ isActive }) {
    return (
        <div className="container mx-auto px-4 md:px-6 py-12 flex flex-col items-center">
            <h1 className={`text-4xl font-extrabold text-zinc-900 ${isActive ? 'bg-sky-50' : 'bg-white'}`}>
                Welcome to NATA
            </h1>
            <button className={cn("px-6 py-3 rounded-xl shadow-lg transition-all", "hover:bg-blue-600 text-white")}>
                Get Started
            </button>
        </div>
    );
}
"#;

    let classes = ClassScanner::scan_source(tsx_source);

    assert!(classes.contains("container"));
    assert!(classes.contains("mx-auto"));
    assert!(classes.contains("px-4"));
    assert!(classes.contains("md:px-6"));
    assert!(classes.contains("py-12"));
    assert!(classes.contains("flex"));
    assert!(classes.contains("flex-col"));
    assert!(classes.contains("items-center"));
    assert!(classes.contains("text-4xl"));
    assert!(classes.contains("font-extrabold"));
    assert!(classes.contains("text-zinc-900"));
    assert!(classes.contains("bg-sky-50"));
    assert!(classes.contains("bg-white"));
    assert!(classes.contains("px-6"));
    assert!(classes.contains("py-3"));
    assert!(classes.contains("rounded-xl"));
    assert!(classes.contains("shadow-lg"));
    assert!(classes.contains("transition-all"));
    assert!(classes.contains("hover:bg-blue-600"));
    assert!(classes.contains("text-white"));
}

#[test]
fn test_rule_generator_static_utilities() {
    let (sel_flex, decl_flex) = RuleGenerator::resolve_utility("flex").expect("Should resolve flex");
    assert_eq!(sel_flex, ".flex");
    assert!(decl_flex.contains("display: flex"));

    let (sel_col, decl_col) = RuleGenerator::resolve_utility("flex-col").expect("Should resolve flex-col");
    assert_eq!(sel_col, ".flex-col");
    assert!(decl_col.contains("flex-direction: column"));

    let (sel_sticky, decl_sticky) = RuleGenerator::resolve_utility("sticky").expect("Should resolve sticky");
    assert_eq!(sel_sticky, ".sticky");
    assert!(decl_sticky.contains("position: sticky"));
}

#[test]
fn test_rule_generator_responsive_and_dark() {
    let mut classes = HashSet::new();
    classes.insert("md:flex".to_string());
    classes.insert("sm:inline-flex".to_string());
    classes.insert("dark:bg-slate-900".to_string());

    let rules = RuleGenerator::generate_rules(&classes);

    assert!(rules.contains("@media (min-width: 768px)"));
    assert!(rules.contains(".md\\:flex"));
    assert!(rules.contains("@media (min-width: 640px)"));
    assert!(rules.contains(".sm\\:inline-flex"));
    assert!(rules.contains("html.dark .dark\\:bg-slate-900"));
}

#[test]
fn test_rule_generator_arbitrary_values() {
    let (sel_size, decl_size) = RuleGenerator::resolve_utility("text-[11px]").expect("Should resolve text-[11px]");
    assert_eq!(sel_size, ".text-\\[11px\\]");
    assert!(decl_size.contains("font-size: 11px"));

    let (sel_max, decl_max) = RuleGenerator::resolve_utility("max-w-6xl").expect("Should resolve max-w-6xl");
    assert_eq!(sel_max, ".max-w-6xl");
    assert!(decl_max.contains("max-width: 72rem"));
}

#[test]
fn test_lightningcss_minification() {
    let raw_css = r#"
        .btn-primary {
            background-color: #0284c7;
            color: #ffffff;
            padding: 12px 24px;
        }
    "#;

    let minified = CssCompiler::minify_and_optimize(raw_css).expect("Failed to minify");
    assert!(!minified.contains("\n        "));
    assert!(minified.contains(".btn-primary{"));
    assert!(minified.contains("background-color:#0284c7"));
}

#[test]
fn test_compile_from_classes_end_to_end() {
    let mut classes = HashSet::new();
    classes.insert("flex".to_string());
    classes.insert("items-center".to_string());
    classes.insert("justify-between".to_string());
    classes.insert("bg-sky-50".to_string());
    classes.insert("text-zinc-900".to_string());

    let custom_css = r#"
        :root {
            --brand: #0ea5e9;
        }
    "#;

    let compiled = CssCompiler::compile_from_classes(&classes, custom_css);

    assert!(compiled.contains(".flex{"));
    assert!(compiled.contains(".items-center{"));
    assert!(compiled.contains(".bg-sky-50{"));
    assert!(compiled.contains("--brand:#0ea5e9"));
}
