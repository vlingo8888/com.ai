import { existsSync, readFileSync, writeFileSync } from "fs";
import { join } from "path";
import { colors, logger } from "../../core/logger";
import type { ZmpAppConfig } from "./types";

export const DEFAULT_APP_ID = "2840505191283749120";

/**
 * Validates whether a given string is a valid Zalo Mini App ID
 */
export function validateZaloAppId(appId?: string): { valid: boolean; error?: string } {
  if (!appId || typeof appId !== "string") {
    return { valid: false, error: "App ID không được để trống." };
  }

  const trimmed = appId.trim();
  if (trimmed.length === 0) {
    return { valid: false, error: "App ID không được để trống." };
  }

  // Zalo App IDs are numeric strings (typically 6-25 digits)
  if (!/^\d{5,30}$/.test(trimmed)) {
    return {
      valid: false,
      error: "App ID không hợp lệ! Zalo App ID phải là chuỗi số (ví dụ: 2840505191283749120).",
    };
  }

  return { valid: true };
}

/**
 * Loads and parses Zalo Mini App config file (app-config.json, zmp.json, or hr.config.json)
 */
export function loadZmpConfig(projectDir: string): ZmpAppConfig {
  const candidateFiles = [
    "app-config.json",
    "zmp.json",
    "hr.config.json",
    "app.config.json",
  ];

  for (const filename of candidateFiles) {
    const fullPath = join(projectDir, filename);
    if (existsSync(fullPath)) {
      try {
        const raw = readFileSync(fullPath, "utf-8");
        return JSON.parse(raw);
      } catch {}
    }
  }

  // Return a safe default config if none found
  return {
    app: {
      title: "Zalo Mini App",
      headerTitle: "Zalo Mini App",
      statusBar: "transparent",
      textColor: "black",
      leftButton: "none",
    },
    listCSS: [],
    listJS: [],
    pages: ["pages/index/index"],
  };
}

/**
 * Saves or updates the App ID in the project config file (app-config.json or .env)
 */
export function saveAppIdToProject(projectDir: string, appId: string): void {
  const appConfigPath = join(projectDir, "app-config.json");
  if (existsSync(appConfigPath)) {
    try {
      const raw = readFileSync(appConfigPath, "utf-8");
      const config = JSON.parse(raw);
      config.appId = appId;
      writeFileSync(appConfigPath, JSON.stringify(config, null, 2), "utf-8");
      logger.info(`${colors.emerald}✔ Đã tự động lưu App ID vào app-config.json${colors.reset}`);
      return;
    } catch {}
  }

  // Otherwise save to .env or .env.local
  const envPath = join(projectDir, ".env");
  try {
    let envContent = existsSync(envPath) ? readFileSync(envPath, "utf-8") : "";
    if (envContent.includes("ZMP_APP_ID=")) {
      envContent = envContent.replace(/ZMP_APP_ID=.*/g, `ZMP_APP_ID=${appId}`);
    } else {
      envContent += `\nZMP_APP_ID=${appId}\n`;
    }
    writeFileSync(envPath, envContent.trim() + "\n", "utf-8");
    logger.info(`${colors.emerald}✔ Đã tự động lưu ZMP_APP_ID vào .env${colors.reset}`);
  } catch {}
}

/**
 * Interactively prompts the user to enter a valid Zalo Mini App ID with loop validation
 */
export async function promptZaloAppId(projectDir: string): Promise<string> {
  if (!process.stdin.isTTY) {
    return DEFAULT_APP_ID;
  }

  const readline = await import("readline");
  const rl = readline.createInterface({
    input: process.stdin,
    output: process.stdout,
  });

  const ask = (query: string): Promise<string> =>
    new Promise((resolve) => rl.question(query, resolve));

  console.log(`\n  ${colors.bold}${colors.yellow}⚡ Chưa tìm thấy Zalo Mini App ID trong dự án.${colors.reset}`);
  console.log(`  ${colors.darkGray}Lấy App ID từ Zalo Developer Portal:${colors.reset} ${colors.sky}https://mini.zalo.me${colors.reset}\n`);

  while (true) {
    const input = await ask(`  ${colors.bold}${colors.white}Nhập Zalo Mini App ID của bạn: ${colors.reset}`);
    const trimmed = input.trim();
    const result = validateZaloAppId(trimmed);

    if (result.valid) {
      rl.close();
      saveAppIdToProject(projectDir, trimmed);
      return trimmed;
    } else {
      console.log(`  ${colors.bold}${colors.red}✖ ${result.error}${colors.reset}`);
      console.log(`  ${colors.darkGray}Ví dụ định dạng đúng: 2840505191283749120${colors.reset}\n`);
    }
  }
}

/**
 * Resolves the Zalo Mini App ID from options, env variables, or package.json
 */
export function resolveAppId(projectDir: string, explicitAppId?: string): string | null {
  if (explicitAppId && explicitAppId.trim()) {
    const check = validateZaloAppId(explicitAppId);
    if (check.valid) return explicitAppId.trim();
  }

  if (process.env.ZMP_APP_ID || process.env.APP_ID) {
    const envVal = (process.env.ZMP_APP_ID || process.env.APP_ID)!.trim();
    if (validateZaloAppId(envVal).valid) return envVal;
  }

  // Check app-config.json / zmp.json
  const candidateFiles = ["app-config.json", "zmp.json", "hr.config.json"];
  for (const filename of candidateFiles) {
    const fullPath = join(projectDir, filename);
    if (existsSync(fullPath)) {
      try {
        const raw = readFileSync(fullPath, "utf-8");
        const json = JSON.parse(raw);
        const val = String(json.appId || json.app?.appId || json.id || "").trim();
        if (validateZaloAppId(val).valid) {
          return val;
        }
      } catch {}
    }
  }

  const pkgPath = join(projectDir, "package.json");
  if (existsSync(pkgPath)) {
    try {
      const raw = readFileSync(pkgPath, "utf-8");
      const pkg = JSON.parse(raw);
      const val = String(pkg.zaloAppId || pkg.appId || pkg.zmp?.appId || "").trim();
      if (validateZaloAppId(val).valid) {
        return val;
      }
    } catch {}
  }

  return null;
}

/**
 * Resolves existing App ID or interactively prompts user to enter one with validation
 */
export async function resolveOrPromptAppId(projectDir: string, explicitAppId?: string): Promise<string> {
  const existing = resolveAppId(projectDir, explicitAppId);
  if (existing) {
    return existing;
  }

  return await promptZaloAppId(projectDir);
}
