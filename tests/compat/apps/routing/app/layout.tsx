import Link from "next/link";

export const metadata = { title: { default: "Routing", template: "%s · Routing" } };

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en">
      <body>
        <nav>
          <Link href="/blog/hello-world">Blog</Link> <Link href="/docs/getting/started/install">Docs</Link> <Link href="/pricing">Pricing</Link>
        </nav>
        {children}
      </body>
    </html>
  );
}
