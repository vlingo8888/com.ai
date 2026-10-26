/**
 * The machine-wide package store: projects have no `node_modules`.
 *
 *   ~/.com/store/v1/
 *     <name>@<version>_<hash>/node_modules/
 *       <name>/                                the package: a clone of Bun's cache (macOS), hard
 *                                              links to it (Linux), or the downloaded tarball
 *       <dependency> → ../../<entry>/node_modules/<dependency>
 *   <project>/.nata/modules/
 *     node_modules/<package> → store entry      what the project sees (the hoisted top level)
 *     store.json                                entries in use, lockfile hash
 *   <project>/node_modules → .nata/modules/node_modules
 *                                               one link (no files): what Bun finds when it loads a
 *                                               file itself (next.config, serverExternalPackages, tests;
 *                                               its runtime plugins do not see those imports), and
 *                                               editors / tsc
 *   ~/.com/projects/<hash>.json                 projects using the store (`com pkg gc`)
 *
 * Bun still resolves versions (`bun install --lockfile-only` writes `bun.lock`); `lockfile.ts` turns
 * the lockfile into store entries. The engine, the worker (NODE_PATH) and the dependency bundler find
 * packages in `.nata/modules/node_modules`. A project that has a real `node_modules` keeps using it;
 * `COM_STORE=0` turns the store off.
 */

import { createHash } from "crypto";
import {
  copyFileSync,
  existsSync,
  linkSync,
  lstatSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  readlinkSync,
  realpathSync,
  renameSync,
  rmSync,
  symlinkSync,
  writeFileSync,
} from "fs";
import { constants as fsConstants } from "fs";
import { homedir } from "os";
import { dirname, join, relative } from "path";
import { comHome } from "../commands/cache";
import { parseLockfile, pathSegments, resolveGraph, type LockPackage, type ResolvedGraph } from "./lockfile";
import { extractTgz } from "./tar";
import { bunEnv } from "../core/self";

/** Bumped when the layout of entries changes */
const STORE_FORMAT = "v1";
const DOWNLOADS = 16;

export function storeRoot(): string {
  return join(comHome(), "store", STORE_FORMAT);
}

export function storeEnabled(): boolean {
  return !["0", "false", "off"].includes(process.env.COM_STORE ?? "");
}

/** `<project>/.nata/modules/node_modules`: links to the store entries the project uses */
export function farmDir(projectDir: string): string {
  return join(projectDir, ".nata", "modules", "node_modules");
}

function isLinkTo(path: string, target: string): boolean {
  try {
    return lstatSync(path).isSymbolicLink() && realpathSync(path) === realpathSync(target);
  } catch {
    return false;
  }
}

/**
 * Whether the project gets its packages from the store: it has no `node_modules` (or only the link
 * `com pkg link` makes for editors)
 */
export function usesStore(projectDir: string): boolean {
  if (!storeEnabled()) return false;
  const nm = join(projectDir, "node_modules");
  return !existsSync(nm) || isLinkTo(nm, farmDir(projectDir));
}

/** Where the project's packages are: `node_modules` or the store links */
export function packagesDir(projectDir: string): string {
  return usesStore(projectDir) ? farmDir(projectDir) : join(projectDir, "node_modules");
}

/** Environment for Bun processes that load the project's packages themselves */
export function storeEnv(projectDir: string): Record<string, string> {
  if (!usesStore(projectDir)) return {};
  const farm = farmDir(projectDir);
  const delimiter = process.platform === "win32" ? ";" : ":";
  return { NODE_PATH: [farm, process.env.NODE_PATH].filter(Boolean).join(delimiter), COM_PACKAGES_DIR: farm };
}

// ---------- lockfile ----------

function sortedJson(value: unknown): string {
  if (!value || typeof value !== "object") return JSON.stringify(value ?? {});
  const obj = value as Record<string, unknown>;
  return `{${Object.keys(obj).sort().map((k) => `${JSON.stringify(k)}:${sortedJson(obj[k])}`).join(",")}}`;
}

const DEPENDENCY_FIELDS = ["dependencies", "devDependencies", "optionalDependencies", "peerDependencies"] as const;

