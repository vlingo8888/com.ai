// The part of esbuild's API the publisher's builder (builder.mjs) uses, on Rolldown: native Rust,
// tree-shaking, no wasm. The builder stays the one shared with the web and the COM app, so the CLI
// publishes the same app they do; only the engine under it changes.
//
// esbuild plugins (onResolve/onLoad with filters and namespaces) run inside one Rolldown plugin:
// a module of namespace `ns` and path `p` is the Rolldown id "\0ns:p".
import { createHash } from "crypto";
import { extname } from "path";

type Handler<A, R> = { filter: RegExp; namespace?: string; callback: (args: A) => R | undefined | Promise<R | undefined> };
type ResolveArgs = { path: string; importer: string; namespace: string; resolveDir: string; kind: string; pluginData?: unknown };
type ResolveResult = { path?: string; namespace?: string; external?: boolean; errors?: Array<{ text: string }>; pluginData?: unknown };
type LoadArgs = { path: string; namespace: string; pluginData?: unknown };
type LoadResult = { contents?: string | Uint8Array; loader?: string; resolveDir?: string; pluginData?: unknown };

export interface BuildError {
  text: string;
  location?: { file?: string; line?: number };
}

const decoder = new TextDecoder();
const toId = (namespace: string, path: string) => `\0${namespace}:${path}`;
function fromId(id: string): { namespace: string; path: string } {
  if (!id.startsWith("\0")) return { namespace: "file", path: id };
  const colon = id.indexOf(":");
  return { namespace: id.slice(1, colon), path: id.slice(colon + 1) };
}

/** esbuild loader → Rolldown module type */
const MODULE_TYPE: Record<string, string> = { js: "js", jsx: "jsx", ts: "ts", tsx: "tsx", json: "json", text: "text", empty: "empty" };

class BuildFailure extends Error {
  constructor(public errors: BuildError[]) {
    super(errors.map((e) => e.text).join("\n"));
  }
}

/**
 * Rolldown's "x is not exported by y" as esbuild words it: the builder resolves those imports
 * leniently and builds again.
 */
function translateError(error: any): BuildError[] {
  const list: any[] = error?.errors ?? [error];
  return list.map((e) => {
    const message = String(e?.message ?? e);
    const missing = /"([^"]+)" is not exported by "([^"]+)"/.exec(message);
    if (e?.code === "MISSING_EXPORT" || missing) {
      const name = e?.binding ?? missing?.[1];
      const file = fromId(String(e?.exporter ?? missing?.[2] ?? "")).path;
      return { text: `No matching export in "${file}" for import "${name}"` };
    }
    const id = e?.id ? fromId(e.id).path : e?.loc?.file;
    return { text: message.split("\n")[0], location: id ? { file: id, line: e?.loc?.line } : undefined };
  });
}

