"use client";
import dynamic from "next/dynamic";

const Widget = dynamic(() => import("./widget"), { ssr: false, loading: () => <p>Loading widget...</p> });

export function DynamicWidget() {
  return <Widget />;
}
