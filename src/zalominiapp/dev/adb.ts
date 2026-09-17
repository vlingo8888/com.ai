import { spawn } from "child_process";
import { logger, colors } from "../../core/logger";

export interface AdbReverseResult {
  success: boolean;
  devices: string[];
  message?: string;
}

/**
 * Lists connected Android devices via `adb devices`
 */
export async function listAdbDevices(): Promise<string[]> {
  return new Promise((resolve) => {
    try {
      const proc = spawn("adb", ["devices"]);
      let stdout = "";
      proc.stdout?.on("data", (data) => {
        stdout += data.toString();
      });

      proc.on("error", () => {
        resolve([]);
      });

      proc.on("close", (code) => {
        if (code !== 0) return resolve([]);
        const lines = stdout.split("\n").slice(1);
        const devices = lines
          .map((l) => l.trim())
          .filter((l) => l && !l.startsWith("*") && l.includes("\tdevice"))
          .map((l) => l.split("\t")[0]);
        resolve(devices);
      });
    } catch {
      resolve([]);
    }
  });
}

/**
 * Attempts to set up ADB reverse socket connection (adb reverse tcp:PORT tcp:PORT)
 * Enables testing on USB-connected Android devices with localhost
 */
export async function setupAdbReverse(port: number): Promise<AdbReverseResult> {
  const devices = await listAdbDevices();
  if (devices.length === 0) {
    return {
      success: false,
      devices: [],
      message: "No connected ADB devices detected",
    };
  }

  return new Promise((resolve) => {
    try {
      const proc = spawn("adb", ["reverse", `tcp:${port}`, `tcp:${port}`]);
      let stderr = "";
      proc.stderr?.on("data", (data) => {
        stderr += data.toString();
      });

      proc.on("error", (err) => {
        resolve({
          success: false,
          devices,
          message: err.message,
        });
      });

      proc.on("close", (code) => {
        if (code === 0) {
          logger.info(
            `${colors.emerald}✔ Reverse socket connected${colors.reset} ${colors.darkGray}(adb reverse tcp:${port} tcp:${port} on ${devices.join(", ")})${colors.reset}`
          );
          resolve({ success: true, devices });
        } else {
          resolve({
            success: false,
            devices,
            message: stderr.trim() || `adb reverse exited with code ${code}`,
          });
        }
      });
    } catch (err: any) {
      resolve({
        success: false,
        devices,
        message: err.message,
      });
    }
  });
}
