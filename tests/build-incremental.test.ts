// `com build` reuses what did not change since the previous build: the client bundle (when every module
// it read is served the same), its compressed copies and the stylesheet. Pages are always prerendered
// again. Runs through the real CLI, which hands its process over to the engine.
//
// Needs the release engine (`bun run build:rust`); skipped otherwise.

import { afterAll, beforeAll, describe, expect, it } from "bun:test";
import { existsSync, readdirSync, readFileSync, rmSync, statSync, writeFileSync } from "fs";
import { join } from "path";
import { copyToTemp, findEngine, repoRoot } from "./compat/lib/harness";

const engine = (() => {
  try {
    return findEngine();
  } catch {
    return null;
  }
})();
const run = engine && existsSync(engine) ? describe : describe.skip;

run("com build: incremental", () => {
  let dir: string;
  const staticDir = () => join(dir, ".nata/build/static");

  function build() {
    const proc = Bun.spawnSync(["bun", join(repoRoot, "bin/cli.ts"), "build", "--dir", dir], {
      env: { ...process.env, COM_COMPILER_PATH: engine!, COM_BUILD_PROFILE: "1" },
      stdout: "pipe",
      stderr: "pipe",
    });
    const output = proc.stdout.toString() + proc.stderr.toString();
    if (proc.exitCode !== 0) throw new Error(`com build failed (${proc.exitCode}):\n${output}`);
    return output;
  }

  /** Static file → modification time */
  function staticFiles() {
    const files: Record<string, number> = {};
    const walk = (d: string) => {
      for (const entry of readdirSync(d, { withFileTypes: true })) {
        const path = join(d, entry.name);
        if (entry.isDirectory()) walk(path);
        else files[path.slice(staticDir().length)] = statSync(path).mtimeMs;
      }
    };
    walk(staticDir());
    return files;
  }

  const bundleWith = (text: string) =>
    readdirSync(staticDir()).filter((f) => f.endsWith(".js") && readFileSync(join(staticDir(), f), "utf-8").includes(text));

  beforeAll(() => {
    dir = copyToTemp(join(repoRoot, "tests/compat/apps/starter"), "build");
    Bun.spawnSync(["bun", "install"], { cwd: dir, stdout: "ignore", stderr: "ignore" });
  }, 120_000);

  afterAll(() => rmSync(dir, { recursive: true, force: true }));

  it("builds, and prints the next steps itself once the CLI handed over to the engine", () => {
    const output = build();
    expect(output).toContain("Compiled in");
    expect(output).toContain("com start");
    expect(existsSync(join(dir, ".nata/build/manifest.json"))).toBe(true);
    expect(bundleWith("Count: ").length).toBe(1);
  }, 120_000);

  it("reuses the client bundle and stylesheet when nothing changed", () => {
    const before = staticFiles();
    const styles = readFileSync(join(dir, ".nata/build/styles.css"), "utf-8");
    build();
    expect(staticFiles()).toEqual(before);
    expect(readFileSync(join(dir, ".nata/build/styles.css"), "utf-8")).toBe(styles);
    expect(existsSync(join(dir, ".nata/build/pages/index.html"))).toBe(true);
  }, 120_000);

  it("prerenders a Server Component edit and compiles its new classes, keeping the bundle", () => {
    const before = staticFiles();
    const page = join(dir, "app/page.tsx");
    writeFileSync(page, readFileSync(page, "utf-8").replace("Get started by editing", 'Rebuilt <b className="text-[#0a0b0c]">fast</b> by editing'));
    build();
    expect(readFileSync(join(dir, ".nata/build/pages/index.html"), "utf-8")).toContain("Rebuilt");
    expect(readFileSync(join(dir, ".nata/build/styles.css"), "utf-8")).toContain("0a0b0c");
    expect(staticFiles()).toEqual(before);
  }, 120_000);

  it("bundles again after a client component edit, with fresh compressed copies", () => {
    const counter = join(dir, "app/counter.tsx");
    writeFileSync(counter, readFileSync(counter, "utf-8").replace("Count: ", "Clicks: "));
    build();
    expect(bundleWith("Count: ")).toEqual([]);
    const [file] = bundleWith("Clicks: ");
    expect(file).toBeDefined();
    const gz = Bun.gunzipSync(readFileSync(join(staticDir(), file + ".gz")));
    expect(new TextDecoder().decode(gz)).toContain("Clicks: ");
    expect(existsSync(join(staticDir(), file + ".br"))).toBe(true);
  }, 120_000);
});
