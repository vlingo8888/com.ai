/**
 * Next.js metadata: `metadata` / `generateMetadata` merged down the layout chain (title templates,
 * Open Graph, Twitter, icons, robots, alternates...) and rendered to `<head>` tags. Shared by the
 * React Server Components renderer and the client-rendered fallback pipeline.
 */

export const escapeHtml = (str: any): string => {
  if (str === null || str === undefined) return "";
  return String(str)
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#039;");
};

export const generateHeadHtml = (resolvedTitle: string | null, meta: Record<string, any>): string => {
  const tags: string[] = [];
  if (resolvedTitle) {
    tags.push(`<title>${escapeHtml(resolvedTitle)}</title>`);
  }
  if (meta.description) {
    tags.push(`<meta name="description" content="${escapeHtml(meta.description)}">`);
  }
  if (meta.applicationName) {
    tags.push(`<meta name="application-name" content="${escapeHtml(meta.applicationName)}">`);
  }
  if (meta.generator) {
    tags.push(`<meta name="generator" content="${escapeHtml(meta.generator)}">`);
  }
  if (meta.keywords) {
    const kw = Array.isArray(meta.keywords) ? meta.keywords.join(", ") : String(meta.keywords);
    tags.push(`<meta name="keywords" content="${escapeHtml(kw)}">`);
  }
  if (meta.referrer) {
    tags.push(`<meta name="referrer" content="${escapeHtml(meta.referrer)}">`);
  }
  if (meta.creator) {
    tags.push(`<meta name="creator" content="${escapeHtml(meta.creator)}">`);
  }
  if (meta.publisher) {
    tags.push(`<meta name="publisher" content="${escapeHtml(meta.publisher)}">`);
  }
  if (meta.themeColor) {
    if (typeof meta.themeColor === "string") {
      tags.push(`<meta name="theme-color" content="${escapeHtml(meta.themeColor)}">`);
    } else if (Array.isArray(meta.themeColor)) {
      for (const tc of meta.themeColor) {
        if (tc && tc.color) {
          const media = tc.media ? ` media="${escapeHtml(tc.media)}"` : "";
          tags.push(`<meta name="theme-color"${media} content="${escapeHtml(tc.color)}">`);
        }
      }
    }
  }
  if (meta.robots) {
    let robotsStr = "";
    if (typeof meta.robots === "string") {
      robotsStr = meta.robots;
    } else if (typeof meta.robots === "object") {
      const parts: string[] = [];
      parts.push(meta.robots.index === false ? "noindex" : "index");
      parts.push(meta.robots.follow === false ? "nofollow" : "follow");
      if (meta.robots.nocache) parts.push("nocache");
      if (meta.robots.noarchive) parts.push("noarchive");
      if (meta.robots.noimageindex) parts.push("noimageindex");
      robotsStr = parts.join(", ");
    }
    if (robotsStr) {
      tags.push(`<meta name="robots" content="${escapeHtml(robotsStr)}">`);
    }
  }
  if (meta.alternates && typeof meta.alternates === "object") {
    if (meta.alternates.canonical) {
      tags.push(`<link rel="canonical" href="${escapeHtml(meta.alternates.canonical)}">`);
    }
    if (meta.alternates.languages && typeof meta.alternates.languages === "object") {
      for (const [lang, url] of Object.entries(meta.alternates.languages)) {
        tags.push(`<link rel="alternate" hreflang="${escapeHtml(lang)}" href="${escapeHtml(url)}">`);
      }
    }
  }
  if (meta.manifest) {
    tags.push(`<link rel="manifest" href="${escapeHtml(meta.manifest)}">`);
  }
  if (meta.icons) {
    if (typeof meta.icons === "string") {
      tags.push(`<link rel="icon" href="${escapeHtml(meta.icons)}">`);
    } else if (typeof meta.icons === "object") {
      const renderIconGroup = (rel: string, val: any) => {
        if (!val) return;
        if (typeof val === "string") {
          tags.push(`<link rel="${rel}" href="${escapeHtml(val)}">`);
        } else if (Array.isArray(val)) {
          for (const item of val) {
            if (typeof item === "string") {
              tags.push(`<link rel="${rel}" href="${escapeHtml(item)}">`);
            } else if (item && item.url) {
              const sizes = item.sizes ? ` sizes="${escapeHtml(item.sizes)}"` : "";
              const type = item.type ? ` type="${escapeHtml(item.type)}"` : "";
              tags.push(`<link rel="${rel}" href="${escapeHtml(item.url)}"${sizes}${type}>`);
            }
          }
        } else if (val.url) {
          const sizes = val.sizes ? ` sizes="${escapeHtml(val.sizes)}"` : "";
          const type = val.type ? ` type="${escapeHtml(val.type)}"` : "";
          tags.push(`<link rel="${rel}" href="${escapeHtml(val.url)}"${sizes}${type}>`);
        }
      };
      renderIconGroup("icon", meta.icons.icon);
      renderIconGroup("apple-touch-icon", meta.icons.apple);
      renderIconGroup("shortcut icon", meta.icons.shortcut);
    }
  }
  if (meta.openGraph && typeof meta.openGraph === "object") {
    const og = meta.openGraph;
    const ogTitle = og.title || resolvedTitle;
    if (ogTitle) tags.push(`<meta property="og:title" content="${escapeHtml(ogTitle)}">`);
    const ogDesc = og.description || meta.description;
    if (ogDesc) tags.push(`<meta property="og:description" content="${escapeHtml(ogDesc)}">`);
    if (og.url) tags.push(`<meta property="og:url" content="${escapeHtml(og.url)}">`);
    if (og.siteName) tags.push(`<meta property="og:site_name" content="${escapeHtml(og.siteName)}">`);
    tags.push(`<meta property="og:type" content="${escapeHtml(og.type || "website")}">`);
    if (og.locale) tags.push(`<meta property="og:locale" content="${escapeHtml(og.locale)}">`);

    if (og.images) {
      const renderOgImg = (img: any) => {
        if (typeof img === "string") {
          tags.push(`<meta property="og:image" content="${escapeHtml(img)}">`);
        } else if (img && typeof img === "object") {
          if (img.url) tags.push(`<meta property="og:image" content="${escapeHtml(img.url)}">`);
          if (img.width) tags.push(`<meta property="og:image:width" content="${escapeHtml(img.width)}">`);
          if (img.height) tags.push(`<meta property="og:image:height" content="${escapeHtml(img.height)}">`);
          if (img.alt) tags.push(`<meta property="og:image:alt" content="${escapeHtml(img.alt)}">`);
        }
      };
      if (Array.isArray(og.images)) {
        og.images.forEach(renderOgImg);
      } else {
        renderOgImg(og.images);
      }
    }
  }
  if (meta.twitter && typeof meta.twitter === "object") {
    const tw = meta.twitter;
    tags.push(`<meta name="twitter:card" content="${escapeHtml(tw.card || "summary_large_image")}">`);
    const twTitle = tw.title || meta.openGraph?.title || resolvedTitle;
    if (twTitle) tags.push(`<meta name="twitter:title" content="${escapeHtml(twTitle)}">`);
    const twDesc = tw.description || meta.openGraph?.description || meta.description;
    if (twDesc) tags.push(`<meta name="twitter:description" content="${escapeHtml(twDesc)}">`);
    if (tw.site) tags.push(`<meta name="twitter:site" content="${escapeHtml(tw.site)}">`);
    if (tw.creator) tags.push(`<meta name="twitter:creator" content="${escapeHtml(tw.creator)}">`);
    if (tw.images) {
      const imgs = Array.isArray(tw.images) ? tw.images : [tw.images];
      for (const img of imgs) {
        const url = typeof img === "string" ? img : img?.url;
        if (url) tags.push(`<meta name="twitter:image" content="${escapeHtml(url)}">`);
      }
    }
  }
  if (meta.other && typeof meta.other === "object") {
    for (const [k, v] of Object.entries(meta.other)) {
      if (v !== undefined && v !== null) {
        if (Array.isArray(v)) {
          for (const item of v) {
            tags.push(`<meta name="${escapeHtml(k)}" content="${escapeHtml(item)}">`);
          }
        } else {
          tags.push(`<meta name="${escapeHtml(k)}" content="${escapeHtml(v)}">`);
        }
      }
    }
  }
  return tags.join("\n  ");
};

