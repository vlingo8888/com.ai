import { existsSync } from "fs";
import { join, resolve } from "path";
import { logger } from "../core/logger";
import { findEngineBinary } from "./dev";
import { engineEnv } from "../core/binaries";
import { execReplace } from "../core/exec";
import { STANDALONE_DIR, standaloneHome } from "./standalone";

export interface StartOptions {
  dir?: string;
  port?: string | number;
  hostname?: string;
  /** Serve `.nata/standalone` (made by `com build --standalone`) */
  standalone?: boolean;
}

/** `com start`: serves the production build made by `com build` */
export async function startCommand(options: StartOptions = {}) {
  const projectDir = resolve(process.cwd(), options.dir || ".", options.standalone ? STANDALONE_DIR : ".");
  if (options.standalone && !existsSync(join(projectDir, ".nata", "build", "manifest.json"))) {
    logger.error("No standalone build", `Run \`com build --standalone\` first (${projectDir})`);
    process.exit(1);
  }
  const engine = findEngineBinary();
  if (!engine) {
    logger.error("Engine binary not found", "Reinstall @com.ai.vn/cli or set COM_COMPILER_PATH");
    process.exit(1);
  }
  const port = String(options.port ?? process.env.PORT ?? 3000);
  const args = [engine, "start", "--dir", projectDir, "--port", port];
  if (options.hostname) args.push("--hostname", options.hostname);
  // A standalone build ships the framework's React in `.nata/home` (nothing to download on the server)
  const home = process.env.COM_HOME ? null : standaloneHome(projectDir);
  const env = { ...process.env, ...engineEnv(projectDir), ...(home ? { COM_HOME: home } : {}), COM_START_TIME: String(Math.round(performance.timeOrigin)) };
  // The engine takes over this pid, so signals reach it directly and no Bun runtime (~55MB) stays
  // alive next to the production server. Returns only where that is not possible (Windows)
  execReplace(engine, args.slice(1), env);
  const proc = Bun.spawn(args, { stdout: "inherit", stderr: "inherit", stdin: "inherit", env });
  // The engine finishes requests in flight on SIGINT / SIGTERM: pass them on and wait
  for (const signal of ["SIGINT", "SIGTERM"] as const) process.on(signal, () => proc.kill(signal));
  process.exit(await proc.exited);
}
