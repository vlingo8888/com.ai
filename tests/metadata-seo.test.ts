import { describe, expect, it, beforeAll, afterAll } from "bun:test";
import { mkdtempSync, mkdirSync, writeFileSync, rmSync, existsSync, readFileSync } from "fs";
import { join } from "path";
import { tmpdir } from "os";

// ============================================================================
// 1. Static Metadata Extraction & Cascade Merging
// ============================================================================
describe("Metadata & SEO Engine - Static Metadata & Cascade Merging", () => {
  let tempDir: string;
  let appDir: string;
  let nataDir: string;

  beforeAll(() => {
    tempDir = mkdtempSync(join(tmpdir(), "com-meta-test-"));
    appDir = join(tempDir, "app");
    nataDir = join(tempDir, ".nata");
    mkdirSync(appDir, { recursive: true });
    mkdirSync(nataDir, { recursive: true });

    // Copy SSR runtime to .nata/
    const sourceRuntime = readFileSync(join(__dirname, "../compiler-rs/src/ssr/runtime.ts"), "utf-8");
    writeFileSync(join(nataDir, "ssr_runtime.ts"), sourceRuntime);

    // Root Layout with title template and base metadata
    writeFileSync(
      join(appDir, "layout.tsx"),
      `
export const metadata = {
  title: {
    template: "%s | Acme Corp",
    default: "Acme Corp - High Performance Framework",
  },
  description: "Next.js compatible App Router framework in Rust & Bun",
  openGraph: {
    siteName: "Acme Corp",
    type: "website",
  },
  icons: {
    icon: "/favicon.ico",
    apple: "/apple-icon.png",
  },
};

export default function RootLayout({ children }: any) {
  return children;
}
`
    );

    // Pricing Page with child metadata and canonical alternate
    const pricingDir = join(appDir, "pricing");
    mkdirSync(pricingDir, { recursive: true });
    writeFileSync(
      join(pricingDir, "page.tsx"),
      `
export const metadata = {
  title: "Simple & Transparent Pricing",
  description: "Explore flexible pricing plans for teams of all sizes.",
  alternates: {
    canonical: "https://acme.com/pricing",
  },
  openGraph: {
    images: ["https://acme.com/og/pricing.png"],
  },
  twitter: {
    card: "summary_large_image",
    creator: "@acmecorp",
  },
};

export default function PricingPage() {
  return "Pricing Page Content";
}
`
    );

    // Absolute Title Page
    const dealDir = join(appDir, "special-deal");
    mkdirSync(dealDir, { recursive: true });
    writeFileSync(
      join(dealDir, "page.tsx"),
      `
export const metadata = {
  title: {
    absolute: "Limited Flash Sale (No Brand Suffix)",
  },
  description: "Special deal page overriding layout template.",
};

export default function DealPage() {
  return "Deal Page Content";
}
`
    );
  });

  afterAll(() => {
    try {
      rmSync(tempDir, { recursive: true, force: true });
    } catch {}
  });

  it("merges metadata cascade: applies layout title template to child page title", async () => {
    const { runSsr } = await import(join(nataDir, "ssr_runtime.ts"));
    let capturedOutput: any = null;
    try {
      capturedOutput = await runSsr({
        pagePath: join(appDir, "pricing", "page.tsx"),
        layoutPaths: [join(appDir, "layout.tsx")],
        params: {},
        searchParams: {},
        cookiesStr: "",
        headers: {},
        urlPath: "/pricing",
      });
    } catch (e: any) {
      console.error("runSsr call failed:", e);
    }

    if (capturedOutput?.error) {
      console.error("Captured error:", capturedOutput.error);
    }
    expect(capturedOutput).not.toBeNull();
    expect(capturedOutput.title).toBe("Simple & Transparent Pricing | Acme Corp");
    expect(capturedOutput.metadata.description).toBe("Explore flexible pricing plans for teams of all sizes.");
    expect(capturedOutput.metadata.alternates.canonical).toBe("https://acme.com/pricing");
    expect(capturedOutput.metadata.openGraph.siteName).toBe("Acme Corp");

    // Verify injected HTML tags
    const headTags = capturedOutput.head_tags;
    expect(headTags).toContain("<title>Simple &amp; Transparent Pricing | Acme Corp</title>");
    expect(headTags).toContain('<meta name="description" content="Explore flexible pricing plans for teams of all sizes.">');
    expect(headTags).toContain('<link rel="canonical" href="https://acme.com/pricing">');
    expect(headTags).toContain('<meta property="og:site_name" content="Acme Corp">');
    expect(headTags).toContain('<meta property="og:image" content="https://acme.com/og/pricing.png">');
    expect(headTags).toContain('<meta name="twitter:card" content="summary_large_image">');
    expect(headTags).toContain('<meta name="twitter:creator" content="@acmecorp">');
    expect(headTags).toContain('<link rel="icon" href="/favicon.ico">');
    expect(headTags).toContain('<link rel="apple-touch-icon" href="/apple-icon.png">');
  });

  it("supports title.absolute to bypass layout title template", async () => {
    const { runSsr } = await import(join(nataDir, "ssr_runtime.ts"));
    let capturedOutput: any = null;

    try {
      capturedOutput = await runSsr({
        pagePath: join(appDir, "special-deal", "page.tsx"),
        layoutPaths: [join(appDir, "layout.tsx")],
        params: {},
        searchParams: {},
        cookiesStr: "",
        headers: {},
        urlPath: "/special-deal",
      });
    } catch (e: any) {
      console.error("runSsr call failed:", e);
    }

    expect(capturedOutput).not.toBeNull();
    expect(capturedOutput.title).toBe("Limited Flash Sale (No Brand Suffix)");
    expect(capturedOutput.head_tags).toContain("<title>Limited Flash Sale (No Brand Suffix)</title>");
  });
});

