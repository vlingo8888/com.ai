import { resolve } from "path";
import { logger } from "../core/logger";
import { findEngineBinary } from "./dev";
import { engineEnv } from "../core/binaries";

export interface StartOptions {
  dir?: string;
  port?: string | number;
  hostname?: string;
}

/** `com start`: serves the production build made by `com build` */
export async function startCommand(options: StartOptions = {}) {
  const projectDir = resolve(process.cwd(), options.dir || ".");
  const engine = findEngineBinary();
  if (!engine) {
    logger.error("Engine binary not found", "Reinstall @com.ai.vn/cli or set COM_COMPILER_PATH");
    process.exit(1);
  }
  const port = String(options.port ?? process.env.PORT ?? 3000);
  const args = [engine, "start", "--dir", projectDir, "--port", port];
  if (options.hostname) args.push("--hostname", options.hostname);
  const proc = Bun.spawn(args, {
    stdout: "inherit",
    stderr: "inherit",
    stdin: "inherit",
    env: { ...process.env, ...engineEnv(), COM_START_TIME: String(Math.round(performance.timeOrigin)) },
  });
  // The engine finishes requests in flight on SIGINT / SIGTERM: pass them on and wait
  for (const signal of ["SIGINT", "SIGTERM"] as const) process.on(signal, () => proc.kill(signal));
  process.exit(await proc.exited);
}
