import { describe, expect, it } from "bun:test";
import { parseJsonc, parseLockfile, pathSegments, platformMatches, resolveGraph, splitNameVersion } from "./lockfile";

/** A bun.lock (trailing commas included) from `packages` entries */
function lockfile(dependencies: Record<string, string>, packages: Record<string, unknown[]>): string {
  const body = Object.entries(packages)
    .map(([k, v]) => `    ${JSON.stringify(k)}: ${JSON.stringify(v)},`)
    .join("\n");
  return `{\n  "lockfileVersion": 1,\n  "workspaces": {\n    "": {\n      "dependencies": ${JSON.stringify(dependencies)},\n    },\n  },\n  "packages": {\n${body}\n  }\n}\n`;
}

const npm = (spec: string, meta: Record<string, unknown> = {}) => [spec, "", meta, "sha512-x"];

describe("store lockfile", () => {
  it("parses JSON with trailing commas, keeping commas inside strings", () => {
    expect(parseJsonc(`{"a": [1, 2,], "b": "x,}", }`)).toEqual({ a: [1, 2], b: "x,}" });
    expect(parseJsonc(`{"a": "quote \\" ,]"}`)).toEqual({ a: 'quote " ,]' });
  });

  it("splits scoped names and paths", () => {
    expect(splitNameVersion("@scope/name@1.2.3")).toEqual(["@scope/name", "1.2.3"]);
    expect(splitNameVersion("react@19.0.0")).toEqual(["react", "19.0.0"]);
    expect(pathSegments("next/@swc/helpers/tslib")).toEqual(["next", "@swc/helpers", "tslib"]);
  });

  it("matches os and cpu fields, with exclusions", () => {
    expect(platformMatches(undefined, "linux")).toBe(true);
    expect(platformMatches("darwin", "darwin")).toBe(true);
    expect(platformMatches(["darwin", "linux"], "win32")).toBe(false);
    expect(platformMatches(["!win32"], "linux")).toBe(true);
    expect(platformMatches(["!win32"], "win32")).toBe(false);
  });

  it("keeps registry packages and reports the others", () => {
    const lock = parseLockfile(
      lockfile({ a: "^1.0.0", g: "github:x/g" }, { a: npm("a@1.0.0"), g: ["g@github:x/g#abc", {}, "x-g-abc"] }),
    );
    expect([...lock.packages.keys()]).toEqual(["a"]);
    expect(lock.unsupported).toEqual(["g@github:x/g#abc"]);
  });

  it("resolves nested copies first, then up the hoisted tree", () => {
    const lock = parseLockfile(
      lockfile(
        { app: "1.0.0", b: "1.0.0" },
        {
          app: npm("app@1.0.0", { dependencies: { b: "^2.0.0", c: "1.0.0" } }),
          "app/b": npm("b@2.0.0"),
          b: npm("b@1.0.0"),
          c: npm("c@1.0.0", { dependencies: { b: "^1.0.0" } }),
        },
      ),
    );
    const graph = resolveGraph(lock, { os: "linux", cpu: "x64" });
    expect(graph.edges.get("app")!.get("b")).toBe("app/b");
    expect(graph.edges.get("c")!.get("b")).toBe("b");
    expect(graph.topLevel.sort()).toEqual(["app", "b", "c"]);
    // `b@1` and `b@2` are different entries; `c` depends on `b@1`
    expect(graph.entries.get("app/b")).not.toBe(graph.entries.get("b"));
  });

  it("skips optional packages of other platforms", () => {
    const lock = parseLockfile(
      lockfile(
        { tool: "1.0.0" },
        {
          tool: npm("tool@1.0.0", { optionalDependencies: { "tool-darwin": "1.0.0", "tool-linux": "1.0.0" } }),
          "tool-darwin": npm("tool-darwin@1.0.0", { os: "darwin", cpu: "arm64" }),
          "tool-linux": npm("tool-linux@1.0.0", { os: "linux", cpu: "x64" }),
        },
      ),
    );
    const linux = resolveGraph(lock, { os: "linux", cpu: "x64" });
    expect([...linux.nodes.keys()].sort()).toEqual(["tool", "tool-linux"]);
    expect([...linux.edges.get("tool")!.keys()]).toEqual(["tool-linux"]);
  });

  it("links peer dependencies, and the entry changes with the peer's version", () => {
    const make = (react: string) =>
      resolveGraph(
        parseLockfile(
          lockfile(
            { ui: "1.0.0", react },
            { ui: npm("ui@1.0.0", { peerDependencies: { react: "*" } }), react: npm(`react@${react}`) },
          ),
        ),
        { os: "linux", cpu: "x64" },
      );
    const a = make("18.3.1");
    const b = make("19.0.0");
    expect(a.edges.get("ui")!.get("react")).toBe("react");
    expect(a.entries.get("ui")).not.toBe(b.entries.get("ui"));
    // Same graph in another project: same entry, so it is shared
    expect(make("18.3.1").entries.get("ui")).toBe(a.entries.get("ui"));
    expect(a.entries.get("ui")).toMatch(/^ui@1\.0\.0_[0-9a-f]{16}$/);
  });

  it("handles dependency cycles", () => {
    const graph = resolveGraph(
      parseLockfile(lockfile({ a: "1.0.0" }, { a: npm("a@1.0.0", { dependencies: { b: "1.0.0" } }), b: npm("b@1.0.0", { dependencies: { a: "1.0.0" } }) })),
      { os: "linux", cpu: "x64" },
    );
    expect(graph.edges.get("b")!.get("a")).toBe("a");
    expect(graph.entries.size).toBe(2);
  });
});
