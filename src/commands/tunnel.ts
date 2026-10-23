import { existsSync } from "fs";
import { join } from "path";
import { homedir } from "os";
import { exeName } from "../core/binaries";
import { logger, colors } from "../core/logger";

export interface TunnelOptions {
  port?: number | string;
  subdomain?: string;
  server?: string;
  provider?: "123c" | "rs";
  showQr?: boolean;
  /** tunnel-rs only: fall back to a mini.123c.vn tunnel when tunnel-rs cannot connect (default true) */
  fallback?: boolean;
}

export interface TunnelHandle {
  publicUrl: string;
  subdomain: string;
  proc: any;
}

export function printTunnelQrCode(url: string, title = "QUÉT MÃ QR BẰNG ZALO HOẶC CAMERA ĐỂ MỞ TRÊN ĐIỆN THOẠI") {
  try {
    // Loaded here: only tunnel and Zalo sessions print a QR code
    const qrcode: typeof import("qrcode-terminal") = require("qrcode-terminal");
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
  const args = [process.execPath, "x", "localtunnel", "--host", "https://mini.123c.vn", "--port", port];
  if (options.subdomain) {
    args.push("--subdomain", options.subdomain);
  }

  return new Promise((resolvePromise, reject) => {
    let resolved = false;
    const proc = Bun.spawn(args, {
      stdout: "pipe",
      stderr: "ignore",
    });

    const timeout = setTimeout(() => {
      if (!resolved) {
        resolved = true;
        proc.kill();
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
  const home = homedir();
  const bin = exeName("tunnel-rs");
  const candidates = [
    join(__dirname, `../${bin}`),
    join(__dirname, `../../${bin}`),
    join(__dirname, `../bin/${bin}`),
    join(__dirname, `../../bin/${bin}`),
    join(process.cwd(), `bin/${bin}`),
    join(process.cwd(), `${bin}`),
    join(home, `Documents/GitHub/NATA/backend/tunnel-rs/target/release/${bin}`),
    join(home, `Documents/GitHub/NATA/backend/tunnel-rs/target/debug/${bin}`),
    join(__dirname, `../../../NATA/backend/tunnel-rs/target/release/${bin}`),
    join(__dirname, `../../../NATA/backend/tunnel-rs/target/debug/${bin}`),
    join(process.cwd(), `tunnel-rs/target/release/${bin}`),
    join(process.cwd(), `tunnel-rs/target/debug/${bin}`),
    join(process.cwd(), `backend/tunnel-rs/target/release/${bin}`),
    join(process.cwd(), `backend/tunnel-rs/target/debug/${bin}`),
    `/usr/local/bin/${bin}`,
    bin,
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
      const buildProc = Bun.spawn([Bun.which(exeName("cargo")) || "cargo", "build", "--release"], {
        cwd: tunnelRsSrcDir,
        stdout: "ignore",
        stderr: "ignore",
      });
      await buildProc.exited;
      const releaseBin = join(tunnelRsSrcDir, `target/release/${exeName("tunnel-rs")}`);
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
    if (options.fallback === false) throw err;
    logger.warn(`tunnel-rs unavailable (${err.message}), falling back to official mini.123c.vn tunnel...`);
    return await start123cTunnel(options);
  }
}

/** Dev server endpoint that always answers 200 locally; used to tell a dead tunnel from a missing page */
const HEALTH_PATH = "/_nata/hmr_version";
const HEALTH_INTERVAL_MS = 20_000;

async function probe(url: string): Promise<boolean> {
  try {
    const res = await fetch(url, { signal: AbortSignal.timeout(10_000), headers: { "bypass-tunnel-reminder": "1" } });
    await res.arrayBuffer().catch(() => {});
    return res.ok;
  } catch {
    return false;
  }
}

/**
 * Keeps a dev tunnel alive. The public URL is baked into the Zalo bootstrap (COM_BACKEND_URL) and the
 * QR code, so a tunnel that silently drops leaves the app hanging on requests that never arrive. The
 * client is restarted when its process exits, or when the relay stops forwarding (the public health
 * check fails twice while the local one passes); it asks for the same subdomain so the URL stays valid.
 */
export function superviseTunnel(
  initial: TunnelHandle,
  options: TunnelOptions & { label: string },
): { current: () => TunnelHandle; stop: () => void } {
  const port = String(options.port || 3000);
  let current = initial;
  let stopped = false;
  let failures = 0;
  let restarting = false;

  const restart = async () => {
    if (stopped || restarting) return;
    restarting = true;
    try { current.proc?.kill(); } catch {}
    // Let the relay release the old subdomain, otherwise it hands out a random one
    await Bun.sleep(2000);
    for (let attempt = 1; !stopped; attempt++) {
      try {
        const next = await startTunnelBackground({ ...options, subdomain: current.subdomain || options.subdomain });
        if (stopped) { try { next.proc?.kill(); } catch {} return; }
        if (next.publicUrl !== current.publicUrl) {
          logger.warn(`${options.label} tunnel reconnected on a NEW URL ${next.publicUrl} (was ${current.publicUrl}); restart \`com dev\` so the app and QR code use it`);
        } else {
          logger.success(`${options.label} tunnel reconnected: ${next.publicUrl}`);
        }
        current = next;
        failures = 0;
        watchExit(next);
        break;
      } catch (err: any) {
        const delay = Math.min(30_000, 1000 * 2 ** attempt);
        logger.warn(`${options.label} tunnel reconnect failed (${err.message}); retrying in ${delay / 1000}s`);
        await Bun.sleep(delay);
      }
    }
    restarting = false;
  };

  const watchExit = (handle: TunnelHandle) => {
    handle.proc?.exited?.then(() => {
      if (stopped || handle !== current) return;
      logger.warn(`${options.label} tunnel process exited; reconnecting...`);
      restart();
    });
  };
  watchExit(initial);

  const timer = setInterval(async () => {
    if (stopped || restarting) return;
    const handle = current;
    if (await probe(handle.publicUrl + HEALTH_PATH)) { failures = 0; return; }
    // The dev server itself is down or restarting: not the tunnel's fault
    if (!(await probe(`http://localhost:${port}${HEALTH_PATH}`))) return;
    if (handle !== current) return;
    if (++failures >= 2) {
      logger.warn(`${options.label} tunnel ${handle.publicUrl} stopped forwarding requests; reconnecting...`);
      failures = 0;
      restart();
    }
  }, HEALTH_INTERVAL_MS);
  timer.unref?.();

  const stop = () => {
    stopped = true;
    clearInterval(timer);
    try { current.proc?.kill(); } catch {}
  };
  process.once("exit", stop);

  return { current: () => current, stop };
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
