export default async function SlowPage() {
  await new Promise((resolve) => setTimeout(resolve, 1200));
  return <h1>Slow data arrived</h1>;
}
