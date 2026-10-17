import React from "react";
// The client runtime keeps the App Router location here (virtual in Zalo Mini Apps)
const nataLocation = () => {
  if (typeof window === "undefined") return { pathname: "/", search: "" };
  return window.__NATA_LOCATION__ || window.location;
};

export const useRouter = () => {
  const [pathname, setPathname] = React.useState(() => nataLocation().pathname);
  const [search, setSearch] = React.useState(() => nataLocation().search);
  const [params, setParams] = React.useState(() => (typeof window !== "undefined" && window.__NATA_PARAMS__) ? window.__NATA_PARAMS__ : {});
  React.useEffect(() => {
    const handler = (e) => {
      setPathname(nataLocation().pathname);
      setSearch(nataLocation().search);
      if (e && e.detail && e.detail.params) {
        setParams(e.detail.params);
      } else if (typeof window !== "undefined" && window.__NATA_PARAMS__) {
        setParams(window.__NATA_PARAMS__);
      }
    };
    window.addEventListener("popstate", handler);
    window.addEventListener("_nata_navigate", handler);
    return () => {
      window.removeEventListener("popstate", handler);
      window.removeEventListener("_nata_navigate", handler);
    };
  }, []);

  return {
    push: (url) => {
      if (typeof window !== "undefined" && typeof window.__NATA_NAVIGATE__ === "function") {
        window.__NATA_NAVIGATE__(url);
      } else if (typeof window !== "undefined") {
        window.location.href = url;
      }
    },
    replace: (url) => {
      if (typeof window !== "undefined" && typeof window.__NATA_NAVIGATE__ === "function") {
        window.__NATA_NAVIGATE__(url, { replace: true });
      } else if (typeof window !== "undefined") {
        window.location.replace(url);
      }
    },
    back: () => { if (typeof window !== "undefined") window.history.back(); },
    forward: () => { if (typeof window !== "undefined") window.history.forward(); },
    reload: () => { if (typeof window !== "undefined") window.location.reload(); },
    pathname,
    asPath: pathname + search,
    query: { ...Object.fromEntries(new URLSearchParams(search)), ...params },
    events: { on: () => {}, off: () => {}, emit: () => {} }
  };
};
export default { useRouter };
