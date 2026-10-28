import { afterAll, describe, expect, it } from "bun:test";
import { mkdtempSync, rmSync, writeFileSync } from "fs";
import { tmpdir } from "os";
import { join } from "path";
import { serialImport } from "../compiler-rs/src/worker_pool/serial_import";

// Parallel first import() calls into one server module graph (cycles + top-level await, as in comx
// builds) make Bun hand out uninitialized bindings; serialImport evaluates them one after another
describe("worker: serialImport", () => {
  const dir = mkdtempSync(join(tmpdir(), "serial-import-"));
  afterAll(() => rmSync(dir, { recursive: true, force: true }));
  const g = globalThis as any;
  g.__serialLog = [] as string[];
  // Each module records when its evaluation starts and ends (top-level await in between)
  const slowModule = (name: string) =>
    `globalThis.__serialLog.push("start ${name}");\nawait new Promise((r) => setTimeout(r, 20));\nglobalThis.__serialLog.push("end ${name}");\nexport const name = "${name}";\n`;
  for (const name of ["a", "b", "c"]) writeFileSync(join(dir, `${name}.ts`), slowModule(name));

  it("evaluates first imports one at a time", async () => {
    const mods = await Promise.all(["a", "b", "c"].map((name) => serialImport(join(dir, `${name}.ts`))));
    expect(mods.map((m) => m.name)).toEqual(["a", "b", "c"]);
    expect(g.__serialLog).toEqual(["start a", "end a", "start b", "end b", "start c", "end c"]);
  });

  it("loads a module once for parallel requests, which all get it fully evaluated", async () => {
    g.__serialLog.length = 0;
    const specifier = join(dir, "a.ts") + "?v=2";
    const mods = await Promise.all([1, 2, 3, 4].map(() => serialImport(specifier)));
    expect(new Set(mods).size).toBe(1);
    expect(mods[0].name).toBe("a");
    expect(g.__serialLog).toEqual(["start a", "end a"]);
    // Already loaded: the same promise, no waiting behind other imports
    expect(serialImport(specifier)).toBe(serialImport(specifier));
  });

  it("tries a failed import again", async () => {
    const missing = join(dir, "later.ts");
    await expect(serialImport(missing)).rejects.toThrow();
    writeFileSync(missing, "export const ok = true;\n");
    // Bun caches the failed resolution of a specifier; a new version is a new specifier
    expect((await serialImport(missing + "?v=2")).ok).toBe(true);
  });

  it("forgets an older version once a newer one is imported", async () => {
    const v1 = join(dir, "b.ts") + "?v=5";
    const first = serialImport(v1);
    await first;
    expect(serialImport(v1)).toBe(first);
    await serialImport(join(dir, "b.ts") + "?v=6");
    // Not the cached promise any more (a new import() of the same specifier, served by Bun's registry)
    expect(serialImport(v1)).not.toBe(first);
  });
});
