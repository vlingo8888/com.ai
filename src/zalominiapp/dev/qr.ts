import { networkInterfaces } from "os";
import qrcode from "qrcode-terminal";
import { colors } from "../../core/logger";

/**
 * Gets the active non-internal IPv4 address for local network access
 */
export function getLocalNetworkIp(): string {
  const nets = networkInterfaces();
  for (const name of Object.keys(nets)) {
    for (const net of nets[name] || []) {
      if (net.family === "IPv4" && !net.internal && net.address !== "127.0.0.1") {
        return net.address;
      }
    }
  }
  return "localhost";
}

/**
 * Builds the official Zalo Mini App local testing deep link URL
 * Format: https://zalo.me/app/link/zapps/{appId}/?env=TESTING_LOCAL&clientIp=http://{host}:{port}
 */
export function generateZaloDeepLink(appId: string, host: string, port: number | string): string {
  const formattedHost = host.startsWith("http://") || host.startsWith("https://")
    ? host
    : `http://${host}:${port}`;

  return `https://zalo.me/app/link/zapps/${appId}/?env=TESTING_LOCAL&clientIp=${formattedHost}`;
}

export function generateZaloShortLink(appId: string, host: string, port: number | string): string {
  const formattedHost = host.startsWith("http://") || host.startsWith("https://")
    ? host
    : `http://${host}:${port}`;

  return `https://zalo.me/s/${appId}/?env=TESTING_LOCAL&clientIp=${formattedHost}`;
}

/**
 * Prints the Zalo testing QR code with clear instructions to terminal
 */
export function printZaloDevQrCode(deepLinkUrl: string, appId: string, networkUrl: string) {
  const shortLink = deepLinkUrl.replace("/app/link/zapps/", "/s/");
  try {
    qrcode.generate(deepLinkUrl, { small: true }, (qr: string) => {
      console.log(`   ${colors.dim}Scan with Zalo camera to preview:${colors.reset}\n`);
      const lines = qr.split("\n");
      for (const line of lines) {
        console.log(`   ${line}`);
      }
      console.log();
      console.log(`   ${colors.dim}-${colors.reset} DeepLink:     ${colors.cyan}${shortLink}${colors.reset}`);
      console.log();
    });
  } catch (err: any) {
    console.log(`   ${colors.dim}-${colors.reset} DeepLink:     ${colors.cyan}${deepLinkUrl}${colors.reset}\n`);
  }
}
