import Link from "next/link";

export const metadata = { title: "About" };

export default function About() {
  return (
    <main className="p-24">
      <h1 className="text-2xl font-semibold">About page</h1>
      <Link href="/">Back home</Link>
    </main>
  );
}
