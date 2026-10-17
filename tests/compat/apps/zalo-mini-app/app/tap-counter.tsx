"use client";
import { useState } from "react";

export function TapCounter() {
  const [taps, setTaps] = useState(0);
  return <button id="taps" onClick={() => setTaps((t) => t + 1)}>Taps: {taps}</button>;
}
