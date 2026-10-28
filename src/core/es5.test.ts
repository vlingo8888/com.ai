import { describe, expect, it } from "bun:test";
import { compileAllEs5 } from "./es5";

describe("compileAllEs5 (Rust engine SWC)", () => {
  it("turns TSX with imports into CommonJS the runtime can evaluate with new Function", () => {
    const [layout] = compileAllEs5([
      {
        path: "app/layout.tsx",
        content:
          'import type { ReactNode } from "react";\nimport "./globals.css";\nimport { Providers } from "@/components/providers";\nexport default function Layout({ children }: { children: ReactNode }) { return <Providers>{children}</Providers>; }',
      },
    ]);
    const es5 = layout.es5;
    expect(es5).not.toMatch(/^\s*import\s/m);
    expect(es5).toContain('require("@/components/providers")');
    expect(es5).toContain('require("./globals.css")');
    expect(es5).toContain("React.createElement");
    expect(() => new Function("require", "module", "exports", "React", es5)).not.toThrow();
  });

  it("inlines interop helpers instead of importing @swc/helpers", () => {
    const [file] = compileAllEs5([{ path: "a.ts", content: 'import x from "./x";\nexport const y = x + 1;' }]);
    expect(file.es5).toContain("function _interop_require_default");
    expect(file.es5).not.toContain("@swc/helpers");
  });

  it("leaves non-code files without es5 and lists files that do not compile", () => {
    const failed: Array<{ path: string; message: string }> = [];
    const out = compileAllEs5(
      [
        { path: "app/globals.css", content: "body { color: red; }" },
        { path: "ok.ts", content: "export const a: number = 1;" },
        { path: "bad.tsx", content: "export const x = <div>;" },
      ],
      failed
    );
    expect(out[0].es5).toBe("");
    expect(out[1].es5).toContain("a = 1");
    expect(out[2].es5).toBe("");
    expect(failed.map((f) => f.path)).toEqual(["bad.tsx"]);
  });
});
