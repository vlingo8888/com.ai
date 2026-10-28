/**
 * Machine-readable events for COM Studio (the desktop app), which sets `COM_STUDIO=1` when it runs
 * `com dev`. Each event is one stdout line `::com-studio::{json}`; the app reads it instead of parsing
 * the human output (QR art, coloured banners). Nothing is printed outside the studio.
 */
export const STUDIO_PREFIX = "::com-studio::";

export type StudioEvent =
  /** The link the Zalo QR code encodes (open the mini app on a phone) */
  | { type: "zalo-qr"; url: string; appId: string; clientIp: string }
  /** A tunnel reconnected on a new public URL: links printed before it no longer work */
  | { type: "tunnel-url-changed"; label: string; url: string; previous: string };

export const inStudio = () => process.env.COM_STUDIO === "1";

export function studioEvent(event: StudioEvent): void {
  if (inStudio()) console.log(`${STUDIO_PREFIX}${JSON.stringify(event)}`);
}
