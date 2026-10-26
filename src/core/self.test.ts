import { describe, it, expect } from "bun:test";
import { dirname } from "path";
import { bunEnv, isCompiled, selfDir } from "./self";
import { engineEnv } from "./binaries";

describe("standalone com: isCompiled", () => {
  it("recognizes Bun's embedded file system on unix and Windows", () => {
    expect(isCompiled("/$bunfs/root/com")).toBe(true);
    expect(isCompiled("B:\\~BUN\\root\\com.exe")).toBe(true);
    expect(isCompiled("B:/~BUN/root/com.exe")).toBe(true);
  });

  it("is false when running from source", () => {
    expect(isCompiled("/repo/bin/cli.ts")).toBe(false);
    expect(isCompiled("C:\\repo\\bin\\cli.ts")).toBe(false);
    expect(isCompiled()).toBe(false);
  });
});

describe("standalone com: bunEnv", () => {
  it("has the standalone com act as Bun", () => {
    expect(bunEnv(process.execPath, { A: "1" }, true)).toEqual({ A: "1", BUN_BE_BUN: "1" });
  });

  it("leaves other binaries and a source run as they are", () => {
    expect(bunEnv("/usr/bin/bun", { A: "1" }, true)).toEqual({ A: "1" });
    expect(bunEnv(process.execPath, { A: "1" }, false)).toEqual({ A: "1" });
  });
});

describe("standalone com: selfDir", () => {
  it("is the executable's folder when compiled, else the caller's", () => {
    expect(selfDir("/repo/src/core", true)).toBe(dirname(process.execPath));
    expect(selfDir("/repo/src/core", false)).toBe("/repo/src/core");
  });
});

describe("standalone com: engineEnv", () => {
  it("points the engine at this executable as both the CLI and its Bun", () => {
    const saved = process.env.COM_BUN;
    delete process.env.COM_BUN;
    try {
      const env = engineEnv(undefined, true);
      expect(env.COM_CLI).toBe(process.execPath);
      expect(env.COM_BUN).toBe(process.execPath);
      expect(env.COM_BUN_SELF).toBe("1");
    } finally {
      if (saved !== undefined) process.env.COM_BUN = saved;
    }
  });

  it("keeps the CLI source and no COM_BUN_SELF from source", () => {
    const env = engineEnv(undefined, false);
    expect(env.COM_CLI.endsWith("cli.ts")).toBe(true);
    expect(env.COM_BUN_SELF).toBeUndefined();
  });
});
