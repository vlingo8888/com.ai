import { describe, expect, it } from "bun:test";
import {
  normalizePackageName,
  extractImportsFromCode,
  scanFilesForDependencies,
} from "../src/package-manager/scanner";

describe("Dependency Scanner - normalizePackageName", () => {
  it("should extract simple npm package names", () => {
    expect(normalizePackageName("react")).toBe("react");
    expect(normalizePackageName("lodash")).toBe("lodash");
    expect(normalizePackageName("axios")).toBe("axios");
    expect(normalizePackageName("clsx")).toBe("clsx");
  });

  it("should extract scoped npm package names", () => {
    expect(normalizePackageName("@radix-ui/react-dialog")).toBe("@radix-ui/react-dialog");
    expect(normalizePackageName("@types/react")).toBe("@types/react");
    expect(normalizePackageName("@tanstack/react-query")).toBe("@tanstack/react-query");
    expect(normalizePackageName("@com.ai.vn/code")).toBe("@com.ai.vn/code");
  });

  it("should normalize subpath imports to root package name", () => {
    expect(normalizePackageName("lodash/debounce")).toBe("lodash");
    expect(normalizePackageName("date-fns/format")).toBe("date-fns");
    expect(normalizePackageName("@radix-ui/react-dialog/dist/index.js")).toBe("@radix-ui/react-dialog");
  });

  it("should ignore relative imports", () => {
    expect(normalizePackageName("./components/Button")).toBeNull();
    expect(normalizePackageName("../utils/format")).toBeNull();
    expect(normalizePackageName("./App")).toBeNull();
    expect(normalizePackageName("/absolute/path")).toBeNull();
  });

  it("should ignore path aliases", () => {
    expect(normalizePackageName("@/components/Header")).toBeNull();
    expect(normalizePackageName("~/lib/utils")).toBeNull();
    expect(normalizePackageName("#internal/config")).toBeNull();
    expect(normalizePackageName("src/modules/news")).toBeNull();
  });

  it("should ignore Node and Bun built-in modules", () => {
    expect(normalizePackageName("fs")).toBeNull();
    expect(normalizePackageName("fs/promises")).toBeNull();
    expect(normalizePackageName("path")).toBeNull();
    expect(normalizePackageName("crypto")).toBeNull();
    expect(normalizePackageName("os")).toBeNull();
    expect(normalizePackageName("node:http")).toBeNull();
    expect(normalizePackageName("bun")).toBeNull();
    expect(normalizePackageName("bun:test")).toBeNull();
    expect(normalizePackageName("bun:sqlite")).toBeNull();
  });

  it("should handle empty or invalid inputs gracefully", () => {
    expect(normalizePackageName("")).toBeNull();
    expect(normalizePackageName("   ")).toBeNull();
    expect(normalizePackageName(null as any)).toBeNull();
  });
});

describe("Dependency Scanner - extractImportsFromCode", () => {
  it("should extract standard ES module imports", () => {
    const code = `
      import React, { useState, useEffect } from 'react';
      import { motion } from "framer-motion";
      import * as Lucide from 'lucide-react';
      import styles from './styles.css';
    `;
    const imports = extractImportsFromCode(code);
    expect(imports).toContain("react");
    expect(imports).toContain("framer-motion");
    expect(imports).toContain("lucide-react");
    expect(imports).toContain("./styles.css");
  });

  it("should extract side-effect imports", () => {
    const code = `
      import "tailwindcss/tailwind.css";
      import "./global.css";
    `;
    const imports = extractImportsFromCode(code);
    expect(imports).toContain("tailwindcss/tailwind.css");
    expect(imports).toContain("./global.css");
  });

  it("should extract CommonJS require calls", () => {
    const code = `
      const express = require('express');
      const { parse } = require("csv-parse");
    `;
    const imports = extractImportsFromCode(code);
    expect(imports).toContain("express");
    expect(imports).toContain("csv-parse");
  });

  it("should extract dynamic imports", () => {
    const code = `
      const module = await import('heavy-math-lib');
      const dynamicComponent = import('./LazyComponent');
    `;
    const imports = extractImportsFromCode(code);
    expect(imports).toContain("heavy-math-lib");
    expect(imports).toContain("./LazyComponent");
  });

  it("should extract re-export statements", () => {
    const code = `
      export { Button } from 'custom-ui-lib';
      export * from './local';
    `;
    const imports = extractImportsFromCode(code);
    expect(imports).toContain("custom-ui-lib");
    expect(imports).toContain("./local");
  });
});

describe("Dependency Scanner - scanFilesForDependencies", () => {
  it("should scan multi-file project and aggregate dependencies", () => {
    const files = [
      {
        path: "app/page.tsx",
        content: `
          import React from "react";
          import { create } from "zustand";
          import { ArrowRight } from "lucide-react";
          import Header from "@/components/Header";
        `,
      },
      {
        path: "modules/news/use-cases/ListArticles.ts",
        content: `
          import axios from "axios";
          import { format } from "date-fns";
          import { fs } from "fs";
        `,
      },
      {
        path: "components/Chart.tsx",
        content: `
          import { ResponsiveContainer } from "recharts";
          import { debounce } from "lodash/debounce";
        `,
      },
    ];

    const result = scanFilesForDependencies(files);

    expect(result.detectedPackages).toContain("zustand");
    expect(result.detectedPackages).toContain("axios");
    expect(result.detectedPackages).toContain("date-fns");
    expect(result.detectedPackages).toContain("recharts");
    expect(result.detectedPackages).toContain("lodash");

    // Standard presets should always be included
    expect(result.dependencies["react"]).toBeDefined();
    expect(result.dependencies["react-dom"]).toBeDefined();
    expect(result.dependencies["zustand"]).toBe("latest");
    expect(result.dependencies["axios"]).toBe("latest");
    expect(result.dependencies["date-fns"]).toBe("latest");
    expect(result.dependencies["recharts"]).toBe("latest");
    expect(result.dependencies["lodash"]).toBe("latest");

    // Dev dependencies should include TypeScript & Bun types
    expect(result.devDependencies["typescript"]).toBeDefined();
    expect(result.devDependencies["@types/react"]).toBeDefined();
  });
});
