import { NextResponse, type NextRequest } from "next/server";

export function middleware(request: NextRequest) {
  const { pathname } = request.nextUrl;
  if (pathname === "/old") return NextResponse.redirect(new URL("/headers", request.url));
  if (pathname.startsWith("/api/blocked")) return NextResponse.json({ error: "blocked by middleware" }, { status: 401 });

  const headers = new Headers(request.headers);
  if (!headers.has("x-compat")) headers.set("x-compat", "from-middleware");
  const response =
    pathname === "/rewritten"
      ? NextResponse.rewrite(new URL("/headers", request.url), { request: { headers } })
      : NextResponse.next({ request: { headers } });
  response.headers.set("x-middleware-ran", "1");
  response.cookies.set("visited", "yes");
  return response;
}

export const config = {
  matcher: ["/((?!api/stream|_next/static|favicon.ico).*)"],
};
