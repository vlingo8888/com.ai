async function loadGreeting() {
  await new Promise((r) => setTimeout(r, 10));
  return `Rendered by a Server Component on the ${typeof window === "undefined" ? "server" : "device"}`;
}

export default async function Home() {
  return <main><h1>Mini App home</h1><p>{await loadGreeting()}</p></main>;
}
