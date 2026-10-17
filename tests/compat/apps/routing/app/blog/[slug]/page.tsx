export function generateMetadata({ params }: { params: { slug: string } }) {
  return { title: `Blog: ${params.slug}` };
}

export default function BlogPost({ params }: { params: { slug: string } }) {
  return <h1>Post slug: {params.slug}</h1>;
}
