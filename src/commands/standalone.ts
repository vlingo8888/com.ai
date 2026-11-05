/**
 * `com build --standalone` (or `output: "standalone"` in next.config): a folder that `com start`
 * serves on another machine with nothing installed but `com` (or nothing at all with `--bundle-com`).
 *
 *   .nata/standalone/
 *     app/ modules/ features/ …     the project's sources: the engine still compiles server code (pages,
 *                                   route handlers, Server Actions) on the first request
 *     .nata/build/                  what `com build` made (client bundles, prerendered pages, manifest)
 *     node_modules/                 real files, only the production packages: the lockfile's tree
 *                                   reachable from `dependencies` (no devDependencies, no links into
 *                                   the machine's store)
 *     .nata/home/react/<version>/   the framework's React, when the project runs on it (COM_HOME)
 *     com, com-compiler             with --bundle-com (the standalone `com` executable only)
 *
 * Packages are copied for this machine's platform: build on the OS / CPU the server runs (native
 * addons such as PGlite's or sharp's are per platform).
 */

import { cpSync, existsSync, mkdirSync, readdirSync, readFileSync, realpathSync, rmSync, statSync } from "fs";
import { basename, join, relative, resolve, sep } from "path";
import { comHome } from "./cache";
import { entryDir, usesStore, packagesDir } from "../store";
import { parseLockfile, pathSegments, resolveGraph, type Lockfile, type ResolvedGraph } from "../store/lockfile";
import { isCompiled } from "../core/self";
import { exeName } from "../core/binaries";

export const STANDALONE_DIR = ".nata/standalone";

