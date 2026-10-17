import React from "react";

function hrefString(href) {
  if (typeof href === "string") return href;
  if (!href || typeof href !== "object") return "#";
  const query = href.query && typeof href.query === "object" ? new URLSearchParams(href.query).toString() : "";
  return (href.pathname || "") + (query ? "?" + query : "") + (href.hash ? (href.hash.startsWith("#") ? href.hash : "#" + href.hash) : "");
}

const isInternal = (url, target) =>
  url && !target && !/^(https?:)?\/\//.test(url) && !url.startsWith("mailto:") && !url.startsWith("tel:") && !url.startsWith("#");

// Like Next.js, links prefetch automatically only in production builds (`__NATA_CONFIG__.prefetch`):
// in development a prefetch would run the whole page on every link in view. `prefetch={true}` and
// router.prefetch() always work.
const autoPrefetch = (prefetch) =>
  prefetch === true || (prefetch !== false && typeof window !== "undefined" && !!(window.__NATA_CONFIG__ || {}).prefetch);

// next.config basePath, prefixed to app paths
const withBasePath = (url) => {
  // Server rendering: set per request by the worker
  const base = (typeof window !== "undefined" ? (window.__NATA_CONFIG__ || {}).basePath : globalThis.__NATA_BASE_PATH__) || "";
  return base && url.startsWith("/") && !url.startsWith("//") && url !== base && !url.startsWith(base + "/") ? base + url : url;
};

export default function Link({ href, children, onClick, onMouseEnter, target, replace, scroll, prefetch, shallow, passHref, legacyBehavior, locale, ...props }) {
  const url = withBasePath(hrefString(href));
  const ref = React.useRef(null);
  // Prefetch when the link scrolls into view and the browser is idle (the router skips Save-Data/2G)
  React.useEffect(() => {
    const router = typeof window !== "undefined" ? window.__NATA_ROUTER__ : null;
    if (!autoPrefetch(prefetch) || !router || !isInternal(url, target) || typeof IntersectionObserver === "undefined" || !ref.current) return;
    const observer = new IntersectionObserver((entries) => {
      if (!entries.some((e) => e.isIntersecting)) return;
      observer.disconnect();
      (window.requestIdleCallback || setTimeout)(() => router.prefetch(url));
    });
    observer.observe(ref.current);
    return () => observer.disconnect();
  }, [url, prefetch, target]);
  const handleClick = (e) => {
    if (onClick) onClick(e);
    if (e.defaultPrevented || !isInternal(url, target) || e.button !== 0 || e.metaKey || e.ctrlKey || e.shiftKey || e.altKey) return;
    e.preventDefault();
    if (typeof window.__NATA_NAVIGATE__ === "function") window.__NATA_NAVIGATE__(url, { replace, scroll: scroll !== false });
    else if (replace) window.location.replace(url);
    else window.location.href = url;
  };
  const handleMouseEnter = (e) => {
    if (onMouseEnter) onMouseEnter(e);
    if (autoPrefetch(prefetch) && window.__NATA_ROUTER__ && isInternal(url, target)) window.__NATA_ROUTER__.prefetch(url);
  };
  return React.createElement("a", { ...props, ref, href: url, target, onClick: handleClick, onMouseEnter: handleMouseEnter }, children);
}
export { Link };
