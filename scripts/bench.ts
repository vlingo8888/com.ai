#!/usr/bin/env bun
// Measures a project under `com dev` against the performance budgets in ROADMAP.md.
//
//   bun scripts/bench.ts [projectDir] [--path /] [--api /api/x] [--engine <com-compiler>]
//                        [--budget] [--save-baseline] [--compare] [--json <file>]
//
//   --budget         exit 1 when a metric exceeds scripts/perf-budget.json (used in CI)
//   --save-baseline  store the results in scripts/bench-baseline.json
//   --compare        exit 1 when a metric is more than 10% worse than the baseline (same machine only)
//
// Defaults to the compat starter app. The project is copied to a temp dir first, so nothing is written
// into it.

import { existsSync, readFileSync, rmSync, writeFileSync } from "fs";
import { join, resolve } from "path";
import { Browser, copyToTemp, findEngine, median, percentile, readJson, repoRoot, serverRssMb, sleep, startDev, timeRequests } from "../tests/compat/lib/harness";

const args = process.argv.slice(2);
const flag = (name: string) => {
  const i = args.indexOf(name);
  return i >= 0 ? args.splice(i, 2)[1] : undefined;
};
const bool = (name: string) => {
  const i = args.indexOf(name);
  return i >= 0 ? (args.splice(i, 1), true) : false;
};

const engine = findEngine(flag("--engine"));
const pagePath = flag("--path") ?? "/";
const apiPath = flag("--api");
const jsonOut = flag("--json");
const checkBudget = bool("--budget");
const saveBaseline = bool("--save-baseline");
const compare = bool("--compare");
const project = resolve(args[0] ?? join(repoRoot, "tests/compat/apps/starter"));

const budgetFile = join(repoRoot, "scripts/perf-budget.json");
const baselineFile = join(repoRoot, "scripts/bench-baseline.json");

type Metrics = Record<string, number>;

// Lower is better for every metric
const LABELS: Record<string, string> = {
  readyMs: "com dev ready",
  firstPageMs: "first page (cold)",
  ssrMedianMs: "SSR TTFB median (warm)",
  ssrP90Ms: "SSR TTFB p90 (warm)",
  apiMedianMs: "API route median (warm)",
  rssPeakMb: "RAM after load (engine + worker)",
  rssIdleMb: "RAM idle (engine + worker)",
  clientJsKb: "client JS transferred",
  clientJsGzipKb: "client JS gzip (prod, no devtools)",
  hmrMs: "HMR client component",
  hmrServerMs: "HMR server component",
};

/**
 * Bytes of every script the page loaded, gzipped (what a production server would send), without the
 * development tools (error overlay, debugger, Fast Refresh) that production builds leave out
 */
async function gzippedScriptsKb(page: import("../tests/compat/lib/harness").Page, base: string) {
  const urls = await page.evaluate<string[]>(
    `performance.getEntriesByType("resource").map((r) => r.name).filter((n) => /\\/_nata\\/(deps|shims|router|rsc|client)|\\/_bundle\\//.test(n) && !/\\.css|styles|react-refresh/.test(n))`,
  );
  let bytes = 0;
  for (const url of new Set(urls.concat(base + "/_nata/router.js"))) {
    const body = new Uint8Array(await (await fetch(url)).arrayBuffer());
    bytes += Bun.gzipSync(body).length;
  }
  return Math.round(bytes / 1024);
}

/** Edits a file of the running project and returns ms from the write until the page shows `expect` */
async function timeHmr(page: import("../tests/compat/lib/harness").Page, file: string, from: string, to: string, expect: string) {
  const source = readFileSync(file, "utf-8");
  if (!source.includes(from)) return NaN;
  await page.evaluate(
    `window.__hmrSeen = null; new MutationObserver(() => { if (!window.__hmrSeen && document.body.innerText.includes(${JSON.stringify(expect)})) window.__hmrSeen = performance.now(); }).observe(document.body, { subtree: true, childList: true, characterData: true });`,
  );
  const start = await page.evaluate<number>("performance.now()");
  writeFileSync(file, source.replace(from, to));
  for (let i = 0; i < 100 && !(await page.evaluate("window.__hmrSeen")); i++) await sleep(20);
  const seen = await page.evaluate<number | null>("window.__hmrSeen");
  return seen ? Math.round(seen - start) : NaN;
}

