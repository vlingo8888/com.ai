"use client";
import { usePathname } from "next/navigation";

export default function About() {
  return <main><h1>About docs</h1><p>pathname={usePathname()}</p></main>;
}
