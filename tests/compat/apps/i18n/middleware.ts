import { NextResponse, type NextRequest } from "next/server";

const locales = ["vi", "en"];

export function middleware(request: NextRequest) {
  const { pathname } = request.nextUrl;
  if (locales.some((l) => pathname === `/${l}` || pathname.startsWith(`/${l}/`))) return;
  const preferred = request.headers.get("accept-language")?.startsWith("en") ? "en" : "vi";
  return NextResponse.redirect(new URL(`/${preferred}${pathname === "/" ? "" : pathname}`, request.url));
}

export const config = { matcher: ["/((?!api|favicon.ico).*)"] };
