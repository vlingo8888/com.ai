import Link from "next/link";
import { TapCounter } from "./tap-counter";

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="vi">
      <body>
        <nav>
          <Link href="/">Home</Link> <Link href="/profile">Profile</Link> <TapCounter />
        </nav>
        {children}
      </body>
    </html>
  );
}
