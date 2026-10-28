import { findEngineBinary } from "../commands/dev";

/**
 * Adds the `es5` field the cloud runtime evaluates with `new Function`: CommonJS with classic
 * `React.createElement` JSX, compiled by the Rust engine's SWC (`com-compiler es5`) like the web
 * editor does. Without it the published app runs the raw source and fails on the first `import`.
 * Non-code files get an empty `es5`; a file that does not compile gets one too and is listed in `failed`.
 */
export function compileAllEs5<T extends { path: string; content: string }>(
  files: T[],
  failed: Array<{ path: string; message: string }> = []
): Array<T & { es5: string }> {
  const engine = findEngineBinary();
  if (!engine) {
    throw new Error("The com engine (com-compiler) is missing: reinstall com to compile files for the cloud.");
  }
  const input = JSON.stringify(files.map((f) => ({ path: f.path, content: f.content })));
  const proc = Bun.spawnSync([engine, "es5"], { stdin: Buffer.from(input), stdout: "pipe", stderr: "pipe" });
  if (proc.exitCode !== 0) {
    const stderr = proc.stderr.toString().trim();
    throw new Error(
      /unrecognized subcommand/.test(stderr)
        ? "The com engine is too old to compile files for the cloud: update com."
        : `The com engine failed to compile files: ${stderr}`
    );
  }
  const output: Array<{ path: string; es5: string; error?: string }> = JSON.parse(proc.stdout.toString());
  return files.map((f, i) => {
    const out = output[i];
    if (out.error) failed.push({ path: f.path, message: out.error });
    return { ...f, es5: out.es5 };
  });
}
