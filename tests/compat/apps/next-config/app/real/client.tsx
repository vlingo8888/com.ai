"use client";
export function ClientEnv() {
  return (
    <p id="client-env">
      client greeting={process.env.NEXT_PUBLIC_GREETING} secret={process.env.SERVER_SECRET ?? "none"} custom={process.env.CUSTOM_VALUE}
    </p>
  );
}