/** Whether `bun.lock` was made from the dependencies package.json declares now */
export function lockfileCurrent(projectDir: string): boolean {
  const lockPath = join(projectDir, "bun.lock");
  if (!existsSync(lockPath)) return false;
  try {
    const pkg = JSON.parse(readFileSync(join(projectDir, "package.json"), "utf-8"));
    const { workspace } = parseLockfile(readFileSync(lockPath, "utf-8"));
    return DEPENDENCY_FIELDS.every((field) => sortedJson(pkg[field] ?? {}) === sortedJson(workspace[field] ?? {}));
  } catch {
    return false;
  }
}

/** `bun install --lockfile-only` resolves versions over the network: never wait on it forever */
const RESOLVE_TIMEOUT_MS = 120_000;
/** The prefetch only speeds the install up: what it has not fetched by then is downloaded below */
const PREFETCH_TIMEOUT_MS = 90_000;
const DOWNLOAD_TIMEOUT_MS = 60_000;

async function run(bun: string, args: string[], cwd: string, timeoutMs: number): Promise<void> {
  const proc = Bun.spawn([bun, ...args], { cwd, stdout: "ignore", stderr: "pipe", env: bunEnv(bun, { ...process.env, COM_STORE: "0" }) });
  let timedOut = false;
  const timer = setTimeout(() => {
    timedOut = true;
    proc.kill();
  }, timeoutMs);
  try {
    const stderr = await new Response(proc.stderr).text();
    const code = await proc.exited;
    if (timedOut) throw new Error(`bun ${args.join(" ")} did not finish in ${timeoutMs / 1000}s (network?)`);
    if (code !== 0) throw new Error(`bun ${args.join(" ")} failed:\n${stderr.trim().split("\n").slice(-8).join("\n")}`);
  } finally {
    clearTimeout(timer);
  }
}

/** `bun.lock` for package.json, without installing anything */
export async function ensureLockfile(projectDir: string, bun: string): Promise<boolean> {
  if (lockfileCurrent(projectDir)) return false;
  const args = ["install", "--lockfile-only"];
  if (existsSync(join(projectDir, "bun.lockb"))) args.push("--save-text-lockfile");
  await run(bun, args, projectDir, RESOLVE_TIMEOUT_MS);
  return true;
}

/** `bun add` / `bun remove` that only update package.json and bun.lock */
export async function editDependencies(projectDir: string, bun: string, command: "add" | "remove", args: string[]): Promise<void> {
  await run(bun, [command, "--lockfile-only", ...args], projectDir, RESOLVE_TIMEOUT_MS);
}

// ---------- store entries ----------

function registryUrl(pkg: LockPackage): string {
  const base = pkg.registry.startsWith("http") ? pkg.registry : process.env.COM_NPM_REGISTRY || process.env.npm_config_registry || "https://registry.npmjs.org";
  const file = pkg.name.split("/").pop();
  return `${base.replace(/\/$/, "")}/${pkg.name}/-/${file}-${pkg.version}.tgz`;
}

function verify(bytes: Uint8Array, integrity: string | undefined, name: string) {
  if (!integrity) return;
  const [algo, expected] = integrity.split(/-(.*)/s);
  const hasher = new Bun.CryptoHasher(algo as any);
  hasher.update(bytes);
  if (hasher.digest("base64") !== expected) throw new Error(`${name}: integrity check failed`);
}

function tmpName(dir: string, name: string): string {
  return join(dir, ".tmp", `${name}-${process.pid}-${Math.random().toString(36).slice(2)}`);
}

/** Removes what interrupted installs left in `.tmp/` (their process, named in the folder, is gone) */
function removeAbandonedTmp() {
  const tmp = join(storeRoot(), ".tmp");
  let names: string[] = [];
  try {
    names = readdirSync(tmp);
  } catch {
    return;
  }
  for (const name of names) {
    const pid = Number(/-(\d+)-[a-z0-9]+$/.exec(name)?.[1]);
    if (!pid || pid === process.pid) continue;
    try {
      process.kill(pid, 0);
    } catch (error: any) {
      if (error?.code === "ESRCH") rmSync(join(tmp, name), { recursive: true, force: true });
    }
  }
}

/** Moves a finished directory into place; another process may have finished it first */
function publish(tmp: string, dest: string) {
  try {
    renameSync(tmp, dest);
  } catch {
    rmSync(tmp, { recursive: true, force: true });
    if (!existsSync(dest)) throw new Error(`could not create ${dest}`);
  }
}

