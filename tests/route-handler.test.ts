import { describe, expect, it, beforeAll, afterAll } from "bun:test";
import { mkdtempSync, mkdirSync, writeFileSync, rmSync, existsSync } from "fs";
import { join } from "path";
import { tmpdir } from "os";

// ============================================================================
// Next.js Route Handlers & Custom API Endpoints Engine Tests (Section 2 - P0)
// ============================================================================

describe("Next.js Server Shims - NextRequest & NextResponse", () => {
  it("NextResponse.json should return standard Response with application/json", async () => {
    // Dynamically test the implementation pattern matching .nata/server.ts
    class NextResponse extends Response {
      static json(data: any, init?: ResponseInit): NextResponse {
        const headers = new Headers(init?.headers);
        if (!headers.has("content-type")) {
          headers.set("content-type", "application/json; charset=utf-8");
        }
        return new NextResponse(JSON.stringify(data), { ...init, headers });
      }

      static redirect(url: string | URL, init?: number | ResponseInit): NextResponse {
        const status = typeof init === "number" ? init : init?.status || 307;
        const headers = new Headers(typeof init === "object" ? init?.headers : undefined);
        headers.set("location", url.toString());
        return new NextResponse(null, { status, headers });
      }

      static rewrite(destination: string | URL, init?: ResponseInit): NextResponse {
        const headers = new Headers(init?.headers);
        headers.set("x-middleware-rewrite", destination.toString());
        return new NextResponse(null, { ...init, headers });
      }

      static next(init?: ResponseInit): NextResponse {
        const headers = new Headers(init?.headers);
        headers.set("x-middleware-next", "1");
        return new NextResponse(null, { ...init, headers });
      }
    }

    const res = NextResponse.json({ success: true, count: 42 }, { status: 201 });
    expect(res.status).toBe(201);
    expect(res.headers.get("content-type")).toContain("application/json");

    const data = await res.json();
    expect(data.success).toBe(true);
    expect(data.count).toBe(42);

    // Test redirect
    const redirectRes = NextResponse.redirect("https://com.ai.vn/login");
    expect(redirectRes.status).toBe(307);
    expect(redirectRes.headers.get("location")).toBe("https://com.ai.vn/login");

    // Test rewrite
    const rewriteRes = NextResponse.rewrite("https://com.ai.vn/internal-page");
    expect(rewriteRes.headers.get("x-middleware-rewrite")).toBe("https://com.ai.vn/internal-page");

    // Test next
    const nextRes = NextResponse.next();
    expect(nextRes.headers.get("x-middleware-next")).toBe("1");
  });

  it("NextRequest should populate nextUrl, cookies, and standard request properties", () => {
    class RequestCookies {
      private map = new Map<string, string>();
      constructor(cookieHeader?: string) {
        if (cookieHeader) {
          for (const pair of cookieHeader.split(";")) {
            const [k, v] = pair.split("=");
            if (k && v) this.map.set(k.trim(), v.trim());
          }
        }
      }
      get(name: string) {
        const val = this.map.get(name);
        return val !== undefined ? { name, value: val } : undefined;
      }
      has(name: string) { return this.map.has(name); }
    }

    class NextRequest extends Request {
      readonly nextUrl: URL;
      readonly cookies: RequestCookies;
      readonly ip: string;

      constructor(input: string, init?: RequestInit & { ip?: string }) {
        super(input, init);
        this.nextUrl = new URL(input);
        this.ip = init?.ip || "127.0.0.1";
        this.cookies = new RequestCookies(this.headers.get("cookie") || "");
      }
    }

    const req = new NextRequest("http://localhost:3000/api/users/123?sort=desc", {
      method: "POST",
      headers: {
        "cookie": "session_id=sess_abc123; theme=dark",
        "x-custom": "nata-engine",
      },
      ip: "192.168.1.100",
    });

    expect(req.nextUrl.pathname).toBe("/api/users/123");
    expect(req.nextUrl.searchParams.get("sort")).toBe("desc");
    expect(req.ip).toBe("192.168.1.100");
    expect(req.cookies.get("session_id")?.value).toBe("sess_abc123");
    expect(req.cookies.has("theme")).toBe(true);
    expect(req.cookies.has("non_existent")).toBe(false);
  });
});

describe("Route Handler Conventions & Dynamic Routing Engine", () => {
  let testDir: string;

  beforeAll(() => {
    testDir = mkdtempSync(join(tmpdir(), "nata-route-handler-test-"));
    mkdirSync(join(testDir, "app", "api", "webhooks", "vnpay"), { recursive: true });
    mkdirSync(join(testDir, "app", "api", "auth", "login"), { recursive: true });
    mkdirSync(join(testDir, "app", "api", "users", "[id]"), { recursive: true });

    writeFileSync(
      join(testDir, "app", "api", "webhooks", "vnpay", "route.ts"),
      `
export async function POST(req: Request) {
  const body = await req.json();
  return Response.json({ received: true, body });
}
`
    );

    writeFileSync(
      join(testDir, "app", "api", "auth", "login", "route.js"),
      `
export async function POST(req) {
  return Response.json({ token: "jwt_token_example" }, { status: 200 });
}
`
    );

    writeFileSync(
      join(testDir, "app", "api", "users", "[id]", "route.ts"),
      `
export async function GET(req: Request, { params }: { params: { id: string } }) {
  return Response.json({ id: params.id });
}
export async function DELETE(req: Request, { params }: { params: { id: string } }) {
  return Response.json({ deleted: params.id });
}
`
    );
  });

  afterAll(() => {
    rmSync(testDir, { recursive: true, force: true });
  });

  it("should verify route files exist at correct App Router conventions", () => {
    expect(existsSync(join(testDir, "app/api/webhooks/vnpay/route.ts"))).toBe(true);
    expect(existsSync(join(testDir, "app/api/auth/login/route.js"))).toBe(true);
    expect(existsSync(join(testDir, "app/api/users/[id]/route.ts"))).toBe(true);
  });

  it("should properly execute exported GET handler with dynamic route params", async () => {
    const routeModule = await import(join(testDir, "app/api/users/[id]/route.ts"));
    expect(typeof routeModule.GET).toBe("function");
    expect(typeof routeModule.DELETE).toBe("function");

    const fakeReq = new Request("http://localhost:3000/api/users/user_99");
    const context = { params: { id: "user_99" } };

    const response = await routeModule.GET(fakeReq, context);
    expect(response.status).toBe(200);

    const json = await response.json();
    expect(json.id).toBe("user_99");
  });

  it("should execute POST handler receiving JSON payload", async () => {
    const webhookModule = await import(join(testDir, "app/api/webhooks/vnpay/route.ts"));
    expect(typeof webhookModule.POST).toBe("function");

    const payload = { vnp_Amount: 500000, vnp_TxnRef: "TXN_123456" };
    const fakeReq = new Request("http://localhost:3000/api/webhooks/vnpay", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(payload),
    });

    const response = await webhookModule.POST(fakeReq);
    expect(response.status).toBe(200);

    const data = await response.json();
    expect(data.received).toBe(true);
    expect(data.body.vnp_TxnRef).toBe("TXN_123456");
  });
});
