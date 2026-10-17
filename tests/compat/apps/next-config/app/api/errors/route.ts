export function GET() {
  return Response.json({ errors: (globalThis as any).__compatErrors ?? [] });
}
