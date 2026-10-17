import Link from "next/link";

export default function Home() {
  return (
    <div>
      <h1>Home page</h1>
      <Link href="/photos/1">Open photo 1</Link> <Link href="/settings">Settings</Link>
    </div>
  );
}
