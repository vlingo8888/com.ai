#!/usr/bin/env bun
// Next.js compatibility suite: runs each app in tests/compat/apps under `com dev` (unmodified Next.js
// App Router code) and checks it in headless Chrome.
//
//   bun tests/compat/run.ts [app...] [--engine <com-compiler>] [--json <file>]
//
// Each app has a compat.json listing checks. A check marked `knownGap` documents a feature that is not
// supported yet: its failure is reported as XFAIL and does not fail the run, while an unexpected pass
// (XPASS) is reported so the gap can be removed from compat.json and ROADMAP.md.

import { readdirSync, readFileSync, rmSync, writeFileSync } from "fs";
import { join } from "path";
import { Browser, copyToTemp, findEngine, readJson, serverRssMb, sleep, startDev, type DevServer, type Page } from "./lib/harness";

interface Check {
  name: string;
  path: string;
  knownGap?: string;
  status?: number;
  /** Text that must be in the server-rendered HTML (before any JavaScript runs) */
  ssr?: string[];
  /** Text that must be visible after the page settled */
  contains?: string[];
  title?: string;
  selector?: string;
  noIssues?: boolean;
  noExternalHosts?: boolean;
  computedStyle?: { selector: string; property: string; equals: string };
  /** Text that must not appear anywhere in the server HTML */
  notInHtml?: string[];
  /** Click before `navigate`, e.g. to put state in a layout */
  prepare?: { click: string; expectText: string };
  /** `keepText`: still visible after navigating (state of a layout shared by both pages) */
  navigate?: { click: string; expectText: string; expectPath?: string; expectLoading?: string; keepText?: string };
  click?: string;
  expectText?: string;
  api?: { method?: string; body?: unknown; status: number; json?: unknown };
  /**
   * Edit a project file after the steps above (put such checks last: the edit stays): the page must
   * show `expectText` without reloading, and still show `keepText` (state from before the edit)
   */
  edit?: { file: string; from: string; to: string; expectText: string; keepText?: string };
}

type Outcome = "PASS" | "FAIL" | "XFAIL" | "XPASS";
interface Result {
  app: string;
  check: string;
  outcome: Outcome;
  detail?: string;
}

const args = process.argv.slice(2);
const flag = (name: string) => {
  const i = args.indexOf(name);
  return i >= 0 ? args.splice(i, 2)[1] : undefined;
};
const engine = findEngine(flag("--engine"));
const jsonOut = flag("--json");
const appsDir = join(import.meta.dir, "apps");
const apps = args.length ? args : readdirSync(appsDir).filter((d) => !d.startsWith(".")).sort();

function isSubset(expected: unknown, actual: unknown): boolean {
  if (expected === null || typeof expected !== "object") return expected === actual;
  if (actual === null || typeof actual !== "object") return false;
  return Object.entries(expected as Record<string, unknown>).every(([k, v]) => isSubset(v, (actual as any)[k]));
}

async function waitForText(page: Page, text: string, timeoutMs = 6000, pollMs = 150): Promise<boolean> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (await page.evaluate<boolean>(`document.body.innerText.includes(${JSON.stringify(text)})`)) return true;
    await sleep(pollMs);
  }
  return false;
}

