import { existsSync } from "fs";
import { join } from "path";
import { logger, colors } from "../core/logger";
import qrcode from "qrcode-terminal";

export interface TunnelOptions {
  port?: number | string;
  subdomain?: string;
  server?: string;
  provider?: "123c" | "rs";
  showQr?: boolean;
}

export function printTunnelQrCode(url: string, title = "QUÉT MÃ QR BẰNG ZALO HOẶC CAMERA ĐỂ MỞ TRÊN ĐIỆN THOẠI") {
  try {
    qrcode.generate(url, { small: true }, (qr: string) => {
      console.log(`\n  ${colors.bold}${colors.cyan}📱 ${title}:${colors.reset}\n`);
      for (const line of qr.split("\n")) {
        console.log(`    ${line}`);
      }
      console.log(`\n    ${colors.bold}${colors.white}Public URL:${colors.reset} ${colors.bold}${colors.green}${url}${colors.reset}\n`);
    });
  } catch {}
}

export async function start123cTunnel(options: TunnelOptions = {}): Promise<{
  publicUrl: string;
  subdomain: string;
  proc: any;
}> {
  const port = String(options.port || 3000);
  const args = ["bun", "x", "localtunnel", "--host", "https://mini.123c.vn", "--port", port];
  if (options.subdomain) {
    args.push("--subdomain", options.subdomain);
  }

  return new Promise((resolvePromise, reject) => {
    let resolved = false;
    const proc = Bun.spawn(args, {
      stdout: "pipe",
      stderr: "pipe",
    });

    const timeout = setTimeout(() => {
      if (!resolved) {
        resolved = true;
        reject(new Error("Timeout connecting to official Zalo tunnel server (mini.123c.vn)"));
      }
    }, 15000);

    proc.exited.then((exitCode) => {
      if (!resolved && exitCode !== 0) {
        resolved = true;
        clearTimeout(timeout);
        reject(new Error(`123c tunnel exited with code ${exitCode}`));
      }
    });

    (async () => {
      const reader = proc.stdout.getReader();
      const decoder = new TextDecoder();
      let buffer = "";

      try {
        while (true) {
          const { done, value } = await reader.read();
          if (done) break;
          buffer += decoder.decode(value);

          const match = buffer.match(/https:\/\/[a-zA-Z0-9.-]+\.mini\.123c\.vn/i);
          if (match && !resolved) {
            resolved = true;
            clearTimeout(timeout);
            const publicUrl = match[0].trim();
            const subdomain = publicUrl.replace("https://", "").replace(".mini.123c.vn", "");
            resolvePromise({ publicUrl, subdomain, proc });
          }
        }
      } catch (err) {
        if (!resolved) {
          clearTimeout(timeout);
          reject(err);
        }
      }
    })();
  });
}

export async function locateOrBuildTunnelBinary(): Promise<string> {
  const home = process.env.HOME || "";
  const candidates = [
    join(__dirname, "../tunnel-rs"),
    join(__dirname, "../../tunnel-rs"),
    join(__dirname, "../bin/tunnel-rs"),
    join(__dirname, "../../bin/tunnel-rs"),
    join(process.cwd(), "bin/tunnel-rs"),
    join(process.cwd(), "tunnel-rs"),
    join(home, "Documents/GitHub/NATA/backend/tunnel-rs/target/release/tunnel-rs"),
    join(home, "Documents/GitHub/NATA/backend/tunnel-rs/target/debug/tunnel-rs"),
    join(__dirname, "../../../NATA/backend/tunnel-rs/target/release/tunnel-rs"),
    join(__dirname, "../../../NATA/backend/tunnel-rs/target/debug/tunnel-rs"),
    join(process.cwd(), "tunnel-rs/target/release/tunnel-rs"),
    join(process.cwd(), "tunnel-rs/target/debug/tunnel-rs"),
    join(process.cwd(), "backend/tunnel-rs/target/release/tunnel-rs"),
    join(process.cwd(), "backend/tunnel-rs/target/debug/tunnel-rs"),
    "/usr/local/bin/tunnel-rs",
    "tunnel-rs",
  ];

  for (const cand of candidates) {
    if (existsSync(cand)) {
      return cand;
    }
  }

  // Fallback: build from source if directory exists
  const tunnelRsSrcDir = join(home, "Documents/GitHub/NATA/backend/tunnel-rs");
  if (existsSync(tunnelRsSrcDir)) {
    const spinner = logger.spinner("Compiling high-performance tunnel-rs engine (first-time build)...");
    try {
      const buildProc = Bun.spawn(["cargo", "build", "--release"], {
        cwd: tunnelRsSrcDir,
        stdout: "ignore",
        stderr: "ignore",
      });
      await buildProc.exited;
      const releaseBin = join(tunnelRsSrcDir, "target/release/tunnel-rs");
      if (existsSync(releaseBin)) {
        spinner.stop(true, "tunnel-rs engine compiled successfully");
        return releaseBin;
      }
    } catch (err: any) {
      spinner.stop(false, "Failed to compile tunnel-rs");
      logger.error("Could not build tunnel-rs engine", err.message);
    }
  }

  throw new Error("Could not find 'tunnel-rs' binary. Please run 'cargo build --release' inside backend/tunnel-rs");
}

