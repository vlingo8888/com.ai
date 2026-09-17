import { readFileSync, existsSync } from "fs";
import { join } from "path";
import { BUILT_IN_MODULES, DEFAULT_DEPENDENCIES, DEFAULT_DEV_DEPENDENCIES } from "./constants";

export interface ScanResult {
  dependencies: Record<string, string>;
  devDependencies: Record<string, string>;
  detectedPackages: string[];
}

/**
 * Normalizes an import specifier into an npm package name
 * Examples:
 *   - 'lodash/debounce' -> 'lodash'
 *   - '@radix-ui/react-dialog/dist/index.js' -> '@radix-ui/react-dialog'
 *   - './components/Button' -> null (relative)
 *   - '@/modules/auth' -> null (alias)
 *   - 'fs/promises' -> null (built-in)
 */
export function normalizePackageName(specifier: string): string | null {
  if (!specifier || typeof specifier !== "string") return null;

  const trimmed = specifier.trim();
  if (!trimmed) return null;

  // 1. Ignore relative paths and internal aliases
  if (
    trimmed.startsWith(".") ||
    trimmed.startsWith("/") ||
    trimmed.startsWith("@/") ||
    trimmed.startsWith("~/") ||
    trimmed.startsWith("#") ||
    trimmed.startsWith("src/")
  ) {
    return null;
  }

  // 2. Ignore Node / Bun built-in modules
  const withoutNodePrefix = trimmed.replace(/^node:/, "");
  if (BUILT_IN_MODULES.has(withoutNodePrefix) || BUILT_IN_MODULES.has(trimmed)) {
    return null;
  }

  // 3. Extract scoped (@org/pkg) or un-scoped package name
  if (trimmed.startsWith("@")) {
    const segments = trimmed.split("/");
    if (segments.length >= 2) {
      return `${segments[0]}/${segments[1]}`;
    }
    return segments[0];
  }

  return trimmed.split("/")[0];
}

/**
 * Extracts raw import and require specifiers from source code string
 */
export function extractImportsFromCode(code: string): string[] {
  if (!code || typeof code !== "string") return [];

  const specifiers: string[] = [];

  // Match:
  // 1. import ... from "pkg"
  // 2. import "pkg"
  // 3. require("pkg")
  // 4. import("pkg")
  // 5. export ... from "pkg"
  const importRegex = /(?:import\s+(?:(?:[\s\S]*?from\s+)?['"]([^'"]+)['"])|(?:require\(\s*['"]([^'"]+)['"]\s*\))|(?:import\(\s*['"]([^'"]+)['"]\s*\))|(?:export\s+[\s\S]*?from\s+['"]([^'"]+)['"]))/g;

  let match;
  while ((match = importRegex.exec(code)) !== null) {
    const raw = match[1] || match[2] || match[3] || match[4];
    if (raw) {
      specifiers.push(raw);
    }
  }

  return specifiers;
}

/**
 * Scans an array of file records and detects all external npm packages
 */
export function scanFilesForDependencies(
  files: Array<{ path: string; content?: string }>,
  projectDir?: string
): ScanResult {
  const discovered = new Set<string>();

  for (const file of files) {
    let content = file.content;
    if (content === undefined && projectDir) {
      try {
        const fullPath = join(projectDir, file.path);
        if (existsSync(fullPath)) {
          content = readFileSync(fullPath, "utf-8");
        }
      } catch {}
    }

    if (!content) continue;

    const rawSpecifiers = extractImportsFromCode(content);
    for (const spec of rawSpecifiers) {
      const pkg = normalizePackageName(spec);
      if (pkg) {
        discovered.add(pkg);
      }
    }
  }

  const dependencies: Record<string, string> = { ...DEFAULT_DEPENDENCIES };
  const devDependencies: Record<string, string> = { ...DEFAULT_DEV_DEPENDENCIES };

  for (const pkg of discovered) {
    if (!dependencies[pkg] && !devDependencies[pkg]) {
      dependencies[pkg] = "latest";
    }
  }

  return {
    dependencies,
    devDependencies,
    detectedPackages: Array.from(discovered).sort(),
  };
}
