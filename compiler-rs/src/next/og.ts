// next/og: ImageResponse renders JSX to PNG (Satori → SVG in the worker, resvg in the engine).

export class ImageResponse extends Response {
  constructor(element: unknown, options: { width?: number; height?: number; fonts?: unknown[]; headers?: HeadersInit; status?: number; emoji?: string } = {}) {
    const width = options.width ?? 1200;
    const height = options.height ?? 630;
    const body = new ReadableStream<Uint8Array>({
      async start(controller) {
        try {
          const png = await renderImage(element, { ...options, width, height });
          controller.enqueue(png);
          controller.close();
        } catch (error) {
          controller.error(error);
        }
      },
    });
    const headers = new Headers(options.headers);
    if (!headers.has("content-type")) headers.set("content-type", "image/png");
    if (!headers.has("cache-control")) headers.set("cache-control", "public, immutable, no-transform, max-age=31536000");
    super(body, { status: options.status ?? 200, headers });
  }
}

let satoriPromise: Promise<any> | null = null;

async function renderImage(element: unknown, options: { width: number; height: number; fonts?: unknown[] }) {
  satoriPromise ??= import("satori").then((m) => m.default ?? m);
  const satori = await satoriPromise;
  const fonts = options.fonts && options.fonts.length ? options.fonts : await defaultFonts();
  const svg: string = await satori(element, { width: options.width, height: options.height, fonts });
  const call = (globalThis as any).__NATA_CALL_ENGINE__;
  const { meta, body } = await call("svg_to_png", { svg });
  if (meta.error) throw new Error(meta.error);
  return body as Uint8Array;
}

let fontsPromise: Promise<unknown[]> | null = null;

/** A sans-serif font for images without `fonts`: the engine provides one (system or Google Fonts) */
async function defaultFonts() {
  fontsPromise ??= (async () => {
    const call = (globalThis as any).__NATA_CALL_ENGINE__;
    const { meta, body } = await call("og_default_font", {});
    if (meta.error) throw new Error(meta.error);
    return [{ name: "sans-serif", data: body.buffer.slice(body.byteOffset, body.byteOffset + body.byteLength), weight: 400, style: "normal" }];
  })();
  return fontsPromise;
}