/** `output: "standalone"` in the project's next.config */
export function configWantsStandalone(projectDir: string): boolean {
  for (const name of ["next.config.ts", "next.config.mjs", "next.config.js", "next.config.cjs", "next.config.mts"]) {
    const file = join(projectDir, name);
    if (existsSync(file) && /\boutput\s*:\s*["'`]standalone["'`]/.test(readFileSync(file, "utf-8"))) return true;
  }
  return false;
}

/**
 * Packages the server needs even when a project lists them as devDependencies: the framework loads
 * them itself (React and Flight for rendering, kysely / pg / PGlite for `core`'s `db`)
 */
const RUNTIME_PACKAGES = [
  "react",
  "react-dom",
  "react-server-dom-webpack",
  "scheduler",
  "kysely",
  "pg",
  "@pglite/core",
  "@electric-sql/pglite",
  "next",
  "next-intl",
];

/**
 * Lockfile paths of the production packages: reachable from `dependencies` / `optionalDependencies`
 * (and the framework's runtime packages) through dependencies and optional dependencies. Peer
 * dependencies are what the package's host provides: a peer nothing else needs (`typescript` for
 * `@pglite/core`) is left out.
 */
export function productionPaths(lock: Lockfile, graph: ResolvedGraph = resolveGraph(lock)): string[] {
  const root = lock.workspace ?? {};
  const roots = new Set([...Object.keys(root.dependencies ?? {}), ...Object.keys(root.optionalDependencies ?? {})]);
  for (const name of RUNTIME_PACKAGES) if (name in (lock.rootDependencies ?? {})) roots.add(name);

  const keep = new Set<string>();
  const queue: string[] = [];
  const visit = (path: string | undefined) => {
    if (path && graph.nodes.has(path) && !keep.has(path)) {
      keep.add(path);
      queue.push(path);
    }
  };
  for (const name of roots) visit(name);
  while (queue.length) {
    const path = queue.shift()!;
    const pkg = graph.nodes.get(path)!;
    for (const [dep, target] of graph.edges.get(path) ?? []) {
      if (dep in pkg.peerDependencies && !(dep in pkg.dependencies) && !(dep in pkg.optionalDependencies)) continue;
      visit(target);
    }
  }
  return [...keep].sort();
}

/** `a/@s/b` → `node_modules/a/node_modules/@s/b` */
export function installPath(lockPath: string): string {
  return pathSegments(lockPath).map((s) => join("node_modules", s)).join(sep);
}

/** Top-level entries of the project never copied (dependencies, build output, local data, VCS) */
const SKIP_TOP = new Set(["node_modules", ".nata", "dist", "build", "out", "coverage", "target", "data", "tests", "__tests__"]);
/** Env files `com start` reads in production (`.env.local` / `.env.development` are for this machine) */
const ENV_FILES = new Set([".env", ".env.production", ".env.production.local"]);
const TEST_FILE = /\.(test|spec)\.[cm]?[jt]sx?$/;

/** Whether a project file goes into the standalone folder (`rel`: path relative to the project) */
export function shipsProjectFile(rel: string): boolean {
  const parts = rel.split(/[\\/]/);
  const top = parts[0];
  if (parts.length === 1 && ENV_FILES.has(top)) return true;
  if (SKIP_TOP.has(top)) return false;
  if (parts.some((p) => p.startsWith(".") || p === "node_modules")) return false;
  return !TEST_FILE.test(parts[parts.length - 1]);
}

/** Whether the project runs on the framework's React (framework_react.rs `needed`) */
function needsFrameworkReact(projectDir: string): boolean {
  if (["0", "false"].includes(process.env.COM_RSC ?? "")) return false;
  let major = 0;
  try {
    major = parseInt(JSON.parse(readFileSync(join(packagesDir(projectDir), "react", "package.json"), "utf-8")).version, 10) || 0;
  } catch {}
  let declaresFlight = false;
  try {
    const manifest = JSON.parse(readFileSync(join(projectDir, "package.json"), "utf-8"));
    declaresFlight = ["dependencies", "devDependencies"].some((k) => manifest[k]?.["react-server-dom-webpack"]);
  } catch {}
  return !(major >= 19 && declaresFlight);
}

const FRAMEWORK_REACT_PACKAGES = ["react", "react-dom", "react-server-dom-webpack", "scheduler"];

/** The newest complete install of the framework's React on this machine (`~/.com/react/<version>`) */
function frameworkReactDir(): string | null {
  const base = join(comHome(), "react");
  let versions: string[] = [];
  try {
    versions = readdirSync(base).filter((v) => FRAMEWORK_REACT_PACKAGES.every((p) => existsSync(join(base, v, "node_modules", p, "package.json"))));
  } catch {
    return null;
  }
  const num = (v: string) => v.split(/[.-]/).map((n) => parseInt(n, 10) || 0);
  versions.sort((a, b) => {
    const [x, y] = [num(a), num(b)];
    for (let i = 0; i < Math.max(x.length, y.length); i++) if ((x[i] ?? 0) !== (y[i] ?? 0)) return (x[i] ?? 0) - (y[i] ?? 0);
    return 0;
  });
  return versions.length ? join(base, versions[versions.length - 1]) : null;
}

function dirSize(dir: string): number {
  let total = 0;
  const walk = (d: string) => {
    for (const name of readdirSync(d)) {
      const p = join(d, name);
      const s = statSync(p, { throwIfNoEntry: false });
      if (!s) continue;
      if (s.isDirectory()) walk(p);
      else total += s.size;
    }
  };
  try {
    walk(dir);
  } catch {}
  return total;
}

/** Copies a directory with real files (links into the store or anywhere else are followed) */
function copyTree(src: string, dest: string, filter?: (path: string) => boolean) {
  cpSync(realpathSync(src), dest, { recursive: true, dereference: true, filter: filter ? (p) => filter(p) : undefined });
}

export interface StandaloneOptions {
  outDir?: string;
  bundleCom?: boolean;
  /** The engine binary (`--bundle-com`) */
  engine?: string | null;
}

export interface StandaloneResult {
  outDir: string;
  packages: number;
  missing: string[];
  frameworkReact: string | null;
  bundledCom: boolean;
  warnings: string[];
  bytes: number;
}

export function createStandalone(projectDir: string, options: StandaloneOptions = {}): StandaloneResult {
  const outDir = resolve(projectDir, options.outDir || STANDALONE_DIR);
  const buildDir = join(projectDir, ".nata", "build");
  if (!existsSync(join(buildDir, "manifest.json"))) throw new Error("No production build: run `com build` first");
  // The folder is emptied first: never the project or one of its parents, and never a folder that
  // holds something other than a previous standalone build
  if (outDir === projectDir || !relative(outDir, projectDir).startsWith("..")) {
    throw new Error(`Refusing to write the standalone build to ${outDir} (it contains the project)`);
  }
  if (existsSync(outDir) && readdirSync(outDir).length > 0 && !existsSync(join(outDir, ".nata", "build", "manifest.json"))) {
    throw new Error(`${outDir} is not empty and is not a standalone build: choose another --out`);
  }
  const warnings: string[] = [];

  rmSync(outDir, { recursive: true, force: true });
  mkdirSync(outDir, { recursive: true });

  // 1. Sources (the output folder itself is skipped: it lives in .nata/, or wherever --out put it)
  for (const name of readdirSync(projectDir)) {
    const src = join(projectDir, name);
    if (src === outDir || !shipsProjectFile(name)) continue;
    cpSync(src, join(outDir, name), {
      recursive: true,
      dereference: true,
      filter: (p) => {
        if (p === outDir || p.startsWith(outDir + sep)) return false;
        const rel = relative(projectDir, p);
        return rel === "" || shipsProjectFile(rel);
      },
    });
  }

  // 2. The build
  copyTree(buildDir, join(outDir, ".nata", "build"));

  // 3. Production packages, as real files in the lockfile's (hoisted) layout
  let packages = 0;
  const missing: string[] = [];
  const lockFile = join(projectDir, "bun.lock");
  const store = usesStore(projectDir);
  if (existsSync(lockFile)) {
    const lock = parseLockfile(readFileSync(lockFile, "utf-8"));
    const graph = resolveGraph(lock);
    for (const path of productionPaths(lock, graph)) {
      const pkg = graph.nodes.get(path)!;
      const src = store
        ? join(entryDir(graph.entries.get(path)!), "node_modules", pkg.name)
        : join(projectDir, installPath(path));
      if (!existsSync(src)) {
        missing.push(`${pkg.name}@${pkg.version}`);
        continue;
      }
      const real = realpathSync(src);
      // A package in a real node_modules holds the nested ones: they are copied as their own paths
      copyTree(src, join(outDir, installPath(path)), (p) => !(p.startsWith(real + sep) && relative(real, p).split(sep)[0] === "node_modules"));
      packages++;
    }
    if (lock.unsupported.length) warnings.push(`Not copied (git / file / URL packages): ${lock.unsupported.join(", ")}`);
    if (missing.length) warnings.push(`Packages not installed here (run \`com pkg install\`, then build again): ${missing.join(", ")}`);
  } else if (existsSync(packagesDir(projectDir))) {
    // No bun.lock (npm / pnpm / yarn project): every installed package goes, devDependencies included
    copyTree(packagesDir(projectDir), join(outDir, "node_modules"));
    packages = readdirSync(join(outDir, "node_modules")).filter((n) => !n.startsWith(".")).length;
    warnings.push("No bun.lock: node_modules was copied whole (devDependencies included)");
  }

  // 4. The framework's React (projects without their own react-server-dom-webpack)
  let frameworkReact: string | null = null;
  if (needsFrameworkReact(projectDir)) {
    const dir = frameworkReactDir();
    if (dir) {
      copyTree(dir, join(outDir, ".nata", "home", "react", basename(dir)));
      frameworkReact = basename(dir);
    } else {
      warnings.push("The framework's React is not installed on this machine: the server downloads it on its first start");
    }
  }

  // 5. `com` itself
  let bundledCom = false;
  if (options.bundleCom) {
    if (!isCompiled()) {
      warnings.push("--bundle-com needs the standalone `com` executable (this com runs from source): install `com` on the server");
    } else if (!options.engine || !existsSync(options.engine)) {
      warnings.push("--bundle-com: the engine binary was not found next to com");
    } else {
      cpSync(process.execPath, join(outDir, exeName("com")));
      cpSync(options.engine, join(outDir, exeName("com-compiler")));
      bundledCom = true;
    }
  }

  return { outDir, packages, missing, frameworkReact, bundledCom, warnings, bytes: dirSize(outDir) };
}

/** `<dir>/.nata/home` when a standalone build ships the framework's React there (COM_HOME for the engine) */
export function standaloneHome(projectDir: string): string | null {
  const home = join(projectDir, ".nata", "home");
  return existsSync(join(home, "react")) ? home : null;
}
