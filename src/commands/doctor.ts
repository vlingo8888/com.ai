import { existsSync, readdirSync, readFileSync } from "fs";
import { arch, cpus, platform, release, totalmem } from "os";
import { join, resolve } from "path";
import { logger, colors } from "../core/logger";
import { findAvailablePort, findEngineBinary } from "./dev";
import { isCompiled } from "../core/self";
import packageJson from "../../package.json";

/**
 * next.config options the engine applies. Keep in sync with `SUPPORTED` in
 * compiler-rs/src/next_config.rs (tests/doctor.test.ts checks this)
 */
export const SUPPORTED_NEXT_CONFIG = [
  "basePath", "trailingSlash", "redirects", "rewrites", "headers", "env", "images", "reactStrictMode",
  "experimental", "typescript", "eslint", "poweredByHeader", "serverExternalPackages", "output",
  "transpilePackages", "compiler", "devIndicators",
];

const NEXT_CONFIG_FILES = ["next.config.ts", "next.config.mts", "next.config.mjs", "next.config.js", "next.config.cjs"];
const MIN_BUN = [1, 2, 0];

export type Status = "ok" | "warn" | "fail";
export interface Check {
  name: string;
  status: Status;
  detail: string;
}

function readJson(file: string): any {
  try {
    return JSON.parse(readFileSync(file, "utf-8"));
  } catch {
    return null;
  }
}

function semverAtLeast(version: string, min: number[]): boolean {
  const parts = version.replace(/^[^\d]*/, "").split(".").map((p) => parseInt(p, 10) || 0);
  for (let i = 0; i < min.length; i++) {
    if ((parts[i] ?? 0) !== min[i]) return (parts[i] ?? 0) > min[i];
  }
  return true;
}

function depVersion(pkg: any, name: string): string | undefined {
  return pkg?.dependencies?.[name] ?? pkg?.devDependencies?.[name];
}

/** Engine version from `com-compiler --version` ("com-compiler 0.1.0") */
export function engineVersion(engine: string): string | null {
  try {
    const out = Bun.spawnSync([engine, "--version"]).stdout.toString().trim();
    return out.split(/\s+/).pop() || null;
  } catch {
    return null;
  }
}

/** next.config keys the engine ignores; the config is evaluated the way the engine does it */
export async function unsupportedNextConfig(projectDir: string): Promise<{ file: string; unknown: string[] } | { file: string; error: string } | null> {
  const name = NEXT_CONFIG_FILES.find((f) => existsSync(join(projectDir, f)));
  if (!name) return null;
  try {
    const mod = await import(join(projectDir, name));
    let config = mod.default ?? mod;
    if (typeof config === "function") config = await config("phase-production-build", { defaultConfig: {} });
    const keys = Object.keys((await config) ?? {});
    return { file: name, unknown: keys.filter((k) => !SUPPORTED_NEXT_CONFIG.includes(k)) };
  } catch (e: any) {
    return { file: name, error: e?.message || String(e) };
  }
}

