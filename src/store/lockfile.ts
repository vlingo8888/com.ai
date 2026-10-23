/**
 * `bun.lock` → the packages a project needs on this machine, and the store entry of each.
 *
 * Bun resolves versions (`bun install --lockfile-only`); the lockfile describes a hoisted
 * `node_modules` tree: key `next/postcss` is the `postcss` nested under `next`. A package's
 * dependencies are found the way Node finds them in that tree (nested first, then up to the root).
 *
 * A store entry is one package version *with* the versions its dependencies resolved to, so two
 * projects share an entry only when the package would behave the same in both (pnpm's global virtual
 * store does the same). The key hashes every (package, dependency, resolved package) edge reachable
 * from the package, which also covers peer dependencies and cycles.
 */

import { createHash } from "crypto";

export interface LockPackage {
  /** Key in the lockfile (install path in the hoisted tree) */
  path: string;
  name: string;
  version: string;
  /** `""` for the default registry */
  registry: string;
  integrity?: string;
  dependencies: Record<string, string>;
  optionalDependencies: Record<string, string>;
  peerDependencies: Record<string, string>;
  optionalPeers: string[];
  os?: string | string[];
  cpu?: string | string[];
}

export interface Lockfile {
  /** Dependencies of the project itself (all kinds), by name */
  rootDependencies: Record<string, string>;
  /** Fields of the project's package.json the lockfile was made from */
  workspace: Record<string, Record<string, string>>;
  packages: Map<string, LockPackage>;
  /** Packages the store cannot install (git, file, tarball URL…) */
  unsupported: string[];
}

/** `bun.lock` is JSON with trailing commas */
export function parseJsonc(text: string): any {
  let out = "";
  for (let i = 0; i < text.length; i++) {
    const c = text[i];
    if (c === '"') {
      // Copy the whole string, escapes included
      let j = i + 1;
      while (j < text.length && text[j] !== '"') j += text[j] === "\\" ? 2 : 1;
      out += text.slice(i, j + 1);
      i = j;
      continue;
    }
    if (c === ",") {
      let j = i + 1;
      while (j < text.length && /\s/.test(text[j])) j++;
      if (text[j] === "}" || text[j] === "]") continue;
    }
    out += c;
  }
  return JSON.parse(out);
}

/** `@scope/name@1.2.3` → `["@scope/name", "1.2.3"]` */
export function splitNameVersion(spec: string): [string, string] {
  const at = spec.lastIndexOf("@");
  return at > 0 ? [spec.slice(0, at), spec.slice(at + 1)] : [spec, ""];
}

const SEMVER = /^\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.+-]+)?$/;

export function parseLockfile(text: string): Lockfile {
  const json = parseJsonc(text);
  const workspace = json.workspaces?.[""] ?? {};
  const rootDependencies = {
    ...workspace.peerDependencies,
    ...workspace.optionalDependencies,
    ...workspace.devDependencies,
    ...workspace.dependencies,
  };
  const packages = new Map<string, LockPackage>();
  const unsupported: string[] = [];
  for (const [path, value] of Object.entries<any[]>(json.packages ?? {})) {
    const [name, version] = splitNameVersion(String(value[0]));
    // npm packages: ["name@1.2.3", registry, meta, integrity]; anything else (git, file:, URLs,
    // workspaces) has another shape
    if (!SEMVER.test(version) || value.length !== 4 || typeof value[1] !== "string") {
      unsupported.push(String(value[0]));
      continue;
    }
    const meta = value[2] ?? {};
    packages.set(path, {
      path,
      name,
      version,
      registry: value[1],
      integrity: typeof value[3] === "string" ? value[3] : undefined,
      dependencies: meta.dependencies ?? {},
      optionalDependencies: meta.optionalDependencies ?? {},
      peerDependencies: meta.peerDependencies ?? {},
      optionalPeers: meta.optionalPeers ?? [],
      os: meta.os,
      cpu: meta.cpu,
    });
  }
  return { rootDependencies, workspace, packages, unsupported };
}

