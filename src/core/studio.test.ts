import { afterEach, describe, expect, spyOn, test } from "bun:test";
import { STUDIO_PREFIX, studioEvent } from "./studio";

describe("studioEvent", () => {
  const original = process.env.COM_STUDIO;
  afterEach(() => {
    if (original === undefined) delete process.env.COM_STUDIO;
    else process.env.COM_STUDIO = original;
  });

  test("prints one JSON line under COM Studio", () => {
    process.env.COM_STUDIO = "1";
    const log = spyOn(console, "log").mockImplementation(() => {});
    studioEvent({ type: "zalo-qr", url: "https://zalo.me/app/link/zapps/1/?env=TESTING_LOCAL&clientIp=https://a.123c.vn", appId: "1", clientIp: "https://a.123c.vn" });
    expect(log).toHaveBeenCalledTimes(1);
    const line = String(log.mock.calls[0][0]);
    expect(line.startsWith(STUDIO_PREFIX)).toBe(true);
    expect(JSON.parse(line.slice(STUDIO_PREFIX.length)).type).toBe("zalo-qr");
    log.mockRestore();
  });

  test("prints nothing in a normal terminal", () => {
    delete process.env.COM_STUDIO;
    const log = spyOn(console, "log").mockImplementation(() => {});
    studioEvent({ type: "tunnel-url-changed", label: "Zalo", url: "https://b", previous: "https://a" });
    expect(log).not.toHaveBeenCalled();
    log.mockRestore();
  });
});
