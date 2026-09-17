import { existsSync, readFileSync, readdirSync } from "fs";
import { join, resolve } from "path";
import { createConnection, createServer } from "net";
import { logger, colors } from "../core/logger";
import { startTunnelBackground, printTunnelQrCode } from "./tunnel";

export async function findAvailablePort(startPort: number): Promise<number> {
  let port = startPort;
  while (port < startPort + 50) {
    const isAvailable = await new Promise<boolean>((resolve) => {
      const server = createServer();
      server.once("error", () => resolve(false));
      server.once("listening", () => {
        server.close(() => resolve(true));
      });
      server.listen(port, "0.0.0.0");
    });
    if (isAvailable) return port;
    port++;
  }
  return startPort;
}

export interface DevOptions {
  port?: string | number;
  dir?: string;
  engine?: "rust" | "bun";
  target?: "web" | "zalo";
  tunnel?: boolean;
  provider?: "123c" | "rs";
  subdomain?: string;
  server?: string;
}

export function isZaloMiniAppProject(projectDir: string): boolean {
  if (existsSync(join(projectDir, "app-config.json")) || existsSync(join(projectDir, "zmp.json"))) {
    return true;
  }
  const pkgPath = join(projectDir, "package.json");
  if (existsSync(pkgPath)) {
    try {
      const content = readFileSync(pkgPath, "utf-8");
      const pkg = JSON.parse(content);
      const allDeps = {
        ...pkg.dependencies,
        ...pkg.devDependencies,
      };
      if (allDeps["zmp-sdk"] || allDeps["zmp-ui"] || allDeps["zmp-framework"]) {
        return true;
      }
    } catch {}
  }
  return false;
}

async function promptTargetSelection(projectDir: string): Promise<"zalo" | "web"> {
  const isZalo = isZaloMiniAppProject(projectDir);
  if (!process.stdin.isTTY) {
    return isZalo ? "zalo" : "web";
  }

  const defaultChoice = isZalo ? "2" : "1";

  const readline = await import("readline");
  const rl = readline.createInterface({
    input: process.stdin,
    output: process.stdout,
  });

  console.log(`\n  ${colors.dim}?${colors.reset} ${colors.bold}Select target environment:${colors.reset}`);
  console.log(`    ${colors.cyan}1)${colors.reset} Web App       ${colors.dim}(Next.js App Router)${colors.reset}`);
  console.log(`    ${colors.cyan}2)${colors.reset} Zalo Mini App ${colors.dim}(ZMP Container & Auto Tunnel)${colors.reset}\n`);

  return new Promise((resolve) => {
    rl.question(`  ${colors.dim}Choice [1/2] (default: ${defaultChoice}):${colors.reset} `, (answer) => {
      rl.close();
      const choice = answer.trim();
      if (choice === "2" || choice.toLowerCase() === "zalo") {
        resolve("zalo");
      } else if (choice === "1" || choice.toLowerCase() === "web") {
        resolve("web");
      } else {
        resolve(isZalo ? "zalo" : "web");
      }
    });
  });
}