export async function runChecks(projectDir: string, port = 3000): Promise<Check[]> {
  const checks: Check[] = [];
  const add = (name: string, status: Status, detail: string) => checks.push({ name, status, detail });

  add("Bun", semverAtLeast(Bun.version, MIN_BUN) ? "ok" : "fail", `v${Bun.version}${isCompiled() ? " (embedded in com)" : semverAtLeast(Bun.version, MIN_BUN) ? "" : ` (need >= ${MIN_BUN.join(".")})`}`);

  const engine = findEngineBinary();
  if (engine) add("Engine", "ok", `${engineVersion(engine) ?? "unknown version"} (${engine})`);
  else add("Engine", "fail", "com-compiler not found: reinstall @com.ai.vn/cli or set COM_COMPILER_PATH");

  const free = (await findAvailablePort(port)) === port;
  add("Port", free ? "ok" : "warn", free ? `${port} is free` : `${port} is in use (com dev will pick the next free port)`);

  const hasApp = existsSync(join(projectDir, "app")) || existsSync(join(projectDir, "src/app"));
  const hasPages = existsSync(join(projectDir, "pages")) || existsSync(join(projectDir, "src/pages"));
  if (hasApp) add("App Router", "ok", "app/ found");
  else add("App Router", "fail", "no app/ directory: only the App Router is supported");
  if (hasPages) add("Pages Router", "warn", "pages/ is not supported and will be ignored");

  const pkg = readJson(join(projectDir, "package.json"));
  const react = depVersion(pkg, "react");
  if (!pkg) add("package.json", "warn", "missing");
  else if (react && !/(^|[^\d])(19|[2-9]\d)/.test(react) && !/latest|canary|rc/.test(react)) add("React", "warn", `${react}: Server Components need React 19`);
  else add("React", "ok", react ?? "not listed (the bundled React 19 is used)");

  const next = depVersion(pkg, "next");
  if (next) add("next", "ok", `${next} (only its APIs are used; com replaces the Next.js runtime)`);

  const config = await unsupportedNextConfig(projectDir);
  if (config && "error" in config) add("next.config", "fail", `${config.file} failed to load: ${config.error}`);
  else if (config && config.unknown.length) add("next.config", "warn", `${config.file}: ignored options ${config.unknown.join(", ")}`);
  else if (config) add("next.config", "ok", config.file);

  if (["middleware", "proxy"].every((n) => [".ts", ".js"].some((e) => existsSync(join(projectDir, n + e)) || existsSync(join(projectDir, "src", n + e))))) {
    add("Middleware", "warn", "both middleware and proxy exist: proxy wins, like Next 16");
  }

  const envFiles = readdirSync(projectDir).filter((f) => f === ".env" || f.startsWith(".env."));
  add("Env", "ok", envFiles.length ? envFiles.join(", ") : "no .env files");

  if (existsSync(join(projectDir, "tsconfig.json"))) {
    const hasTypes = existsSync(join(projectDir, "node_modules/@types/react"));
    add("Types", hasTypes ? "ok" : "warn", hasTypes ? "tsconfig.json + @types/react" : "no node_modules/@types/react: `com typecheck` needs `bun install`");
  }

  return checks;
}

export interface DoctorOptions {
  dir?: string;
  json?: boolean;
  port?: number;
}

export async function doctorCommand(options: DoctorOptions = {}) {
  const projectDir = resolve(process.cwd(), options.dir || ".");
  const checks = await runChecks(projectDir, options.port);
  if (options.json) {
    console.log(JSON.stringify(checks, null, 2));
  } else {
    logger.hero(packageJson.version);
    logger.section("DOCTOR");
    const icon = { ok: `${colors.green}✓`, warn: `${colors.yellow}⚠`, fail: `${colors.red}⨯` };
    const width = Math.max(...checks.map((c) => c.name.length));
    for (const c of checks) console.log(`   ${icon[c.status]}${colors.reset} ${c.name.padEnd(width)}  ${colors.dim}${c.detail}${colors.reset}`);
    console.log();
  }
  if (checks.some((c) => c.status === "fail")) process.exit(1);
}

export async function infoCommand(options: { dir?: string; json?: boolean } = {}) {
  const projectDir = resolve(process.cwd(), options.dir || ".");
  const pkg = readJson(join(projectDir, "package.json"));
  const engine = findEngineBinary();
  const info: Record<string, string> = {
    "Operating System": `${platform()} ${release()} (${arch()})`,
    CPUs: String(cpus().length),
    Memory: `${Math.round(totalmem() / 1024 ** 3)} GB`,
    CLI: `@com.ai.vn/cli v${packageJson.version}`,
    Engine: engine ? `${engineVersion(engine) ?? "unknown"} (${engine})` : "not found",
    Bun: `v${Bun.version}`,
  };
  for (const dep of ["next", "react", "react-dom", "typescript"]) info[dep] = depVersion(pkg, dep) ?? "N/A";
  info["next.config"] = NEXT_CONFIG_FILES.find((f) => existsSync(join(projectDir, f))) ?? "N/A";

  if (options.json) {
    console.log(JSON.stringify(info, null, 2));
    return;
  }
  logger.hero(packageJson.version);
  logger.card("ENVIRONMENT", Object.entries(info).map(([label, value]) => ({ label, value })));
}