/** Bun's own extracted copy of the package, when this machine installed it with Bun before */
function bunCacheDir(pkg: LockPackage): string | null {
  const cache = process.env.BUN_INSTALL_CACHE_DIR || join(process.env.BUN_INSTALL || join(homedir(), ".bun"), "install", "cache");
  const dir = join(cache, `${pkg.name}@${pkg.version}@@@1`);
  return existsSync(join(dir, "package.json")) ? dir : null;
}

let clonefile: ((src: string, dest: string) => boolean) | null | undefined;

/** macOS `clonefile(2)`: a whole directory tree in one call, sharing blocks until written (APFS) */
function cloneDirectory(src: string, dest: string): boolean {
  if (clonefile === undefined) {
    clonefile = null;
    if (process.platform === "darwin") {
      try {
        const ffi = require("bun:ffi");
        const lib = ffi.dlopen("libSystem.B.dylib", { clonefile: { args: ["ptr", "ptr", "u32"], returns: "i32" } });
        clonefile = (a, b) => {
          // The buffers stay referenced until the call returns
          const [pa, pb] = [Buffer.from(a + "\0"), Buffer.from(b + "\0")];
          return lib.symbols.clonefile(ffi.ptr(pa), ffi.ptr(pb), 0) === 0;
        };
      } catch {}
    }
  }
  return clonefile ? clonefile(src, dest) : false;
}

/**
 * Puts a copy of `src` at `dest`: a clone of the directory (macOS), else hard links (what Bun itself
 * does on Linux: its cache is never written to), else copies
 */
function copyPackage(src: string, dest: string) {
  mkdirSync(dirname(dest), { recursive: true });
  if (cloneDirectory(src, dest)) return;
  linkTree(src, dest);
}

/** Mirrors `src` into `dest` with hard links (a copy across file systems) */
function linkTree(src: string, dest: string) {
  mkdirSync(dest, { recursive: true });
  for (const entry of readdirSync(src, { withFileTypes: true })) {
    const from = join(src, entry.name);
    const to = join(dest, entry.name);
    if (entry.isDirectory()) linkTree(from, to);
    else if (entry.isFile()) {
      try {
        linkSync(from, to);
      } catch {
        copyFileSync(from, to, fsConstants.COPYFILE_FICLONE);
      }
    }
  }
}

/** Downloads the package from its registry into `dest` (when Bun's cache does not have it) */
async function download(pkg: LockPackage, dest: string) {
  const url = registryUrl(pkg);
  let bytes: Uint8Array | undefined;
  for (let attempt = 1; !bytes; attempt++) {
    try {
      const res = await fetch(url, { signal: AbortSignal.timeout(DOWNLOAD_TIMEOUT_MS) });
      if (!res.ok) throw new Error(`${pkg.name}@${pkg.version}: ${url} answered ${res.status}`);
      bytes = new Uint8Array(await res.arrayBuffer());
    } catch (error: any) {
      if (attempt === 3 || /answered 4\d\d/.test(error?.message ?? "")) {
        throw error?.name === "TimeoutError" ? new Error(`${pkg.name}@${pkg.version}: ${url} did not answer in ${DOWNLOAD_TIMEOUT_MS / 1000}s`) : error;
      }
    }
  }
  verify(bytes, pkg.integrity, `${pkg.name}@${pkg.version}`);
  mkdirSync(dest, { recursive: true });
  extractTgz(bytes, dest);
}

/**
 * Fills Bun's cache with the lockfile's packages: one `bun install` in a scratch folder downloads
 * them all at once, with the project's registries and credentials (`.npmrc`, `bunfig.toml`)
 */
async function prefetch(projectDir: string, bun: string) {
  const scratch = tmpName(storeRoot(), "prefetch");
  mkdirSync(scratch, { recursive: true });
  try {
    for (const file of ["package.json", "bun.lock", ".npmrc", "bunfig.toml"]) {
      if (existsSync(join(projectDir, file))) copyFileSync(join(projectDir, file), join(scratch, file));
    }
    await run(bun, ["install", "--frozen-lockfile", "--ignore-scripts"], scratch, PREFETCH_TIMEOUT_MS);
  } finally {
    rmSync(scratch, { recursive: true, force: true });
  }
}

