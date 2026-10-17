import { NextResponse } from "next/server";
import { cookies } from "next/headers";

export async function GET() {
  (await cookies()).set("session", "abc", { httpOnly: true });
  const res = NextResponse.json({ ok: true });
  res.cookies.set("theme", "dark");
  return res;
}

export async function POST() {
  return NextResponse.redirect("http://localhost/headers", 303);
}
