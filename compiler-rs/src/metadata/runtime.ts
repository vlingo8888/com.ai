/**
 * Native Next.js Metadata Route Runtime
 * Executes robots.ts, sitemap.ts, and manifest.ts in Bun server context.
 */

const META_DELIM_START = "__NATA_META_OUT_START__";
const META_DELIM_END = "__NATA_META_OUT_END__";

interface MetadataInput {
  filePath: string;
  kind: "Robots" | "Sitemap" | "Manifest";
  baseUrl?: string;
}

function escapeXml(str: any): string {
  if (str === null || str === undefined) return "";
  return String(str)
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&apos;");
}

function formatRobots(data: any): string {
  if (!data || typeof data !== "object") return "User-agent: *\nAllow: /\n";
  const lines: string[] = [];

  const renderRule = (rule: any) => {
    if (!rule) return;
    const userAgents = Array.isArray(rule.userAgent) ? rule.userAgent : [rule.userAgent || "*"];
    for (const ua of userAgents) {
      lines.push(`User-agent: ${ua}`);
    }
    if (rule.allow) {
      const allows = Array.isArray(rule.allow) ? rule.allow : [rule.allow];
      for (const a of allows) lines.push(`Allow: ${a}`);
    }
    if (rule.disallow) {
      const disallows = Array.isArray(rule.disallow) ? rule.disallow : [rule.disallow];
      for (const d of disallows) lines.push(`Disallow: ${d}`);
    }
    if (rule.crawlDelay !== undefined) {
      lines.push(`Crawl-delay: ${rule.crawlDelay}`);
    }
    lines.push("");
  };

  if (data.rules) {
    if (Array.isArray(data.rules)) {
      data.rules.forEach(renderRule);
    } else {
      renderRule(data.rules);
    }
  } else {
    lines.push("User-agent: *");
    lines.push("Allow: /");
    lines.push("");
  }

  if (data.sitemap) {
    const sitemaps = Array.isArray(data.sitemap) ? data.sitemap : [data.sitemap];
    for (const sm of sitemaps) {
      lines.push(`Sitemap: ${sm}`);
    }
  }
  if (data.host) {
    lines.push(`Host: ${data.host}`);
  }

  return lines.join("\n").trim() + "\n";
}

function formatSitemap(data: any): string {
  const items = Array.isArray(data) ? data : [];
  const lines: string[] = [];
  lines.push('<?xml version="1.0" encoding="UTF-8"?>');
  lines.push('<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">');

  for (const item of items) {
    if (!item || !item.url) continue;
    lines.push("  <url>");
    lines.push(`    <loc>${escapeXml(item.url)}</loc>`);
    if (item.lastModified) {
      let dStr: string;
      try {
        const d = item.lastModified instanceof Date ? item.lastModified : new Date(item.lastModified);
        dStr = isNaN(d.getTime()) ? String(item.lastModified) : d.toISOString();
      } catch {
        dStr = String(item.lastModified);
      }
      lines.push(`    <lastmod>${escapeXml(dStr)}</lastmod>`);
    }
    if (item.changeFrequency) {
      lines.push(`    <changefreq>${escapeXml(item.changeFrequency)}</changefreq>`);
    }
    if (item.priority !== undefined) {
      lines.push(`    <priority>${Number(item.priority).toFixed(1)}</priority>`);
    }
    lines.push("  </url>");
  }

  lines.push("</urlset>");
  return lines.join("\n");
}

function formatManifest(data: any): string {
  return JSON.stringify(data || {}, null, 2);
}

export async function runMetadataHandler(input: MetadataInput) {
  try {
    const mod = await import(input.filePath);
    let handler = mod.default || mod.robots || mod.sitemap || mod.manifest;

    if (!handler && typeof mod === "object") {
      handler = Object.values(mod).find((v) => typeof v === "function" || (v && typeof v === "object"));
    }

    let resultData = typeof handler === "function" ? await handler({ id: 0 }) : handler;

    let content = "";
    let contentType = "text/plain; charset=utf-8";

    switch (input.kind) {
      case "Robots":
        content = formatRobots(resultData);
        contentType = "text/plain; charset=utf-8";
        break;
      case "Sitemap":
        content = formatSitemap(resultData);
        contentType = "application/xml; charset=utf-8";
        break;
      case "Manifest":
        content = formatManifest(resultData);
        contentType = "application/manifest+json; charset=utf-8";
        break;
    }

    const output = {
      content,
      content_type: contentType,
      status: 200,
      error: null,
    };
    console.log(META_DELIM_START + JSON.stringify(output) + META_DELIM_END);
    return output;
  } catch (err: any) {
    const output = {
      content: "",
      content_type: "text/plain; charset=utf-8",
      status: 500,
      error: err?.message || String(err),
    };
    console.log(META_DELIM_START + JSON.stringify(output) + META_DELIM_END);
    return output;
  }
}
