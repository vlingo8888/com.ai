#!/usr/bin/env bun
import { cloneView } from "../src/commands/clone";
import { devCommand } from "../src/commands/dev";
import { buildCommand } from "../src/commands/build";
import { dbPullCommand } from "../src/commands/db";
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
    ${colors.green}build${colors.reset}                 Package project for Production or Zalo Mini App
    ${colors.green}db pull | db sync${colors.reset}     Introspect database schema & generate types/db.d.ts for AI
    ${colors.green}version | -v${colors.reset}          Display system, CLI, and runtime engine versions
    ${colors.green}push | sync | save${colors.reset}    Synchronize local changes back to the Cloud

  ${colors.bold}${colors.white}OPTIONS:${colors.reset}
    ${colors.yellow}--target, -t${colors.reset} ${colors.darkGray}<web|zalo>${colors.reset} Target runtime mode (auto-detects Zalo Mini App)
    ${colors.yellow}--dir${colors.reset}        ${colors.darkGray}<path>${colors.reset}     Target directory (default: current directory)
    ${colors.yellow}--port, -p${colors.reset}   ${colors.darkGray}<port>${colors.reset}     Port to listen on in dev mode (default: 3000)
    ${colors.yellow}--api${colors.reset}        ${colors.darkGray}<url>${colors.reset}      Custom backend API URL (default: https://base.myworkbeast.com)
    ${colors.yellow}--url${colors.reset}        ${colors.darkGray}<url>${colors.reset}      Database connection URL for db pull
    ${colors.yellow}--env${colors.reset}        ${colors.darkGray}<file>${colors.reset}     Custom env file path for db pull
    ${colors.yellow}--token${colors.reset}      ${colors.darkGray}<token>${colors.reset}    Pass an access token manually
    ${colors.yellow}--force, -f${colors.reset}            Overwrite existing non-empty directory on clone
    ${colors.yellow}--install${colors.reset}              Explicitly install local node_modules (optional; default is ESM zero-install)
    ${colors.yellow}-h, --help${colors.reset}                 Show this help message
    ${colors.yellow}-v, --version${colors.reset}              Show CLI version

  ${colors.bold}${colors.white}EXAMPLES:${colors.reset}
    ${colors.darkGray}# 1. Start local dev server (auto-detects Web or Zalo Mini App)${colors.reset}
    ${colors.cyan}$ com dev${colors.reset}

    ${colors.darkGray}# 2. Introspect database schema and generate types for AI coding${colors.reset}
    ${colors.cyan}$ com db pull${colors.reset}

    ${colors.darkGray}# 3. Check CLI and engine versions${colors.reset}
    ${colors.cyan}$ com --version${colors.reset}

    ${colors.darkGray}# 4. Clone View by ID (e.g. 105)${colors.reset}
    ${colors.cyan}$ com clone 105${colors.reset}
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
        }
      }
      await devCommand({ port, dir, engine, target });
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

    case "db": {
      const subCommand = args[1] || "pull";
      let dir: string | undefined;
      let env: string | undefined;
      let dbUrl: string | undefined;

      for (let i = 1; i < args.length; i++) {
        if (args[i] === "--dir" && args[i + 1]) {
          dir = args[++i];
        } else if (args[i] === "--env" && args[i + 1]) {
          env = args[++i];
        } else if (args[i] === "--url" && args[i + 1]) {
          dbUrl = args[++i];
        }
      }

      if (subCommand === "pull" || subCommand === "sync" || subCommand === "introspect" || subCommand.startsWith("-")) {
        await dbPullCommand({ dir, env, dbUrl });
      } else {
        logger.error(
          `Unknown db subcommand: "${subCommand}"`,
          "Usage: com db pull\n    Or: com db sync"
        );
        process.exit(1);
      }
      break;
    }

    case "push":
    case "sync":
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
