import { describe, it, expect } from "bun:test";
import {
  loadZmpConfig,
  resolveAppId,
  generateZaloDeepLink,
  getZmpMockBridgeScript,
  createSimulatorHtml,
  DEFAULT_APP_ID,
  getLocalNetworkIp,
  startZmpDev,
  validateZaloAppId,
} from "../src/zalominiapp/dev";
import { findAvailablePort } from "../src/commands/dev";

describe("Zalo Mini App Dev Engine (zalominiapp/dev)", () => {
  it("validates Zalo Mini App ID format accurately", () => {
    // Valid cases (numeric IDs)
    expect(validateZaloAppId("2840505191283749120").valid).toBe(true);
    expect(validateZaloAppId("123456789").valid).toBe(true);
    expect(validateZaloAppId("  9988776655  ").valid).toBe(true);

    // Invalid cases
    expect(validateZaloAppId("").valid).toBe(false);
    expect(validateZaloAppId("   ").valid).toBe(false);
    expect(validateZaloAppId("abc12345").valid).toBe(false);
    expect(validateZaloAppId("123-456").valid).toBe(false);
    expect(validateZaloAppId(undefined).valid).toBe(false);
  });

  it("resolves default or custom App ID correctly", () => {
    const explicit = resolveAppId(process.cwd(), "999888777");
    expect(explicit).toBe("999888777");

    const invalid = resolveAppId(process.cwd(), "not_valid_id");
    expect(invalid).toBeNull();
  });

  it("loads ZMP config safely with fallbacks", () => {
    const config = loadZmpConfig(process.cwd());
    expect(config).toBeDefined();
    expect(config.app?.title).toBeDefined();
  });

  it("generates correct Zalo DeepLink for local testing", () => {
    const deepLink = generateZaloDeepLink("123456789", "192.168.1.10", 3000);
    expect(deepLink).toBe(
      "https://zalo.me/app/link/zapps/123456789/?env=TESTING_LOCAL&clientIp=http://192.168.1.10:3000"
    );
  });

  it("generates correct Zalo DeepLink when using public HTTPS tunnel", () => {
    const tunnelUrl = "https://tunnel-a4ece850.myworkbeast.com";
    const deepLink = generateZaloDeepLink("2071908192560425880", tunnelUrl, 3001);
    expect(deepLink).toBe(
      "https://zalo.me/app/link/zapps/2071908192560425880/?env=TESTING_LOCAL&clientIp=https://tunnel-a4ece850.myworkbeast.com"
    );
  });

  it("findAvailablePort finds an open port without colliding", async () => {
    const port = await findAvailablePort(18320);
    expect(typeof port).toBe("number");
    expect(port).toBeGreaterThanOrEqual(18320);
  });

  it("produces full in-browser ZMP Mock Bridge script", () => {
    const script = getZmpMockBridgeScript("2840505191283749120");
    expect(script).toContain("window.ZMP");
    expect(script).toContain("window.zmp");
    expect(script).toContain("getUserInfo");
    expect(script).toContain("getPhoneNumber");
    expect(script).toContain("getSystemInfo");
    expect(script).toContain("setStorage");
    expect(script).toContain("openChat");
    expect(script).toContain("2840505191283749120");
  });

  it("renders mobile simulator HTML frame", () => {
    const html = createSimulatorHtml(
      "http://localhost:3000",
      { app: { headerTitle: "My Test App" } },
      "123456789",
      "https://zalo.me/app/link/zapps/123456789"
    );
    expect(html).toContain("<!DOCTYPE html>");
    expect(html).toContain("My Test App");
    expect(html).toContain("phone-mockup");
    expect(html).toContain("http://localhost:3000");
  });

  it("detects local network IP", () => {
    const ip = getLocalNetworkIp();
    expect(typeof ip).toBe("string");
    expect(ip.length).toBeGreaterThan(0);
  });

  it("starts ZMP dev context without throwing", async () => {
    const ctx = await startZmpDev({
      cwd: process.cwd(),
      port: 8899,
      appId: "9876543210123",
    });

    expect(ctx.appId).toBe("9876543210123");
    expect(ctx.port).toBe(8899);
    expect(ctx.deepLinkUrl).toContain("9876543210123");
    expect(ctx.deepLinkUrl).toContain("8899");
  });
});
