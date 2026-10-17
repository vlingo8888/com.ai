import { cookies, headers } from "next/headers";

export default async function HeadersPage() {
  const h = await headers();
  const c = await cookies();
  return (
    <main>
      <p>Header x-compat: {h.get("x-compat") ?? "none"}</p>
      <p>Cookie theme: {c.get("theme")?.value ?? "none"}</p>
    </main>
  );
}
