import { after } from "next/server";

export async function GET(request: Request) {
  const mode = new URL(request.url).searchParams.get("mode");
  if (mode === "check") return Response.json({ ran: (globalThis as any).__compatAfterRan === true });
  after(() => {
    (globalThis as any).__compatAfterRan = true;
  });
  return Response.json({ scheduled: true });
}
