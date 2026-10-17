import { NextResponse } from "next/server";

export async function GET(request: Request) {
  const name = new URL(request.url).searchParams.get("name") ?? "world";
  return NextResponse.json({ message: `hello ${name}` });
}

export async function POST(request: Request) {
  const body = await request.json();
  return NextResponse.json({ received: body }, { status: 201 });
}
