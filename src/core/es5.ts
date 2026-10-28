import { transform } from "sucrase";

const CODE_FILE = /\.(tsx?|jsx?|mjs|cjs)$/;

/**
 * The `es5` field the cloud runtime evaluates with `new Function`: CommonJS with classic
 * `React.createElement` JSX, like the web editor's compile. Without it the published app runs the
 * raw source and fails on the first `import`. Non-code files have no `es5`.
 */
export function compileEs5(path: string, content: string): string {
  if (!CODE_FILE.test(path) || !content) return "";
  return transform(content, {
    transforms: ["typescript", "jsx", "imports"],
    jsxRuntime: "classic",
    production: true,
    filePath: path,
  }).code;
}

/** Compiles every file; a file that does not compile gets an empty `es5` and is listed in `failed`. */
export function compileAllEs5<T extends { path: string; content: string }>(
  files: T[],
  failed: Array<{ path: string; message: string }> = []
): Array<T & { es5: string }> {
  return files.map((f) => {
    try {
      return { ...f, es5: compileEs5(f.path, f.content) };
    } catch (err: any) {
      failed.push({ path: f.path, message: err?.message || String(err) });
      return { ...f, es5: "" };
    }
  });
}
