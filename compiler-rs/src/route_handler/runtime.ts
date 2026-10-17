/**
 * Native Route Handler Evaluator for Com.AI.VN / NATA Framework
 * Executes exported HTTP method functions (GET, POST, PUT, DELETE, PATCH, OPTIONS, HEAD)
 * in Next.js App Router API endpoints (`route.ts` / `route.js`).
 */

const ROUTE_DELIM_START = "__NATA_ROUTE_OUT_START__";
const ROUTE_DELIM_END = "__NATA_ROUTE_OUT_END__";

interface RouteHandlerInput {
  filePath: string;
  /** `rsc:` specifier when the engine serves server modules (edits apply without a restart) */
  moduleSpecifier?: string | null;
  url: string;
  method: string;
  params: Record<string, string>;
  searchParams: Record<string, string>;
  headers: Record<string, string>;
  cookiesStr: string;
  bodyBase64?: string | null;
  /** Raw request body (persistent worker); takes precedence over `bodyBase64` */
  bodyBytes?: Uint8Array | null;
}

interface RouteHandlerResult {
  /** Response metadata, same shape the engine deserializes into `RouteHandlerResponse` (minus the body) */
  output: Record<string, unknown>;
  body: Uint8Array;
}

const encoder = new TextEncoder();

/** One-off process entry point: prints the result with a base64 body between delimiters */
export async function runRouteHandler(input: RouteHandlerInput) {
  const { output, body } = await handleRoute(input);
  const payload = { ...output, body_base64: Buffer.from(body).toString("base64") };
  process.stdout.write(ROUTE_DELIM_START + JSON.stringify(payload) + ROUTE_DELIM_END + "\n");
}

