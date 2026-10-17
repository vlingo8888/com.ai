export default function Docs({ params }: { params: { parts: string[] } }) {
  return <h1>Docs path: {params.parts.join(" / ")}</h1>;
}
