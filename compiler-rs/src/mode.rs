//! Development (`com dev`) or production (`com build`, `com start`). Production is `COM_ENV=production`,
//! set by the `build` and `start` commands before anything starts, so the Bun worker inherits it.
//!
//! Production means: React and dependencies in their production builds on both sides, no Fast
//! Refresh, no HMR socket, no dev overlay or debug drawer, no file watcher.

pub fn is_production() -> bool {
    #[cfg(test)]
    if let Some(forced) = FORCED.with(|f| f.get()) {
        return forced;
    }
    matches!(std::env::var("COM_ENV").as_deref(), Ok("production"))
}

#[cfg(test)]
thread_local! {
    static FORCED: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) };
}

/// Runs `f` with production mode on for this thread only (tests run in parallel)
#[cfg(test)]
pub fn with_production<T>(f: impl FnOnce() -> T) -> T {
    FORCED.with(|c| c.set(Some(true)));
    let out = f();
    FORCED.with(|c| c.set(None));
    out
}

/// `NODE_ENV` for the worker, dependency bundles and code inlined into the browser
pub fn node_env() -> &'static str {
    if is_production() {
        "production"
    } else {
        "development"
    }
}

/// Called by `build` / `start` before the runtime starts (no other thread reads the environment yet)
pub fn set_production() {
    std::env::set_var("COM_ENV", "production");
}

/// Oldest Bun able to run production builds: React's production server build does not load in 1.2,
/// whose bundler also cannot take plugin-resolved entry points
pub const MIN_PRODUCTION_BUN: (u32, u32) = (1, 3);

/// `Err` with what to do when the worker's Bun is too old for `com build` / `com start`
pub fn check_production_bun() -> Result<(), String> {
    let bun = crate::rpc::find_bun_bin();
    let out = crate::rpc::bun_std_command().arg("--version").output().map_err(|e| format!("Could not run {}: {e}", bun.display()))?;
    let version = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if bun_at_least(&version, MIN_PRODUCTION_BUN) {
        Ok(())
    } else {
        Err(format!(
            "Production builds need Bun >= {}.{} (found {version} at {}). Run `bun upgrade`, or set COM_BUN to a newer Bun.",
            MIN_PRODUCTION_BUN.0,
            MIN_PRODUCTION_BUN.1,
            bun.display()
        ))
    }
}

fn bun_at_least(version: &str, (major, minor): (u32, u32)) -> bool {
    let mut parts = version.split('.').map(|p| p.trim().parse::<u32>().unwrap_or(0));
    let (a, b) = (parts.next().unwrap_or(0), parts.next().unwrap_or(0));
    a > major || (a == major && b >= minor)
}

#[cfg(test)]
mod tests {
    #[test]
    fn compares_bun_versions() {
        assert!(!super::bun_at_least("1.2.17", (1, 3)));
        assert!(super::bun_at_least("1.3.0", (1, 3)));
        assert!(super::bun_at_least("1.4.2", (1, 3)));
        assert!(super::bun_at_least("2.0.0", (1, 3)));
        assert!(!super::bun_at_least("", (1, 3)));
    }
}
