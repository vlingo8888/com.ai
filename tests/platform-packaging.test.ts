import { describe, it, expect, beforeEach, afterEach } from "bun:test";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "fs";
import { join } from "path";
import { tmpdir } from "os";
import { spawnSync } from "child_process";
import { SUPPORTED_COMPILER_TARGETS, compilerPackageName, exeName, resolvePrebuiltCompiler } from "../src/core/binaries";
import { buildPlatformPackages } from "../scripts/build-platform-packages";
import packageJson from "../package.json";

const ROOT = join(import.meta.dir, "..");

describe("Platform binaries: naming", () => {
  it("adds .exe only on Windows", () => {
    expect(exeName("com-compiler", "win32")).toBe("com-compiler.exe");
    expect(exeName("com-compiler", "darwin")).toBe("com-compiler");
    expect(exeName("com-compiler", "linux")).toBe("com-compiler");
  });

  it("builds the compiler package name from platform and arch", () => {
    expect(compilerPackageName("win32", "x64")).toBe("@com.ai.vn/compiler-win32-x64");
  });

  it("declares an optional dependency for every supported target, locked to the CLI version", () => {
    const optional = (packageJson as any).optionalDependencies as Record<string, string>;
    for (const target of SUPPORTED_COMPILER_TARGETS) {
      expect(optional[`@com.ai.vn/compiler-${target}`]).toBe(packageJson.version);
    }
  });

  it("points every bin entry at the Node launcher", () => {
    for (const entry of Object.values(packageJson.bin)) {
      expect(entry).toBe("./bin/com.js");
    }
  });
});

describe("resolvePrebuiltCompiler", () => {
  let tmp: string;

  beforeEach(() => {
    tmp = mkdtempSync(join(tmpdir(), "com-bin-"));
  });

  afterEach(() => {
    rmSync(tmp, { recursive: true, force: true });
  });

  it("returns null when the platform package is not installed", () => {
    expect(resolvePrebuiltCompiler(tmp)).toBeNull();
  });

  it("finds the binary inside the installed platform package", () => {
    const pkgDir = join(tmp, "node_modules", compilerPackageName());
    mkdirSync(join(pkgDir, "bin"), { recursive: true });
    writeFileSync(join(pkgDir, "package.json"), JSON.stringify({ name: compilerPackageName(), version: "0.0.0" }));
    writeFileSync(join(pkgDir, "bin", exeName("com-compiler")), "");

    expect(resolvePrebuiltCompiler(tmp)).toBe(join(pkgDir, "bin", exeName("com-compiler")));
  });
});

describe("buildPlatformPackages", () => {
  let tmp: string;

  beforeEach(() => {
    tmp = mkdtempSync(join(tmpdir(), "com-pkg-"));
  });

  afterEach(() => {
    rmSync(tmp, { recursive: true, force: true });
  });

  it("creates os/cpu-scoped packages only for targets with a binary", () => {
    const bins = join(tmp, "binaries");
    mkdirSync(join(bins, "win32-x64"), { recursive: true });
    mkdirSync(join(bins, "linux-x64"), { recursive: true });
    writeFileSync(join(bins, "win32-x64", "com-compiler.exe"), "MZ");
    writeFileSync(join(bins, "linux-x64", "com-compiler"), "ELF");

    const built = buildPlatformPackages(bins, join(tmp, "out"), "9.9.9");
    expect(built).toHaveLength(2);

    const winPkg = JSON.parse(readFileSync(join(tmp, "out", "compiler-win32-x64", "package.json"), "utf-8"));
    expect(winPkg.name).toBe("@com.ai.vn/compiler-win32-x64");
    expect(winPkg.version).toBe("9.9.9");
    expect(winPkg.os).toEqual(["win32"]);
    expect(winPkg.cpu).toEqual(["x64"]);
    expect(existsSync(join(tmp, "out", "compiler-win32-x64", "bin", "com-compiler.exe"))).toBe(true);
    expect(existsSync(join(tmp, "out", "compiler-darwin-arm64"))).toBe(false);
  });
});

describe("bin/com.js launcher", () => {
  it("resolves a Bun runtime", () => {
    const { resolveBun } = require("../bin/com.js");
    const bunPath = resolveBun();
    expect(bunPath).toBeTruthy();
    expect(existsSync(bunPath)).toBe(true);
  });

  it("runs the CLI under plain Node and forwards the exit code", () => {
    const node = Bun.which("node");
    if (!node) return;
    const res = spawnSync(node, [join(ROOT, "bin", "com.js"), "--version"], { encoding: "utf-8" });
    expect(res.status).toBe(0);
    expect(res.stdout).toContain(packageJson.version);
  });
});
