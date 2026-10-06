import { describe, expect, it } from "bun:test";
import { setupProjectEnvironment } from "../src/package-manager";
import { mkdtempSync, rmSync, existsSync, readFileSync } from "fs";
import { join } from "path";
import { tmpdir } from "os";

describe("Package Manager Facade - setupProjectEnvironment", () => {
  it("should parse files, generate manifests and return detected dependencies while filtering out @native libraries", async () => {
    const tempDir = mkdtempSync(join(tmpdir(), "com-env-test-"));

    try {
      const files = [
        {
          path: "modules/news/use-cases/ListArticlesUseCase.ts",
          content: `
            import axios from "axios";
            import { format } from "date-fns";
            import { NativeStorage } from "@native/storage";
            import { NativeCamera } from "@native/camera";
            export class ListArticlesUseCase {}
          `,
        },
        {
          path: "app/page.tsx",
          content: `
            import React from "react";
            import { motion } from "framer-motion";
            import { Bell } from "lucide-react";
            import "@native/ui";
            export default function Page() { return <div>News</div>; }
          `,
        },
      ];

      const { isPackageCreated, detectedPackages } = await setupProjectEnvironment(
        tempDir,
        "news-view-1023",
        files
      );

      expect(isPackageCreated).toBe(true);
      expect(detectedPackages).toContain("axios");
      expect(detectedPackages).toContain("date-fns");
      expect(detectedPackages).toContain("framer-motion");
      expect(detectedPackages).toContain("lucide-react");
      expect(detectedPackages).not.toContain("@native/storage");
      expect(detectedPackages).not.toContain("@native/camera");
      expect(detectedPackages).not.toContain("@native/ui");
      expect(detectedPackages).not.toContain("@native");

      const pkgJson = JSON.parse(readFileSync(join(tempDir, "package.json"), "utf-8"));
      expect(pkgJson.name).toBe("news-view-1023");
      expect(pkgJson.dependencies["axios"]).toBeDefined();
      expect(pkgJson.dependencies["date-fns"]).toBeDefined();
      expect(pkgJson.dependencies["framer-motion"]).toBeDefined();
      expect(pkgJson.dependencies["react"]).toBeDefined();
      expect(pkgJson.dependencies["@native/storage"]).toBeUndefined();
      expect(pkgJson.dependencies["@native/camera"]).toBeUndefined();
      expect(pkgJson.dependencies["@native/ui"]).toBeUndefined();
      expect(pkgJson.dependencies["@native"]).toBeUndefined();
    } finally {
      rmSync(tempDir, { recursive: true, force: true });
    }
  });
});
