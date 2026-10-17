export function GET() {
  const g = globalThis as any;
  g.__compatCounter = (g.__compatCounter ?? 0) + 1;
  return Response.json({ count: g.__compatCounter });
}
