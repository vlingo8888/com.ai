# Native CSS Compiler Engine (`css_compiler`)

A blazing-fast, server-side CSS compilation and utility generator engine built directly in Rust using [`lightningcss`](https://github.com/parcel-bundler/lightningcss).

## 🚀 Performance Overview

| Feature | Browser-side `@tailwindcss/browser` | **Native Rust `css_compiler`** |
| :--- | :--- | :--- |
| **Execution Context** | Client-side JavaScript DOM MutationObserver | **Server-side Rust SIMD & LightningCSS** |
| **Render Latency** | FOUC (50ms – 100ms unstyled delay) | **0ms (Instant Hardware-Accelerated CSS)** |
| **Client JS Overhead** | +400 KB JS Script & high CPU load | **0 KB JS / 0% Client CPU** |
| **HTML Delivery** | Dynamic script evaluation | `<link rel="stylesheet" href="/_nata/styles.css">` |

---

## 📦 Module Structure

```
packages/cli/compiler-rs/src/css_compiler/
├── mod.rs        # Public module facade & exports (CssCompiler, ClassScanner, RuleGenerator)
├── scanner.rs    # High-speed SIMD/Regex token scanner for TSX, JSX, JS, TS, HTML
├── rules.rs      # Tailwind utility rule generator (preflight, spacing, layout, arbitrary values)
├── engine.rs     # Core LightningCSS pipeline (parsing, nesting, vendor prefixing, minification)
├── tests.rs      # Comprehensive unit tests
└── README.md     # Documentation
```

---

## 🛠️ Public API

```rust
use compiler_rs::css_compiler::CssCompiler;

// Compiles and minifies CSS for an entire workspace directory
let css = CssCompiler::compile_workspace("/path/to/project");
```

---

## 🧪 Testing

```bash
cargo test css_compiler
```
