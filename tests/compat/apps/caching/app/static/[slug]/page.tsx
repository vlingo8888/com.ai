export const dynamicParams = false;

export function generateStaticParams() {
  return [{ slug: "first" }, { slug: "second" }];
}

export default async function StaticPage({ params }: { params: Promise<{ slug: string }> }) {
  const { slug } = await params;
  return <p>Static {slug}</p>;
}
