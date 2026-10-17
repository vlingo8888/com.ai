import { NextRequest, NextResponse, userAgent } from "next/server";

export function GET(request: NextRequest) {
  const { browser, device, os } = userAgent(request);
  return NextResponse.json({ browser: browser.name, os: os.name, mobile: device.type === "mobile", path: request.nextUrl.pathname });
}
