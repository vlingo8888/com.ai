import { existsSync, readFileSync, readdirSync, writeFileSync, mkdirSync } from "fs";
import { join, resolve } from "path";
import { createConnection, createServer } from "net";
import { logger, colors } from "../core/logger";
import { startTunnelBackground, superviseTunnel, printTunnelQrCode } from "./tunnel";
import { exeName, resolvePrebuiltCompiler, compilerPackageName, engineEnv } from "../core/binaries";
import packageJson from "../../package.json";

/** The Rust engine binary (`com-compiler`): COM_COMPILER_PATH, the prebuilt package, a local build */
export function findEngineBinary(): string | null {
  const exeExt = process.platform === "win32" ? ".exe" : "";
  const prebuiltCompiler = resolvePrebuiltCompiler();
  const candidates = [
    ...(process.env.COM_COMPILER_PATH ? [process.env.COM_COMPILER_PATH] : []),
    ...(prebuiltCompiler ? [prebuiltCompiler] : []),
    join(__dirname, `../compiler-rs/target/release/com-compiler${exeExt}`),
    join(__dirname, `../../compiler-rs/target/release/com-compiler${exeExt}`),
    join(process.cwd(), `compiler-rs/target/release/com-compiler${exeExt}`),
    join(__dirname, `../com-compiler${exeExt}`),
    join(__dirname, `../../com-compiler${exeExt}`),
    `/usr/local/bin/com-compiler${exeExt}`,
    `com-compiler${exeExt}`,
    "com-compiler",
  ];
  for (const candidate of candidates) {
    if (existsSync(candidate)) return candidate;
  }
  const which = Bun.which(`com-compiler${exeExt}`) || Bun.which("com-compiler");
  return which && existsSync(which) ? which : null;
}

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
  /** Load the Eruda mobile console in the Zalo bootstrap */
  eruda?: boolean;
  /** Install the fake `window.ZMP` bridge in the browser preview instead of relying on the real zmp-sdk */
  zmpMock?: boolean;
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

export interface DatabaseStatus {
  label: string;
  color: string;
  isLive: boolean;
  isConfigured: boolean;
  target?: string;
  dbUrl?: string;
  envFile?: string;
}

export async function detectDatabaseStatus(projectDir: string): Promise<DatabaseStatus> {
  let dbUrl = process.env.DATABASE_URL || process.env.POSTGRES_URL;
  let envFile: string | undefined = undefined;

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
            if (!dbUrl) dbUrl = val;
            envFile = fullPath;
            break;
          }
        }
      } catch {}
    }
    if (envFile) break;
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
          isLive: true,
          isConfigured: true,
          target,
          dbUrl,
          envFile,
        };
      } else {
        return {
          label: `PostgreSQL (${target} - offline)`,
          color: colors.yellow,
          isLive: false,
          isConfigured: true,
          target,
          dbUrl,
          envFile,
        };
      }
    } catch {
      const masked = dbUrl.replace(/:([^:@]+)@/, ":***@");
      return {
        label: `PostgreSQL (${masked})`,
        color: colors.green,
        isLive: true,
        isConfigured: true,
        target: masked,
        dbUrl,
        envFile,
      };
    }
  }

  return {
    label: "PGlite (embedded)",
    color: colors.cyan,
    isLive: true,
    isConfigured: false,
  };
}

async function promptSwitchToPglite(target: string): Promise<boolean> {
  if (!process.stdin.isTTY) {
    return false;
  }

  const readline = await import("readline");
  const rl = readline.createInterface({
    input: process.stdin,
    output: process.stdout,
  });

  console.log(`\n ${colors.yellow}⚠${colors.reset} ${colors.bold}PostgreSQL (${target}) is offline / unreachable.${colors.reset}`);
  console.log(`    ${colors.dim}Không thể kết nối đến server PostgreSQL tại địa chỉ này.${colors.reset}\n`);

  return new Promise((resolve) => {
    rl.question(
      `  ${colors.cyan}?${colors.reset} ${colors.bold}Chuyển sang dùng PGlite (cơ sở dữ liệu nhúng local) để tiếp tục phát triển?${colors.reset} ${colors.dim}[Y/n] (mặc định: Y):${colors.reset} `,
      (answer) => {
        rl.close();
        const clean = answer.trim().toLowerCase();
        if (clean === "" || clean === "y" || clean === "yes") {
          resolve(true);
        } else {
          resolve(false);
        }
      }
    );
  });
}

function commentOutDatabaseUrlInEnv(envFilePath: string) {
  try {
    const content = readFileSync(envFilePath, "utf-8");
    const updated = content
      .split("\n")
      .map((line) => {
        const trimmed = line.trim();
        if (trimmed.startsWith("DATABASE_URL=") || trimmed.startsWith("POSTGRES_URL=")) {
          return `# ${line} # (tạm tắt bởi com dev - chuyển sang PGlite)`;
        }
        return line;
      })
      .join("\n");
    writeFileSync(envFilePath, updated, "utf-8");
  } catch {}
}

