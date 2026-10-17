"use client";
import { QueryClient, QueryClientProvider, useQuery } from "@tanstack/react-query";
import { useState } from "react";

function Answer() {
  const { data, isPending } = useQuery({ queryKey: ["answer"], queryFn: async () => "Query resolved: 42" });
  return <p id="query-result">{isPending ? "Loading query..." : data}</p>;
}

export function QueryDemo() {
  const [client] = useState(() => new QueryClient());
  return (
    <QueryClientProvider client={client}>
      <Answer />
    </QueryClientProvider>
  );
}
