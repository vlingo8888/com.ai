"use client";
// next/script: `beforeInteractive` scripts are part of the server HTML; `afterInteractive` (default)
// load once the page hydrated; `lazyOnload` (and `worker`) when the browser is idle after `load`.
// A script with the same `id` or `src` loads once per page.

import * as React from "react";

const loaded = new Set();

function inject({ src, id, onLoad, onReady, onError, children, dangerouslySetInnerHTML, strategy, ...attrs }) {
  const key = id || src;
  if (key && loaded.has(key)) {
    onReady?.();
    return;
  }
  if (key) loaded.add(key);
  const el = document.createElement("script");
  for (const [name, value] of Object.entries(attrs)) {
    if (value === undefined || value === false) continue;
    el.setAttribute(name === "className" ? "class" : name, value === true ? "" : String(value));
  }
  if (id) el.id = id;
  if (src) {
    el.src = src;
    el.addEventListener("load", (event) => {
      onLoad?.(event);
      onReady?.();
    });
    el.addEventListener("error", (event) => onError?.(event));
  } else {
    el.textContent = dangerouslySetInnerHTML?.__html ?? (typeof children === "string" ? children : Array.isArray(children) ? children.join("") : "");
  }
  document.body.appendChild(el);
  if (!src) onReady?.();
}

export default function Script(props) {
  const { strategy = "afterInteractive", src, id, children, dangerouslySetInnerHTML, onLoad, onReady, onError, ...rest } = props;
  React.useEffect(() => {
    if (strategy === "beforeInteractive") {
      if (id || src) loaded.add(id || src);
      onReady?.();
      return;
    }
    const run = () => inject(props);
    if (strategy === "lazyOnload" || strategy === "worker") {
      const idle = () => (window.requestIdleCallback ? window.requestIdleCallback(run) : setTimeout(run, 1));
      if (document.readyState === "complete") idle();
      else window.addEventListener("load", idle, { once: true });
    } else {
      run();
    }
  }, [src, id, strategy]);
  if (strategy !== "beforeInteractive") return null;
  // Part of the server-rendered HTML, so it runs before hydration
  return React.createElement("script", {
    ...rest,
    id,
    src,
    dangerouslySetInnerHTML: src ? undefined : dangerouslySetInnerHTML ?? (children ? { __html: [children].flat().join("") } : undefined),
  });
}
