// next-intl/server for `[locale]` routes: getLocale, getMessages, getTranslations (ICU plural /
// select / selectordinal, rich text), getFormatter, setRequestLocale, getRequestConfig. Messages come
// from the project's request config (`i18n/request.ts`, next-intl's `getRequestConfig`) or from
// `messages/<locale>.json`. Everything is per request (no cross-request locale state).

import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import * as React from "react";

const g = globalThis as any;
const cwd = () => process.cwd();

const REQUEST_CONFIGS = ["i18n/request.ts", "src/i18n/request.ts", "i18n/request.js", "src/i18n/request.js", "i18n.ts", "src/i18n.ts"];
const ROUTING_FILES = ["i18n/routing.ts", "src/i18n/routing.ts"];

async function defaultLocale(): Promise<string> {
  for (const file of ROUTING_FILES) {
    const path = join(cwd(), file);
    if (!existsSync(path)) continue;
    try {
      const mod = await import(path);
      const routing = mod.routing ?? mod.default;
      if (routing?.defaultLocale) return routing.defaultLocale;
    } catch {}
  }
  return process.env.DEFAULT_LOCALE || process.env.NEXT_PUBLIC_DEFAULT_LOCALE || "en";
}

/** The request's locale: setRequestLocale(), the `[locale]` param, next-intl's header, the cookie, the default */
export async function getLocale(): Promise<string> {
  if (g.__NATA_INTL_LOCALE__) return g.__NATA_INTL_LOCALE__;
  const param = g.__NATA_PARAMS__?.locale;
  if (typeof param === "string" && param) return param;
  const headers = g.__NATA_HEADERS__ || {};
  if (headers["x-next-intl-locale"]) return headers["x-next-intl-locale"];
  const cookie = g.__NATA_COOKIES__?.get?.("NEXT_LOCALE");
  if (cookie) return cookie;
  return defaultLocale();
}

/** Static rendering hint in next-intl; here it fixes the locale for the rest of the request */
export function setRequestLocale(locale: string) {
  g.__NATA_INTL_LOCALE__ = locale;
}
export const unstable_setRequestLocale = setRequestLocale;

export function getRequestConfig(fn: unknown) {
  return fn;
}

async function requestConfig(locale: string): Promise<{ locale?: string; messages?: Record<string, unknown>; timeZone?: string; now?: Date; formats?: unknown } | null> {
  for (const file of REQUEST_CONFIGS) {
    const path = join(cwd(), file);
    if (!existsSync(path)) continue;
    const mod = await import(path);
    const fn = mod.default ?? mod.getRequestConfig;
    if (typeof fn === "function") return (await fn({ locale, requestLocale: Promise.resolve(locale) })) ?? null;
  }
  return null;
}

export async function getMessages(opts?: { locale?: string }): Promise<Record<string, any>> {
  const locale = opts?.locale || (await getLocale());
  const config = await requestConfig(locale);
  if (config?.messages) return config.messages as Record<string, any>;
  for (const dir of ["messages", "locales", "config/locales", "public/locales"]) {
    const path = join(cwd(), dir, `${locale}.json`);
    if (existsSync(path)) return JSON.parse(readFileSync(path, "utf-8"));
  }
  return {};
}

function lookup(obj: any, path: string): any {
  return path.split(".").reduce((node, key) => (node && typeof node === "object" ? node[key] : undefined), obj);
}

// ---------- ICU message format (plural, select, selectordinal, # and {arg}) ----------

type Values = Record<string, unknown>;

function findClosing(text: string, from: number): number {
  let depth = 0;
  for (let i = from; i < text.length; i++) {
    if (text[i] === "{") depth++;
    else if (text[i] === "}") {
      depth--;
      if (depth === 0) return i;
    }
  }
  return -1;
}

