// Data cache end to end (ROADMAP P2): "use cache" + cacheTag, fetch(..., { next }), unstable_cache,
// revalidateTag / revalidatePath, dynamicParams. Needs the release engine; skipped otherwise.

import { afterAll, beforeAll, describe, expect, it } from "bun:test";
import { existsSync } from "fs";
import { join } from "path";
import { copyToTemp, findEngine, repoRoot, startDev, type DevServer } from "./compat/lib/harness";

const engine = (() => {
  try {
    return findEngine();
  } catch {
    return null;
  }
})();

const value = async (server: DevServer, path: string) => {
  const html = await (await fetch(server.url + path)).text();
  const match = html.match(/<p id="value">([^<]*(?:<!-- -->[^<]*)*)<\/p>/);
  expect(match).not.toBeNull();
  return match![1].replace(/<!-- -->/g, "");
};

(engine && existsSync(engine) ? describe : describe.skip)("data cache", () => {
  let server: DevServer;
  beforeAll(async () => {
    server = await startDev(copyToTemp(join(repoRoot, "tests/compat/apps/caching"), "caching"), { engine: engine! });
  }, 240_000);
  afterAll(async () => server && (await server.stop()));

  it('"use cache" returns the cached result until its tag is revalidated', async () => {
    const first = await value(server, "/use-cache");
    expect(await value(server, "/use-cache")).toBe(first);
    await fetch(server.url + "/api/revalidate?tag=stamp");
    const after = await value(server, "/use-cache");
    expect(after).not.toBe(first);
    expect(await value(server, "/use-cache")).toBe(after);
  }, 60_000);

  it("revalidatePath() purges what a page cached", async () => {
    const first = await value(server, "/use-cache");
    await fetch(server.url + "/api/revalidate?path=/use-cache");
    expect(await value(server, "/use-cache")).not.toBe(first);
  }, 60_000);

  it("fetch with next.revalidate is cached, cache: no-store is not", async () => {
    const first = await value(server, "/fetch");
    const second = await value(server, "/fetch");
    const cached = (s: string) => s.match(/cached=(\d+)/)![1];
    const fresh = (s: string) => Number(s.match(/fresh=(\d+)/)![1]);
    expect(cached(second)).toBe(cached(first));
    expect(fresh(second)).toBeGreaterThan(fresh(first));
    await fetch(server.url + "/api/revalidate?tag=counter");
    expect(cached(await value(server, "/fetch"))).not.toBe(cached(first));
  }, 60_000);

  it("unstable_cache caches by key and tag", async () => {
    const first = await value(server, "/unstable");
    expect(await value(server, "/unstable")).toBe(first);
    await fetch(server.url + "/api/revalidate?tag=random");
    expect(await value(server, "/unstable")).not.toBe(first);
  }, 60_000);

  it("dynamicParams = false limits params to generateStaticParams", async () => {
    expect((await fetch(server.url + "/static/second")).status).toBe(200);
    expect((await fetch(server.url + "/static/other")).status).toBe(404);
  }, 60_000);
});
