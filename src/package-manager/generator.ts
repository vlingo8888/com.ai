import { existsSync, mkdirSync } from "fs";
import { join, dirname } from "path";
import { scanFilesForDependencies, ScanResult } from "./scanner";
import { TAILWIND_V4_CSS_TEMPLATE } from "./constants";

export interface PackageManifest {
  name: string;
  version: string;
  type: string;
  scripts: Record<string, string>;
  dependencies: Record<string, string>;
  devDependencies: Record<string, string>;
}

export interface TsConfigManifest {
  compilerOptions: Record<string, any>;
  include: string[];
}

export function createPackageManifest(
  projectName: string,
  scanResult: ScanResult
): PackageManifest {
  const normalizedName = projectName
    .toLowerCase()
    .replace(/[^a-z0-9_-]/g, "-")
    .replace(/^-+|-+$/g, "") || "com-view-app";

  const cleanDependencies: Record<string, string> = {};
  for (const [pkg, ver] of Object.entries(scanResult.dependencies || {})) {
    if (pkg !== "@native" && !pkg.startsWith("@native/")) {
      cleanDependencies[pkg] = ver;
    }
  }

  const cleanDevDependencies: Record<string, string> = {};
  for (const [pkg, ver] of Object.entries(scanResult.devDependencies || {})) {
    if (pkg !== "@native" && !pkg.startsWith("@native/")) {
      cleanDevDependencies[pkg] = ver;
    }
  }

  return {
    name: normalizedName,
    version: "1.0.0",
    type: "module",
    scripts: {
      dev: "com dev",
      start: "com dev",
      build: "bun build ./index.tsx --outfile dist/bundle.js",
    },
    dependencies: cleanDependencies,
    devDependencies: cleanDevDependencies,
  };
}

export function createTsConfigManifest(): TsConfigManifest {
  return {
    compilerOptions: {
      lib: ["ESNext", "DOM", "DOM.Iterable"],
      module: "esnext",
      target: "esnext",
      moduleResolution: "bundler",
      moduleDetection: "force",
      allowImportingTsExtensions: true,
      noEmit: true,
      composite: true,
      strict: true,
      downlevelIteration: true,
      skipLibCheck: true,
      jsx: "react-jsx",
      allowSyntheticDefaultImports: true,
      forceConsistentCasingInFileNames: true,
      allowJs: true,
      types: ["bun-types"],
      baseUrl: ".",
      paths: {
        "@/*": ["./*"],
        "~/*": ["./*"],
      },
    },
    include: ["**/*.ts", "**/*.tsx", "**/*.js", "**/*.jsx"],
  };
}

/**
 * Checks if any CSS file in the project already has Tailwind imports
 */
export function hasTailwindCssFile(files: Array<{ path: string; content?: string }>): boolean {
  return files.some((f) => {
    if (!f.path.endsWith(".css")) return false;
    const content = f.content || "";
    return content.includes("@import \"tailwindcss\"") || content.includes("@tailwind");
  });
}

/**
 * Ensures Tailwind 4 CSS template exists in the project
 */
export async function setupTailwindV4Styles(
  projectDir: string,
  files: Array<{ path: string; content?: string }>
): Promise<boolean> {
  const hasExisting = hasTailwindCssFile(files);
  if (hasExisting) return false;

  const targetCssPath = join(projectDir, "app", "globals.css");
  if (!existsSync(targetCssPath)) {
    const parentDir = dirname(targetCssPath);
    if (!existsSync(parentDir)) {
      mkdirSync(parentDir, { recursive: true });
    }
    await Bun.write(targetCssPath, TAILWIND_V4_CSS_TEMPLATE);
    return true;
  }
  return false;
}

/**
 * Writes package.json, tsconfig.json and Tailwind 4 configuration to project directory
 */
export async function writeProjectManifests(
  projectDir: string,
  projectName: string,
  files: Array<{ path: string; content?: string }>
): Promise<{
  isPackageCreated: boolean;
  isTsConfigCreated: boolean;
  isTailwindCreated: boolean;
  detectedPackages: string[];
}> {
  const packageJsonPath = join(projectDir, "package.json");
  const tsconfigJsonPath = join(projectDir, "tsconfig.json");

  let isPackageCreated = false;
  let isTsConfigCreated = false;
  let detectedPackages: string[] = [];

  if (!existsSync(packageJsonPath)) {
    const scanResult = scanFilesForDependencies(files, projectDir);
    detectedPackages = scanResult.detectedPackages;
    const manifest = createPackageManifest(projectName, scanResult);
    await Bun.write(packageJsonPath, JSON.stringify(manifest, null, 2) + "\n");
    isPackageCreated = true;
  }

  if (!existsSync(tsconfigJsonPath)) {
    const tsconfig = createTsConfigManifest();
    await Bun.write(tsconfigJsonPath, JSON.stringify(tsconfig, null, 2) + "\n");
    isTsConfigCreated = true;
  }

  const isTailwindCreated = await setupTailwindV4Styles(projectDir, files);

  return { isPackageCreated, isTsConfigCreated, isTailwindCreated, detectedPackages };
}