export function formatIcu(message: string, values: Values, locale: string, pound?: number): string {
  let out = "";
  let i = 0;
  while (i < message.length) {
    const c = message[i];
    if (c === "'" && message[i + 1] === "'") {
      out += "'";
      i += 2;
      continue;
    }
    if (c === "#" && pound !== undefined) {
      out += new Intl.NumberFormat(locale).format(pound);
      i++;
      continue;
    }
    if (c !== "{") {
      out += c;
      i++;
      continue;
    }
    const end = findClosing(message, i);
    if (end === -1) {
      out += message.slice(i);
      break;
    }
    const body = message.slice(i + 1, end);
    const [name, type, ...rest] = body.split(",").map((p) => p.trim());
    const value = values[name];
    if (!type) {
      out += value === undefined ? `{${name}}` : value instanceof Date ? new Intl.DateTimeFormat(locale).format(value) : typeof value === "number" ? new Intl.NumberFormat(locale).format(value) : String(value);
    } else if (type === "number") {
      out += new Intl.NumberFormat(locale, rest[0] === "percent" ? { style: "percent" } : undefined).format(Number(value));
    } else if (type === "date" || type === "time") {
      out += new Intl.DateTimeFormat(locale, type === "time" ? { timeStyle: "short" } : { dateStyle: (rest[0] as any) || "medium" }).format(value as any);
    } else if (type === "plural" || type === "selectordinal" || type === "select") {
      const optionsText = body.slice(body.indexOf(",", body.indexOf(",") + 1) + 1);
      const options = new Map<string, string>();
      let j = 0;
      let offset = 0;
      while (j < optionsText.length) {
        const match = /^\s*([^\s{]+)\s*\{/.exec(optionsText.slice(j));
        if (!match) break;
        const key = match[1];
        const open = j + match[0].length - 1;
        const close = findClosing(optionsText, open);
        if (key.startsWith("offset:")) offset = Number(key.slice(7));
        options.set(key, optionsText.slice(open + 1, close));
        j = close + 1;
      }
      let chosen: string | undefined;
      if (type === "select") {
        chosen = options.get(String(value)) ?? options.get("other");
      } else {
        const n = Number(value) - offset;
        chosen = options.get(`=${Number(value)}`) ?? options.get(new Intl.PluralRules(locale, { type: type === "selectordinal" ? "ordinal" : "cardinal" }).select(n)) ?? options.get("other");
      }
      out += chosen !== undefined ? formatIcu(chosen, values, locale, type === "select" ? pound : Number(value) - offset) : "";
    } else {
      out += String(value ?? "");
    }
    i = end + 1;
  }
  return out;
}

/** `t.rich("key", { b: (chunks) => <b>{chunks}</b> })`: tags become the elements their function returns */
function rich(text: string, values: Values): React.ReactNode {
  const parts: React.ReactNode[] = [];
  const tag = /<(\w+)>([\s\S]*?)<\/\1>|<(\w+)\s*\/>/g;
  let last = 0;
  let index = 0;
  for (const m of text.matchAll(tag)) {
    if (m.index! > last) parts.push(text.slice(last, m.index));
    const name = m[1] ?? m[3];
    const render = values[name];
    const inner = m[2] !== undefined ? rich(m[2], values) : undefined;
    const node = typeof render === "function" ? render(inner) : inner;
    parts.push(React.isValidElement(node) ? React.cloneElement(node, { key: `${name}-${index++}` }) : node);
    last = m.index! + m[0].length;
  }
  if (last < text.length) parts.push(text.slice(last));
  return parts.length === 1 ? parts[0] : parts;
}

export async function getTranslations(optsOrNamespace?: string | { locale?: string; namespace?: string; messages?: Record<string, unknown> }) {
  const opts = typeof optsOrNamespace === "string" ? { namespace: optsOrNamespace } : optsOrNamespace ?? {};
  const locale = opts.locale || (await getLocale());
  const messages = (opts.messages as Record<string, any>) || (await getMessages({ locale }));
  const scope = opts.namespace ? lookup(messages, opts.namespace) ?? {} : messages;
  const full = (key: string) => (opts.namespace ? `${opts.namespace}.${key}` : key);
  const raw = (key: string) => lookup(scope, key);

  const t = (key: string, values: Values = {}) => {
    const message = raw(key);
    if (typeof message !== "string") return full(key);
    return formatIcu(message, values, locale);
  };
  t.rich = (key: string, values: Values = {}) => {
    const message = raw(key);
    if (typeof message !== "string") return full(key);
    const textValues = Object.fromEntries(Object.entries(values).filter(([, v]) => typeof v !== "function"));
    return rich(formatIcu(message, textValues, locale), values);
  };
  t.markup = (key: string, values: Values = {}) => {
    const message = raw(key);
    if (typeof message !== "string") return full(key);
    const html = formatIcu(message, Object.fromEntries(Object.entries(values).filter(([, v]) => typeof v !== "function")), locale);
    return html.replace(/<(\w+)>([\s\S]*?)<\/\1>/g, (_m, name, inner) => (typeof values[name] === "function" ? (values[name] as (c: string) => string)(inner) : inner));
  };
  t.raw = (key: string) => raw(key);
  t.has = (key: string) => raw(key) !== undefined;
  return t;
}

export async function getTimeZone(): Promise<string> {
  const config = await requestConfig(await getLocale()).catch(() => null);
  return config?.timeZone || Intl.DateTimeFormat().resolvedOptions().timeZone || "UTC";
}

export async function getNow(): Promise<Date> {
  return new Date();
}

export async function getFormatter(opts?: { locale?: string }) {
  const locale = opts?.locale || (await getLocale());
  return {
    dateTime: (date: Date | number, options?: Intl.DateTimeFormatOptions) => new Intl.DateTimeFormat(locale, options).format(date),
    number: (num: number | bigint, options?: Intl.NumberFormatOptions) => new Intl.NumberFormat(locale, options).format(num),
    relativeTime: (date: Date | number, now?: Date | number | { now?: Date | number; unit?: Intl.RelativeTimeFormatUnit }) => {
      const base = typeof now === "object" && !(now instanceof Date) ? now.now ?? Date.now() : now ?? Date.now();
      const seconds = (Number(date) - Number(base)) / 1000;
      const units: [Intl.RelativeTimeFormatUnit, number][] = [["year", 31536000], ["month", 2592000], ["week", 604800], ["day", 86400], ["hour", 3600], ["minute", 60], ["second", 1]];
      const [unit, size] = units.find(([, s]) => Math.abs(seconds) >= s) ?? ["second", 1];
      return new Intl.RelativeTimeFormat(locale, { numeric: "auto" }).format(Math.round(seconds / size), unit);
    },
    list: (items: string[], options?: Intl.ListFormatOptions) => new Intl.ListFormat(locale, options).format(items),
  };
}

export const getExtracted = () => ({});

export default { getLocale, getMessages, getTranslations, getRequestConfig, setRequestLocale, getTimeZone, getNow, getFormatter, getExtracted };
