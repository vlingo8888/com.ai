/**
 * Replaces the CLI process with another program (`execve`), keeping the pid, stdio and exit code.
 *
 * A command that only hands over to the engine (`com build`) would otherwise keep a whole Bun runtime
 * (~50MB) alive just to wait for it. Returns only when the program could not be started this way
 * (Windows, no FFI): the caller then spawns it as a child instead.
 */
export function execReplace(file: string, args: string[], env: Record<string, string | undefined>): void {
  if (process.platform === "win32" || process.env.COM_NO_EXEC) return;
  let ffi: typeof import("bun:ffi");
  try {
    ffi = require("bun:ffi");
  } catch {
    return;
  }
  const libc = process.platform === "darwin" ? "libSystem.B.dylib" : "libc.so.6";
  let lib;
  try {
    lib = ffi.dlopen(libc, { execve: { args: ["ptr", "ptr", "ptr"], returns: "i32" } });
  } catch {
    return;
  }

  // NUL-terminated strings and NULL-terminated pointer arrays; the buffers stay referenced here until
  // execve copies them
  const keep: Buffer[] = [];
  const cString = (s: string) => {
    const buf = Buffer.from(s + "\0");
    keep.push(buf);
    return ffi.ptr(buf);
  };
  const cArray = (items: string[]) => {
    const pointers = new BigUint64Array(items.length + 1);
    items.forEach((item, i) => (pointers[i] = BigInt(cString(item))));
    return ffi.ptr(pointers);
  };
  const envList = Object.entries(env)
    .filter((e): e is [string, string] => e[1] !== undefined)
    .map(([k, v]) => `${k}=${v}`);
  const argv = cArray([file, ...args]);
  const envp = cArray(envList);
  // Returns only on failure (missing binary, no permission): fall back to spawning
  lib.symbols.execve(cString(file), argv, envp);
  lib.close();
}