import { getLocalNetworkIp, generateZaloDeepLink, printZaloDevQrCode, resolveOrPromptAppId, setupAdbReverse, ensureHrConfigFile } from "../zalominiapp/dev";

/** Dependencies declared in package.json that are not present in node_modules */
export function missingDependencies(projectDir: string): string[] {
  const pkgPath = join(projectDir, "package.json");
  if (!existsSync(pkgPath)) return [];
  try {
    const pkg = JSON.parse(readFileSync(pkgPath, "utf-8"));
    const names = Object.keys({ ...pkg.dependencies, ...pkg.devDependencies });
    return names.filter((name) => !existsSync(join(projectDir, "node_modules", name, "package.json")));
  } catch {
    return [];
  }
}

/**
 * Client and server modules are both served from node_modules (no CDN), so install what is missing
 * before the engine starts. Bun's global cache keeps this fast and usually offline-capable.
 */
async function ensureDependenciesInstalled(projectDir: string) {
  const missing = missingDependencies(projectDir);
  if (missing.length === 0) return;
  const spinner = logger.spinner(`Installing ${missing.length} missing ${missing.length === 1 ? "dependency" : "dependencies"} (bun install)...`);
  const proc = Bun.spawn([process.execPath, "install"], { cwd: projectDir, stdout: "ignore", stderr: "pipe" });
  const stderr = await new Response(proc.stderr).text();
  if ((await proc.exited) === 0) {
    spinner.stop(true, "Dependencies installed");
  } else {
    spinner.stop(false, "bun install failed; missing packages will be loaded from esm.sh");
    const detail = stderr.trim().split("\n").slice(-5).join("\n");
    if (detail) console.log(`   ${colors.dim}${detail}${colors.reset}`);
  }
}

function listEnvFiles(projectDir: string): string[] {
  return [".env.local", ".env.development", ".env"].filter((f) => existsSync(join(projectDir, f)));
}

/** Next.js-style startup banner: `▲ com.ai.vn 1.0.2` followed by aligned `- Label:  value` rows. */
function printDevBanner(rows: Array<[string, string | null | undefined]>) {
  const visible = rows.filter(([, value]) => value);
  const width = Math.max(...visible.map(([label]) => label.length)) + 1;
  console.log();
  console.log(`   ${colors.bold}▲ com.ai.vn ${packageJson.version}${colors.reset}`);
  for (const [label, value] of visible) {
    console.log(`   - ${`${label}:`.padEnd(width)} ${value}`);
  }
  console.log();
}

