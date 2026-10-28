import { resolve } from "path";
import { NataApiClient, SaveConflictError } from "../core/api";
import { getLocalViewConfig, saveLocalViewConfig } from "../core/config";
import { logger, colors } from "../core/logger";
import { compileAllEs5 } from "../core/es5";
import { collectProjectFiles, computeDeleted, recordSave } from "./save";

export interface PublishOptions {
  dir?: string;
  viewId?: number | string;
  api?: string;
  token?: string;
  domain?: string;
  skipSave?: boolean;
}

export async function publishCommand(options: PublishOptions = {}) {
  const targetDir = resolve(process.cwd(), options.dir || ".");
  const localConfig = getLocalViewConfig(targetDir);

  const viewIdArg = options.viewId || localConfig?.viewId;

  if (!viewIdArg) {
    logger.hero();
    logger.error(
      "No View ID found in this directory.",
      "Ensure you are inside a cloned view project (with .nata/view.json) or specify viewId:\n    com publish --view <viewId>\n    com publish <viewId>"
    );
    process.exit(1);
  }

  const viewId = typeof viewIdArg === "string" ? parseInt(viewIdArg.replace(/^view_/, ""), 10) : viewIdArg;

  if (!viewId || isNaN(viewId)) {
    logger.error("Please provide a valid viewId", "Usage: com publish [options]");
    process.exit(1);
  }

  const apiUrl = options.api || localConfig?.apiUrl || "https://base.myworkbeast.com";
  const token = options.token;
  const client = new NataApiClient(apiUrl, token);

  logger.hero();
  console.log(`\n  ${colors.bold}${colors.indigo}◆ PUBLISHING VIEW #${viewId} TO PRODUCTION${colors.reset}\n`);

  // Step 1: Save/sync latest code to Cloud before publishing (unless --skip-save)
  if (!options.skipSave) {
    const spinner = logger.spinner("Packaging and syncing latest changes to Cloud before publishing...");
    const files = collectProjectFiles(targetDir);
    const deleted = computeDeleted(targetDir, localConfig?.files, new Set(files.map((f) => f.path)));
    if (files.length > 0 || deleted.length > 0) {
      try {
        const failed: Array<{ path: string; message: string }> = [];
        const rawFilesPayload = compileAllEs5(
          files.map((f) => ({ path: f.path, content: f.content })),
          failed
        );
        if (failed.length > 0) {
          spinner.stop(false, `${failed.length} file(s) did not compile`);
          for (const f of failed) {
            console.log(`    ${colors.red}✖ ${f.path}${colors.reset} ${colors.darkGray}${f.message.split("\n")[0]}${colors.reset}`);
          }
          logger.error("Fix these files before publishing: the published app cannot run them.");
          process.exit(1);
        }
        // Same save as `com save`: merge keeps binary files on the server, and the base version
        // catches edits made elsewhere instead of overwriting them.
        const res = await client.saveViewRawFiles(viewId, rawFilesPayload, {
          name: localConfig?.name,
          mode: "merge",
          deleted,
          baseUpdatedAt: localConfig?.remoteUpdatedAt,
        });
        if (localConfig) recordSave(targetDir, localConfig, files, deleted, res.updated_at);
        spinner.stop(true, `Synced ${files.length} files to Cloud`);
      } catch (err: any) {
        spinner.stop(false, "Failed to sync local files");
        if (err instanceof SaveConflictError) {
          logger.error(
            "The view was changed on the server since your last clone/save.",
            `Compare first with \`com clone ${viewId} --dir <other-dir>\`, or overwrite with \`com save --force\`, then \`com publish --skip-save\`.`
          );
          process.exit(1);
        }
        logger.error(err.message || String(err), "Could not save files before publishing.");
        process.exit(1);
      }
    } else {
      spinner.stop(true, "No local files needed to sync");
    }
  }

  // Step 2: Trigger publish endpoint
  const pubSpinner = logger.spinner(`Publishing View #${viewId} on Com.AI.VN Cloud (${apiUrl})...`);
  const startTime = Date.now();

  try {
    const res = await client.publishView(viewId, {
      domain: options.domain,
    });

    const elapsed = ((Date.now() - startTime) / 1000).toFixed(2);
    pubSpinner.stop(true, `Successfully published View #${viewId} in ${elapsed}s`);

    // Publishing bumps the view's `updated_at`: record it, or the next save reports a false conflict
    // (re-read the config: the save above already updated it)
    const latestConfig = getLocalViewConfig(targetDir);
    if (latestConfig) {
      const remoteUpdatedAt = await client
        .getViewDetails(viewId)
        .then((v) => v.updated_at)
        .catch(() => undefined);
      saveLocalViewConfig(targetDir, {
        ...latestConfig,
        remoteUpdatedAt: remoteUpdatedAt ?? latestConfig.remoteUpdatedAt,
        lastSyncedAt: new Date().toISOString(),
      });
    }

    const domainDisplay = options.domain || `view-${viewId}.com.ai.vn`;
    const liveUrl = options.domain 
      ? `https://${options.domain}` 
      : `${apiUrl}/api/views/${viewId}`;

    logger.card("PUBLISH SUMMARY", [
      { label: "View Name", value: localConfig?.name || `View #${viewId}`, color: colors.bold + colors.white },
      { label: "View ID", value: String(viewId), color: colors.sky },
      { label: "Production Status", value: "Active / Published (Live)", color: colors.emerald },
      { label: "Public Endpoint", value: liveUrl, color: colors.bold + colors.cyan },
      { label: "Assigned Domain", value: domainDisplay, color: colors.yellow },
      { label: "Cloud Server", value: apiUrl, color: colors.darkGray },
      { label: "Published At", value: new Date().toLocaleTimeString(), color: colors.darkGray },
    ]);

    logger.nextSteps([
      { cmd: `com dev`, desc: "Continue development on local server" },
      { cmd: `open ${liveUrl}`, desc: "Open live view in web browser" },
    ]);
  } catch (err: any) {
    pubSpinner.stop(false, `Failed to publish View #${viewId}`);
    logger.error(
      err.message || String(err),
      "Check server logs or verify authentication with `com login`"
    );
    process.exit(1);
  }
}
