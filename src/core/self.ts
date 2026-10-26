import { dirname } from "path";

/**
 * Whether this is the standalone `com` executable (`bun build --compile`): its modules live in
 * Bun's embedded file system (`/$bunfs/...`, `B:\~BUN\...` on Windows) and it embeds Bun itself.
 */
export function isCompiled(main: string = Bun.main): boolean {
  return main.startsWith("/$bunfs/") || /^[a-z]:[\\/]~BUN[\\/]/i.test(main);
}

/**
 * Environment for a process running `bin` as the Bun CLI: the standalone `com` acts as Bun only
 * with `BUN_BE_BUN=1`, otherwise it would run as `com` again.
 */
export function bunEnv(
  bin: string,
  env: Record<string, string | undefined> = process.env,
  compiled: boolean = isCompiled(),
): Record<string, string | undefined> {
  return compiled && bin === process.execPath ? { ...env, BUN_BE_BUN: "1" } : env;
}

/**
 * The folder `com` was started from: next to the standalone executable, the engine binaries ship;
 * from source, the folder of this module's caller (`__dirname`, which `--compile` would freeze to
 * the build machine's path).
 */
export function selfDir(fallback: string, compiled: boolean = isCompiled()): string {
  return compiled ? dirname(process.execPath) : fallback;
}
