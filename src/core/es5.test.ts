import { describe, expect, it } from "bun:test";
import { compileAllEs5, compileEs5 } from "./es5";

describe("compileEs5", () => {
  it("turns TSX with imports into CommonJS the runtime can evaluate with new Function", () => {
    const es5 = compileEs5(
      "app/layout.tsx",
      'import type { ReactNode } from "react";\nimport "./globals.css";\nimport { Providers } from "@/components/providers";\nexport default function Layout({ children }: { children: ReactNode }) { return <Providers>{children}</Providers>; }'
    );
    expect(es5).not.toMatch(/^\s*import\s/m);
    expect(es5).toContain("require('@/components/providers')");
    expect(es5).toContain("require('./globals.css')");
    expect(es5).toContain("React.createElement");
    expect(() => new Function("require", "module", "exports", "React", es5)).not.toThrow();
  });

  it("leaves non-code files without es5", () => {
    expect(compileEs5("app/globals.css", "body { color: red; }")).toBe("");
    expect(compileEs5("README.md", "# Hi")).toBe("");
  });

  it("lists files that do not compile instead of throwing", () => {
    const failed: Array<{ path: string; message: string }> = [];
    const out = compileAllEs5(
      [
        { path: "ok.ts", content: "export const a: number = 1;" },
        { path: "bad.tsx", content: "export const x = <div>;" },
      ],
      failed
    );
    expect(out[0].es5).toContain("exports.a = a");
    expect(out[1].es5).toBe("");
    expect(failed.map((f) => f.path)).toEqual(["bad.tsx"]);
  });
});
