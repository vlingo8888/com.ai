import { existsSync } from "fs";
import { createRequire } from "module";
import { dirname, join } from "path";

/** Platforms that get a prebuilt `@com.ai.vn/compiler-<platform>-<arch>` package on npm. */
export const SUPPORTED_COMPILER_TARGETS = [
  "darwin-arm64",
  "darwin-x64",
  "linux-x64",
  "linux-arm64",
  "win32-x64",
] as const;

export function exeName(name: string, platform: NodeJS.Platform = process.platform): string {
  return platform === "win32" ? `${name}.exe` : name;
}

export function compilerPackageName(platform: string = process.platform, arch: string = process.arch): string {
  return `@com.ai.vn/compiler-${platform}-${arch}`;
}

/**
 * Locates the prebuilt com-compiler binary shipped in the platform-specific
 * optional dependency. Returns null when the package is not installed
 * (unsupported platform, or optional deps were skipped).
 */
export function resolvePrebuiltCompiler(fromDir: string = __dirname): string | null {
  try {
    const req = createRequire(join(fromDir, "noop.js"));
    const pkgJson = req.resolve(`${compilerPackageName()}/package.json`);
    const bin = join(dirname(pkgJson), "bin", exeName("com-compiler"));
    return existsSync(bin) ? bin : null;
  } catch {
    return null;
  }
}