export async function devCommand(options: DevOptions = {}) {
  const projectDir = resolve(process.cwd(), options.dir || ".");

  // Ensure local data directory exists for embedded PGlite
  const initialDataDir = join(projectDir, "data");
  if (!existsSync(initialDataDir)) {
    try { mkdirSync(initialDataDir, { recursive: true }); } catch {}
  }

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
  let rustBinPath: string | null = findEngineBinary();

  if (!rustBinPath) {
    const buildSpinner = logger.spinner("Compiling Rust App Router Engine (first-time build)...");
    const compilerDir = join(__dirname, "../compiler-rs");
    try {
      const cargoCmd = Bun.which(exeName("cargo")) || Bun.which("cargo");
      if (!cargoCmd) {
        throw new Error(
          `No prebuilt engine for ${process.platform}-${process.arch} (${compilerPackageName()}) and Rust toolchain not found. ` +
            "Reinstall with optional dependencies enabled, install Rust from https://rustup.rs, or set COM_COMPILER_PATH."
        );
      }
      const buildProc = Bun.spawn([cargoCmd, "build", "--release"], {
        cwd: compilerDir,
        stdout: "ignore",
        stderr: "pipe",
      });
      const buildErr = await new Response(buildProc.stderr).text();
      if ((await buildProc.exited) !== 0) {
        throw new Error(`cargo build failed:\n${buildErr.trim().split("\n").slice(-20).join("\n")}`);
      }
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

  await ensureDependenciesInstalled(projectDir);

  let dbStatus = await detectDatabaseStatus(projectDir);

  if (dbStatus.isConfigured && !dbStatus.isLive) {
    const shouldSwitch = await promptSwitchToPglite(dbStatus.target || "PostgreSQL");
    if (shouldSwitch) {
      if (dbStatus.envFile) {
        commentOutDatabaseUrlInEnv(dbStatus.envFile);
      }
      delete process.env.DATABASE_URL;
      delete process.env.POSTGRES_URL;
      const dataDir = join(projectDir, "data");
      if (!existsSync(dataDir)) {
        try { mkdirSync(dataDir, { recursive: true }); } catch {}
      }
      dbStatus = {
        label: "PGlite (embedded)",
        color: colors.cyan,
        isLive: true,
        isConfigured: false,
      };
      logger.success("Đã chuyển sang PGlite (data/app.db). Bạn có thể bật lại PostgreSQL bất cứ lúc nào trong .env\n");
    } else {
      logger.warn("Tiếp tục chạy với PostgreSQL offline. Các truy vấn CSDL có thể gặp lỗi kết nối.\n");
    }
  } else if (!dbStatus.isConfigured || dbStatus.label.includes("PGlite")) {
    const dataDir = join(projectDir, "data");
    if (!existsSync(dataDir)) {
      try { mkdirSync(dataDir, { recursive: true }); } catch {}
    }
  }

  const localIp = getLocalNetworkIp() || "localhost";
  const networkUrl = `http://${localIp}:${port}`;

  // If types/db.d.ts is missing and DATABASE_URL is set, auto-generate types in background
  const typesPath = join(projectDir, "types", "db.d.ts");
  if (!existsSync(typesPath)) {
    const dbUrl = process.env.DATABASE_URL || process.env.POSTGRES_URL;
    if (dbUrl && dbStatus.isLive) {
      try {
        const { introspectAndGenerateSchema } = await import("./db");
        await introspectAndGenerateSchema(projectDir, dbUrl);
      } catch {}
    }
  }

  // Zalo target automatically enables public tunnels (no extra --tunnel flag required)
  const isTunnel = options.tunnel !== undefined ? options.tunnel : (target === "zalo");
  let zaloTunnelUrl: string | null = null;
  let backendTunnelUrl: string | null = null;

  if (isTunnel) {
    if (target === "zalo") {
      // Both tunnels connect at the same time: 1. the Zalo Mini App frontend tunnel via 123c (so the
      // Zalo mobile webview can load the app), 2. the backend tunnel via tunnel-rs for API / RPC calls.
      // tunnel-rs gets no 123c fallback: the Zalo tunnel already is one, a second only adds a process
      const [zTunnel, bTunnel] = await Promise.allSettled([
        startTunnelBackground({ port, subdomain: options.subdomain, provider: "123c" }),
        startTunnelBackground({ port, server: options.server, provider: "rs", fallback: false }),
      ]);
      if (zTunnel.status === "fulfilled") {
        zaloTunnelUrl = zTunnel.value.publicUrl;
        superviseTunnel(zTunnel.value, { port, provider: "123c", label: "Zalo" });
      } else {
        logger.warn(`Could not start Zalo 123c tunnel: ${zTunnel.reason?.message}`);
      }
      if (bTunnel.status === "fulfilled") {
        backendTunnelUrl = bTunnel.value.publicUrl;
        superviseTunnel(bTunnel.value, { port, server: options.server, provider: "rs", fallback: false, label: "Backend" });
      } else {
        backendTunnelUrl = zaloTunnelUrl;
      }
    } else {
      // Standard Web App tunnel via tunnel-rs (myworkbeast)
      try {
        const provider = options.provider || "rs";
        const tunnel = await startTunnelBackground({
          port,
          subdomain: options.subdomain,
          server: options.server,
          provider,
        });
        backendTunnelUrl = tunnel.publicUrl;
        superviseTunnel(tunnel, { port, server: options.server, provider, label: "Public" });
      } catch (err: any) {
        logger.warn(`Could not start tunnel: ${err.message}`);
      }
    }
  }

  const effectiveBackendUrl = backendTunnelUrl || zaloTunnelUrl || networkUrl;

  printDevBanner([
    ["Local", `http://localhost:${port}`],
    ["Network", networkUrl],
    ["Environments", listEnvFiles(projectDir).join(", ")],
    ["Target", target === "zalo" ? `Zalo Mini App${zaloAppId ? ` (${zaloAppId})` : ""}` : "Web App"],
    ["Database", dbStatus.label],
    ["Zalo Tunnel", zaloTunnelUrl],
    ["Backend URL", backendTunnelUrl && backendTunnelUrl !== zaloTunnelUrl ? backendTunnelUrl : null],
  ]);

  if (target === "zalo" && zaloAppId) {
    // Zalo opens via 123c tunnel
    const clientEndpoint = zaloTunnelUrl || networkUrl;
    const deepLinkUrl = generateZaloDeepLink(zaloAppId, clientEndpoint, port);
    if (!zaloTunnelUrl) {
      try {
        await setupAdbReverse(port);
      } catch {}
    }
    printZaloDevQrCode(deepLinkUrl, zaloAppId, clientEndpoint);
  } else if (backendTunnelUrl) {
    printTunnelQrCode(backendTunnelUrl);
  }

  // The engine prints ` ✓ Ready in …` once it is listening, measured from COM_START_TIME
  logger.success("Starting...");

  // Launch Rust binary directly with interactive IO, target flag and COM_BACKEND_URL
  const proc = Bun.spawn([rustBinPath, "dev", "--dir", projectDir, "--port", String(port), "--target", target], {
    stdout: "inherit",
    stderr: "inherit",
    stdin: "inherit",
    env: {
      ...process.env,
      ...engineEnv(),
      COM_MANAGED: "1",
      COM_START_TIME: String(Math.round(performance.timeOrigin)),
      COM_BACKEND_URL: effectiveBackendUrl,
      ...(options.eruda ? { COM_ERUDA: "1" } : {}),
      ...(options.zmpMock ? { COM_ZMP_MOCK: "1" } : {}),
      NEXT_PUBLIC_API_URL: effectiveBackendUrl,
    },
  });

  await proc.exited;
}
