import { describe, expect, it } from "bun:test";
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from "fs";
import { join } from "path";
import { tmpdir } from "os";
import { publishCommand, publishStatic } from "../src/commands/publish";
import { getLocalViewConfig, saveLocalViewConfig } from "../src/core/config";
import { NataApiClient } from "../src/core/api";

describe("Publish Command - static publish API", () => {
  it("prepares with GET and uploads with POST to /api/views/:id/publish-static/browser", async () => {
    const calls: Array<{ method: string; path: string; auth: string | null; body?: any }> = [];
    const server = Bun.serve({
      port: 0,
      async fetch(req) {
        const call = { method: req.method, path: new URL(req.url).pathname, auth: req.headers.get("authorization") };
        if (req.method === "GET") {
          calls.push(call);
          return Response.json({ unchanged: true });
        }
        calls.push({ ...call, body: await req.json() });
        return calls.length === 2 ? new Response("changed", { status: 409 }) : Response.json({ ok: true });
      },
    });

    try {
      const client = new NataApiClient(`http://localhost:${server.port}`, "test-token");
      expect((await client.prepareStaticPublish(42)).unchanged).toBe(true);
      expect(await client.uploadStaticPublish(42, { ok: true, files: [] })).toEqual({ status: "conflict" });
      expect((await client.uploadStaticPublish(42, { ok: true, files: [] })).status).toBe("ok");
      const path = "/api/views/42/publish-static/browser";
      expect(calls.map((c) => [c.method, c.path, c.auth])).toEqual([
        ["GET", path, "Bearer test-token"],
        ["POST", path, "Bearer test-token"],
        ["POST", path, "Bearer test-token"],
      ]);
      expect(calls[1].body).toEqual({ ok: true, files: [] });
    } finally {
      server.stop();
    }
  });

  it("reports a failed prepare instead of falling back to the old publish", async () => {
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
      await expect(publishStatic(client, 42)).rejects.toThrow("Failed to prepare the publish (404)");
      expect(paths).toEqual(["/api/views/42/publish-static/browser"]);
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
        if (pathname === "/api/views/publish/v2") throw new Error("the old publish must not be called");
        // The published app already has this code: nothing to build
        if (pathname === "/api/views/42/publish-static/browser") return Response.json({ unchanged: true });
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
