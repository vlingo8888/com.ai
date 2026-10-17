export default function RootLayout({ children, team, modal }: { children: React.ReactNode; team: React.ReactNode; modal: React.ReactNode }) {
  return (
    <html lang="en">
      <body>
        <main>{children}</main>
        <aside id="team">{team}</aside>
        <div id="modal">{modal}</div>
      </body>
    </html>
  );
}
