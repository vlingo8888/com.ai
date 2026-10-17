import { unstable_cache } from "next/cache";

const getRandom = unstable_cache(async () => Math.random(), ["random"], { tags: ["random"] });

export default async function UnstablePage() {
  return <p id="value">{await getRandom()}</p>;
}
