/**
 * Extracts an npm package tarball (`.tgz`): ustar entries, pax and GNU long names. The first path
 * segment (`package/` for npm) is dropped. Only regular files and directories are written; links and
 * anything that would land outside `dest` are skipped.
 */

import { chmodSync, mkdirSync, writeFileSync } from "fs";
import { dirname, join, normalize, sep } from "path";

const decoder = new TextDecoder();

function field(block: Uint8Array, offset: number, length: number): string {
  const bytes = block.subarray(offset, offset + length);
  const end = bytes.indexOf(0);
  return decoder.decode(end === -1 ? bytes : bytes.subarray(0, end));
}

function octal(block: Uint8Array, offset: number, length: number): number {
  const text = field(block, offset, length).trim();
  return text ? parseInt(text, 8) : 0;
}

/** `path` of a pax extended header (`<len> path=<value>\n` records) */
function paxPath(data: Uint8Array): string | undefined {
  const text = decoder.decode(data);
  let i = 0;
  while (i < text.length) {
    const space = text.indexOf(" ", i);
    if (space === -1) break;
    const length = parseInt(text.slice(i, space), 10);
    if (!length) break;
    const record = text.slice(space + 1, i + length - 1);
    const eq = record.indexOf("=");
    if (record.slice(0, eq) === "path") return record.slice(eq + 1);
    i += length;
  }
  return undefined;
}

/** Returns the number of files written */
export function extractTgz(tgz: Uint8Array, dest: string): number {
  const tar = Bun.gunzipSync(tgz as Uint8Array<ArrayBuffer>);
  const root = normalize(dest);
  let files = 0;
  let longName: string | undefined;
  let offset = 0;
  while (offset + 512 <= tar.length) {
    const header = tar.subarray(offset, offset + 512);
    if (header.every((b) => b === 0)) break;
    const size = octal(header, 124, 12);
    const type = String.fromCharCode(header[156] || 48);
    const data = tar.subarray(offset + 512, offset + 512 + size);
    offset += 512 + Math.ceil(size / 512) * 512;

    if (type === "x") {
      longName = paxPath(data) ?? longName;
      continue;
    }
    if (type === "L") {
      longName = decoder.decode(data).replace(/\0+$/, "");
      continue;
    }
    if (type === "g") continue;

    const prefix = field(header, 345, 155);
    const name = longName ?? (prefix ? `${prefix}/${field(header, 0, 100)}` : field(header, 0, 100));
    longName = undefined;
    // `package/lib/x.js` → `lib/x.js`
    const relative = name.split("/").slice(1).join("/");
    if (!relative) continue;
    const target = normalize(join(root, relative));
    if (target !== root && !target.startsWith(root + sep)) continue;

    if (type === "5") {
      mkdirSync(target, { recursive: true });
    } else if (type === "0" || type === "7") {
      mkdirSync(dirname(target), { recursive: true });
      writeFileSync(target, data);
      // Keep the executable bit (CLIs, native binaries); everything readable
      const mode = octal(header, 100, 8);
      if (mode & 0o111) chmodSync(target, 0o755);
      files++;
    }
  }
  return files;
}
