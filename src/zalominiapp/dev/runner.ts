import { resolve } from "path";
import { logger, colors } from "../../core/logger";
import { loadZmpConfig, resolveOrPromptAppId } from "./config";
import { setupAdbReverse } from "./adb";
import { getLocalNetworkIp, generateZaloDeepLink, printZaloDevQrCode } from "./qr";
import { createSimulatorHtml } from "./simulator";
import type { ZmpDevOptions, ZmpDevContext } from "./types";

/**
 * Initializes and starts the complete Zalo Mini App Dev environment
 * without external Zalo tools (zmp-cli, zmp-sdk).
 */
export async function startZmpDev(options: ZmpDevOptions = {}): Promise<ZmpDevContext> {
  const projectDir = resolve(process.cwd(), options.cwd || ".");
  const port = Number(options.port || process.env.PORT || 3000);
  const host = options.host || "0.0.0.0";
  const localIp = getLocalNetworkIp();
  const appId = await resolveOrPromptAppId(projectDir, options.appId);
  const config = loadZmpConfig(projectDir);

  const networkUrl = `http://${localIp}:${port}`;
  const deepLinkUrl = generateZaloDeepLink(appId, localIp, port);

  const context: ZmpDevContext = {
    projectDir,
    appId,
    port,
    host,
    localIp,
    networkUrl,
    deepLinkUrl,
    config,
  };

  logger.section("ZALO MINI APP DEV ENGINE");

  // 1. Setup ADB Reverse connection for USB-connected Android devices
  try {
    await setupAdbReverse(port);
  } catch {}

  // 2. Display dev environment information
  logger.card("ZALO MINI APP DEV ACTIVE", [
    { label: "App ID", value: appId, color: colors.bold + colors.yellow },
    { label: "Local URL", value: `http://localhost:${port}`, color: colors.bold + colors.sky },
    { label: "Network URL", value: networkUrl, color: colors.bold + colors.green },
    { label: "Simulator Frame", value: `http://localhost:${port}/__simulator`, color: colors.cyan },
    { label: "DeepLink", value: deepLinkUrl, color: colors.darkGray },
    { label: "Mock Bridge", value: "● Active (window.ZMP / window.zmp)", color: colors.emerald },
  ]);

  // 3. Print QR code on terminal for live scanning via Zalo camera
  printZaloDevQrCode(deepLinkUrl, appId, networkUrl);

  return context;
}
