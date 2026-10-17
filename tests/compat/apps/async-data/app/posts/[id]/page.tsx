import { notFound } from "next/navigation";
import { getPost } from "../../../lib/posts";

export async function generateMetadata({ params }: { params: Promise<{ id: string }> }) {
  const { id } = await params;
  return { title: `Post ${id}` };
}

export default async function PostPage({ params }: { params: Promise<{ id: string }> }) {
  const { id } = await params;
  const post = await getPost(id);
  if (!post) notFound();
  return <article><h1>{post.title}</h1></article>;
}