/**
 * Links `path` to `target`. `finalPath` is where `path` will be once its directory is moved into place
 * (entries are built in `.tmp/`): relative links are computed from there.
 */
function link(target: string, path: string, finalPath = path) {
  mkdirSync(dirname(path), { recursive: true });
  // Junctions need no privileges on Windows and take absolute targets
  if (process.platform === "win32") symlinkSync(target, path, "junction");
  // A relative link is followed from the real directory it is in: compute it between real paths
  // (`/var` is a link to `/private/var` on macOS, so `../` counts differ otherwise)
  else symlinkSync(relative(realPath(dirname(finalPath)), realPath(target)), path);
}

/** `realpath` of the longest existing prefix of `path`, with the rest appended */
function realPath(path: string): string {
  const rest: string[] = [];
  let current = path;
  while (true) {
    try {
      return join(realpathSync(current), ...rest.reverse());
    } catch {
      const parent = dirname(current);
      if (parent === current) return path;
      rest.push(current.slice(parent.length).replace(/^[\\/]/, ""));
      current = parent;
    }
  }
}

export function entryDir(entry: string): string {
  return join(storeRoot(), entry);
}

function entryComplete(entry: string): boolean {
  return existsSync(join(entryDir(entry), ".complete"));
}

const INSTALL_SCRIPTS = ["preinstall", "install", "postinstall"];

/** Creates the store entry of `path` (its files and links to its dependencies' entries) */
async function ensureEntry(graph: ResolvedGraph, path: string): Promise<{ downloaded: boolean; scripts: boolean }> {
  const entry = graph.entries.get(path)!;
  if (entryComplete(entry)) return { downloaded: false, scripts: false };
  const pkg = graph.nodes.get(path)!;
  const tmp = tmpName(storeRoot(), entry);
  const modules = join(tmp, "node_modules");
  const pkgDir = join(modules, pkg.name);
  const cached = bunCacheDir(pkg);
  if (cached) copyPackage(cached, pkgDir);
  else await download(pkg, pkgDir);
  for (const [dep, target] of graph.edges.get(path)!) {
    const targetPkg = graph.nodes.get(target)!;
    link(join(entryDir(graph.entries.get(target)!), "node_modules", targetPkg.name), join(modules, dep), join(entryDir(entry), "node_modules", dep));
  }
  let scripts = false;
  try {
    const manifest = JSON.parse(readFileSync(join(pkgDir, "package.json"), "utf-8"));
    scripts = INSTALL_SCRIPTS.some((s) => manifest.scripts?.[s]);
  } catch {}
  writeFileSync(join(tmp, ".complete"), `${pkg.name}@${pkg.version}\n`);
  publish(tmp, entryDir(entry));
  return { downloaded: !cached, scripts };
}

async function pool<T>(items: T[], limit: number, work: (item: T) => Promise<void>) {
  let next = 0;
  await Promise.all(Array.from({ length: Math.min(limit, items.length) }, async () => {
    while (next < items.length) await work(items[next++]);
  }));
}

// ---------- projects ----------

export interface ProjectState {
  format: string;
  lockHash: string;
  /** Store entries the project uses */
  entries: string[];
  /** Names linked in `.nata/modules/node_modules` */
  links: string[];
}

function stateFile(projectDir: string): string {
  return join(projectDir, ".nata", "modules", "store.json");
}

export function readProjectState(projectDir: string): ProjectState | null {
  try {
    return JSON.parse(readFileSync(stateFile(projectDir), "utf-8"));
  } catch {
    return null;
  }
}

function projectsDir(): string {
  return join(comHome(), "projects");
}

/** One file per project, so concurrent installs never overwrite each other's registration */
function register(projectDir: string) {
  const root = realpathSync(projectDir);
  mkdirSync(projectsDir(), { recursive: true });
  const file = join(projectsDir(), `${createHash("sha256").update(root).digest("hex").slice(0, 16)}.json`);
  writeFileSync(file, JSON.stringify({ root, updated: new Date().toISOString() }));
}