const dir = copyToTemp(project, "bench");
const server = await startDev(dir, { engine });
const metrics: Metrics = {};
try {
  metrics.readyMs = server.readyMs;
  const first = performance.now();
  await (await fetch(server.url + pagePath)).arrayBuffer();
  metrics.firstPageMs = Math.round(performance.now() - first);

  await timeRequests(server.url + pagePath, 5);
  const ssr = await timeRequests(server.url + pagePath, 50);
  metrics.ssrMedianMs = +median(ssr).toFixed(2);
  metrics.ssrP90Ms = +percentile(ssr, 0.9).toFixed(2);
  if (apiPath) {
    await timeRequests(server.url + apiPath, 5);
    metrics.apiMedianMs = +median(await timeRequests(server.url + apiPath, 50)).toFixed(2);
  }

  const browser = await Browser.launch();
  try {
    const page = await browser.newPage();
    const report = await page.load(server.url + pagePath, 4000);
    metrics.clientJsKb = Math.round(report.jsBytes / 1024);
    // Save → screen: a client component (Fast Refresh) and a Server Component (payload refetch)
    if (pagePath === "/" && existsSync(join(dir, "app/counter.tsx"))) {
      metrics.hmrMs = await timeHmr(page, join(dir, "app/counter.tsx"), "Count: {count}", "Clicks: {count}", "Clicks: 0");
      metrics.hmrServerMs = await timeHmr(page, join(dir, "app/page.tsx"), "Save and see your changes instantly.", "Saved, and seen instantly.", "Saved, and seen instantly.");
    }
    page.close();
  } finally {
    browser.close();
  }

  // Right after the request burst, garbage is still uncollected (the worker runs with --smol, which
  // collects when idle): report that as the peak, and "idle" once memory has settled
  metrics.rssPeakMb = serverRssMb(server);
  let idle = metrics.rssPeakMb;
  for (let i = 0; i < 15; i++) {
    await sleep(1000);
    idle = Math.min(idle, serverRssMb(server));
  }
  metrics.rssIdleMb = idle;
} finally {
  await server.stop();
  rmSync(dir, { recursive: true, force: true });
}

// Client JavaScript as production would ship it: dependencies in their production builds
{
  const prodDir = copyToTemp(project, "bench-prod");
  const prod = await startDev(prodDir, { engine, env: { COM_DEPS_PROD: "1" } });
  const browser = await Browser.launch();
  try {
    const page = await browser.newPage();
    await page.load(prod.url + pagePath, 4000);
    metrics.clientJsGzipKb = await gzippedScriptsKb(page, prod.url);
    page.close();
  } finally {
    browser.close();
    await prod.stop();
    rmSync(prodDir, { recursive: true, force: true });
  }
}

const budget = existsSync(budgetFile) ? readJson<{ limits: Metrics; targets: Metrics }>(budgetFile) : { limits: {}, targets: {} };
const baseline = existsSync(baselineFile) ? readJson<Metrics>(baselineFile) : null;

console.log(`\nBenchmark: ${project}${pagePath === "/" ? "" : ` ${pagePath}`}\n`);
console.log(`  ${"metric".padEnd(28)} ${"value".padStart(9)} ${"limit".padStart(9)} ${"target".padStart(9)} ${"baseline".padStart(9)}`);
const failures: string[] = [];
for (const [key, value] of Object.entries(metrics)) {
  const unit = key.endsWith("Mb") ? "MB" : key.endsWith("Kb") ? "KB" : "ms";
  const fmt = (v: number | undefined) => (v === undefined ? "—" : `${v}${unit}`);
  const limit = budget.limits[key];
  const target = budget.targets[key];
  const base = baseline?.[key];
  let mark = " ";
  if (limit !== undefined && value > limit) {
    mark = "⨯";
    if (checkBudget) failures.push(`${LABELS[key] ?? key}: ${fmt(value)} exceeds the limit ${fmt(limit)}`);
  } else if (target !== undefined && value > target) {
    mark = "○";
  } else if (target !== undefined) {
    mark = "✓";
  }
  if (compare && base !== undefined && value > base * 1.1 && value - base > 1) {
    failures.push(`${LABELS[key] ?? key}: ${fmt(value)} is >10% worse than the baseline ${fmt(base)}`);
  }
  console.log(` ${mark} ${(LABELS[key] ?? key).padEnd(28)} ${fmt(value).padStart(9)} ${fmt(limit).padStart(9)} ${fmt(target).padStart(9)} ${fmt(base).padStart(9)}`);
}
console.log("\n  ✓ meets target  ○ within limit, above target  ⨯ over limit");

if (saveBaseline) {
  writeFileSync(baselineFile, JSON.stringify(metrics, null, 2) + "\n");
  console.log(`\nSaved baseline to ${baselineFile}`);
}
if (jsonOut) writeFileSync(jsonOut, JSON.stringify(metrics, null, 2));
if (failures.length) {
  console.log(`\n${failures.map((f) => ` ⨯ ${f}`).join("\n")}`);
  process.exit(1);
}