/** Runs one check; returns a failure message, or null when it passes */
async function runCheck(server: DevServer, page: Page, check: Check): Promise<string | null> {
  const url = server.url + check.path;

  if (check.api) {
    const res = await fetch(url, {
      method: check.api.method ?? "GET",
      headers: check.api.body !== undefined ? { "content-type": "application/json" } : undefined,
      body: check.api.body !== undefined ? JSON.stringify(check.api.body) : undefined,
    });
    if (res.status !== check.api.status) return `status ${res.status}, expected ${check.api.status}`;
    if (check.api.json !== undefined) {
      const body = await res.json().catch(() => null);
      if (!isSubset(check.api.json, body)) return `body ${JSON.stringify(body)} does not match ${JSON.stringify(check.api.json)}`;
    }
    return null;
  }

  if (check.ssr) {
    // React separates adjacent text nodes with `<!-- -->` in server HTML
    const html = (await (await fetch(url)).text()).replace(/<!--.*?-->/g, "");
    const missing = check.ssr.filter((t) => !html.includes(t));
    if (missing.length) return `not in server HTML: ${missing.map((t) => JSON.stringify(t)).join(", ")}`;
  }
  if (check.notInHtml) {
    const html = await (await fetch(url)).text();
    const present = check.notInHtml.filter((t) => html.includes(t));
    if (present.length) return `server HTML contains: ${present.map((t) => JSON.stringify(t)).join(", ")}`;
  }

  const report = await page.load(url);
  if (check.status !== undefined && report.status !== check.status) return `status ${report.status}, expected ${check.status}`;
  for (const text of check.contains ?? []) {
    if (!(await waitForText(page, text))) return `text not visible: ${JSON.stringify(text)} (page: ${JSON.stringify(report.text.slice(0, 120))})`;
  }
  if (check.title !== undefined) {
    const title = await page.evaluate<string>("document.title");
    if (title !== check.title) return `title ${JSON.stringify(title)}, expected ${JSON.stringify(check.title)}`;
  }
  if (check.selector && !(await page.evaluate<boolean>(`!!document.querySelector(${JSON.stringify(check.selector)})`))) {
    return `no element matches ${check.selector}`;
  }
  if (check.noIssues && report.overlay) return `dev overlay shows: ${report.overlay}`;
  if (check.noExternalHosts && report.externalHosts.length) return `contacted external hosts: ${report.externalHosts.join(", ")}`;
  if (check.computedStyle) {
    const { selector, property, equals } = check.computedStyle;
    const value = await page.evaluate<string | null>(
      `(() => { const el = document.querySelector(${JSON.stringify(selector)}); return el ? getComputedStyle(el).getPropertyValue(${JSON.stringify(property)}) : null; })()`,
    );
    if (value !== equals) return `${selector} ${property} = ${JSON.stringify(value)}, expected ${JSON.stringify(equals)}`;
  }
  if (check.prepare) {
    await page.evaluate(`document.querySelector(${JSON.stringify(check.prepare.click)})?.click()`);
    if (!(await waitForText(page, check.prepare.expectText))) return `after clicking ${check.prepare.click}, text not visible: ${JSON.stringify(check.prepare.expectText)}`;
  }
  if (check.navigate) {
    const { click, expectText, expectPath, expectLoading, keepText } = check.navigate;
    const marker = await page.evaluate<number>("(window.__compatMarker = Math.random())");
    await page.evaluate(`document.querySelector(${JSON.stringify(click)})?.click()`);
    if (expectLoading && !(await waitForText(page, expectLoading, 1000, 20))) {
      return `after clicking ${click}, loading UI ${JSON.stringify(expectLoading)} was not shown`;
    }
    if (!(await waitForText(page, expectText))) return `after clicking ${click}, text not visible: ${JSON.stringify(expectText)}`;
    if (expectPath && (await page.evaluate<string>("location.pathname")) !== expectPath) return `after clicking ${click}, path is not ${expectPath}`;
    if ((await page.evaluate<number>("window.__compatMarker")) !== marker) return `clicking ${click} reloaded the page instead of navigating client-side`;
    if (keepText && !(await waitForText(page, keepText, 500))) return `after clicking ${click}, layout state is gone (${JSON.stringify(keepText)})`;
  }
  if (check.click) {
    await page.evaluate(`document.querySelector(${JSON.stringify(check.click)})?.click()`);
    if (check.expectText && !(await waitForText(page, check.expectText))) return `after clicking ${check.click}, text not visible: ${JSON.stringify(check.expectText)}`;
  }
  if (check.edit) {
    const { file, from, to, expectText, keepText } = check.edit;
    const path = join(server.dir, file);
    const source = readFileSync(path, "utf-8");
    if (!source.includes(from)) return `${file} does not contain ${JSON.stringify(from)}`;
    const marker = await page.evaluate<number>("(window.__compatMarker = Math.random())");
    writeFileSync(path, source.replace(from, to));
    if (!(await waitForText(page, expectText))) return `after editing ${file}, text not visible: ${JSON.stringify(expectText)}`;
    if ((await page.evaluate<number>("window.__compatMarker")) !== marker) return `editing ${file} reloaded the page instead of updating it`;
    if (keepText && !(await waitForText(page, keepText, 500))) return `editing ${file} lost component state (${JSON.stringify(keepText)} is gone)`;
  }
  return null;
}

const CHECK_TIMEOUT_MS = 30_000;
const results: Result[] = [];
const metrics: Record<string, unknown>[] = [];
const browser = await Browser.launch();

try {
  for (const app of apps) {
    const appDir = join(appsDir, app);
    // `target: "zalo"` runs the app as a Zalo Mini App (client-rendered from the backend's payload)
    const { checks, target } = readJson<{ checks: Check[]; target?: "web" | "zalo" }>(join(appDir, "compat.json"));
    const dir = copyToTemp(appDir, app);
    let server: DevServer | null = null;
    try {
      server = await startDev(dir, { engine, target });
      console.log(`\n▲ ${app}  (ready in ${server.readyMs}ms)`);
      const page = await browser.newPage();
      for (const check of checks) {
        let failure: string | null;
        try {
          // A hung check must not hang the whole suite
          failure = await Promise.race([
            runCheck(server, page, check),
            sleep(CHECK_TIMEOUT_MS).then(() => `timed out after ${CHECK_TIMEOUT_MS / 1000}s`),
          ]);
        } catch (err: any) {
          failure = `threw: ${err?.message || err}`;
        }
        const outcome: Outcome = check.knownGap ? (failure ? "XFAIL" : "XPASS") : failure ? "FAIL" : "PASS";
        const detail = outcome === "XFAIL" ? check.knownGap : outcome === "XPASS" ? "now passes: remove `knownGap`" : failure ?? undefined;
        results.push({ app, check: check.name, outcome, detail });
        const icon = { PASS: "✓", FAIL: "⨯", XFAIL: "○", XPASS: "!" }[outcome];
        console.log(`  ${icon} ${outcome.padEnd(5)} ${check.name}${detail ? `\n          ${detail}` : ""}`);
      }
      page.close();
      metrics.push({ app, readyMs: server.readyMs, rssMb: serverRssMb(server) });
    } catch (err: any) {
      results.push({ app, check: "start com dev", outcome: "FAIL", detail: String(err?.message || err).slice(0, 2000) });
      console.log(`\n⨯ ${app}: ${err?.message || err}`);
    } finally {
      if (server) await server.stop();
      rmSync(dir, { recursive: true, force: true });
    }
  }
} finally {
  browser.close();
}

const count = (o: Outcome) => results.filter((r) => r.outcome === o).length;
console.log(`\n${count("PASS")} passed, ${count("FAIL")} failed, ${count("XFAIL")} known gaps, ${count("XPASS")} unexpectedly passing`);
if (jsonOut) writeFileSync(jsonOut, JSON.stringify({ results, metrics }, null, 2));
process.exit(count("FAIL") > 0 ? 1 : 0);

