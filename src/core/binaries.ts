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

/** Bun releases able to load React's production server build in the worker (1.2.x cannot) */
const MIN_PRODUCTION_BUN = [1, 3];

function bunVersionAtLeast(version: string, min: number[]): boolean {
  const [major = 0, minor = 0] = version.split(".").map((p) => parseInt(p, 10) || 0);
  return major > min[0] || (major === min[0] && minor >= min[1]);
}

/**
 * The Bun the engine's worker should run on: the one installed with the CLI (`bun` dependency), else
 * the Bun running the CLI when it is recent enough. `null` leaves the engine to find one.
 */
export function resolveWorkerBun(fromDir: string = __dirname): string | null {
  try {
    const req = createRequire(join(fromDir, "noop.js"));
    const pkg = req.resolve("bun/package.json");
    for (const name of ["bin/bun.exe", "bin/bun"]) {
      const bin = join(dirname(pkg), name);
      if (existsSync(bin)) return bin;
    }
  } catch {}
  const self = process.execPath;
  if (/[\\/]bun(\.exe)?$/.test(self) && bunVersionAtLeast(Bun.version, MIN_PRODUCTION_BUN)) return self;
  return null;
}

/** Environment for engine processes: `COM_BUN` unless the user set it */
export function engineEnv(): Record<string, string> {
  if (process.env.COM_BUN) return {};
  const bun = resolveWorkerBun();
  return bun ? { COM_BUN: bun } : {};
}
