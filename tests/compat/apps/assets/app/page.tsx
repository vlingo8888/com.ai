import Image from "next/image";
import Script from "next/script";
import photo from "./photo.png";

export default function Home() {
  return (
    <main>
      <h1>Assets</h1>
      <Image id="photo" src={photo} alt="Photo" sizes="100vw" placeholder="blur" priority />
      <Image id="avatar" src="/avatar.png" alt="Avatar" width={32} height={32} />
      <p id="script-target">script pending</p>
      <Script id="after-hydration" strategy="afterInteractive">{`document.getElementById("script-target").textContent = "script ran";`}</Script>
    </main>
  );
}
