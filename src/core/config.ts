import { homedir } from "os";
import { join } from "path";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "fs";

export interface GlobalConfig {
  apiUrl: string;
  token?: string;
  user?: {
    id: string;
    email: string;
    name?: string;
  };
}

export interface LocalViewConfig {
  viewId: number;
  apiUrl: string;
  name?: string;
  dbCode?: string;
  clonedAt: string;
  lastSyncedAt?: string;
  files: Array<{
    path: string;
    checksum?: string;
    size?: number;
  }>;
}

const GLOBAL_CONFIG_DIR = join(homedir(), ".nata");
const GLOBAL_CONFIG_FILE = join(GLOBAL_CONFIG_DIR, "config.json");

export function getGlobalConfig(): GlobalConfig {
  const defaultApiUrl = process.env.COM_API_URL || process.env.NATA_API_URL || "https://base.myworkbeast.com";
  if (!existsSync(GLOBAL_CONFIG_FILE)) {
    return { apiUrl: defaultApiUrl };
  }
  try {
    const raw = readFileSync(GLOBAL_CONFIG_FILE, "utf-8");
    const parsed = JSON.parse(raw);
    return {
      apiUrl: parsed.apiUrl || defaultApiUrl,
      token: parsed.token,
      user: parsed.user,
    };
  } catch {
    return { apiUrl: defaultApiUrl };
  }
}

export function saveGlobalConfig(config: Partial<GlobalConfig>): void {
  if (!existsSync(GLOBAL_CONFIG_DIR)) {
    mkdirSync(GLOBAL_CONFIG_DIR, { recursive: true });
  }
  const current = getGlobalConfig();
  const updated = { ...current, ...config };
  writeFileSync(GLOBAL_CONFIG_FILE, JSON.stringify(updated, null, 2), "utf-8");
}

export function saveLocalViewConfig(targetDir: string, config: LocalViewConfig): void {
  const dotNataDir = join(targetDir, ".nata");
  if (!existsSync(dotNataDir)) {
    mkdirSync(dotNataDir, { recursive: true });
  }
  const configFile = join(dotNataDir, "view.json");
  writeFileSync(configFile, JSON.stringify(config, null, 2), "utf-8");
}

export function getLocalViewConfig(targetDir: string): LocalViewConfig | null {
  const configFile = join(targetDir, ".nata", "view.json");
  if (!existsSync(configFile)) {
    return null;
  }
  try {
    const raw = readFileSync(configFile, "utf-8");
    return JSON.parse(raw) as LocalViewConfig;
  } catch {
    return null;
  }
}
