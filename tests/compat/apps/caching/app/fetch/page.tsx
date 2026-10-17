import { headers } from "next/headers";

export default async function FetchPage() {
  const host = (await headers()).get("host");
  const res = await fetch(`http://${host}/api/counter`, { next: { revalidate: 3600, tags: ["counter"] } });
  const { count } = await res.json();
  const fresh = await (await fetch(`http://${host}/api/counter`, { cache: "no-store" })).json();
  return <p id="value">cached={count} fresh={fresh.count}</p>;
}
