import { existsSync, mkdirSync, copyFileSync, writeFileSync } from "fs";
import { join, resolve } from "path";
import { logger, colors } from "../core/logger";
import { findEngineBinary, isZaloMiniAppProject } from "./dev";
import { engineEnv } from "../core/binaries";
import { prerenderPayloads } from "../zalominiapp/prerender";
import { execReplace } from "../core/exec";
import { ensurePackages } from "../store/ensure";

export interface BuildOptions {
  dir?: string;
  target?: "web" | "zalo";
  outDir?: string;
}

export async function buildCommand(options: BuildOptions = {}) {
  logger.hero();

  const projectDir = resolve(process.cwd(), options.dir || ".");
  const isZmp = options.target === "zalo" || (!options.target && isZaloMiniAppProject(projectDir));
  const targetMode = isZmp ? "zalo" : "web";
  const outputDir = resolve(projectDir, options.outDir || (isZmp ? "dist" : ".nata/build"));

  logger.section(`BUILDING FOR ${isZmp ? "ZALO MINI APP" : "PRODUCTION WEB"}`);

  const spinner = logger.spinner(
    isZmp
      ? "Packaging Zalo Mini App static distribution..."
      : "Optimizing production bundle via Rust Engine..."
  );

  try {
    if (!existsSync(outputDir)) {
      mkdirSync(outputDir, { recursive: true });
    }

    if (isZmp) {
      // 1. Ensure app-config.json exists
      const configSrc = join(projectDir, "app-config.json");
      const configDest = join(outputDir, "app-config.json");
      if (existsSync(configSrc)) {
        copyFileSync(configSrc, configDest);
      } else {
        const defaultConfig = {
          app: {
            title: "Zalo Mini App",
            headerTitle: "Zalo Mini App",
            headerColor: "#0068FF",
            textColor: "white",
            statusBar: "transparent",
            leftAction: "onlyBack",
          },
          pages: ["pages/index/index"],
          listCSS: ["assets/app.css"],
          listSyncJS: ["assets/app.js"],
        };
        writeFileSync(configDest, JSON.stringify(defaultConfig, null, 2));
      }

      // 2. Generate minimal app entry shell
      const assetsDir = join(outputDir, "assets");
      if (!existsSync(assetsDir)) {
        mkdirSync(assetsDir, { recursive: true });
      }

      // 3. Prerendered payloads of static routes (read by the router before asking the backend)
      const engine = findEngineBinary();
      let prerendered: Record<string, string> = {};
      let prerenderFailed: string[] = [];
      if (engine) {
        spinner.update("Prerendering static routes...");
        const result = await prerenderPayloads(projectDir, outputDir, engine);
        prerendered = result.manifest;
        prerenderFailed = result.failed;
      }

      const htmlEntry = `<!DOCTYPE html>
<html lang="vi">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0, maximum-scale=1.0, user-scalable=no">
  <title>Zalo Mini App</title>
  <link rel="stylesheet" href="./assets/app.css">
</head>
<body>
  <div id="root"></div>
  <script>window.__NATA_PRERENDERED__ = ${JSON.stringify(prerendered)};</script>
  <script src="./assets/app.js"></script>
</body>
</html>`;
      writeFileSync(join(outputDir, "index.html"), htmlEntry);
      writeFileSync(join(outputDir, "app.html"), htmlEntry);

      spinner.stop(true, "Zalo Mini App packaged successfully");

      logger.card("ZALO MINI APP BUILD COMPLETE", [
        { label: "Target", value: "📱 Zalo Mini App (ZMP SPA Bundle)", color: colors.bold + colors.cyan },
        { label: "Output Directory", value: outputDir, color: colors.green },
        { label: "Config File", value: join(outputDir, "app-config.json"), color: colors.sky },
        {
          label: "Prerendered",
          value: engine
            ? `${Object.keys(prerendered).length} static route(s)${prerenderFailed.length ? `, failed: ${prerenderFailed.join(", ")}` : ""}`
            : "skipped (engine binary not found)",
          color: colors.sky,
        },
        { label: "Deploy Command", value: "zmp deploy (or open in Zalo Mini App Studio)", color: colors.yellow },
      ]);
    } else {
      spinner.stop(true, "Building with the Rust engine");
      const engine = findEngineBinary();
      if (!engine) throw new Error("Engine binary not found: reinstall @com.ai.vn/cli or set COM_COMPILER_PATH");
      // The engine prints the route table; output goes to .nata/build (and `output` of next.config)
      const args = ["build", "--dir", projectDir, "--target", "web"];
      await ensurePackages(projectDir, { quiet: true });
      const env = { ...process.env, ...engineEnv(projectDir), COM_MANAGED: "1", COM_BUILD_NEXT_STEPS: "1" };
      // Nothing is left to do here once the engine runs: let it take over this process
      execReplace(engine, args, env);
      const proc = Bun.spawn([engine, ...args], {
        stdout: "inherit",
        stderr: "inherit",
        stdin: "ignore",
        env,
      });
      const code = await proc.exited;
      // The engine printed the next steps (COM_BUILD_NEXT_STEPS)
      if (code !== 0) process.exit(code);
    }
  } catch (err: any) {
    spinner.stop(false, "Build failed");
    logger.error("Build execution error", err.message || String(err));
    process.exit(1);
  }
}
