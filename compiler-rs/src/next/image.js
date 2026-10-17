"use client";
// next/image: <img> with responsive `srcSet` served by the image optimizer (`/_next/image?url=&w=&q=`,
// resized and re-encoded by the engine), `sizes`, `fill`, `priority`, blur placeholders, static image
// imports ({ src, width, height, blurDataURL }) and `unoptimized`. Same widths as Next.js.

import * as React from "react";

const config = () => (typeof window !== "undefined" ? window.__NATA_IMAGES__ : globalThis.__NATA_IMAGES__) || {};
const DEVICE_SIZES = [640, 750, 828, 1080, 1200, 1920, 2048, 3840];
const IMAGE_SIZES = [16, 32, 48, 64, 96, 128, 256, 384];

function defaultLoader({ src, width, quality }) {
  const path = config().path || "/_next/image";
  return `${path}?url=${encodeURIComponent(src)}&w=${width}&q=${quality || 75}`;
}

function widthsFor(width, sizes) {
  const deviceSizes = config().deviceSizes || DEVICE_SIZES;
  const all = [...(config().imageSizes || IMAGE_SIZES), ...deviceSizes].sort((a, b) => a - b);
  if (sizes) {
    // Smallest vw used in `sizes` decides the smallest useful width
    const percents = [...sizes.matchAll(/(^|\s)(1?\d?\d)vw/g)].map((m) => parseInt(m[2], 10));
    if (percents.length) {
      const smallest = Math.min(...percents) * 0.01;
      return { widths: all.filter((w) => w >= deviceSizes[0] * smallest), kind: "w" };
    }
    return { widths: all, kind: "w" };
  }
  if (typeof width !== "number") return { widths: deviceSizes, kind: "w" };
  // 1x and 2x of the rendered width, rounded up to known sizes
  const widths = [...new Set([width, width * 2].map((w) => all.find((p) => p >= w) || all[all.length - 1]))];
  return { widths, kind: "x" };
}

const isStaticImport = (src) => src && typeof src === "object" && typeof (src.default ?? src).src === "string";
const toNumber = (v) => (typeof v === "number" ? v : typeof v === "string" && v !== "" ? parseInt(v, 10) : undefined);

export function getImageProps(props) {
  let { src, alt = "", width, height, fill, sizes, quality, priority, loading, placeholder, blurDataURL, unoptimized, loader, style, className, overrideSrc, onLoadingComplete, preload, ...rest } = props;
  if (isStaticImport(src)) {
    const data = src.default ?? src;
    width = width ?? data.width;
    height = height ?? data.height;
    blurDataURL = blurDataURL ?? data.blurDataURL;
    src = data.src;
  }
  width = toNumber(width);
  height = toNumber(height);
  const raw = typeof src === "string" ? src : "";
  const skip = unoptimized || config().unoptimized || raw.startsWith("data:") || raw.startsWith("blob:") || raw.endsWith(".svg");
  const load = loader || defaultLoader;
  let srcSet;
  let finalSrc = raw;
  if (!skip && raw) {
    const { widths, kind } = widthsFor(width, sizes || (fill ? "100vw" : undefined));
    srcSet = widths.map((w, i) => `${load({ src: raw, width: w, quality })} ${kind === "w" ? `${w}w` : `${i + 1}x`}`).join(", ");
    finalSrc = load({ src: raw, width: widths[widths.length - 1], quality });
  }
  const blurStyle =
    placeholder === "blur" && blurDataURL
      ? { backgroundImage: `url("${blurDataURL}")`, backgroundSize: fill ? "cover" : "cover", backgroundPosition: "50% 50%", backgroundRepeat: "no-repeat" }
      : placeholder && placeholder.startsWith?.("data:image/")
        ? { backgroundImage: `url("${placeholder}")`, backgroundSize: "cover" }
        : {};
  const fillStyle = fill ? { position: "absolute", height: "100%", width: "100%", left: 0, top: 0, right: 0, bottom: 0, objectFit: "cover" } : {};
  return {
    props: {
      ...rest,
      alt,
      width: fill ? undefined : width,
      height: fill ? undefined : height,
      decoding: "async",
      loading: priority || preload ? undefined : loading || "lazy",
      fetchPriority: priority || preload ? "high" : rest.fetchPriority,
      className,
      style: { color: "transparent", ...fillStyle, ...blurStyle, ...style },
      sizes: sizes || (fill ? "100vw" : undefined),
      srcSet,
      src: overrideSrc || finalSrc,
    },
  };
}

const Image = React.forwardRef(function Image(props, ref) {
  const { props: imgProps } = getImageProps(props);
  const { onLoad, onLoadingComplete } = props;
  return React.createElement("img", {
    ...imgProps,
    ref,
    onLoad: (event) => {
      onLoad?.(event);
      onLoadingComplete?.(event.currentTarget);
    },
  });
});

export default Image;
