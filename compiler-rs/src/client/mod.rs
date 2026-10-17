/// Browser runtime shared by the web shell and the Zalo Mini App bootstrap, served at `/_nata/client.js`.
pub const CLIENT_RUNTIME_JS: &str = include_str!("runtime.js");

/// Next.js-style error overlay imported by the runtime (and first by the Zalo bootstrap), at `/_nata/overlay.js`.
pub const DEV_OVERLAY_JS: &str = include_str!("overlay.js");

/// Query & action debugger drawer (dev only), at `/_nata/debug.js`.
pub const DEBUG_JS: &str = include_str!("debug.js");

/// React Fast Refresh runtime setup (dev), at `/_nata/refresh.js`.
pub const REFRESH_JS: &str = include_str!("refresh.js");
