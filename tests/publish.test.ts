import { describe, expect, it } from "bun:test";
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