// ============================================================================
// 2. Dynamic generateMetadata() Engine
// ============================================================================
describe("Metadata & SEO Engine - Dynamic generateMetadata()", () => {
  let tempDir: string;
  let appDir: string;
  let nataDir: string;

  beforeAll(() => {
    tempDir = mkdtempSync(join(tmpdir(), "com-dynamic-meta-test-"));
    appDir = join(tempDir, "app");
    nataDir = join(tempDir, ".nata");
    mkdirSync(appDir, { recursive: true });
    mkdirSync(nataDir, { recursive: true });

    const sourceRuntime = readFileSync(join(__dirname, "../compiler-rs/src/ssr/runtime.ts"), "utf-8");
    writeFileSync(join(nataDir, "ssr_runtime.ts"), sourceRuntime);

    writeFileSync(
      join(appDir, "layout.tsx"),
      `export const metadata = { title: { template: "%s - Beast App", default: "Beast App" } }; export default function RootLayout({ children }: any) { return children; }`
    );

    const usersDir = join(appDir, "users", "[id]");
    mkdirSync(usersDir, { recursive: true });
    writeFileSync(
      join(usersDir, "page.tsx"),
      `
export async function generateMetadata({ params, searchParams }: any, parent: any) {
  const parentMeta = await parent;
  return {
    title: "User Profile #" + params.id,
    description: "Personal details and activity for user " + params.id,
    openGraph: {
      title: "Profile of " + params.id,
      images: ["https://example.com/avatar/" + params.id + ".jpg"],
    },
  };
}

export default function UserDetailPage({ params }: any) {
  return "User " + params.id;
}
`
    );
  });

  afterAll(() => {
    try {
      rmSync(tempDir, { recursive: true, force: true });
    } catch {}
  });

  it("calls async generateMetadata({ params, searchParams }) and merges with layout template", async () => {
    const { runSsr } = await import(join(nataDir, "ssr_runtime.ts"));
    let capturedOutput: any = null;

    try {
      capturedOutput = await runSsr({
        pagePath: join(appDir, "users", "[id]", "page.tsx"),
        layoutPaths: [join(appDir, "layout.tsx")],
        params: { id: "987" },
        searchParams: {},
        cookiesStr: "",
        headers: {},
        urlPath: "/users/987",
      });
    } catch (e: any) {
      console.error("runSsr call failed:", e);
    }

    expect(capturedOutput).not.toBeNull();
    expect(capturedOutput.title).toBe("User Profile #987 - Beast App");
    expect(capturedOutput.metadata.description).toBe("Personal details and activity for user 987");
    expect(capturedOutput.head_tags).toContain("<title>User Profile #987 - Beast App</title>");
    expect(capturedOutput.head_tags).toContain('<meta property="og:title" content="Profile of 987">');
    expect(capturedOutput.head_tags).toContain('<meta property="og:image" content="https://example.com/avatar/987.jpg">');
  });
});