export async function startTunnelBackground(options: TunnelOptions = {}): Promise<{
  publicUrl: string;
  subdomain: string;
  proc: any;
}> {
  if (options.provider === "123c" || options.server?.includes("123c")) {
    return await start123cTunnel(options);
  }

  try {
    const binPath = await locateOrBuildTunnelBinary();
    const port = String(options.port || 3000);
    const server = options.server || process.env.TUNNEL_SERVER || "tunnel.myworkbeast.com:8080";

    const args = [binPath, "client", "--server", server, "--port", port];
    if (options.subdomain) {
      args.push("--subdomain", options.subdomain);
    }

    return await new Promise((resolvePromise, reject) => {
      let resolved = false;
      const proc = Bun.spawn(args, {
        stdout: "pipe",
        stderr: "pipe",
      });

      const timeout = setTimeout(() => {
        if (!resolved) {
          resolved = true;
          reject(new Error(`Tunnel connection timed out to server ${server}`));
        }
      }, 15000);

      proc.exited.then((exitCode) => {
        if (!resolved && exitCode !== 0) {
          resolved = true;
          clearTimeout(timeout);
          reject(new Error(`tunnel-rs exited with code ${exitCode}`));
        }
      });

      (async () => {
        const reader = proc.stdout.getReader();
        const decoder = new TextDecoder();
        let buffer = "";

        try {
          while (true) {
            const { done, value } = await reader.read();
            if (done) break;
            const text = decoder.decode(value);
            buffer += text;

            // Strip ANSI escape codes
            const clean = buffer.replace(/\x1b\[[0-9;]*[a-zA-Z]/g, "");

            // Match Public URL and Subdomain in the clean text
            const matchUrl = clean.match(/Public URL:\s+([^\s]+)/i);
            const matchSub = clean.match(/Subdomain:\s+([^\s]+)/i);

            if (matchUrl && !resolved) {
              resolved = true;
              clearTimeout(timeout);
              const publicUrl = matchUrl[1].trim();
              const subdomain = matchSub ? matchSub[1].trim() : "";
              resolvePromise({ publicUrl, subdomain, proc });
            }
          }
        } catch (err) {
          if (!resolved) {
            clearTimeout(timeout);
            reject(err);
          }
        }
      })();
    });
  } catch (err: any) {
    logger.warn(`tunnel-rs unavailable (${err.message}), falling back to official mini.123c.vn tunnel...`);
    return await start123cTunnel(options);
  }
}

export async function tunnelCommand(options: TunnelOptions = {}) {
  logger.hero();

  const port = Number(options.port || 3000);
  const server = options.server || process.env.TUNNEL_SERVER || "tunnel.myworkbeast.com:8080";

  logger.info(`Starting tunnel to local port ${colors.bold}${colors.green}${port}${colors.reset}...`);
  logger.info(`Connecting to relay server: ${colors.cyan}${server}${colors.reset}`);

  try {
    const { publicUrl, subdomain, proc } = await startTunnelBackground({
      port,
      subdomain: options.subdomain,
      server,
    });

    logger.card("TUNNEL ESTABLISHED", [
      { label: "Status", value: "● Online (Active)", color: colors.bold + colors.green },
      { label: "Local Target", value: `http://localhost:${port}`, color: colors.cyan },
      { label: "Public URL", value: publicUrl, color: colors.bold + colors.emerald },
      { label: "Subdomain", value: subdomain, color: colors.yellow },
      { label: "Relay Server", value: server, color: colors.darkGray },
    ]);

    if (options.showQr !== false) {
      printTunnelQrCode(publicUrl);
    }

    console.log(`  ${colors.darkGray}Tunnel is running. Press Ctrl+C to terminate.${colors.reset}\n`);

    await proc.exited;
  } catch (err: any) {
    logger.error("Tunnel failed to start", err.message);
    process.exit(1);
  }
}
