import { describe, expect, it } from "bun:test";
import { mkdtempSync, rmSync, mkdirSync, writeFileSync, existsSync } from "fs";
import { join } from "path";
import { tmpdir } from "os";
import { collectProjectFiles, computeDeleted, isBinaryFile } from "../src/commands/save";
import { NataApiClient, SaveConflictError } from "../src/core/api";
import { saveLocalViewConfig, getLocalViewConfig } from "../src/core/config";

describe("Save Command - collectProjectFiles", () => {
  it("should collect project files and ignore system, build, and secret files", () => {
    const tempDir = mkdtempSync(join(tmpdir(), "com-save-test-"));

    try {
      // 1. Valid files
      mkdirSync(join(tempDir, "app"), { recursive: true });
      writeFileSync(join(tempDir, "app", "page.tsx"), "export default function Page() { return <h1>Home</h1>; }");
      writeFileSync(join(tempDir, "package.json"), '{"name": "test-app"}');
      writeFileSync(join(tempDir, "README.md"), "# Test App");

      // 2. Ignored files & directories
      mkdirSync(join(tempDir, "node_modules", "react"), { recursive: true });
      writeFileSync(join(tempDir, "node_modules", "react", "index.js"), "module.exports = {};");

      mkdirSync(join(tempDir, ".git"), { recursive: true });
      writeFileSync(join(tempDir, ".git", "config"), "[core]");

      mkdirSync(join(tempDir, ".nata"), { recursive: true });
      writeFileSync(join(tempDir, ".nata", "view.json"), '{"viewId": 100}');

      mkdirSync(join(tempDir, "data"), { recursive: true });
      writeFileSync(join(tempDir, "data", "app.db"), "binary database content");

      writeFileSync(join(tempDir, ".env"), 'DATABASE_URL="postgres://..."');
      writeFileSync(join(tempDir, ".env.local"), 'SECRET="xyz"');
      writeFileSync(join(tempDir, ".DS_Store"), "ds-store-binary");

      const files = collectProjectFiles(tempDir);
      const paths = files.map((f) => f.path);

      expect(paths).toContain("app/page.tsx");
      expect(paths).toContain("package.json");
      expect(paths).toContain("README.md");

      expect(paths.some((p) => p.includes("node_modules"))).toBe(false);
      expect(paths.some((p) => p.includes(".git"))).toBe(false);
      expect(paths.some((p) => p.includes(".nata"))).toBe(false);
      expect(paths.some((p) => p.includes(".env"))).toBe(false);
      expect(paths.some((p) => p.endsWith(".db"))).toBe(false);
      expect(paths.some((p) => p.includes(".DS_Store"))).toBe(false);
    } finally {
      rmSync(tempDir, { recursive: true, force: true });
    }
  });
});

describe("API Client - saveViewRawFiles", () => {
  it("should send formatted files payload to server", async () => {
    let capturedMethod = "";
    let capturedBody: any = null;

    const mockServer = Bun.serve({
      port: 0,
      fetch(req) {
        capturedMethod = req.method;
        return req.json().then((body) => {
          capturedBody = body;
          return Response.json({ success: true, id: 999, files_count: body.files?.length });
        });
      },
    });

    try {
      const client = new NataApiClient(`http://localhost:${mockServer.port}`, "test-token");
      const files = [
        { path: "app/page.tsx", content: "export default () => <div>Hello</div>;" },
        { path: "modules/news/index.ts", content: "export const list = [];" },
      ];

      const res = await client.saveViewRawFiles(999, files, { name: "Test View" });

      expect(capturedMethod).toBe("POST");
      expect(capturedBody).toBeDefined();
      expect(capturedBody.files.length).toBe(2);
      expect(capturedBody.files[0].path).toBe("app/page.tsx");
      expect(capturedBody.name).toBe("Test View");
      expect(res.success).toBe(true);
      expect(res.id).toBe(999);
      expect(res.files_count).toBe(2);
    } finally {
      mockServer.stop();
    }
  });

  it("should trigger publish endpoint with viewId", async () => {
    let capturedMethod = "";
    let capturedBody: any = null;

    const mockServer = Bun.serve({
      port: 0,
      fetch(req) {
        capturedMethod = req.method;
        return req.json().then((body) => {
          capturedBody = body;
          return Response.json({ success: true, viewId: body.viewId || body.view_id });
        });
      },
    });

    try {
      const client = new NataApiClient(`http://localhost:${mockServer.port}`, "test-token");
      const res = await client.publishView(888, { domain: "demo.com.ai.vn" });

      expect(capturedMethod).toBe("POST");
      expect(capturedBody).toBeDefined();
      expect(capturedBody.viewId).toBe(888);
      expect(capturedBody.domain).toBe("demo.com.ai.vn");
      expect(res.success).toBe(true);
      expect(res.viewId).toBe(888);
    } finally {
      mockServer.stop();
    }
  });
});

