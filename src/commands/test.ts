import { existsSync } from "fs";
import { resolve, relative, join } from "path";
import { logger, colors } from "../core/logger";

export interface TestOptions {
  dir?: string;
  pattern?: string;
  filter?: string;
  watch?: boolean;
  coverage?: boolean;
  bail?: boolean;
  timeout?: number;
  updateSnapshots?: boolean;
  only?: boolean;
  todo?: boolean;
  json?: boolean;
  env?: string;
}

/**
 * Builds array of CLI arguments to pass to `bun test`
 */
export function buildBunTestArgs(pattern?: string, options: TestOptions = {}): string[] {
  const args: string[] = ["test"];

  if (pattern && pattern.trim().length > 0) {
    args.push(pattern.trim());
  }

  if (options.filter) {
    args.push("--test-name-pattern", options.filter);
  }

  if (options.watch) {
    args.push("--watch");
  }

  if (options.coverage) {
    args.push("--coverage");
  }

  if (options.bail) {
    args.push("--bail");
  }

  if (options.timeout !== undefined && options.timeout > 0) {
    args.push("--timeout", String(options.timeout));
  }

  if (options.updateSnapshots) {
    args.push("--update-snapshots");
  }

  if (options.only) {
    args.push("--only");
  }

  if (options.todo) {
    args.push("--todo");
  }

  if (options.json) {
    args.push("--json");
  }

  if (options.env) {
    args.push("--env-file", options.env);
  }

  return args;
}

import { ensureRuntimeEnvironment } from "../core/runtime";

/**
 * Executes project tests using Bun's native high-performance test runner
 */
export async function testCommand(rawArgs: string[] = [], options: TestOptions = {}): Promise<number> {
  const projectDir = resolve(process.cwd(), options.dir || ".");

  // Automatically provision runtime files (.nata/core.ts & node_modules/core) for testing
  try {
    ensureRuntimeEnvironment(projectDir);
  } catch {}

  // If pattern not explicitly passed in options, check if first positional arg is a pattern
  let pattern = options.pattern;
  if (!pattern && rawArgs.length > 0 && !rawArgs[0].startsWith("-")) {
    pattern = rawArgs[0];
  }

  const bunArgs = buildBunTestArgs(pattern, options);
  const hasSchema = existsSync(join(projectDir, "schema.sql"));

  if (!options.json) {
    logger.hero();
    const relDir = relative(process.cwd(), projectDir) || "./";
    
    logger.card("COM.AI.VN TEST RUNNER", [
      {
        label: "Engine",
        value: `⚡ Bun Test Engine (v${Bun.version})`,
        color: colors.bold + colors.green,
      },
      {
        label: "Target Directory",
        value: relDir,
        color: colors.cyan,
      },
      {
        label: "Database Mode",
        value: hasSchema
          ? "● In-Memory Postgres MockDB (Auto-loaded schema.sql)"
          : "○ In-Memory Embedded Postgres (@pglite/core)",
        color: hasSchema ? colors.bold + colors.emerald : colors.yellow,
      },
      {
        label: "Pattern Filter",
        value: pattern ? `"${pattern}"` : "All test files (*.test.ts, *.test.tsx, *.spec.ts)",
        color: colors.sky,
      },
      {
        label: "Execution Mode",
        value: options.watch
          ? "🔄 Watch Mode (Live Re-run on save)"
          : options.coverage
          ? "📊 Code Coverage Report"
          : "🚀 Fast In-Memory Runner",
        color: options.watch ? colors.yellow : colors.emerald,
      },
      {
        label: "Path Aliases",
        value: "Active (@/* -> src/*, features/*, modules/*)",
        color: colors.darkGray,
      },
    ]);
  }

  // Spawn bun test with inherited stdio to preserve rich colors, terminal dimensions, and interactive controls
  const proc = Bun.spawn(["bun", ...bunArgs], {
    cwd: projectDir,
    stdout: "inherit",
    stderr: "inherit",
    stdin: options.watch ? "inherit" : "ignore",
    env: {
      ...process.env,
      NODE_ENV: "test",
      COM_TEST: "true",
    },
  });

  const exitCode = await proc.exited;

  if (exitCode !== 0 && !options.watch && !options.json) {
    console.log(`\n  ${colors.bold}${colors.red}✖ Some tests failed (Exit code ${exitCode}).${colors.reset}\n`);
  }

  return exitCode;
}
