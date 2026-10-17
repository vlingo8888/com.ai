// React Server Components pipeline end to end, over HTTP (ROADMAP P1.3–P1.7): streaming SSR of async
// Server Components, not-found, Flight payloads for the client router (with layouts the client already
// has left out), Server Actions called with encodeReply and posted by a form without JavaScript, and
// server code edits applied without restarting the worker.
//
// Needs the release engine (`bun run build:rust`); skipped otherwise.

import { afterAll, beforeAll, describe, expect, it } from "bun:test";
import { existsSync, readFileSync, writeFileSync } from "fs";
import { join } from "path";
import { copyToTemp, findEngine, repoRoot, sleep, startDev, type DevServer } from "./compat/lib/harness";

const engine = (() => {
  try {
    return findEngine();
  } catch {
    return null;
  }
})();
const run = engine && existsSync(engine) ? describe : describe.skip;

async function flightClient(dir: string) {
  return await import(join(dir, "node_modules/react-server-dom-webpack/client.edge.js"));
}

/** The root row of a Flight payload (`0:{...}`), references left as `$` strings */
async function flightRoot(res: Response) {
  const text = await res.text();
  const line = text.split("\n").find((l) => l.startsWith("0:{"));
  expect(line).toBeDefined();
  return { model: JSON.parse(line!.slice(2)), contentType: res.headers.get("content-type") };
}

const manifest = { serverConsumerManifest: { moduleMap: {}, moduleLoading: null, serverModuleMap: null } };

run("RSC pipeline: async-data", () => {
  let server: DevServer;
  beforeAll(async () => {
    server = await startDev(copyToTemp(join(repoRoot, "tests/compat/apps/async-data"), "rsc-async"), { engine: engine! });
  }, 240_000);
  afterAll(async () => server && (await server.stop()));

  it("streams the document: loading.tsx first, then the async page", async () => {
    const res = await fetch(server.url + "/");
    expect(res.status).toBe(200);
    expect(res.headers.get("content-type")).toContain("text/html");
    const html = await res.text();
    expect(html.startsWith("<!DOCTYPE html>")).toBe(true);
    expect(html.indexOf("Loading posts...")).toBeLessThan(html.indexOf("Hello from an async component"));
    expect(html).toContain("self.__nata_f");
    expect(html).toContain('<script type="importmap">');
    expect(html.trimEnd().endsWith("</body></html>")).toBe(true);
    // Head metadata from the layout's title template
    expect(html).toContain("<title>Async Data</title>");
  }, 60_000);

  it("renders not-found.tsx with status 404 for unknown routes", async () => {
    const unknown = await fetch(server.url + "/no/such/route");
    expect(unknown.status).toBe(404);
    expect(await unknown.text()).toContain("Custom not found page");
  }, 60_000);

  it("streams notFound() below loading.tsx to the client boundary, like Next.js (status already sent)", async () => {
    const res = await fetch(server.url + "/posts/999");
    expect(res.status).toBe(200);
    const html = await res.text();
    expect(html).toContain("NEXT_NOT_FOUND");
    expect(html).toContain("NotFoundBoundary");
  }, 60_000);

  it("serves the Flight payload to the client router and leaves out layouts it already has", async () => {
    const { model: full, contentType } = await flightRoot(await fetch(server.url + "/posts/1", { headers: { RSC: "1" } }));
    expect(contentType).toContain("text/x-component");
    expect(full.keys).toEqual(["app", "app/posts", "app/posts/1", "app/posts/[id]/page.tsx"]);
    expect(full.nodes.every((n: unknown) => n !== null)).toBe(true);
    expect(full.title).toBe("Post 1 | Async Data");

    const state = encodeURIComponent(JSON.stringify(["app", "app/page.tsx"]));
    const { model: partial } = await flightRoot(
      await fetch(server.url + "/posts/1", { headers: { RSC: "1", "Nata-Router-State": state } }),
    );
    expect(partial.nodes[0]).toBeNull();
    expect(partial.nodes[partial.nodes.length - 1]).not.toBeNull();
  }, 60_000);

  it("picks up an edited server module on the next request, without restarting the worker", async () => {
    await fetch(server.url + "/");
    const workersBefore = server.pids();
    const file = join(server.dir, "lib/posts.ts");
    writeFileSync(file, readFileSync(file, "utf-8").replace("Hello from an async component", "Edited without a restart"));
    let html = "";
    for (let i = 0; i < 40 && !html.includes("Edited without a restart"); i++) {
      await sleep(100);
      html = await (await fetch(server.url + "/")).text();
    }
    expect(html).toContain("Edited without a restart");
    expect(server.pids()).toEqual(workersBefore);
  }, 60_000);
});

run("RSC pipeline: Server Actions", () => {
  let server: DevServer;
  beforeAll(async () => {
    server = await startDev(copyToTemp(join(repoRoot, "tests/compat/apps/server-actions"), "rsc-actions"), { engine: engine! });
    await fetch(server.url + "/");
  }, 240_000);
  afterAll(async () => server && (await server.stop()));

  const actionId = async (name: string) => {
    const stub = await (await fetch(server.url + "/_bundle/app/actions.ts")).text();
    const match = stub.match(new RegExp(`export const ${name} = __nata_server_ref\\("([0-9a-f]+)"`));
    expect(match).not.toBeNull();
    return match![1];
  };

  it("ships only references to the browser", async () => {
    const stub = await (await fetch(server.url + "/_bundle/app/actions.ts")).text();
    expect(stub).toContain("createServerReference");
    expect(stub).not.toContain("Read the docs");
    expect(stub).not.toContain("added on the");
  });

  it("runs an action called by the client router (encodeReply) on the server", async () => {
    const { encodeReply, createFromFetch } = await flightClient(server.dir);
    const id = await actionId("greet");
    const res = fetch(server.url + "/", {
      method: "POST",
      headers: { "Next-Action": id, Accept: "text/x-component" },
      body: await encodeReply(["Zalo"]),
    });
    const result = await createFromFetch(res, manifest);
    expect(await result.a).toBe("Hello, Zalo (ran on the server)");
    expect(result.f).toBeNull();
  }, 60_000);

  it("handles a <form action> posted without JavaScript and re-renders with the form state", async () => {
    const html = await (await fetch(server.url + "/")).text();
    const form = new FormData();
    for (const [, name, value] of html.matchAll(/<input type="hidden" name="([^"]+)"(?: value="([^"]*)")?/g)) {
      form.append(name.replace(/&amp;/g, "&"), (value ?? "").replace(/&quot;/g, '"').replace(/&amp;/g, "&"));
    }
    expect([...form.keys()].some((k) => k.startsWith("$ACTION_"))).toBe(true);
    form.append("title", "No JS");
    const res = await fetch(server.url + "/", { method: "POST", body: form });
    expect(res.status).toBe(200);
    expect(await res.text()).toContain("No JS (added on the server)");
  }, 60_000);
});
