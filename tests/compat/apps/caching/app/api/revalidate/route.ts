import { revalidatePath, revalidateTag } from "next/cache";

export async function GET(request: Request) {
  const url = new URL(request.url);
  const tag = url.searchParams.get("tag");
  const path = url.searchParams.get("path");
  if (tag) await revalidateTag(tag);
  if (path) await revalidatePath(path);
  return Response.json({ revalidated: true });
}
