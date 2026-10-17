"use client";
import { useState } from "react";
import { whoAmI } from "../actions";

export function WhoAmI() {
  const [answer, setAnswer] = useState("");
  return (
    <div>
      <button id="who" onClick={async () => setAnswer(await whoAmI())}>Who am I</button>
      <p>{answer}</p>
    </div>
  );
}
