import Image from "next/image";
import Link from "next/link";
import { Counter } from "./counter";

export default function Home() {
  return (
    <main className="flex min-h-screen flex-col items-center gap-8 p-24">
      <Image src="/next.svg" alt="Next.js logo" width={180} height={38} priority />
      <ol className="list-inside list-decimal text-sm">
        <li>Get started by editing app/page.tsx.</li>
        <li>Save and see your changes instantly.</li>
      </ol>
      <Counter />
      <Link href="/about" className="rounded-full bg-black px-5 py-3 text-white">
        About this app
      </Link>
    </main>
  );
}
