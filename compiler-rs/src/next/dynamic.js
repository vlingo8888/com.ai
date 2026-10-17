// next/dynamic: lazy-loaded components with an optional loading UI; `ssr: false` renders the loading
// UI on the server and loads the component in the browser after hydration. Same module in the
// browser and on the server.

import * as React from "react";

const h = React.createElement;

export default function dynamic(loaderOrOptions, options = {}) {
  const opts = typeof loaderOrOptions === "function" ? options : { ...loaderOrOptions, ...options };
  const loader = typeof loaderOrOptions === "function" ? loaderOrOptions : opts.loader;
  if (typeof loader !== "function") throw new Error("next/dynamic: pass a function returning import(...)");
  const Lazy = React.lazy(() =>
    Promise.resolve(loader()).then((mod) => ({ default: mod && typeof mod === "object" && "default" in mod ? mod.default : mod })),
  );
  const Loading = opts.loading;
  const fallback = Loading ? h(Loading, { isLoading: true, pastDelay: true, error: null }) : null;

  function DynamicComponent(props) {
    if (opts.ssr === false) {
      if (typeof React.useState !== "function") {
        throw new Error("`ssr: false` is not allowed with `next/dynamic` in Server Components. Use it in a Client Component.");
      }
      const [mounted, setMounted] = React.useState(false);
      React.useEffect(() => setMounted(true), []);
      if (!mounted) return fallback;
    }
    return h(React.Suspense, { fallback }, h(Lazy, props));
  }
  DynamicComponent.displayName = "LoadableComponent";
  return DynamicComponent;
}