/** Runs the route handler and returns the response body as raw bytes (persistent worker entry point) */
export async function handleRoute(input: RouteHandlerInput): Promise<RouteHandlerResult> {
  const startTime = performance.now();

  try {
    // 1. Setup incoming cookies in global state
    const cookieMap = new Map<string, string>();
    if (input.cookiesStr && typeof input.cookiesStr === "string") {
      for (const pair of input.cookiesStr.split(";")) {
        const idx = pair.indexOf("=");
        if (idx !== -1) {
          const k = decodeURIComponent(pair.slice(0, idx).trim());
          const v = decodeURIComponent(pair.slice(idx + 1).trim());
          if (k) cookieMap.set(k, v);
        }
      }
    }
    (globalThis as any).__NATA_COOKIES__ = cookieMap;
    (globalThis as any).__NATA_SET_COOKIES__ = [];

    // 2. Import Next.js server shims
    const serverModule = await import("./server.ts").catch(() => null);
    const NextRequest = serverModule?.NextRequest || (globalThis as any).Request;
    const NextResponse = serverModule?.NextResponse || (globalThis as any).Response;

    // 3. Dynamically import the target route handler module
    const mod = await import(input.moduleSpecifier || input.filePath);

    // List of standard HTTP methods supported by Next.js route handlers
    const httpMethods = ["GET", "POST", "PUT", "DELETE", "PATCH", "OPTIONS", "HEAD"];
    const availableMethods: string[] = [];

    for (const m of httpMethods) {
      if (typeof mod[m] === "function" || (mod.default && typeof mod.default[m] === "function")) {
        availableMethods.push(m);
      }
    }

    const requestedMethod = input.method.toUpperCase();
    let handler = mod[requestedMethod];
    if (!handler && mod.default && typeof mod.default[requestedMethod] === "function") {
      handler = mod.default[requestedMethod];
    }

    // 4. Handle method not allowed / automatic OPTIONS
    if (!handler) {
      if (requestedMethod === "OPTIONS") {
        const allowHeader = availableMethods.join(", ");
        const resHeaders: Record<string, string> = {
          "Allow": allowHeader,
          "Content-Length": "0",
        };
        const duration = Number((performance.now() - startTime).toFixed(2));
        const output = {
          status: 204,
          status_text: "No Content",
          headers: resHeaders,
          cookies: [],
          duration_ms: duration,
          error: null,
        };
        return { output, body: new Uint8Array() };
      }

      const allowHeader = availableMethods.join(", ");
      const duration = Number((performance.now() - startTime).toFixed(2));
      const output = {
        status: 405,
        status_text: "Method Not Allowed",
        headers: {
          "Allow": allowHeader,
          "Content-Type": "text/plain; charset=utf-8",
        },
        cookies: [],
        duration_ms: duration,
        error: `Method ${requestedMethod} Not Allowed`,
      };
      return { output, body: encoder.encode(`Method ${requestedMethod} Not Allowed`) };
    }

    // 5. Construct NextRequest
    let reqBody: Uint8Array | undefined = undefined;
    if (requestedMethod !== "GET" && requestedMethod !== "HEAD") {
      if (input.bodyBytes && input.bodyBytes.length) reqBody = input.bodyBytes;
      else if (input.bodyBase64) reqBody = Buffer.from(input.bodyBase64, "base64");
    }

    const requestHeaders = new Headers();
    for (const [k, v] of Object.entries(input.headers || {})) {
      requestHeaders.set(k, v);
    }
    if (input.cookiesStr && !requestHeaders.has("cookie")) {
      requestHeaders.set("cookie", input.cookiesStr);
    }

    const nextReq = new NextRequest(input.url, {
      method: requestedMethod,
      headers: requestHeaders,
      body: reqBody,
    });

    // 6. Build route context with dynamic params
    // Compatible with both Next.js 14 sync params and Next.js 15 async Promise params
    const rawParams = input.params || {};
    const paramsPromise = Promise.resolve(rawParams);
    const context = {
      params: Object.assign(paramsPromise, rawParams),
    };

    // 7. Invoke the route handler
    const responseResult = await handler(nextReq, context);

    // 8. Process returned response
    let status = 200;
    let statusText = "OK";
    const responseHeaders: Record<string, string> = {};
    let body: Uint8Array = new Uint8Array();

    if (responseResult instanceof Response) {
      status = responseResult.status;
      statusText = responseResult.statusText || (status === 200 ? "OK" : "");

      responseResult.headers.forEach((val, key) => {
        responseHeaders[key] = val;
      });

      body = new Uint8Array(await responseResult.arrayBuffer());
    } else if (responseResult === null || responseResult === undefined) {
      status = 204;
      statusText = "No Content";
    } else if (typeof responseResult === "object" || typeof responseResult === "string" || typeof responseResult === "number" || typeof responseResult === "boolean") {
      // Auto-fallback for handlers returning direct data without wrapping in Response
      status = 200;
      statusText = "OK";
      responseHeaders["content-type"] = "application/json; charset=utf-8";
      body = encoder.encode(JSON.stringify(responseResult));
    }

    // 9. Collect cookies from NextResponse.cookies and global state
    const setCookiesList: any[] = [];

    // From NextResponse cookies instance if present
    if (responseResult && typeof (responseResult as any).cookies?.getAll === "function") {
      const respCookies = (responseResult as any).cookies.getAll();
      for (const c of respCookies) {
        setCookiesList.push({
          name: c.name,
          value: c.value,
          path: c.path || "/",
          maxAge: c.maxAge,
          expires: c.expires instanceof Date ? c.expires.toUTCString() : (c.expires ? String(c.expires) : undefined),
          domain: c.domain,
          secure: c.secure,
          sameSite: c.sameSite || "Lax",
          deleted: c.maxAge === 0,
        });
      }
    }

    // From global cookie store
    const globalCookies = (globalThis as any).__NATA_SET_COOKIES__ || [];
    for (const c of globalCookies) {
      setCookiesList.push(c);
    }

    const duration = Number((performance.now() - startTime).toFixed(2));
    const output = {
      status,
      status_text: statusText,
      headers: responseHeaders,
      cookies: setCookiesList,
      duration_ms: duration,
      error: null,
    };

    return { output, body };
  } catch (err: any) {
    const duration = Number((performance.now() - startTime).toFixed(2));
    const errMsg = err?.message || String(err);
    const errStack = err?.stack || "";
    console.error(`\x1b[31m✖ [Route Handler Error]\x1b[0m ${errMsg}\n${errStack}`);

    const output = {
      status: 500,
      status_text: "Internal Server Error",
      headers: { "Content-Type": "application/json; charset=utf-8" },
      cookies: [],
      duration_ms: duration,
      error: errMsg,
    };

    return { output, body: encoder.encode(JSON.stringify({ error: errMsg, stack: errStack })) };
  }
}
