// Loads next.config.(ts|mjs|js|cjs) for the engine: the exported object, or the result of the
// exported (async) function `(phase, { defaultConfig })`. `redirects()`, `rewrites()` and `headers()`
// are called here, so the engine gets plain data (see next_config.rs).

export async function loadNextConfig(input: { specifier: string }) {
  const mod = await import(input.specifier);
  let config = mod.default ?? mod;
  if (typeof config === "function") config = await config("phase-development-server", { defaultConfig: {} });
  config = (await config) ?? {};
  const call = async (key: string) => (typeof config[key] === "function" ? await config[key]() : config[key]);
  const rewrites = await call("rewrites");
  // `env` is readable by server code too
  for (const [k, v] of Object.entries(config.env ?? {})) if (process.env[k] === undefined) process.env[k] = String(v);
  return {
    ...JSON.parse(JSON.stringify(config, (_k, v) => (typeof v === "function" ? undefined : v))),
    redirects: (await call("redirects")) ?? [],
    // `rewrites()` may return an array or { beforeFiles, afterFiles, fallback }
    rewrites: Array.isArray(rewrites) ? { beforeFiles: [], afterFiles: rewrites, fallback: [] } : { beforeFiles: [], afterFiles: [], fallback: [], ...(rewrites ?? {}) },
    headers: (await call("headers")) ?? [],
  };
}
