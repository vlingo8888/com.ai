//! Native Tailwind CSS v4 compiler (port of tailwindcss@4.3.3 and the `@tailwindcss/oxide` extractor),
//! vendored from NATA `backend/tailwind-rs` without its HTTP service. See README.md.

pub mod css_resolve;
pub mod engine;
pub mod scan;
pub mod scanner;
pub mod tw;
pub mod types;

pub use types::FileItem;
