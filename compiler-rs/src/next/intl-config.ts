// next-intl's request config (`next-intl/config`) for a project without one (no i18n/request.ts): the
// package's own file throws "Couldn't find next-intl config file", which crashed every server render of
// <NextIntlClientProvider> not given all of its props (it reads formats / now from this config). The
// locale and messages come from the next-intl/server shim (cookie, `[locale]` param, messages/*.json).

import { getLocale, getMessages } from "./next-intl-server.ts";

export default async function requestConfig(params?: { locale?: string }) {
  const locale = params?.locale || (await getLocale());
  let messages: Record<string, unknown> = {};
  try {
    messages = await getMessages({ locale });
  } catch {}
  return { locale, messages };
}
