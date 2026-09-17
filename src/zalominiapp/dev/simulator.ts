import type { ZmpAppConfig } from "./types";

/**
 * Generates an interactive Mobile Simulator Frame HTML (iPhone / Android viewport)
 * with status bar, header, and reload controls.
 */
export function createSimulatorHtml(
  targetUrl: string,
  config: ZmpAppConfig,
  appId: string,
  deepLinkUrl: string
): string {
  const headerTitle = config.app?.headerTitle || config.app?.title || "Zalo Mini App";
  const statusBar = config.app?.statusBar || "transparent";
  const customColor = config.app?.actionBar?.customColor || "#0068ff";

  return `<!DOCTYPE html>
<html lang="vi">
<head>
  <meta charset="UTF-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1.0" />
  <title>Zalo Mini App Simulator - ${headerTitle}</title>
  <link rel="preconnect" href="https://fonts.googleapis.com">
  <link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
  <link href="https://fonts.googleapis.com/css2?family=Plus+Jakarta+Sans:wght@400;500;600;700;800&display=swap" rel="stylesheet">
  <style>
    * {
      box-sizing: border-box;
      margin: 0;
      padding: 0;
    }
    body {
      font-family: 'Plus Jakarta Sans', -apple-system, BlinkMacSystemFont, sans-serif;
      background: radial-gradient(circle at 50% 20%, #171d2b 0%, #0b0f17 100%);
      color: #e2e8f0;
      min-height: 100vh;
      display: flex;
      flex-direction: column;
      align-items: center;
      justify-content: center;
      overflow-x: hidden;
      padding: 20px;
    }
    .top-bar {
      display: flex;
      align-items: center;
      justify-content: space-between;
      width: 100%;
      max-width: 900px;
      margin-bottom: 20px;
      padding: 12px 20px;
      background: rgba(30, 41, 59, 0.7);
      backdrop-filter: blur(12px);
      border: 1px solid rgba(255, 255, 255, 0.1);
      border-radius: 16px;
      box-shadow: 0 8px 32px rgba(0, 0, 0, 0.3);
    }
    .logo-group {
      display: flex;
      align-items: center;
      gap: 10px;
    }
    .logo-badge {
      background: linear-gradient(135deg, #0068ff, #0099ff);
      color: white;
      font-weight: 800;
      font-size: 13px;
      padding: 4px 10px;
      border-radius: 8px;
      letter-spacing: 0.5px;
    }
    .app-title {
      font-weight: 700;
      font-size: 15px;
      color: #f8fafc;
    }
    .app-id {
      font-size: 12px;
      color: #94a3b8;
    }
    .controls {
      display: flex;
      gap: 8px;
    }
    .btn {
      background: rgba(255, 255, 255, 0.08);
      border: 1px solid rgba(255, 255, 255, 0.15);
      color: #f1f5f9;
      padding: 6px 14px;
      border-radius: 8px;
      font-size: 13px;
      font-weight: 600;
      cursor: pointer;
      transition: all 0.2s ease;
      display: inline-flex;
      align-items: center;
      gap: 6px;
      text-decoration: none;
    }
    .btn:hover {
      background: rgba(255, 255, 255, 0.18);
      transform: translateY(-1px);
    }
    .btn-primary {
      background: #0068ff;
      border-color: #0068ff;
    }
    .btn-primary:hover {
      background: #0052cc;
    }
    .main-container {
      display: flex;
      gap: 32px;
      align-items: center;
      justify-content: center;
      flex-wrap: wrap;
    }
    /* Phone Shell */
    .phone-mockup {
      width: 390px;
      height: 800px;
      background: #000000;
      border-radius: 48px;
      box-shadow: 0 25px 60px -15px rgba(0, 0, 0, 0.7), 0 0 0 10px #1e293b, 0 0 0 12px rgba(255, 255, 255, 0.1);
      position: relative;
      overflow: hidden;
      display: flex;
      flex-direction: column;
    }
    /* Dynamic Island / Notch */
    .notch {
      position: absolute;
      top: 10px;
      left: 50%;
      transform: translateX(-50%);
      width: 120px;
      height: 28px;
      background: #000;
      border-radius: 20px;
      z-index: 100;
      display: flex;
      align-items: center;
      justify-content: flex-end;
      padding-right: 12px;
    }
    .notch-camera {
      width: 10px;
      height: 10px;
      border-radius: 50%;
      background: #111e38;
      border: 1px solid #1e293b;
    }
    /* Status Bar */
    .mobile-status-bar {
      height: 44px;
      padding: 0 24px;
      display: flex;
      align-items: center;
      justify-content: space-between;
      font-size: 12px;
      font-weight: 600;
      color: #ffffff;
      background: ${customColor};
      z-index: 90;
      user-select: none;
    }
    /* ZMP Mock Header */
    .zmp-header {
      height: 48px;
      background: ${customColor};
      display: flex;
      align-items: center;
      justify-content: space-between;
      padding: 0 16px;
      color: #ffffff;
      border-bottom: 1px solid rgba(255, 255, 255, 0.1);
      z-index: 90;
    }
    .zmp-header-title {
      font-weight: 700;
      font-size: 15px;
      white-space: nowrap;
      overflow: hidden;
      text-overflow: ellipsis;
    }
    .zmp-capsule {
      background: rgba(0, 0, 0, 0.2);
      border: 1px solid rgba(255, 255, 255, 0.2);
      border-radius: 20px;
      display: flex;
      align-items: center;
      padding: 4px 8px;
      gap: 8px;
      font-size: 12px;
    }
    /* Screen Iframe */
    .screen-iframe {
      flex: 1;
      width: 100%;
      height: 100%;
      border: none;
      background: #ffffff;
    }
    /* Home Indicator */
    .home-bar {
      position: absolute;
      bottom: 8px;
      left: 50%;
      transform: translateX(-50%);
      width: 130px;
      height: 4px;
      background: rgba(255, 255, 255, 0.6);
      border-radius: 4px;
      z-index: 100;
      pointer-events: none;
    }
  </style>
</head>
<body>
  <div class="top-bar">
    <div class="logo-group">
      <div class="logo-badge">ZMP DEV</div>
      <div>
        <div class="app-title">${headerTitle}</div>
        <div class="app-id">App ID: ${appId} &bull; Engine: High-Performance Rust & Bun</div>
      </div>
    </div>
    <div class="controls">
      <button class="btn" onclick="document.getElementById('app-frame').contentWindow.location.reload()">
        🔄 Làm mới
      </button>
      <a class="btn btn-primary" href="${targetUrl}" target="_blank">
        ↗ Mở Tab Riêng
      </a>
    </div>
  </div>

  <div class="main-container">
    <div class="phone-mockup">
      <div class="notch"><div class="notch-camera"></div></div>
      <div class="mobile-status-bar">
        <span>9:41</span>
        <span>5G &bull; 100%</span>
      </div>
      <div class="zmp-header">
        <div class="zmp-header-title">${headerTitle}</div>
        <div class="zmp-capsule">
          <span>&bull;&bull;&bull;</span>
          <span>&times;</span>
        </div>
      </div>
      <iframe id="app-frame" class="screen-iframe" src="${targetUrl}"></iframe>
      <div class="home-bar"></div>
    </div>
  </div>
</body>
</html>`;
}