// ============================================================================
// 3. Special Metadata Route Files (robots.ts, sitemap.ts, manifest.ts)
// ============================================================================
describe("Metadata & SEO Engine - Special Metadata Files", () => {
  let tempDir: string;
  let nataDir: string;
  let metaRuntimePath: string;

  beforeAll(() => {
    tempDir = mkdtempSync(join(tmpdir(), "com-special-meta-test-"));
    nataDir = join(tempDir, ".nata");
    mkdirSync(nataDir, { recursive: true });

    const sourceRuntime = readFileSync(join(__dirname, "../compiler-rs/src/metadata/runtime.ts"), "utf-8");
    metaRuntimePath = join(nataDir, "metadata_runtime.ts");
    writeFileSync(metaRuntimePath, sourceRuntime);
  });

  afterAll(() => {
    try {
      rmSync(tempDir, { recursive: true, force: true });
    } catch {}
  });

  it("evaluates robots.ts and produces formatted robots.txt plain text", async () => {
    const robotsPath = join(tempDir, "robots.ts");
    writeFileSync(
      robotsPath,
      `
export default function robots() {
  return {
    rules: [
      {
        userAgent: "*",
        allow: "/",
        disallow: ["/admin", "/private"],
      },
      {
        userAgent: "Googlebot",
        allow: "/public-crawl",
        crawlDelay: 2,
      },
    ],
    sitemap: "https://acme.com/sitemap.xml",
    host: "acme.com",
  };
}
`
    );

    const { runMetadataHandler } = await import(metaRuntimePath);
    const captured: any = await runMetadataHandler({
      filePath: robotsPath,
      kind: "Robots",
    });

    expect(captured).not.toBeNull();
    expect(captured.status).toBe(200);
    expect(captured.content_type).toBe("text/plain; charset=utf-8");
    expect(captured.content).toContain("User-agent: *");
    expect(captured.content).toContain("Allow: /");
    expect(captured.content).toContain("Disallow: /admin");
    expect(captured.content).toContain("Disallow: /private");
    expect(captured.content).toContain("User-agent: Googlebot");
    expect(captured.content).toContain("Crawl-delay: 2");
    expect(captured.content).toContain("Sitemap: https://acme.com/sitemap.xml");
    expect(captured.content).toContain("Host: acme.com");
  });

  it("evaluates sitemap.ts and produces valid XML Sitemap 0.9", async () => {
    const sitemapPath = join(tempDir, "sitemap.ts");
    writeFileSync(
      sitemapPath,
      `
export default async function sitemap() {
  return [
    {
      url: "https://acme.com",
      lastModified: new Date("2026-09-01T00:00:00.000Z"),
      changeFrequency: "daily",
      priority: 1.0,
    },
    {
      url: "https://acme.com/pricing",
      changeFrequency: "monthly",
      priority: 0.8,
    },
  ];
}
`
    );

    const { runMetadataHandler } = await import(metaRuntimePath);
    const captured: any = await runMetadataHandler({
      filePath: sitemapPath,
      kind: "Sitemap",
    });

    expect(captured).not.toBeNull();
    expect(captured.status).toBe(200);
    expect(captured.content_type).toBe("application/xml; charset=utf-8");
    expect(captured.content).toContain('<?xml version="1.0" encoding="UTF-8"?>');
    expect(captured.content).toContain('<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">');
    expect(captured.content).toContain("<loc>https://acme.com</loc>");
    expect(captured.content).toContain("<lastmod>2026-09-01T00:00:00.000Z</lastmod>");
    expect(captured.content).toContain("<changefreq>daily</changefreq>");
    expect(captured.content).toContain("<priority>1.0</priority>");
    expect(captured.content).toContain("<loc>https://acme.com/pricing</loc>");
    expect(captured.content).toContain("<priority>0.8</priority>");
    expect(captured.content).toContain("</urlset>");
  });

  it("evaluates manifest.ts and produces standard Web App Manifest JSON", async () => {
    const manifestPath = join(tempDir, "manifest.ts");
    writeFileSync(
      manifestPath,
      `
export default function manifest() {
  return {
    name: "Acme Portal Progressive Web App",
    short_name: "AcmePortal",
    description: "Next Generation Fullstack Portal",
    start_url: "/",
    display: "standalone",
    background_color: "#0f172a",
    theme_color: "#0284c7",
    icons: [
      {
        src: "/icon-192.png",
        sizes: "192x192",
        type: "image/png",
      },
    ],
  };
}
`
    );

    const { runMetadataHandler } = await import(metaRuntimePath);
    const captured: any = await runMetadataHandler({
      filePath: manifestPath,
      kind: "Manifest",
    });

    expect(captured).not.toBeNull();
    expect(captured.status).toBe(200);
    expect(captured.content_type).toBe("application/manifest+json; charset=utf-8");
    const manifestObj = JSON.parse(captured.content);
    expect(manifestObj.name).toBe("Acme Portal Progressive Web App");
    expect(manifestObj.short_name).toBe("AcmePortal");
    expect(manifestObj.display).toBe("standalone");
    expect(manifestObj.theme_color).toBe("#0284c7");
    expect(manifestObj.icons[0].src).toBe("/icon-192.png");
  });
});
