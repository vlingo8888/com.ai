import { describe, expect, it } from "bun:test";
import {
  createPackageManifest,
  createTsConfigManifest,
  writeProjectManifests,
} from "../src/package-manager/generator";
import { scanFilesForDependencies } from "../src/package-manager/scanner";
import { mkdtempSync, rmSync, existsSync, readFileSync } from "fs";
import { join } from "path";
import { tmpdir } from "os";

describe("Manifest Generator - createPackageManifest", () => {
  it("should create a clean package.json manifest with sanitized name and scripts", () => {
    const scanResult = scanFilesForDependencies([
      { path: "index.tsx", content: "import axios from 'axios';" },
    ]);

    const manifest = createPackageManifest("My Awesome View #105", scanResult);

    expect(manifest.name).toBe("my-awesome-view--105");
    expect(manifest.version).toBe("1.0.0");
    expect(manifest.type).toBe("module");
    expect(manifest.scripts.dev).toBe("com dev");
    expect(manifest.scripts.start).toBe("com dev");
    expect(manifest.dependencies["axios"]).toBe("latest");
    expect(manifest.dependencies["react"]).toBeDefined();
    expect(manifest.devDependencies["typescript"]).toBeDefined();
  });

  it("should fallback to default name when input name is blank", () => {
    const scanResult = scanFilesForDependencies([]);
    const manifest = createPackageManifest("---", scanResult);
    expect(manifest.name).toBe("com-view-app");
  });
});

describe("Manifest Generator - createTsConfigManifest", () => {
  it("should create tsconfig with path aliases and react-jsx", () => {
    const tsconfig = createTsConfigManifest();

    expect(tsconfig.compilerOptions.jsx).toBe("react-jsx");
    expect(tsconfig.compilerOptions.paths["@/*"]).toEqual(["./*"]);
    expect(tsconfig.compilerOptions.paths["~/*"]).toEqual(["./*"]);
    expect(tsconfig.compilerOptions.moduleResolution).toBe("bundler");
    expect(tsconfig.include).toContain("**/*.tsx");
  });
});

describe("Manifest Generator - writeProjectManifests", () => {
  it("should write package.json and tsconfig.json on disk if missing", async () => {
    const tempDir = mkdtempSync(join(tmpdir(), "com-cli-test-"));

    try {
      const files = [
        { path: "App.tsx", content: "import { motion } from 'framer-motion';" },
      ];

      const res = await writeProjectManifests(tempDir, "demo-app", files);

      expect(res.isPackageCreated).toBe(true);
      expect(res.isTsConfigCreated).toBe(true);
      expect(res.isTailwindCreated).toBe(true);
      expect(res.detectedPackages).toContain("framer-motion");

      // Verify files exist on disk
      const pkgPath = join(tempDir, "package.json");
      const tsPath = join(tempDir, "tsconfig.json");
      const cssPath = join(tempDir, "app", "globals.css");

      expect(existsSync(pkgPath)).toBe(true);
      expect(existsSync(tsPath)).toBe(true);
      expect(existsSync(cssPath)).toBe(true);

      const parsedPkg = JSON.parse(readFileSync(pkgPath, "utf-8"));
      expect(parsedPkg.dependencies["framer-motion"]).toBe("latest");
      expect(parsedPkg.devDependencies["tailwindcss"]).toBe("^4.0.0");
      expect(parsedPkg.devDependencies["@tailwindcss/postcss"]).toBe("^4.0.0");

      const cssContent = readFileSync(cssPath, "utf-8");
      expect(cssContent).toContain('@import "tailwindcss";');

      // Second run: should not overwrite existing files
      const secondRun = await writeProjectManifests(tempDir, "demo-app", files);
      expect(secondRun.isPackageCreated).toBe(false);
      expect(secondRun.isTsConfigCreated).toBe(false);
      expect(secondRun.isTailwindCreated).toBe(false);
    } finally {
      rmSync(tempDir, { recursive: true, force: true });
    }
  });
});
