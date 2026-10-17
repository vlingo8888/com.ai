export async function register() {
  (globalThis as any).__compatRegistered = "yes";
}

export async function onRequestError(error: unknown, request: { path: string }) {
  const g = globalThis as any;
  g.__compatErrors = [...(g.__compatErrors ?? []), `${request.path}: ${(error as Error).message}`];
}
