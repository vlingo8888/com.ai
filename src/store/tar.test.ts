import { afterEach, describe, expect, it } from "bun:test";
import { existsSync, mkdtempSync, readFileSync, rmSync, statSync } from "fs";
import { tmpdir } from "os";
import { join } from "path";
import { extractTgz } from "./tar";

/** A ustar block for `name` (`type` 0: file, 5: directory, x: pax header) */
function header(name: string, size: number, type: string, mode = 0o644): Uint8Array {
  const block = new Uint8Array(512);
  const put = (text: string, offset: number) => block.set(new TextEncoder().encode(text), offset);
  put(name.slice(0, 100), 0);
  put(mode.toString(8).padStart(7, "0"), 100);
  put(size.toString(8).padStart(11, "0"), 124);
  put(type, 156);
  put("ustar", 257);
  return block;
}

function tgz(entries: Array<{ name: string; data?: string; type?: string; mode?: number }>): Uint8Array {
  const parts: Uint8Array[] = [];
  for (const e of entries) {
    const data = new TextEncoder().encode(e.data ?? "");
    parts.push(header(e.name, data.length, e.type ?? "0", e.mode));
    const padded = new Uint8Array(Math.ceil(data.length / 512) * 512);
    padded.set(data);
    parts.push(padded);
  }
  parts.push(new Uint8Array(1024));
  const tar = new Uint8Array(parts.reduce((n, p) => n + p.length, 0));
  let offset = 0;
  for (const p of parts) {
    tar.set(p, offset);
    offset += p.length;
  }
  return Bun.gzipSync(tar);
}

/** A pax record `<len> path=<value>\n` */
function pax(path: string): string {
  const body = ` path=${path}\n`;
  let length = body.length + 2;
  while (`${length}${body}`.length !== length) length++;
  return `${length}${body}`;
}

describe("store tar extraction", () => {
  let dir: string;
  afterEach(() => rmSync(dir, { recursive: true, force: true }));

  it("drops the package/ prefix and keeps executables executable", () => {
    dir = mkdtempSync(join(tmpdir(), "com-tar-"));
    const count = extractTgz(
      tgz([
        { name: "package/", type: "5" },
        { name: "package/package.json", data: '{"name":"x"}' },
        { name: "package/bin/cli.js", data: "#!/usr/bin/env node", mode: 0o755 },
      ]),
      dir,
    );
    expect(count).toBe(2);
    expect(readFileSync(join(dir, "package.json"), "utf-8")).toBe('{"name":"x"}');
    if (process.platform !== "win32") expect(statSync(join(dir, "bin/cli.js")).mode & 0o111).toBeTruthy();
  });

  it("reads long names from pax headers", () => {
    dir = mkdtempSync(join(tmpdir(), "com-tar-"));
    const long = `package/${"deep/".repeat(30)}file.js`;
    extractTgz(tgz([{ name: "PaxHeader", type: "x", data: pax(long) }, { name: "package/short", data: "long one" }]), dir);
    expect(readFileSync(join(dir, long.slice("package/".length)), "utf-8")).toBe("long one");
  });

  it("never writes outside the destination", () => {
    dir = mkdtempSync(join(tmpdir(), "com-tar-"));
    extractTgz(tgz([{ name: "package/../escape.js", data: "x" }, { name: "package/ok.js", data: "y" }]), join(dir, "out"));
    expect(existsSync(join(dir, "escape.js"))).toBe(false);
    expect(existsSync(join(dir, "out/ok.js"))).toBe(true);
  });
});