/** Projects registered with the store; unregisters those that no longer use it */
export function registeredProjects(): string[] {
  const roots: string[] = [];
  let files: string[] = [];
  try {
    files = readdirSync(projectsDir());
  } catch {}
  for (const file of files) {
    try {
      const { root } = JSON.parse(readFileSync(join(projectsDir(), file), "utf-8"));
      if (existsSync(stateFile(root))) roots.push(root);
      else rmSync(join(projectsDir(), file), { force: true });
    } catch {}
  }
  return roots;
}

function lockHash(text: string): string {
  return createHash("sha256").update(`${STORE_FORMAT}\n${process.platform}-${process.arch}\n${text}`).digest("hex");
}

function upToDate(projectDir: string, hash: string): boolean {
  const state = readProjectState(projectDir);
  if (!state || state.format !== STORE_FORMAT || state.lockHash !== hash) return false;
  const farm = farmDir(projectDir);
  return (
    state.entries.every(entryComplete) &&
    state.links.every((name) => existsSync(join(farm, name))) &&
    isLinkTo(join(projectDir, "node_modules"), farm)
  );
}

export class StoreUnsupportedError extends Error {}

export interface InstallResult {
  upToDate: boolean;
  packages: number;
  downloaded: number;
  created: number;
  /** Packages with install scripts, which the store does not run */
  skippedScripts: string[];
  ms: number;
}

/**
 * Makes every package of the project's `bun.lock` available from the store and links them into
 * `.nata/modules/node_modules`. Fast when nothing changed (a hash and a few `stat`s).
 */
export async function installProject(projectDir: string, bun: string, progress: (step: string) => void = () => {}): Promise<InstallResult> {
  const started = performance.now();
  if (!existsSync(join(projectDir, "package.json"))) {
    return { upToDate: true, packages: 0, downloaded: 0, created: 0, skippedScripts: [], ms: 0 };
  }
  if (!lockfileCurrent(projectDir)) progress("Resolving package versions (bun install --lockfile-only)...");
  await ensureLockfile(projectDir, bun);
  const text = readFileSync(join(projectDir, "bun.lock"), "utf-8");
  const hash = lockHash(text);
  if (upToDate(projectDir, hash)) {
    register(projectDir);
    return { upToDate: true, packages: readProjectState(projectDir)!.entries.length, downloaded: 0, created: 0, skippedScripts: [], ms: performance.now() - started };
  }

  const lock = parseLockfile(text);
  if (lock.unsupported.length) {
    throw new StoreUnsupportedError(`the store only installs registry packages (not ${lock.unsupported.slice(0, 3).join(", ")})`);
  }
  const graph = resolveGraph(lock);
  removeAbandonedTmp();
  mkdirSync(join(storeRoot(), ".tmp"), { recursive: true });
  const missing = [...graph.nodes.keys()].filter((p) => !entryComplete(graph.entries.get(p)!) && !bunCacheDir(graph.nodes.get(p)!));
  // Many packages to fetch: Bun downloads them faster (and with the project's registry settings);
  // what it could not fetch is downloaded below
  if (missing.length > 3) {
    progress(`Downloading ${missing.length} packages...`);
    await prefetch(projectDir, bun).catch(() => {});
  }

  let downloaded = 0;
  let created = 0;
  const skippedScripts: string[] = [];
  let done = 0;
  await pool([...graph.nodes.keys()], DOWNLOADS, async (path) => {
    const fresh = !entryComplete(graph.entries.get(path)!);
    const result = await ensureEntry(graph, path);
    progress(`Linking packages from the shared store (${++done}/${graph.nodes.size})...`);
    if (fresh) created++;
    if (result.downloaded) downloaded++;
    if (result.scripts) skippedScripts.push(graph.nodes.get(path)!.name);
  });

  // Links the project sees, replacing the previous ones (files the framework writes there, such as
  // `core`, stay unless a package of that name replaces them)
  const farm = farmDir(projectDir);
  mkdirSync(farm, { recursive: true });
  const previous = new Set(readProjectState(projectDir)?.links ?? []);
  // The folder name is the lockfile key, not the package name: an alias (`"@swc/legacy-helpers":
  // "npm:@swc/helpers@0.4.14"`) lives under its alias, next to the real `@swc/helpers`
  const linkName = (path: string) => pathSegments(path).at(-1)!;
  const links = graph.topLevel.map(linkName);
  for (const name of new Set([...previous, ...links])) {
    const at = join(farm, name);
    let exists = false;
    try {
      lstatSync(at);
      exists = true;
    } catch {}
    if (exists && (previous.has(name) || links.includes(name))) rmSync(at, { recursive: true, force: true });
  }
  for (const path of graph.topLevel) {
    const pkg = graph.nodes.get(path)!;
    link(join(entryDir(graph.entries.get(path)!), "node_modules", pkg.name), join(farm, linkName(path)));
  }

  // The project's `node_modules` is one link to them (see the top of this file)
  linkNodeModules(projectDir);

  const state: ProjectState = { format: STORE_FORMAT, lockHash: hash, entries: [...new Set(graph.entries.values())].sort(), links: links.sort() };
  writeFileSync(stateFile(projectDir), JSON.stringify(state, null, 2));
  register(projectDir);
  return { upToDate: false, packages: state.entries.length, downloaded, created, skippedScripts: [...new Set(skippedScripts)].sort(), ms: performance.now() - started };
}

