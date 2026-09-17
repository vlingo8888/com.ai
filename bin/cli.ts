#!/usr/bin/env bun
import { cloneView } from "../src/commands/clone";
import { devCommand } from "../src/commands/dev";
import { tunnelCommand } from "../src/commands/tunnel";
import { buildCommand } from "../src/commands/build";
import { testCommand } from "../src/commands/test";
import { syncCommand } from "../src/commands/sync";
import {
  dbPullCommand,
  dbMigrateCommand,
  dbListCommand,
  dbDescribeCommand,
  dbSearchCommand,
} from "../src/commands/db";
import { loginCommand, logoutCommand, whoamiCommand } from "../src/commands/login";
import { logger, colors } from "../src/core/logger";
import packageJson from "../package.json";

function printHelp() {
  logger.hero();
  console.log(`
  ${colors.bold}${colors.white}USAGE:${colors.reset}
    ${colors.cyan}$ com${colors.reset} ${colors.green}<command>${colors.reset} [options]

  ${colors.bold}${colors.white}COMMANDS:${colors.reset}
    ${colors.green}login${colors.reset}                 Authenticate via Google (Center Auth)
    ${colors.green}whoami${colors.reset}                Display the currently authenticated user
    ${colors.green}logout${colors.reset}                Log out and revoke local credentials
    ${colors.green}clone${colors.reset}  ${colors.sky}<viewId>${colors.reset}       Download project source code by View ID
    ${colors.green}dev${colors.reset}                   Start local development server with Hot Reload
    ${colors.green}tunnel${colors.reset}                Expose local port to the internet via tunnel-rs
    ${colors.green}build${colors.reset}                 Package project for Production or Zalo Mini App
    ${colors.green}test | t${colors.reset}  ${colors.sky}[pattern]${colors.reset}   Run unit, integration & component tests with Bun Test
    ${colors.green}sync | pull${colors.reset}             Sync database schema, types/db.d.ts, AGENTS.md & dependencies
    ${colors.green}db pull | db sync${colors.reset}     Introspect database schema & generate types/db.d.ts for AI
    ${colors.green}db list | tables${colors.reset}      List all database tables with business descriptions
    ${colors.green}db describe${colors.reset} ${colors.sky}<table>${colors.reset}   Inspect columns, keys, types, and comments of a table
    ${colors.green}db search${colors.reset} ${colors.sky}<query>${colors.reset}    Search database tables and columns by keyword
    ${colors.green}migrate${colors.reset}  ${colors.sky}<sql|file>${colors.reset}  Execute SQL migration and auto-sync database schema
    ${colors.green}version | -v${colors.reset}          Display system, CLI, and runtime engine versions
    ${colors.green}push | save${colors.reset}           Synchronize local changes back to the Cloud

  ${colors.bold}${colors.white}OPTIONS:${colors.reset}
    ${colors.yellow}--watch, -w${colors.reset}            Watch files for changes and re-run tests
    ${colors.yellow}--coverage${colors.reset}                 Generate test coverage report
    ${colors.yellow}--bail, -b${colors.reset}                 Exit test suite immediately on first error
    ${colors.yellow}--filter${colors.reset}      ${colors.darkGray}<pattern>${colors.reset}  Run only tests matching description pattern
    ${colors.yellow}--timeout${colors.reset}     ${colors.darkGray}<ms>${colors.reset}       Per-test timeout in milliseconds
    ${colors.yellow}--target, -t${colors.reset} ${colors.darkGray}<web|zalo>${colors.reset} Target runtime mode (auto-detects Zalo Mini App)
    ${colors.yellow}--tunnel, -T${colors.reset}                  Auto-create public tunnel & QR code during dev
    ${colors.yellow}--provider, -P${colors.reset} ${colors.darkGray}<123c|rs>${colors.reset} Tunnel provider (default: 123c for Zalo, rs for Web)
    ${colors.yellow}--subdomain, -s${colors.reset} ${colors.darkGray}<name>${colors.reset}   Custom subdomain for tunnel
    ${colors.yellow}--dir${colors.reset}        ${colors.darkGray}<path>${colors.reset}     Target directory (default: current directory)
    ${colors.yellow}--port, -p${colors.reset}   ${colors.darkGray}<port>${colors.reset}     Port to listen on in dev mode (default: 3000)
    ${colors.yellow}--api${colors.reset}        ${colors.darkGray}<url>${colors.reset}      Custom backend API URL (default: https://base.myworkbeast.com)
    ${colors.yellow}--url${colors.reset}        ${colors.darkGray}<url>${colors.reset}      Database connection URL
    ${colors.yellow}--env${colors.reset}        ${colors.darkGray}<file>${colors.reset}     Custom env file path
    ${colors.yellow}--json${colors.reset}                 Output results in JSON format (for AI parsing)
    ${colors.yellow}--token${colors.reset}      ${colors.darkGray}<token>${colors.reset}    Pass an access token manually
    ${colors.yellow}--force, -f${colors.reset}            Overwrite existing non-empty directory on clone
    ${colors.yellow}--install${colors.reset}              Explicitly install local node_modules (optional; default is ESM zero-install)
    ${colors.yellow}-h, --help${colors.reset}                 Show this help message
    ${colors.yellow}-v, --version${colors.reset}              Show CLI version

  ${colors.bold}${colors.white}EXAMPLES:${colors.reset}
    ${colors.darkGray}# 1. Start local dev server (auto-detects Web or Zalo Mini App)${colors.reset}
    ${colors.cyan}$ com dev${colors.reset}

    ${colors.darkGray}# 2. Run all unit & integration tests${colors.reset}
    ${colors.cyan}$ com test${colors.reset}

    ${colors.darkGray}# 3. Run specific test file in watch mode${colors.reset}
    ${colors.cyan}$ com test news --watch${colors.reset}

    ${colors.darkGray}# 4. View all database tables and descriptions${colors.reset}
    ${colors.cyan}$ com db list${colors.reset}

    ${colors.darkGray}# 5. Inspect a specific table in detail${colors.reset}
    ${colors.cyan}$ com db describe wellness_assessments${colors.reset}

    ${colors.darkGray}# 6. Search tables/columns by keyword${colors.reset}
    ${colors.cyan}$ com db search "khảo sát"${colors.reset}

    ${colors.darkGray}# 7. Migrate database with SQL statement and auto-sync schema${colors.reset}
    ${colors.cyan}$ com migrate "ALTER TABLE users ADD COLUMN phone TEXT;"${colors.reset}

    ${colors.darkGray}# 8. Check CLI and engine versions${colors.reset}
    ${colors.cyan}$ com --version${colors.reset}
`);
}

