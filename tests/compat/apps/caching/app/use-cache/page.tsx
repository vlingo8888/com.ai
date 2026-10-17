import { cacheTag } from "next/cache";

async function stamp() {
  "use cache";
  cacheTag("stamp");
  return `stamp-${Date.now()}-${Math.random()}`;
}

export default async function UseCachePage() {
  return <p id="value">{await stamp()}</p>;
}
