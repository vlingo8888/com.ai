export * from "./constants";
export * from "./scanner";
export * from "./generator";
export * from "./installer";

import { writeProjectManifests } from "./generator";
import { installDependencies } from "./installer";
import { colors } from "../core/logger";

export interface SetupEnvironmentOptions {
  install?: boolean;
}

/**
 * Facade method to setup manifests for a cloned project.
 * Uses ESM Zero-Install by default (no bun install needed).
 */
export async function setupProjectEnvironment(
  projectDir: string,
  projectName: string,
  files: Array<{ path: string; content?: string }>,
  options: SetupEnvironmentOptions = {}
) {
  const { isPackageCreated, detectedPackages } = await writeProjectManifests(
    projectDir,
    projectName,
    files
  );

  if (detectedPackages.length > 0) {
    console.log(
      `    ${colors.darkGray}› Detected ${colors.bold}${detectedPackages.length}${colors.reset}${colors.darkGray} third-party packages: ${colors.cyan}${detectedPackages.slice(0, 5).join(", ")}${detectedPackages.length > 5 ? "..." : ""}${colors.reset}`
    );
  }

  // ESM Zero-Install runtime: Only install local node_modules if explicitly requested via --install
  if (options.install && isPackageCreated) {
    await installDependencies({ cwd: projectDir });
  } else {
    console.log(
      `    ${colors.darkGray}› ${colors.emerald}⚡ ESM Zero-Install Active${colors.reset}${colors.darkGray} (Native compiler resolves dependencies dynamically via ESM CDN)${colors.reset}`
    );
  }

  return { isPackageCreated, detectedPackages };
}
