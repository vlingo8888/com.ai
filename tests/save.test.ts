import { describe, expect, it } from "bun:test";
import { mkdtempSync, rmSync, mkdirSync, writeFileSync, existsSync } from "fs";
import { join } from "path";
import { tmpdir } from "os";
import { collectProjectFiles } from "../src/commands/save";
import { NataApiClient } from "../src/core/api";
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
