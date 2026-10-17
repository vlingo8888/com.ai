export interface Post {
  id: string;
  title: string;
}

const POSTS: Post[] = [
  { id: "1", title: "Hello from an async component" },
  { id: "2", title: "Streaming with Suspense" },
];

export async function getPosts(): Promise<Post[]> {
  await new Promise((resolve) => setTimeout(resolve, 20));
  return POSTS;
}

export async function getPost(id: string): Promise<Post | undefined> {
  await new Promise((resolve) => setTimeout(resolve, 20));
  return POSTS.find((p) => p.id === id);
}
