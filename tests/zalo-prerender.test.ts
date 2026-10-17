import { afterAll, describe, expect, it } from "bun:test";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "fs";
import { tmpdir } from "os";
import { join } from "path";
import { payloadFileName, prerenderPayloads, staticRoutes } from "../src/zalominiapp/prerender";
import { copyToTemp, findEngine, repoRoot } from "./compat/lib/harness";

describe("Zalo prerender: static routes", () => {
  const dir = mkdtempSync(join(tmpdir(), "com-prerender-"));
  afterAll(() => rmSync(dir, { recursive: true, force: true }));

  it("lists pages without dynamic segments, route groups removed", () => {
    for (const rel of [
      "app/page.tsx",
      "app/about/page.tsx",
      "app/(shop)/cart/page.tsx",
      "app/blog/[slug]/page.tsx",
      "app/_private/page.tsx",
      "app/@modal/login/page.tsx",
      "app/docs/layout.tsx",
    ]) {
      mkdirSync(join(dir, rel, ".."), { recursive: true });
      writeFileSync(join(dir, rel), "export default function P() { return null; }");
    }
    expect(staticRoutes(dir)).toEqual(["/", "/about", "/cart"]);
  });

  it("names payload files after the route", () => {
    expect(payloadFileName("/")).toBe("index.rsc");
    expect(payloadFileName("/blog/intro")).toBe("blog__intro.rsc");
  });
});

const engine = (() => {
  try {
    return findEngine();
  } catch {
    return null;
  }
})();

(engine && existsSync(engine) ? describe : describe.skip)("Zalo prerender: payloads", () => {
  it("renders the Flight payload of each static route into the package", async () => {
    const project = copyToTemp(join(repoRoot, "tests/compat/apps/routing"), "prerender");
    Bun.spawnSync(["bun", "install"], { cwd: project, stdout: "ignore", stderr: "ignore" });
    const out = join(project, "dist");
    const result = await prerenderPayloads(project, out, engine!);
    expect(result.failed).toEqual([]);
    expect(Object.keys(result.manifest).sort()).toEqual(["/", "/pricing"]);
    const home = readFileSync(join(out, result.manifest["/"]), "utf-8");
    expect(home).toContain("Routing home");
    expect(JSON.parse(readFileSync(join(out, "assets/rsc/manifest.json"), "utf-8"))["/pricing"]).toBe("assets/rsc/pricing.rsc");
    rmSync(project, { recursive: true, force: true });
  }, 240_000);
});
