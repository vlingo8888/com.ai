import { existsSync, readFileSync, realpathSync } from "fs";
import { join } from "path";
import { packagesDir } from "../store";

/**
 * The PGlite constructor: the project's own `@pglite/core` first, whose native addon sits next to
 * it (the copy bundled into the standalone `com` looks for the addon on the build machine), then
 * the CLI's, then the WASM `@electric-sql/pglite`. `null` when none loads.
 */
export function loadPGliteClass(projectDir: string): any | null {
  // Each `require` of a literal stays directly in its `try`: Bun's bundler then leaves a package it
  // cannot resolve to fail at runtime instead of failing the build
  try {
    const PGlite = pgliteCore(requireFromProject(projectDir, "@pglite/core"));
    if (PGlite) return PGlite;
  } catch {}
  try {
    const PGlite = pgliteCore(require("@pglite/core"));
    if (PGlite) return PGlite;
  } catch {}
  try {
    return requireFromProject(projectDir, "@electric-sql/pglite").PGlite;
  } catch {}
  try {
    return require("@electric-sql/pglite").PGlite;
  } catch {}
  return null;
}

function pgliteCore(pkg: any): any {
  return pkg.PGLite || pkg.PGLiteNative;
}

/**
 * A package of the project (node_modules or the store), required by the path of its entry: the
 * standalone `com` does not resolve bare specifiers from disk, even through `createRequire`
 */
export function requireFromProject(projectDir: string, name: string): any {
  const dir = realpathSync(join(packagesDir(projectDir), name));
  const pkg = JSON.parse(readFileSync(join(dir, "package.json"), "utf-8"));
  const entry = packageEntry(pkg);
  const file = join(dir, entry);
  if (!existsSync(file)) throw new Error(`${name}: no ${entry}`);
  return require(file);
}

/** The file `require(<package>)` loads: `exports["."]` (or its require / default / import condition), else `main` */
export function packageEntry(pkg: { main?: string; exports?: any }): string {
  let root = pkg.exports && typeof pkg.exports === "object" && "." in pkg.exports ? pkg.exports["."] : pkg.exports;
  while (root && typeof root === "object") root = root.require ?? root.default ?? root.import ?? null;
  return typeof root === "string" ? root : pkg.main || "index.js";
}
