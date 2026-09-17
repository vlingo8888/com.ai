import { existsSync, readFileSync } from "fs";
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

async function promptTargetSelection(): Promise<"zalo" | "web"> {
  // If not running in an interactive terminal, default to zalo
  if (!process.stdin.isTTY) {
    return "zalo";
  }

  const readline = await import("readline");
  const rl = readline.createInterface({
    input: process.stdin,
    output: process.stdout,
  });

  console.log(`\n  ${colors.bold}${colors.yellow}? Phát hiện Zalo Mini App SDK trong dự án. Bạn muốn khởi chạy chế độ nào?${colors.reset}`);
  console.log(`    ${colors.bold}${colors.cyan}1)${colors.reset} 📱 ${colors.bold}Zalo Mini App${colors.reset} ${colors.darkGray}(ZMP Simulator, Native Mock APIs & Mobile Header)${colors.reset}`);
  console.log(`    ${colors.bold}${colors.cyan}2)${colors.reset} 🌐 ${colors.bold}Standard Web App${colors.reset} ${colors.darkGray}(App Router SSR & Client Hydration)${colors.reset}`);

  return new Promise((resolve) => {
    rl.question(`\n  ${colors.bold}${colors.white}Lựa chọn của bạn [1/2] (mặc định: 1): ${colors.reset}`, (answer) => {
      rl.close();
      const choice = answer.trim();
      if (choice === "2" || choice.toLowerCase() === "web") {
        resolve("web");
      } else {
        resolve("zalo");
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
          label: `● PostgreSQL (Connected: ${target})`,
          color: colors.bold + colors.emerald,
        };
      } else {
        return {
          label: `▲ PostgreSQL (Offline: ${target})`,
          color: colors.bold + colors.yellow,
        };
      }
    } catch {
      const masked = dbUrl.replace(/:([^:@]+)@/, ":***@");
      return {
        label: `● PostgreSQL (${masked})`,
        color: colors.bold + colors.emerald,
      };
    }
  }

  return {
    label: "○ PGlite (Local Embedded: data/app.db)",
    color: colors.cyan,
  };
}

import { getLocalNetworkIp, generateZaloDeepLink, printZaloDevQrCode, resolveOrPromptAppId, setupAdbReverse } from "../zalominiapp/dev";

export async function devCommand(options: DevOptions = {}) {
  logger.hero();

  const projectDir = resolve(process.cwd(), options.dir || ".");
  const requestedPort = Number(options.port || process.env.PORT || 3000);
  const port = await findAvailablePort(requestedPort);
  if (port !== requestedPort) {
    logger.warn(`Port ${requestedPort} is in use, automatically switched to port ${port}`);
  }

  // Target selection: CLI option -> Interactive Prompt if ZMP detected -> default "web"
  let target: "web" | "zalo" = options.target || "web";
  if (!options.target && isZaloMiniAppProject(projectDir)) {
    target = await promptTargetSelection();
  }

  let zaloAppId: string | null = null;
  if (target === "zalo") {
    zaloAppId = await resolveOrPromptAppId(projectDir);
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

  let tunnelUrl: string | null = null;
  if (options.tunnel) {
    try {
      logger.info("Initializing public tunnel via tunnel-rs...");
      const tunnel = await startTunnelBackground({
        port,
        subdomain: options.subdomain,
        server: options.server,
      });
      tunnelUrl = tunnel.publicUrl;
      logger.info(`Public Tunnel Active: ${colors.bold}${colors.green}${tunnelUrl}${colors.reset}`);
    } catch (err: any) {
      logger.warn(`Could not start tunnel: ${err.message}`);
    }
  }

  const cardItems = [
    { label: "Engine", value: "🦀 Pure Rust (Axum + App Router Matcher)", color: colors.bold + colors.green },
    { label: "Project Path", value: projectDir, color: colors.cyan },
    {
      label: "Target Mode",
      value: target === "zalo" ? "📱 Zalo Mini App (ZMP Simulator)" : "🌐 Standard Web App",
      color: colors.bold + (target === "zalo" ? colors.cyan : colors.emerald),
    },
  ];

  if (target === "zalo" && zaloAppId) {
    cardItems.push({ label: "Zalo App ID", value: zaloAppId, color: colors.bold + colors.yellow });
  }

  cardItems.push(
    { label: "Local URL", value: `http://localhost:${port}`, color: colors.bold + colors.sky },
    ...(tunnelUrl ? [{ label: "Public Tunnel", value: tunnelUrl, color: colors.bold + colors.emerald }] : []),
    { label: "Network URL", value: networkUrl, color: colors.bold + colors.green },
    { label: "Database", value: dbStatus.label, color: dbStatus.color },
    { label: "HMR WebSocket", value: "● Active (/_hmr)", color: colors.emerald }
  );

  logger.card("RUST DEV ENGINE STARTING", cardItems);

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
  });

  await proc.exited;
}
