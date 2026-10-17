import { getLocale, getTranslations } from "next-intl/server";

export default async function About() {
  const locale = await getLocale();
  const t = await getTranslations("About");
  return <p>{t("locale", { locale })}</p>;
}