/** `os` / `cpu` fields: a value, a list, or `!value` exclusions */
export function platformMatches(field: string | string[] | undefined, value: string): boolean {
  if (field === undefined) return true;
  const list = Array.isArray(field) ? field : [field];
  if (list.length === 0 || list.includes("none")) return list.length === 0;
  const allowed = list.filter((v) => !v.startsWith("!"));
  if (list.includes(`!${value}`)) return false;
  return allowed.length === 0 || allowed.includes(value);
}

/** `a/@s/b/c` → `["a", "@s/b", "c"]` */
export function pathSegments(path: string): string[] {
  const parts = path.split("/");
  const segments: string[] = [];
  for (let i = 0; i < parts.length; i++) {
    segments.push(parts[i].startsWith("@") && i + 1 < parts.length ? `${parts[i]}/${parts[++i]}` : parts[i]);
  }
  return segments;
}

export interface Platform {
  os: string;
  cpu: string;
}

export interface ResolvedGraph {
  /** Packages installed on this platform, by lockfile path */
  nodes: Map<string, LockPackage>;
  /** Lockfile path → dependency name → lockfile path of the package it resolves to */
  edges: Map<string, Map<string, string>>;
  /** Lockfile path → store entry name */
  entries: Map<string, string>;
  /** Packages at the top of the hoisted tree: what the project sees (direct and hoisted) */
  topLevel: string[];
}

/**
 * The packages this platform installs (reachable from the project, optional ones only when their
 * `os`/`cpu` match), how each one's dependencies resolve, and their store entries
 */
export function resolveGraph(lock: Lockfile, platform: Platform = { os: process.platform, cpu: process.arch }): ResolvedGraph {
  const fits = (p: LockPackage) => platformMatches(p.os, platform.os) && platformMatches(p.cpu, platform.cpu);
  const lookup = (from: string[], dep: string): string | undefined => {
    for (let k = from.length; k >= 0; k--) {
      const candidate = [...from.slice(0, k), dep].join("/");
      const pkg = lock.packages.get(candidate);
      if (pkg && fits(pkg)) return candidate;
    }
    return undefined;
  };

  const nodes = new Map<string, LockPackage>();
  const edges = new Map<string, Map<string, string>>();
  const queue: string[] = [];
  const visit = (path: string | undefined) => {
    if (path && !nodes.has(path)) {
      nodes.set(path, lock.packages.get(path)!);
      queue.push(path);
    }
  };
  for (const name of Object.keys(lock.rootDependencies)) visit(lookup([], name));
  while (queue.length) {
    const path = queue.shift()!;
    const pkg = nodes.get(path)!;
    const segments = pathSegments(path);
    const out = new Map<string, string>();
    const names = new Set([...Object.keys(pkg.dependencies), ...Object.keys(pkg.optionalDependencies), ...Object.keys(pkg.peerDependencies)]);
    for (const dep of names) {
      // A package listing itself would replace its own files with a link
      if (dep === pkg.name) continue;
      const target = lookup(segments, dep);
      if (!target) continue; // optional (dependency or peer) and not installed on this platform
      out.set(dep, target);
      visit(target);
    }
    edges.set(path, out);
  }

  const id = (path: string) => `${nodes.get(path)!.name}@${nodes.get(path)!.version}`;
  const entries = new Map<string, string>();
  for (const path of nodes.keys()) {
    const seen = new Set([path]);
    const stack = [path];
    const triples: string[] = [id(path)];
    while (stack.length) {
      const current = stack.pop()!;
      for (const [dep, target] of edges.get(current)!) {
        triples.push(`${id(current)}>${dep}>${id(target)}`);
        if (!seen.has(target)) {
          seen.add(target);
          stack.push(target);
        }
      }
    }
    triples.sort();
    const hash = createHash("sha256").update(triples.join("\n")).digest("hex").slice(0, 16);
    const pkg = nodes.get(path)!;
    entries.set(path, `${pkg.name.replace("/", "+")}@${pkg.version}_${hash}`);
  }

  const topLevel = [...nodes.keys()].filter((path) => pathSegments(path).length === 1);
  return { nodes, edges, entries, topLevel };
}
