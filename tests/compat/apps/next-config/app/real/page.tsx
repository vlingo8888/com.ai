import { ClientEnv } from "./client";

export default function Real() {
  return (
    <main>
      <h1>Real page</h1>
      <p>server secret={process.env.SERVER_SECRET} custom={process.env.CUSTOM_VALUE} registered={(globalThis as any).__compatRegistered}</p>
      <ClientEnv />
    </main>
  );
}
