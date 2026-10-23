//! Server-side implementations of `next/*` modules (sources in `src/next/`), written to
//! `.nata/next/` with `node_modules/next/<name>.js` pointing at them, so both the React Server
//! Components layer and the SSR layer resolve `next/headers`, `next/server`, ... to them.

use std::path::Path;

/// Isomorphic modules (also served to the browser at `/_nata/shims/next/<name>`)
pub const DYNAMIC_JS: &str = include_str!("next/dynamic.js");
pub const SCRIPT_JS: &str = include_str!("next/script.js");
pub const IMAGE_JS: &str = include_str!("next/image.js");
pub const LINK_JS: &str = include_str!("next/link.js");
pub const ROUTER_JS: &str = include_str!("next/router.js");
pub const HEAD_JS: &str = include_str!("next/head.js");

/// `next/<name>` → `.nata/next/<file>` in the server layers (resolver aliases, see resolver/oxc.rs):
/// independent of what `node_modules/next` holds (the real package when the app installs it)
pub const SERVER_MODULES: [(&str, &str); 11] = [
    ("headers", "headers.ts"),
    ("server", "server.ts"),
    ("navigation", "navigation.ts"),
    ("cache", "cache.ts"),
    ("og", "og.ts"),
    ("dynamic", "dynamic.js"),
    ("script", "script.js"),
    ("image", "image.js"),
    ("link", "link.js"),
    ("router", "router.js"),
    ("head", "head.js"),
];

const FILES: [(&str, &str); 15] = [
    ("config-runtime.ts", include_str!("next/config-runtime.ts")),
    ("dynamic.js", DYNAMIC_JS),
    ("script.js", SCRIPT_JS),
    ("image.js", IMAGE_JS),
    ("link.js", LINK_JS),
    ("router.js", ROUTER_JS),
    ("head.js", HEAD_JS),
    ("middleware-runtime.ts", include_str!("next/middleware-runtime.ts")),
    ("request.ts", include_str!("next/request.ts")),
    ("headers.ts", include_str!("next/headers.ts")),
    ("server.ts", include_str!("next/server.ts")),
    ("og.ts", include_str!("next/og.ts")),
    ("navigation.ts", include_str!("next/navigation.ts")),
    ("cache.ts", include_str!("next/cache.ts")),
    ("cache-runtime.ts", include_str!("next/cache-runtime.ts")),
];

/// `next/<name>` → `.nata/next/<file>`
const ENTRIES: [(&str, &str); 5] = [
    ("headers", "headers.ts"),
    ("server", "server.ts"),
    ("navigation", "navigation.ts"),
    ("cache", "cache.ts"),
    ("og", "og.ts"),
];

fn write_if_changed(path: &Path, contents: &str) {
    if std::fs::read_to_string(path).ok().as_deref() != Some(contents) {
        let _ = std::fs::write(path, contents);
    }
}

pub fn write(root: &Path) {
    let dir = root.join(".nata").join("next");
    let _ = std::fs::create_dir_all(&dir);
    for (name, source) in FILES {
        write_if_changed(&dir.join(name), &crate::worker_pool::runtime_source(&format!("next/{name}"), source));
    }
    // Older import paths (tsconfig `paths` written by earlier versions point here)
    write_if_changed(&root.join(".nata/headers.ts"), "export * from \"./next/headers.ts\";\nexport { default } from \"./next/headers.ts\";\n");
    write_if_changed(&root.join(".nata/server.ts"), "export * from \"./next/server.ts\";\nexport { default } from \"./next/server.ts\";\n");
}

/// `node_modules/next/<name>.js` pointing at `.nata/next/`: for projects without the real `next`
/// package (packages Bun loads natively import `next/*` from there)
pub fn write_node_modules(root: &Path) {
    let nm_next = crate::store::packages_dir(root).join("next");
    if crate::store::is_shared(&nm_next) {
        return;
    }
    let nata = crate::store::nata_from_package(root);
    let _ = std::fs::create_dir_all(&nm_next);
    for (entry, file) in ENTRIES {
        write_if_changed(&nm_next.join(format!("{entry}.js")), &format!("module.exports = require(\"{nata}/next/{file}\");\n"));
    }
    // Components: CommonJS `require("next/image")` gets the component, `import Image` its default
    for (entry, file) in [("dynamic", "dynamic.js"), ("script", "script.js"), ("image", "image.js")] {
        write_if_changed(
            &nm_next.join(format!("{entry}.js")),
            &format!(
                "const m = require(\"{nata}/next/{file}\");\nmodule.exports = Object.assign(m.default, m, {{ default: m.default, __esModule: true }});\n"
            ),
        );
    }
}
