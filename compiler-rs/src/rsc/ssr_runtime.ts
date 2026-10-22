// SSR of a React Server Components payload: reads the Flight stream with React's client runtime (the
// SSR layer: regular `react`, `react-dom/server`), streams the HTML document, and inlines the Flight
// chunks as <script> tags so the browser hydrates without fetching the payload again.

import { createElement as h, use } from "react";
import { renderToReadableStream } from "react-dom/server.edge";
import { createFromReadableStream } from "react-server-dom-webpack/client.edge";
import { AppRoot } from "./rsc_boundaries.js";

// One import at a time for modules a request has not loaded yet (serial_import.ts)
const importModule = (specifier: string): Promise<any> => ((globalThis as any).__nata_import ?? ((s: string) => import(s)))(specifier);

// ---------- client references in the SSR layer ----------
// Ids are browser URLs; each is loaded once per version through the engine (`ssr:` specifier) or, for
// packages and framework shims, natively (one shared instance with react-dom/server).

export const ssrModules = new Map<string, unknown>();

export async function loadClientModule(id: string) {
  const callEngine = (globalThis as any).__NATA_CALL_ENGINE__;
  const { meta } = await callEngine("client_reference", { id });
  if (meta.error) throw new Error(meta.error);
  const mod = await importModule(meta.specifier);
  ssrModules.set(id, mod);
  return mod;
}

const moduleMap = new Proxy(
  {},
  {
    get(_target, id) {
      if (typeof id !== "string") return undefined;
      return new Proxy({}, { get: (_t, name) => (typeof name === "string" ? { id, chunks: [id, ""], name } : undefined) });
    },
  },
);

export const serverConsumerManifest = { moduleMap, moduleLoading: null, serverModuleMap: null };

// ---------- document stream ----------

const encoder = new TextEncoder();

function flightScript(text: string) {
  // JSON in a script: neutralise `</script>`, `<!--` and line separators
  const json = JSON.stringify(text).replace(/</g, "\\u003c").replaceAll("\u2028", "\\u2028").replaceAll("\u2029", "\\u2029");
  return `<script>(self.__nata_f=self.__nata_f||[]).push(${json})</script>`;
}

/**
 * Interleaves the inlined Flight scripts with React's HTML: after each batch React flushes (one task),
 * so a script never lands inside a half-written tag; `</body></html>` moves to the very end.
 */
function documentStream(html: ReadableStream<Uint8Array>, flight: ReadableStream<Uint8Array>, headStart: string) {
  const decoder = new TextDecoder();
  let pendingScripts: string[] = [];
  let headInjected = false;
  let flightDone: Promise<void>;

  return new ReadableStream<Uint8Array>({
    async start(controller) {
      const flightDecoder = new TextDecoder();
      flightDone = (async () => {
        for await (const chunk of flight as any) pendingScripts.push(flightScript(flightDecoder.decode(chunk, { stream: true })));
        const rest = flightDecoder.decode();
        if (rest) pendingScripts.push(flightScript(rest));
      })();

      let buffered = "";
      let timer: ReturnType<typeof setTimeout> | null = null;
      const flush = () => {
        timer = null;
        let out = buffered;
        buffered = "";
        if (!headInjected) {
          const head = out.match(/<head(\s[^>]*)?>/i);
          if (!head) {
            buffered = out;
            return;
          }
          const at = head.index! + head[0].length;
          out = out.slice(0, at) + headStart + out.slice(at);
          headInjected = true;
        }
        out = out.replace("</body></html>", "");
        out += pendingScripts.join("");
        pendingScripts = [];
        if (out) controller.enqueue(encoder.encode(out));
      };

      for await (const chunk of html as any) {
        buffered += decoder.decode(chunk, { stream: true });
        if (!timer) timer = setTimeout(flush, 0);
      }
      buffered += decoder.decode();
      if (timer) clearTimeout(timer);
      await flightDone;
      flush();
      controller.enqueue(encoder.encode("</body></html>"));
      controller.close();
    },
  });
}

export interface HtmlOptions {
  headStart: string;
  bootstrap: string;
  formState?: unknown;
}

/**
 * Streams the HTML document for a Flight payload. Resolves once the shell is ready, or rejects with
 * the shell error (notFound() / redirect() / a render error outside any Suspense boundary).
 */
export async function renderHtml(flight: ReadableStream<Uint8Array>, options: HtmlOptions) {
  const [forSsr, forInline] = flight.tee();
  const model: Promise<any> = createFromReadableStream(forSsr, { serverConsumerManifest });
  function Root() {
    const { nodes, keys, selected } = use(model);
    return h(AppRoot, { nodes, keys, selected, router: null });
  }
  const html = await renderToReadableStream(h(Root), {
    bootstrapModules: [options.bootstrap],
    formState: options.formState as any,
    onError(error: any) {
      if (error && typeof error.digest === "string") return error.digest;
      console.error(error);
    },
  });
  return documentStream(html, forInline, options.headStart);
}

/**
 * The document when server rendering failed: no markup, just the inlined payload; the browser renders
 * it (error boundaries included), like Next.js's "switched to client rendering"
 */
export function clientRenderedDocument(flight: ReadableStream<Uint8Array>, options: HtmlOptions) {
  const shell = `<!DOCTYPE html><html><head></head><body><script>self.__nata_csr=true</script><script type="module" src="${options.bootstrap}" async></script></body></html>`;
  const html = new ReadableStream<Uint8Array>({
    start(controller) {
      controller.enqueue(encoder.encode(shell));
      controller.close();
    },
  });
  return documentStream(html, flight, options.headStart);
}