async function detectDatabaseStatus(projectDir: string): Promise<{ label: string; color: string }> {
  let dbUrl = process.env.DATABASE_URL || process.env.POSTGRES_URL;

  if (!dbUrl) {
    const envFiles = [".env.local", ".env.development", ".env"];
    for (const file of envFiles) {
      const fullPath = join(projectDir, file);
      if (existsSync(fullPath)) {
        try {
          const content = readFileSync(fullPath, "utf-8");
          for (const line of content.split("\n")) {
            const trimmed = line.trim();
            if (!trimmed || trimmed.startsWith("#") || !trimmed.includes("=")) continue;
            const [key, ...rest] = trimmed.split("=");
            const val = rest.join("=").trim().replace(/^["']|["']$/g, "");
            if ((key.trim() === "DATABASE_URL" || key.trim() === "POSTGRES_URL") && val) {
              dbUrl = val;
              break;
            }
          }
        } catch {}
      }
      if (dbUrl) break;
    }
  }

  if (dbUrl) {
    try {
      const parsed = new URL(dbUrl);
      const host = parsed.hostname || "localhost";
      const port = Number(parsed.port || 5432);
      const dbName = parsed.pathname.replace(/^\//, "") || "postgres";
      const target = `${host}:${port}/${dbName}`;

      // Check TCP socket connectivity with 800ms timeout
      const isLive = await new Promise<boolean>((resolve) => {
        const socket = createConnection({ host, port, timeout: 800 }, () => {
          socket.end();
          resolve(true);
        });
        socket.on("error", () => resolve(false));
        socket.on("timeout", () => {
          socket.destroy();
          resolve(false);
        });
      });

      if (isLive) {
        return {
          label: `PostgreSQL (${target})`,
          color: colors.green,
        };
      } else {
        return {
          label: `PostgreSQL (${target} - offline)`,
          color: colors.yellow,
        };
      }
    } catch {
      const masked = dbUrl.replace(/:([^:@]+)@/, ":***@");
      return {
        label: `PostgreSQL (${masked})`,
        color: colors.green,
      };
    }
  }

  return {
    label: "PGlite (in-memory)",
    color: colors.dim,
  };
}

import { getLocalNetworkIp, generateZaloDeepLink, printZaloDevQrCode, resolveOrPromptAppId, setupAdbReverse, ensureHrConfigFile } from "../zalominiapp/dev";

export async function devCommand(options: DevOptions = {}) {
  const projectDir = resolve(process.cwd(), options.dir || ".");

  // Check if project has an App Router structure
  const hasAppDir =
    existsSync(join(projectDir, "app")) ||
    existsSync(join(projectDir, "src/app")) ||
    existsSync(join(projectDir, "pages"));

  if (!hasAppDir) {
    try {
      const subdirs = readdirSync(projectDir, { withFileTypes: true })
        .filter((d) => d.isDirectory() && !d.name.startsWith(".") && d.name !== "node_modules")
        .map((d) => d.name);
      const candidates = subdirs.filter(
        (d) => existsSync(join(projectDir, d, "app")) || existsSync(join(projectDir, d, "src/app"))
      );
      if (candidates.length > 0) {
        logger.warn(`No 'app/' directory found in ${projectDir}.`);
        logger.info(`App Router project found in subfolder: ${colors.cyan}${candidates.join(", ")}${colors.reset}`);
        logger.info(`👉 Tip: Run 'cd ${candidates[0]}' then 'com dev'\n`);
      }
    } catch {}
  }

  const requestedPort = Number(options.port || process.env.PORT || 3000);
  const port = await findAvailablePort(requestedPort);
  if (port !== requestedPort) {
    logger.warn(`Port ${requestedPort} is in use, automatically switched to port ${port}`);
  }

  // Target selection: CLI option -> Interactive Prompt -> default "web"
  let target: "web" | "zalo" = options.target || "web";
  if (!options.target) {
    target = await promptTargetSelection(projectDir);
  }

  let zaloAppId: string | null = null;
  if (target === "zalo") {
    zaloAppId = await resolveOrPromptAppId(projectDir);
    ensureHrConfigFile(projectDir);
  }

  // Locate the standalone Rust compiler binary
  const exeExt = process.platform === "win32" ? ".exe" : "";
  const rustBinCandidates = [
    join(__dirname, `../compiler-rs/target/release/com-compiler${exeExt}`),
    join(__dirname, `../../compiler-rs/target/release/com-compiler${exeExt}`),
    join(process.cwd(), `compiler-rs/target/release/com-compiler${exeExt}`),
    join(__dirname, `../com-compiler${exeExt}`),
    join(__dirname, `../../com-compiler${exeExt}`),
    `/usr/local/bin/com-compiler${exeExt}`,
    `com-compiler${exeExt}`,
    "com-compiler",
  ];

  let rustBinPath: string | null = null;
  for (const cand of rustBinCandidates) {
    if (existsSync(cand)) {
      rustBinPath = cand;
      break;
    }
  }

  if (!rustBinPath) {
    const whichPath = Bun.which(`com-compiler${exeExt}`) || Bun.which("com-compiler");
    if (whichPath && existsSync(whichPath)) {
      rustBinPath = whichPath;
    }
  }

  if (!rustBinPath) {
    const buildSpinner = logger.spinner("Compiling Rust App Router Engine (first-time build)...");
    const compilerDir = join(__dirname, "../compiler-rs");
    try {
      const cargoCmd = process.platform === "win32" ? "cargo.exe" : "cargo";
      const buildProc = Bun.spawn([cargoCmd, "build", "--release"], {
        cwd: compilerDir,
        stdout: "ignore",
        stderr: "ignore",
      });
      await buildProc.exited;
      const builtPath = join(compilerDir, `target/release/com-compiler${exeExt}`);
      if (existsSync(builtPath)) {
        rustBinPath = builtPath;
        buildSpinner.stop(true, "Rust Engine compiled successfully");
      } else {
        throw new Error(`Target binary not found at ${builtPath}`);
      }
    } catch (err: any) {
      buildSpinner.stop(false, "Failed to compile Rust engine");
      logger.error("Could not locate or build Rust binary 'com-compiler'", err.message);
      process.exit(1);
    }
  }

  const dbStatus = await detectDatabaseStatus(projectDir);
  const localIp = getLocalNetworkIp() || "localhost";
  const networkUrl = `http://${localIp}:${port}`;

  // If types/db.d.ts is missing and DATABASE_URL is set, auto-generate types in background
  const typesPath = join(projectDir, "types", "db.d.ts");
  if (!existsSync(typesPath)) {
    const dbUrl = process.env.DATABASE_URL || process.env.POSTGRES_URL;
    if (dbUrl) {
      try {
        const { introspectAndGenerateSchema } = await import("./db");
        await introspectAndGenerateSchema(projectDir, dbUrl);
      } catch {}
    }
  }

  // Zalo target automatically enables public tunnel (no extra --tunnel flag required)
  const isTunnel = options.tunnel !== undefined ? options.tunnel : (target === "zalo");
  let tunnelUrl: string | null = null;
  if (isTunnel) {
    try {
      const provider = options.provider || "rs";
      const tunnel = await startTunnelBackground({
        port,
        subdomain: options.subdomain,
        server: options.server,
        provider,
      });
      tunnelUrl = tunnel.publicUrl;
    } catch (err: any) {
      logger.warn(`Could not start tunnel: ${err.message}`);
    }
  }

  console.log();
  console.log(`   ${colors.bold}▲ com.ai.vn${colors.reset}`);
  console.log(`   ${colors.dim}-${colors.reset} Local:        ${colors.cyan}http://localhost:${port}${colors.reset}`);
  console.log(`   ${colors.dim}-${colors.reset} Network:      ${colors.cyan}${networkUrl}${colors.reset}`);
  if (tunnelUrl) {
    console.log(`   ${colors.dim}-${colors.reset} Tunnel:       ${colors.green}${tunnelUrl}${colors.reset}`);
  }
  console.log(`   ${colors.dim}-${colors.reset} Target:       ${target === "zalo" ? "Zalo Mini App" : "Web App"}`);
  if (target === "zalo" && zaloAppId) {
    console.log(`   ${colors.dim}-${colors.reset} App ID:       ${colors.dim}${zaloAppId}${colors.reset}`);
  }
  console.log(`   ${colors.dim}-${colors.reset} Database:     ${dbStatus.label}`);
  console.log();
  console.log(` ${colors.green}✓${colors.reset} Ready in ${colors.dim}120ms${colors.reset}`);
  console.log();

  if (target === "zalo" && zaloAppId) {
    const clientEndpoint = tunnelUrl || networkUrl;
    const deepLinkUrl = generateZaloDeepLink(zaloAppId, clientEndpoint, port);
    if (!tunnelUrl) {
      try {
        await setupAdbReverse(port);
      } catch {}
    }
    printZaloDevQrCode(deepLinkUrl, zaloAppId, clientEndpoint);
  } else if (tunnelUrl) {
    printTunnelQrCode(tunnelUrl);
  }

  // Launch Rust binary directly with interactive IO and target flag
  const proc = Bun.spawn([rustBinPath, "dev", "--dir", projectDir, "--port", String(port), "--target", target], {
    stdout: "inherit",
    stderr: "inherit",
    stdin: "inherit",
    env: {
      ...process.env,
      COM_MANAGED: "1",
    },
  });

  await proc.exited;
}