export async function createRolldownEsbuild(rolldownModule: string) {
  const { rolldown, VERSION } = await import(rolldownModule);

  async function build(options: any) {
    const resolvers: Handler<ResolveArgs, ResolveResult>[] = [];
    const loaders: Handler<LoadArgs, LoadResult>[] = [];
    for (const plugin of options.plugins ?? []) {
      plugin.setup({
        onResolve: (o: any, callback: any) => resolvers.push({ ...o, callback }),
        onLoad: (o: any, callback: any) => loaders.push({ ...o, callback }),
      });
    }
    const errors: BuildError[] = [];
    const pluginData = new Map<string, unknown>();
    const resolveDirs = new Map<string, string>();
    /** Files esbuild's "file" loader copies next to the chunks */
    const assets = new Map<string, Uint8Array>();
    const publicPath = String(options.publicPath ?? "").replace(/\/?$/, "/");
    const STDIN = toId("stdin", "<stdin>");

    async function resolve(args: ResolveArgs) {
      // esbuild: a handler with a namespace only sees imports from modules of that namespace
      for (const h of resolvers) {
        if ((h.namespace !== undefined && h.namespace !== args.namespace) || !h.filter.test(args.path)) continue;
        const result = await h.callback(args);
        if (result) return result;
      }
      return undefined;
    }

    const bridge = {
      name: "esbuild-plugins",
      async resolveId(source: string, importer: string | undefined, extra: any) {
        if (source === STDIN) return source;
        const from = importer ? fromId(importer) : { namespace: "file", path: "" };
        const fromStdin = importer === STDIN;
        const args: ResolveArgs = {
          path: source,
          importer: fromStdin ? "<stdin>" : from.path,
          // esbuild: plugins without a namespace filter see imports from any namespace
          namespace: fromStdin ? "file" : from.namespace,
          resolveDir: (importer && resolveDirs.get(importer)) ?? (fromStdin ? options.stdin?.resolveDir : undefined) ?? "/",
          kind: !importer ? "entry-point" : extra?.kind === "dynamic-import" ? "dynamic-import" : "import-statement",
          pluginData: importer ? pluginData.get(importer) : undefined,
        };
        const result = await resolve(args);
        if (result?.errors?.length) {
          errors.push(...result.errors.map((e) => ({ text: e.text, location: { file: args.importer } })));
          return { id: toId("error", `${errors.length}`) };
        }
        if (!result?.path) {
          errors.push({ text: `Could not resolve "${source}"`, location: { file: args.importer } });
          return { id: toId("error", `${errors.length}`) };
        }
        if (result.external) return { id: result.path, external: true };
        const id = toId(result.namespace ?? "file", result.path);
        if (result.pluginData !== undefined) pluginData.set(id, result.pluginData);
        return { id };
      },
      async load(id: string) {
        if (id === STDIN) return { code: options.stdin.contents, moduleType: MODULE_TYPE[options.stdin.loader] ?? "js" };
        const { namespace, path } = fromId(id);
        if (namespace === "error") return { code: "", moduleType: "js" };
        for (const h of loaders) {
          if ((h.namespace !== undefined && h.namespace !== namespace) || !h.filter.test(path)) continue;
          const result = await h.callback({ path, namespace, pluginData: pluginData.get(id) });
          if (!result) continue;
          if (result.resolveDir) resolveDirs.set(id, result.resolveDir);
          if (result.pluginData !== undefined) pluginData.set(id, result.pluginData);
          const contents = result.contents ?? "";
          if (result.loader === "file") {
            const bytes = typeof contents === "string" ? new TextEncoder().encode(contents) : contents;
            const ext = extname(path);
            const hash = createHash("sha256").update(bytes).digest("base64url").slice(0, 8).toUpperCase();
            const name = `${path.slice(path.lastIndexOf("/") + 1, path.length - ext.length)}-${hash}${ext}`;
            assets.set(name, bytes);
            return { code: `export default ${JSON.stringify(publicPath + name)};`, moduleType: "js" };
          }
          return {
            code: typeof contents === "string" ? contents : decoder.decode(contents),
            moduleType: MODULE_TYPE[result.loader ?? "js"] ?? "js",
          };
        }
        errors.push({ text: `No loader for "${path}" (${namespace})` });
        return { code: "", moduleType: "js" };
      },
    };

    const input = options.stdin ? [STDIN] : options.entryPoints;
    const name = (pattern: string | undefined) => `${(pattern ?? "[name]").replace("[hash]", "[hash:8]")}.js`;
    let output: any[];
    try {
      const bundle = await rolldown({
        input,
        platform: options.platform ?? "browser",
        plugins: [bridge],
        transform: {
          define: options.define,
          target: options.target,
          jsx: options.jsx === "automatic" ? "react-jsx" : undefined,
        },
        treeshake: true,
        logLevel: "silent",
      });
      try {
        ({ output } = await bundle.generate({
          format: "esm",
          entryFileNames: name(options.entryNames),
          chunkFileNames: name(options.chunkNames),
          minify: !!options.minify,
          keepNames: !!options.keepNames,
          comments: options.legalComments === "none" ? false : undefined,
        }));
      } finally {
        await bundle.close();
      }
    } catch (error) {
      throw new BuildFailure([...errors, ...translateError(error)]);
    }
    if (errors.length) throw new BuildFailure(errors);

    const outdir = String(options.outdir ?? "/out").replace(/^\//, "").replace(/\/$/, "");
    const outPath = (file: string) => `${outdir}/${file}`;
    const outputFiles: Array<{ path: string; contents: Uint8Array; text: string }> = [];
    const outputs: Record<string, any> = {};
    for (const chunk of output) {
      const bytes = chunk.type === "chunk" ? new TextEncoder().encode(chunk.code) : typeof chunk.source === "string" ? new TextEncoder().encode(chunk.source) : chunk.source;
      outputFiles.push({ path: `/${outPath(chunk.fileName)}`, contents: bytes, text: decoder.decode(bytes) });
      if (chunk.type !== "chunk") continue;
      const facade = chunk.facadeModuleId ? fromId(chunk.facadeModuleId) : null;
      outputs[outPath(chunk.fileName)] = {
        entryPoint: facade && (chunk.isEntry || chunk.isDynamicEntry) ? `${facade.namespace}:${facade.path}` : undefined,
        imports: [
          ...chunk.imports.map((p: string) => ({ path: outPath(p), kind: "import-statement" })),
          ...chunk.dynamicImports.map((p: string) => ({ path: outPath(p), kind: "dynamic-import" })),
        ],
      };
    }
    for (const [file, bytes] of assets) {
      outputFiles.push({ path: `/${outPath(file)}`, contents: bytes, text: "" });
      outputs[outPath(file)] = { imports: [] };
    }
    return { outputFiles, metafile: { outputs }, errors: [], warnings: [] };
  }

  return {
    version: `rolldown-${VERSION}`,
    build,
    /** The builder minifies CSS this way; tailwind-rs already returns it minified */
    async transform(code: string) {
      return { code };
    },
  };
}
