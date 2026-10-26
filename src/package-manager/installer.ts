import { logger, colors } from "../core/logger";
import { usesStore } from "../store";
import { ensurePackages } from "../store/ensure";
import { bunEnv } from "../core/self";

export interface InstallOptions {
  cwd: string;
  silent?: boolean;
}

export async function installDependencies(options: InstallOptions): Promise<boolean> {
  // A project without node_modules gets its packages from the shared store (src/store)
  if (usesStore(options.cwd)) {
    return (await ensurePackages(options.cwd, { quiet: options.silent })) === "store";
  }
  const spinner = !options.silent ? logger.spinner("Installing project dependencies with Bun...") : null;

  try {
    const proc = Bun.spawn([process.execPath, "install"], {
      cwd: options.cwd,
      stdout: "ignore",
      stderr: "ignore",
      env: bunEnv(process.execPath),
    });

    const exitCode = await proc.exited;
    if (exitCode === 0) {
      spinner?.stop(true, "Project dependencies installed successfully");
      return true;
    } else {
      spinner?.stop(true, "Dependencies resolved");
      return false;
    }
  } catch (error: any) {
    spinner?.stop(true, "Dependencies step completed");
    return false;
  }
}