async function main() {
  const args = process.argv.slice(2);
  const command = args[0];

  if (!command || command === "--help" || command === "-h" || command === "help") {
    printHelp();
    return;
  }

  if (
    command === "--version" ||
    command === "-v" ||
    command === "-V" ||
    command === "version" ||
    args.includes("--version") ||
    args.includes("-v")
  ) {
    logger.hero();
    logger.card("COM.AI.VN SYSTEM VERSION", [
      {
        label: "CLI Package",
        value: `@com.ai.vn/cli v${packageJson.version}`,
        color: colors.bold + colors.cyan,
      },
      {
        label: "Compiler Engine",
        value: "compiler-rs (Rust v0.1.0)",
        color: colors.bold + colors.green,
      },
      {
        label: "Bun Runtime",
        value: `v${Bun.version} (${process.arch}-${process.platform})`,
        color: colors.yellow,
      },
      {
        label: "Database Engine",
        value: "Bun.SQL Postgres Introspector",
        color: colors.sky,
      },
      {
        label: "Zero-Install ESM",
        value: "Active",
        color: colors.emerald,
      },
    ]);
    return;
  }

  switch (command) {
    case "login": {
      let token: string | undefined;
      let api: string | undefined;
      let authUrl: string | undefined;
      for (let i = 1; i < args.length; i++) {
        if (args[i] === "--token" && args[i + 1]) token = args[++i];
        if (args[i] === "--api" && args[i + 1]) api = args[++i];
        if (args[i] === "--auth-url" && args[i + 1]) authUrl = args[++i];
      }
      await loginCommand({ token, api, authUrl });
      break;
    }

    case "logout": {
      logoutCommand();
      break;
    }

    case "whoami": {
      whoamiCommand();
      break;
    }

    case "clone": {
      const viewId = args[1];
      if (!viewId || viewId.startsWith("-")) {
        logger.error("Missing required <viewId> argument!", "Usage: com clone <viewId>\n    Example: com clone 105");
        process.exit(1);
      }

      // Parse options
      let dir: string | undefined;
      let api: string | undefined;
      let token: string | undefined;
      let force = false;
      let install = false;

      for (let i = 2; i < args.length; i++) {
        if (args[i] === "--dir" && args[i + 1]) {
          dir = args[++i];
        } else if (args[i] === "--api" && args[i + 1]) {
          api = args[++i];
        } else if (args[i] === "--token" && args[i + 1]) {
          token = args[++i];
        } else if (args[i] === "--force" || args[i] === "-f") {
          force = true;
        } else if (args[i] === "--install") {
          install = true;
        }
      }

      await cloneView(viewId, { dir, api, token, force, install });
      break;
    }

    case "dev":
    case "start": {
      let port: string | undefined;
      let dir: string | undefined;
      let engine: "rust" | "bun" | undefined;
      let target: "web" | "zalo" | undefined;
      let tunnel = false;
      let provider: "123c" | "rs" | undefined;
      let subdomain: string | undefined;
      let server: string | undefined;
      for (let i = 1; i < args.length; i++) {
        if ((args[i] === "--port" || args[i] === "-p") && args[i + 1]) {
          port = args[++i];
        } else if (args[i] === "--dir" && args[i + 1]) {
          dir = args[++i];
        } else if ((args[i] === "--target" || args[i] === "-t") && args[i + 1]) {
          const val = args[++i].toLowerCase();
          if (val === "zalo" || val === "web") target = val;
        } else if (args[i] === "--engine" && args[i + 1]) {
          const val = args[++i].toLowerCase();
          if (val === "rust" || val === "bun") engine = val;
        } else if (args[i] === "--tunnel" || args[i] === "-T") {
          tunnel = true;
        } else if ((args[i] === "--provider" || args[i] === "-P") && args[i + 1]) {
          const val = args[++i].toLowerCase();
          if (val === "123c" || val === "rs") provider = val;
        } else if ((args[i] === "--subdomain" || args[i] === "-s") && args[i + 1]) {
          subdomain = args[++i];
        } else if (args[i] === "--server" && args[i + 1]) {
          server = args[++i];
        }
      }
      await devCommand({ port, dir, engine, target, tunnel, subdomain, server, provider });
      break;
    }

    case "tunnel": {
      let port: string | number = 3000;
      let provider: "123c" | "rs" | undefined;
      let subdomain: string | undefined;
      let server: string | undefined;
      let showQr = true;
      for (let i = 1; i < args.length; i++) {
        if ((args[i] === "--port" || args[i] === "-p") && args[i + 1]) {
          port = args[++i];
        } else if ((args[i] === "--provider" || args[i] === "-P") && args[i + 1]) {
          const val = args[++i].toLowerCase();
          if (val === "123c" || val === "rs") provider = val;
        } else if ((args[i] === "--subdomain" || args[i] === "-s") && args[i + 1]) {
          subdomain = args[++i];
        } else if (args[i] === "--server" && args[i + 1]) {
          server = args[++i];
        } else if (args[i] === "--no-qr") {
          showQr = false;
        }
      }
      await tunnelCommand({ port, subdomain, server, showQr, provider });
      break;
    }

    case "build": {
      let dir: string | undefined;
      let target: "web" | "zalo" | undefined;
      let outDir: string | undefined;
      for (let i = 1; i < args.length; i++) {
        if (args[i] === "--dir" && args[i + 1]) {
          dir = args[++i];
        } else if ((args[i] === "--target" || args[i] === "-t") && args[i + 1]) {
          const val = args[++i].toLowerCase();
          if (val === "zalo" || val === "web") target = val;
        } else if ((args[i] === "--out" || args[i] === "-o") && args[i + 1]) {
          outDir = args[++i];
        }
      }
      await buildCommand({ dir, target, outDir });
      break;
    }

    case "test":
    case "t": {
      let pattern: string | undefined;
      let dir: string | undefined;
      let filter: string | undefined;
      let watch = false;
      let coverage = false;
      let bail = false;
      let timeout: number | undefined;
      let updateSnapshots = false;
      let only = false;
      let todo = false;
      let json = false;
      let env: string | undefined;

      const passthroughArgs: string[] = [];

      for (let i = 1; i < args.length; i++) {
        const arg = args[i];
        if ((arg === "--dir" || arg === "-d") && args[i + 1]) {
          dir = args[++i];
        } else if ((arg === "--filter" || arg === "-f") && args[i + 1]) {
          filter = args[++i];
        } else if (arg === "--watch" || arg === "-w") {
          watch = true;
        } else if (arg === "--coverage") {
          coverage = true;
        } else if (arg === "--bail" || arg === "-b") {
          bail = true;
        } else if (arg === "--timeout" && args[i + 1]) {
          timeout = Number(args[++i]);
        } else if (arg === "--update-snapshots" || arg === "-u") {
          updateSnapshots = true;
        } else if (arg === "--only") {
          only = true;
        } else if (arg === "--todo") {
          todo = true;
        } else if (arg === "--json") {
          json = true;
        } else if (arg === "--env" && args[i + 1]) {
          env = args[++i];
        } else if (!arg.startsWith("-") && !pattern) {
          pattern = arg;
        } else {
          passthroughArgs.push(arg);
        }
      }

      const exitCode = await testCommand(passthroughArgs, {
        dir,
        pattern,
        filter,
        watch,
        coverage,
        bail,
        timeout,
        updateSnapshots,
        only,
        todo,
        json,
        env,
      });

      if (exitCode !== 0) {
        process.exit(exitCode);
      }
      break;
    }

    case "db": {
      const subCommand = args[1] || "pull";
      let dir: string | undefined;
      let env: string | undefined;
      let dbUrl: string | undefined;
      let json = false;

      for (let i = 1; i < args.length; i++) {
        if (args[i] === "--dir" && args[i + 1]) {
          dir = args[++i];
        } else if (args[i] === "--env" && args[i + 1]) {
          env = args[++i];
        } else if (args[i] === "--url" && args[i + 1]) {
          dbUrl = args[++i];
        } else if (args[i] === "--json") {
          json = true;
        }
      }

      if (subCommand === "pull" || subCommand === "sync" || subCommand === "introspect") {
        await dbPullCommand({ dir, env, dbUrl });
      } else if (subCommand === "list" || subCommand === "tables" || subCommand === "ls") {
        await dbListCommand({ dir, env, dbUrl, json });
      } else if (subCommand === "describe" || subCommand === "show" || subCommand === "info" || subCommand === "desc") {
        const tableName = args[2];
        await dbDescribeCommand(tableName, { dir, env, dbUrl, json });
      } else if (subCommand === "search" || subCommand === "find" || subCommand === "grep") {
        const query = args[2];
        await dbSearchCommand(query, { dir, env, dbUrl, json });
      } else if (subCommand === "migrate") {
        const sqlOrFile = args[2];
        await dbMigrateCommand(sqlOrFile, { dir, env, dbUrl });
      } else if (subCommand.startsWith("-")) {
        await dbPullCommand({ dir, env, dbUrl });
      } else {
        logger.error(
          `Unknown db subcommand: "${subCommand}"`,
          "Available db subcommands:\n    com db list                  (List all tables)\n    com db describe <table_name> (Inspect a table)\n    com db search <keyword>      (Search tables/columns)\n    com db pull                  (Regenerate types/db.d.ts & schema.sql)\n    com db migrate \"<SQL>\"       (Execute SQL migration)"
        );
        process.exit(1);
      }
      break;
    }

    case "migrate": {
      const sqlOrFile = args[1];
      let dir: string | undefined;
      let env: string | undefined;
      let dbUrl: string | undefined;

      for (let i = 2; i < args.length; i++) {
        if (args[i] === "--dir" && args[i + 1]) {
          dir = args[++i];
        } else if (args[i] === "--env" && args[i + 1]) {
          env = args[++i];
        } else if (args[i] === "--url" && args[i + 1]) {
          dbUrl = args[++i];
        }
      }

      await dbMigrateCommand(sqlOrFile, { dir, env, dbUrl });
      break;
    }

    case "sync":
    case "pull": {
      let dir: string | undefined;
      let env: string | undefined;
      let dbUrl: string | undefined;
      let install = false;
      let json = false;

      for (let i = 1; i < args.length; i++) {
        if (args[i] === "--dir" && args[i + 1]) {
          dir = args[++i];
        } else if (args[i] === "--env" && args[i + 1]) {
          env = args[++i];
        } else if (args[i] === "--url" && args[i + 1]) {
          dbUrl = args[++i];
        } else if (args[i] === "--install") {
          install = true;
        } else if (args[i] === "--json") {
          json = true;
        }
      }

      await syncCommand({ dir, env, dbUrl, install, json });
      break;
    }

    case "push":
    case "save": {
      logger.hero();
      logger.section("SYNC / PUSH VIEW");
      logger.info("Cloud synchronization engine is ready...");
      break;
    }

    default:
      logger.error(`Unknown command: "${command}"`, "Run `com --help` for available commands.");
      process.exit(1);
  }
}

main().catch((err) => {
  logger.error(err.message || String(err));
  process.exit(1);
});