describe("Local Config - saveLocalViewConfig & getLocalViewConfig", () => {
  it("should persist and read local view config correctly", () => {
    const tempDir = mkdtempSync(join(tmpdir(), "com-cfg-test-"));
    try {
      saveLocalViewConfig(tempDir, {
        viewId: 1054,
        apiUrl: "https://base.myworkbeast.com",
        name: "Demo Project",
        clonedAt: new Date().toISOString(),
        files: [],
      });

      const config = getLocalViewConfig(tempDir);
      expect(config).toBeDefined();
      expect(config?.viewId).toBe(1054);
      expect(config?.name).toBe("Demo Project");
    } finally {
      rmSync(tempDir, { recursive: true, force: true });
    }
  });
});

describe("Save Command - binary files and deletions", () => {
  it("skips binary and oversized files instead of uploading them as corrupted text", () => {
    const tempDir = mkdtempSync(join(tmpdir(), "com-save-bin-"));
    try {
      mkdirSync(join(tempDir, "public"), { recursive: true });
      writeFileSync(join(tempDir, "public", "logo.png"), Buffer.from([0x89, 0x50, 0x4e, 0x47]));
      writeFileSync(join(tempDir, "blob.dat"), Buffer.from([0x41, 0x00, 0x42]));
      writeFileSync(join(tempDir, "big.txt"), "x".repeat(2 * 1024 * 1024 + 1));
      writeFileSync(join(tempDir, "page.tsx"), "export default () => null;");

      const skipped: string[] = [];
      const paths = collectProjectFiles(tempDir, tempDir, skipped).map((f) => f.path);

      expect(paths).toEqual(["page.tsx"]);
      expect(skipped).toEqual(["big.txt", "blob.dat", "public/logo.png"]);
    } finally {
      rmSync(tempDir, { recursive: true, force: true });
    }
  });

  it("detects binary content by extension or NUL byte", () => {
    expect(isBinaryFile("a/Font.WOFF2", Buffer.from("text"))).toBe(true);
    expect(isBinaryFile("data.bin2", Buffer.from([1, 0, 2]))).toBe(true);
    expect(isBinaryFile("app/page.tsx", Buffer.from("const a = 1;"))).toBe(false);
  });

  it("only deletes tracked files that are gone from disk", () => {
    const tempDir = mkdtempSync(join(tmpdir(), "com-save-del-"));
    try {
      writeFileSync(join(tempDir, "kept.tsx"), "1");
      writeFileSync(join(tempDir, "logo.png"), Buffer.from([0x89, 0x50]));
      const tracked = [{ path: "kept.tsx" }, { path: "logo.png" }, { path: "removed.tsx" }];

      expect(computeDeleted(tempDir, tracked, new Set(["kept.tsx"]))).toEqual(["removed.tsx"]);
      expect(computeDeleted(tempDir, undefined, new Set())).toEqual([]);
    } finally {
      rmSync(tempDir, { recursive: true, force: true });
    }
  });
});

describe("API Client - merge saves and conflicts", () => {
  it("sends merge fields and turns 409 into SaveConflictError", async () => {
    let capturedBody: any = null;
    let conflict = false;
    const mockServer = Bun.serve({
      port: 0,
      async fetch(req) {
        capturedBody = await req.json();
        if (conflict) return Response.json({ error: "stale", status: 409, updated_at: "t9" }, { status: 409 });
        return Response.json({ success: true, id: 7, mode: "merge", updated_at: "t2", realtime: "reset" });
      },
    });

    try {
      const client = new NataApiClient(`http://localhost:${mockServer.port}`, "test-token");
      const files = [{ path: "a.tsx", content: "A" }];
      const res = await client.saveViewRawFiles(7, files, {
        mode: "merge",
        deleted: ["b.tsx"],
        baseUpdatedAt: "t1",
      });

      expect(capturedBody.mode).toBe("merge");
      expect(capturedBody.deleted).toEqual(["b.tsx"]);
      expect(capturedBody.base_updated_at).toBe("t1");
      expect(res.mode).toBe("merge");
      expect(res.updated_at).toBe("t2");

      conflict = true;
      const err = await client.saveViewRawFiles(7, files, { baseUpdatedAt: "t1" }).catch((e) => e);
      expect(err).toBeInstanceOf(SaveConflictError);
      expect(err.updatedAt).toBe("t9");
    } finally {
      mockServer.stop();
    }
  });
});