// ---------- inspection and cleanup ----------

export interface StoreEntryInfo {
  entry: string;
  name: string;
  version: string;
}

export function parseEntryName(entry: string): StoreEntryInfo | null {
  const match = /^(.+)@([^@_]+)_([0-9a-f]{16})$/.exec(entry);
  return match ? { entry, name: match[1].replace("+", "/"), version: match[2] } : null;
}

export function listEntries(): StoreEntryInfo[] {
  let names: string[] = [];
  try {
    names = readdirSync(storeRoot());
  } catch {}
  return names
    .filter((n) => !n.startsWith("."))
    .map(parseEntryName)
    .filter((e): e is StoreEntryInfo => !!e && entryComplete(e.entry));
}

/** Store entry → projects using it */
export function usage(): Map<string, string[]> {
  const used = new Map<string, string[]>();
  for (const root of registeredProjects()) {
    for (const entry of readProjectState(root)?.entries ?? []) {
      used.set(entry, [...(used.get(entry) ?? []), root]);
    }
  }
  return used;
}

export interface GcResult {
  entries: string[];
}

/** Removes the entries no registered project uses */
export function gc(dryRun = false): GcResult {
  const used = usage();
  const entries = listEntries().map((e) => e.entry).filter((e) => !used.has(e));
  if (!dryRun) {
    for (const entry of entries) rmSync(entryDir(entry), { recursive: true, force: true });
    rmSync(join(storeRoot(), ".tmp"), { recursive: true, force: true });
  }
  return { entries };
}

/** Chains of packages from the project to `name` (`["next", "postcss"]`) */
export function why(projectDir: string, name: string): string[][] {
  const lock = parseLockfile(readFileSync(join(projectDir, "bun.lock"), "utf-8"));
  const graph = resolveGraph(lock);
  const chains: string[][] = [];
  const roots = Object.keys(lock.rootDependencies)
    .map((dep) => graph.topLevel.find((p) => graph.nodes.get(p)!.name === dep))
    .filter((p): p is string => !!p);
  const queue: Array<[string, string[]]> = roots.map((p) => [p, [graph.nodes.get(p)!.name]]);
  const seen = new Set<string>();
  while (queue.length && chains.length < 20) {
    const [path, chain] = queue.shift()!;
    if (graph.nodes.get(path)!.name === name) {
      chains.push(chain);
      continue;
    }
    if (seen.has(path)) continue;
    seen.add(path);
    for (const [dep, target] of graph.edges.get(path) ?? []) queue.push([target, [...chain, dep]]);
  }
  return chains;
}

/** Version installed for the project (`.nata/modules/node_modules/<name>/package.json`) */
export function installedVersion(projectDir: string, name: string): string | null {
  try {
    return JSON.parse(readFileSync(join(packagesDir(projectDir), name, "package.json"), "utf-8")).version ?? null;
  } catch {
    return null;
  }
}

/** For `com pkg link`: a `node_modules` link to the store links, for editors and `tsc` */
export function linkNodeModules(projectDir: string): "linked" | "exists" {
  const nm = join(projectDir, "node_modules");
  if (existsSync(nm) || (() => { try { return !!readlinkSync(nm); } catch { return false; } })()) {
    return isLinkTo(nm, farmDir(projectDir)) ? "linked" : "exists";
  }
  link(farmDir(projectDir), nm);
  return "linked";
}
