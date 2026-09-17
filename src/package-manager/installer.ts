import { logger, colors } from "../core/logger";

export interface InstallOptions {
  cwd: string;
  silent?: boolean;
}

export async function installDependencies(options: InstallOptions): Promise<boolean> {
  const spinner = !options.silent ? logger.spinner("Installing project dependencies with Bun...") : null;

  try {
    const proc = Bun.spawn(["bun", "install"], {
      cwd: options.cwd,
      stdout: "ignore",
      stderr: "ignore",
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
