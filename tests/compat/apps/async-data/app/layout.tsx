export const metadata = { title: { default: "Async Data", template: "%s | Async Data" } };

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en">
      <body>{children}</body>
    </html>
  );
}
