import { describe, it, expect, beforeEach, afterEach } from "bun:test";
import { mkdtempSync, rmSync, writeFileSync } from "fs";
import { tmpdir } from "os";
import { join } from "path";
import { resolveTarget, savedTarget } from "../src/commands/dev";

describe("com dev: target selection", () => {
  let dir: string;
  let asked: number;
  const ask = (answer: "web" | "zalo") => async () => {
    asked++;
    return answer;
  };

  beforeEach(() => {
    dir = mkdtempSync(join(tmpdir(), "com-dev-target-"));
    asked = 0;
  });
  afterEach(() => rmSync(dir, { recursive: true, force: true }));

  it("starts a web project right away, without asking or remembering", async () => {
    expect(await resolveTarget(dir, undefined, ask("zalo"))).toBe("web");
    expect(asked).toBe(0);
    expect(savedTarget(dir)).toBeNull();
  });

  it("asks once for a Zalo Mini App project, then remembers the answer", async () => {
    writeFileSync(join(dir, "app-config.json"), "{}");
    expect(await resolveTarget(dir, undefined, ask("web"))).toBe("web");
    expect(await resolveTarget(dir, undefined, ask("zalo"))).toBe("web");
    expect(asked).toBe(1);
  });

  it("uses zalo for a Zalo project when it cannot ask", async () => {
    writeFileSync(join(dir, "app-config.json"), "{}");
    expect(await resolveTarget(dir, undefined, null)).toBe("zalo");
  });

  it("--target wins and becomes the remembered choice", async () => {
    expect(await resolveTarget(dir, "zalo", ask("web"))).toBe("zalo");
    expect(await resolveTarget(dir, undefined, ask("web"))).toBe("zalo");
    expect(asked).toBe(0);
  });
});