export const resolveCascadeMetadata = async (modules: any[], routeContext: { params: any; searchParams: any }) => {
  let mergedMeta: Record<string, any> = {};
  let titleTemplate: string | null = null;
  let resolvedTitle: string | null = null;

  for (const mod of modules) {
    if (!mod) continue;
    let meta: any = null;
    if (typeof mod.generateMetadata === "function") {
      try {
        meta = await mod.generateMetadata(routeContext, Promise.resolve(mergedMeta));
      } catch (err) {
        console.warn("[NATA Metadata] Error in generateMetadata:", err);
      }
    } else if (mod.metadata) {
      meta = typeof mod.metadata === "function" ? await mod.metadata() : mod.metadata;
    }

    if (!meta || typeof meta !== "object") continue;

    if (meta.title !== undefined) {
      if (typeof meta.title === "string") {
        if (titleTemplate) {
          resolvedTitle = titleTemplate.includes("%s") ? titleTemplate.replace("%s", meta.title) : meta.title;
        } else {
          resolvedTitle = meta.title;
        }
      } else if (typeof meta.title === "object" && meta.title !== null) {
        if (meta.title.absolute) {
          resolvedTitle = meta.title.absolute;
        } else if (meta.title.default) {
          if (titleTemplate) {
            resolvedTitle = titleTemplate.includes("%s") ? titleTemplate.replace("%s", meta.title.default) : meta.title.default;
          } else {
            resolvedTitle = meta.title.default;
          }
        }
        if (meta.title.template) {
          titleTemplate = meta.title.template;
        }
      }
    }

    for (const key of ["description", "applicationName", "generator", "referrer", "creator", "publisher", "manifest", "themeColor"]) {
      if (meta[key] !== undefined) {
        mergedMeta[key] = meta[key];
      }
    }
    if (meta.keywords !== undefined) mergedMeta.keywords = meta.keywords;
    if (meta.authors !== undefined) mergedMeta.authors = meta.authors;
    if (meta.icons !== undefined) mergedMeta.icons = meta.icons;
    if (meta.robots !== undefined) mergedMeta.robots = meta.robots;

    if (meta.openGraph && typeof meta.openGraph === "object") {
      mergedMeta.openGraph = { ...(mergedMeta.openGraph || {}), ...meta.openGraph };
    }
    if (meta.twitter && typeof meta.twitter === "object") {
      mergedMeta.twitter = { ...(mergedMeta.twitter || {}), ...meta.twitter };
    }
    if (meta.alternates && typeof meta.alternates === "object") {
      mergedMeta.alternates = { ...(mergedMeta.alternates || {}), ...meta.alternates };
    }
    if (meta.other && typeof meta.other === "object") {
      mergedMeta.other = { ...(mergedMeta.other || {}), ...meta.other };
    }
  }

  if (resolvedTitle) {
    mergedMeta.title = resolvedTitle;
  }

  const headHtml = generateHeadHtml(resolvedTitle, mergedMeta);
  return { resolvedTitle, mergedMeta, headHtml };
};

