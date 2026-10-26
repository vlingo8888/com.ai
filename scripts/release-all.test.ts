import { describe, it, expect } from "bun:test";
import { resolve } from "path";
import { nextVersion, parseArgs, withVersion } from "./release-all";

describe("release:all: nextVersion", () => {
  it("bumps patch, minor and major", () => {
    expect(nextVersion("1.0.3", "patch")).toBe("1.0.4");
    expect(nextVersion("1.0.3", "minor")).toBe("1.1.0");
    expect(nextVersion("1.0.3", "major")).toBe("2.0.0");
    expect(nextVersion("1.1.0-beta.1", "patch")).toBe("1.1.1");
  });

  it("takes an explicit version", () => {
    expect(nextVersion("1.0.3", "1.2.0")).toBe("1.2.0");
    expect(nextVersion("1.0.3", "2.0.0-rc.1")).toBe("2.0.0-rc.1");
  });

  it("rejects a missing or unknown bump", () => {
    expect(() => nextVersion("1.0.3", "")).toThrow("got \"nothing\"");
    expect(() => nextVersion("1.0.3", "huge")).toThrow();
  });
});

describe("release:all: parseArgs", () => {
  it("defaults to the full release", () => {
    expect(parseArgs(["patch"])).toEqual({
      bump: "patch",
      dryRun: false,
      build: true,
      test: true,
      installers: true,
      npm: true,
      allowDirty: false,
    });
  });

  it("reads every option", () => {
    const options = parseArgs(["1.2.0", "--dry-run", "--no-build", "--no-test", "--skip-npm", "--allow-dirty", "--env", "x.env", "--otp=123456"]);
    expect(options).toMatchObject({ bump: "1.2.0", dryRun: true, build: false, test: false, npm: false, installers: true, allowDirty: true, otp: "123456" });
    expect(options.envFile).toBe(resolve("x.env"));
    expect(parseArgs(["patch", "--otp", "654321", "--skip-installers"])).toMatchObject({ otp: "654321", installers: false });
  });

  it("rejects an unknown option", () => {
    expect(() => parseArgs(["patch", "--upload"])).toThrow("Unknown option: --upload");
  });
});

describe("release:all: withVersion", () => {
  it("pins the engine packages to the new version and leaves the rest", () => {
    const pkg = {
      name: "@com.ai.vn/cli",
      version: "1.0.3",
      dependencies: { bun: "1.4.2" },
      optionalDependencies: { "@com.ai.vn/compiler-darwin-arm64": "1.0.3", "other": "^2.0.0" },
    };
    const next = withVersion(pkg, "1.0.4");
    expect(next.version).toBe("1.0.4");
    expect(next.optionalDependencies).toEqual({ "@com.ai.vn/compiler-darwin-arm64": "1.0.4", "other": "^2.0.0" });
    expect(next.dependencies).toEqual({ bun: "1.4.2" });
    expect(pkg.version).toBe("1.0.3");
  });
});
