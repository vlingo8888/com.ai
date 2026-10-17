"use client";
import { useState } from "react";

// State living in the root layout: client navigations must keep it (the layout is not re-rendered)
export function LayoutCounter() {
  const [clicks, setClicks] = useState(0);
  return (
    <button id="layout-counter" onClick={() => setClicks((c) => c + 1)}>
      Layout clicks: {clicks}
    </button>
  );
}
