import { describe, expect, it } from "bun:test";
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from "fs";
import { join } from "path";
import { tmpdir } from "os";
import { publishCommand } from "../src/commands/publish";
import { getLocalViewConfig, saveLocalViewConfig } from "../src/core/config";
import { NataApiClient } from "../src/core/api";

describe("Publish Command - publishView (Publish V2)", () => {
  it("posts { view_id } to /api/views/publish/v2", async () => {
    const calls: Array<{ path: string; body: any }> = [];
    const server = Bun.serve({
      port: 0,
      async fetch(req) {
        calls.push({ path: new URL(req.url).pathname, body: await req.json() });
        return Response.json({ success: true, viewId: 42 });
      },
    });

    try {
      const client = new NataApiClient(`http://localhost:${server.port}`, "test-token");
      const res = await client.publishView(42);

      expect(res.success).toBe(true);
      expect(calls).toEqual([{ path: "/api/views/publish/v2", body: { view_id: 42 } }]);
    } finally {
      server.stop();
    }
  });

  it("reports a failed V2 publish instead of falling back to the V1 endpoints", async () => {
    const paths: string[] = [];
    const server = Bun.serve({
      port: 0,
      fetch(req) {
        paths.push(new URL(req.url).pathname);
        return new Response("Not Found", { status: 404 });
      },
    });

    try {
      const client = new NataApiClient(`http://localhost:${server.port}`, "test-token");
      await expect(client.publishView(42)).rejects.toThrow("Failed to publish view (404)");
      expect(paths).toEqual(["/api/views/publish/v2"]);
    } finally {
      server.stop();
    }
  });
});

describe("Publish Command - publishCommand", () => {
  it("saves with es5 and records the version publishing leaves, so the next save does not conflict", async () => {
    const dir = mkdtempSync(join(tmpdir(), "com-publish-test-"));
    const saves: any[] = [];
    const server = Bun.serve({
      port: 0,
      async fetch(req) {
        const { pathname } = new URL(req.url);
        if (pathname === "/api/views/42/raw_files") {
          saves.push(await req.json());
          return Response.json({ success: true, id: 42, mode: "merge", updated_at: "2026-10-28T05:51:51Z" });
        }
        if (pathname === "/api/views/publish/v2") return Response.json({ success: true, viewId: 42 });
        if (pathname === "/api/views/42") return Response.json({ id: 42, updated_at: "2026-10-28T05:51:53Z" });
        return new Response("not found", { status: 404 });
      },
    });
    try {
      mkdirSync(join(dir, "app"));
      writeFileSync(join(dir, "app", "page.tsx"), 'import { A } from "./a";\nexport default function Page() { return <A />; }');
      saveLocalViewConfig(dir, {
        viewId: 42,
        apiUrl: `http://localhost:${server.port}`,
        clonedAt: "2026-10-28T00:00:00Z",
        remoteUpdatedAt: "2026-10-28T05:00:00Z",
        files: [],
      });

      await publishCommand({ dir, token: "test-token" });

      expect(saves).toHaveLength(1);
      expect(saves[0].mode).toBe("merge");
      expect(saves[0].base_updated_at).toBe("2026-10-28T05:00:00Z");
      const page = saves[0].files.find((f: any) => f.path === "app/page.tsx");
      expect(page.es5).toContain('require("./a")');
      expect(getLocalViewConfig(dir)?.remoteUpdatedAt).toBe("2026-10-28T05:51:53Z");
    } finally {
      server.stop();
      rmSync(dir, { recursive: true, force: true });
    }
  });
});
