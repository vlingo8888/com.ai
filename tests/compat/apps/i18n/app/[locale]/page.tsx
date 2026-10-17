import { getTranslations } from "next-intl/server";

export default async function Home() {
  const t = await getTranslations("Home");
  return (
    <main>
      <h1>{t("title")}</h1>
      <p id="zero">{t("items", { count: 0 })}</p>
      <p id="many">{t("items", { count: 3 })}</p>
      <p id="rich">{t.rich("bold", { b: (chunks) => <b>{chunks}</b> })}</p>
    </main>
  );
}
