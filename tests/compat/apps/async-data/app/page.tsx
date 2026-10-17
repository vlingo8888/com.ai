import Link from "next/link";
import { getPosts } from "../lib/posts";

export default async function Home() {
  const posts = await getPosts();
  return (
    <main>
      <h1>Posts</h1>
      <Link href="/slow">Slow page</Link>
      <ul>
        {posts.map((post) => (
          <li key={post.id}>
            <Link href={`/posts/${post.id}`}>{post.title}</Link>
          </li>
        ))}
      </ul>
    </main>
  );
}
