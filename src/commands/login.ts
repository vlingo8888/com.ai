import { saveGlobalConfig, getGlobalConfig } from "../core/config";
import { logger, colors } from "../core/logger";

export interface LoginOptions {
  token?: string;
  api?: string;
  authUrl?: string;
}

export async function loginCommand(options: LoginOptions = {}) {
  logger.hero();
  console.log(`\n  ${colors.bold}${colors.indigo}◆ AUTHENTICATING WITH COM.AI.VN${colors.reset}\n`);

  let token = options.token;

  if (!token) {
    const spinner = logger.spinner("Initializing secure authentication session...");

    // 1. Create temporary local HTTP callback server
    let receivedToken: string | null = null;
    let receivedUser: any = null;
    let authServer: any;

    try {
      authServer = Bun.serve({
        port: 0, // Automatically pick an available ephemeral port
        fetch(req) {
          const url = new URL(req.url);
          if (url.pathname === "/callback") {
            const queryToken =
              url.searchParams.get("token") ||
              url.searchParams.get("access_token") ||
              url.searchParams.get("jwt") ||
              url.searchParams.get("sessionKey") ||
              url.searchParams.get("key") ||
              url.searchParams.get("apiKey");

            const rawUser = url.searchParams.get("user");
            if (rawUser) {
              try {
                receivedUser = JSON.parse(decodeURIComponent(rawUser));
              } catch {}
            }

            if (queryToken) {
              receivedToken = queryToken;
              return new Response(
                `<!DOCTYPE html>
                <html lang="en">
                  <head>
                    <meta charset="utf-8">
                    <title>Com.AI.VN - Authentication Successful</title>
                    <meta name="viewport" content="width=device-width, initial-scale=1">
                    <style>
                      * { box-sizing: border-box; margin: 0; padding: 0; }
                      body {
                        font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
                        display: flex;
                        align-items: center;
                        justify-content: center;
                        min-height: 100vh;
                        background: radial-gradient(circle at 50% 0%, #1e1b4b 0%, #090d16 80%);
                        color: #f8fafc;
                      }
                      .card {
                        text-align: center;
                        padding: 52px 40px;
                        background: rgba(17, 24, 39, 0.8);
                        backdrop-filter: blur(20px);
                        border-radius: 24px;
                        box-shadow: 0 25px 60px rgba(0,0,0,0.6), 0 0 50px rgba(99, 102, 241, 0.15);
                        border: 1px solid rgba(255, 255, 255, 0.12);
                        max-width: 460px;
                        width: 90%;
                        animation: slideUp 0.4s ease-out;
                      }
                      @keyframes slideUp {
                        from { opacity: 0; transform: translateY(20px); }
                        to { opacity: 1; transform: translateY(0); }
                      }
                      .icon-check {
                        width: 64px;
                        height: 64px;
                        background: linear-gradient(135deg, #10b981, #059669);
                        border-radius: 50%;
                        display: inline-flex;
                        align-items: center;
                        justify-content: center;
                        font-size: 32px;
                        margin-bottom: 20px;
                        box-shadow: 0 0 30px rgba(16, 185, 129, 0.3);
                      }
                      h1 {
                        font-size: 24px;
                        font-weight: 700;
                        background: linear-gradient(135deg, #38bdf8, #818cf8);
                        -webkit-background-clip: text;
                        -webkit-text-fill-color: transparent;
                        margin-bottom: 12px;
                      }
                      p { color: #94a3b8; font-size: 15px; line-height: 1.6; }
                      .badge {
                        display: inline-block;
                        margin-top: 28px;
                        padding: 8px 20px;
                        background: rgba(56, 189, 248, 0.1);
                        border: 1px solid rgba(56, 189, 248, 0.3);
                        color: #38bdf8;
                        border-radius: 9999px;
                        font-size: 13px;
                        font-weight: 600;
                        letter-spacing: 0.5px;
                      }
                    </style>
                  </head>
                  <body>
                    <div class="card">
                      <div class="icon-check">✓</div>
                      <h1>Authentication Successful</h1>
                      <p>Center Auth has granted access. You may now close this browser tab and return to your terminal.</p>
                      <div class="badge">COM.AI.VN CLI CONNECTED</div>
                    </div>
                  </body>
                </html>`,
                { headers: { "Content-Type": "text/html; charset=utf-8" } }
              );
            }

            // Fallback for OAuth hash fragments (#token=... or #access_token=...)
            return new Response(
              `<!DOCTYPE html>
              <html>
                <head><title>Processing Authentication...</title></head>
                <body style="background:#090d16;color:#fff;font-family:sans-serif;text-align:center;padding-top:60px;">
                  <p>Processing token verification...</p>
                  <script>
                    const hash = window.location.hash.substring(1);
                    if (hash) {
                      const params = new URLSearchParams(hash);
                      const t = params.get('token') || params.get('access_token') || params.get('jwt');
                      if (t) {
                        window.location.href = '/callback?token=' + encodeURIComponent(t);
                      }
                    }
                  </script>
                </body>
              </html>`,
              { headers: { "Content-Type": "text/html; charset=utf-8" } }
            );
          }
          return new Response("Invalid auth request", { status: 400 });
        },
      });

      const callbackUrl = `http://localhost:${authServer.port}/callback`;

      // Central Auth Service URL (auth.myworkbeast.com/auth/google)
      const authBase = (
        options.authUrl ||
        process.env.COM_AUTH_URL ||
        "https://auth.myworkbeast.com"
      ).replace(/\/$/, "");

      const targetLoginUrl = authBase.includes("/auth/google")
        ? `${authBase}?redirect=${encodeURIComponent(callbackUrl)}`
        : `${authBase}/auth/google?redirect=${encodeURIComponent(callbackUrl)}`;

      spinner.stop(true, "Authentication session ready");

      console.log(`\n  ${colors.darkGray}┌────────────────────────────────────────────────────────┐${colors.reset}`);
      console.log(`  ${colors.darkGray}│${colors.reset}  ${colors.bold}Opening browser for Google authentication:${colors.reset}`);
      console.log(`  ${colors.darkGray}│${colors.reset}  🔗 ${colors.cyan}${colors.underline}${targetLoginUrl}${colors.reset}`);
      console.log(`  ${colors.darkGray}└────────────────────────────────────────────────────────┘${colors.reset}\n`);

      // Automatically launch default system browser
      try {
        const openCmd =
          process.platform === "darwin"
            ? "open"
            : process.platform === "win32"
            ? "start"
            : "xdg-open";
        Bun.spawn([openCmd, targetLoginUrl]);
      } catch {
        logger.info("If the browser does not open automatically, click the URL above.");
      }

      const waitSpinner = logger.spinner("Waiting for browser confirmation...");

      // Wait for token with 120s timeout
      const startTime = Date.now();
      while (!receivedToken && Date.now() - startTime < 120000) {
        await Bun.sleep(500);
      }

      authServer.stop();

      if (!receivedToken) {
        waitSpinner.stop(false, "Authentication timed out (120s)");
        logger.error("Authentication process timed out. Please retry with `com login`.");
        process.exit(1);
      }

      waitSpinner.stop(true, "Authentication token received successfully");
      token = receivedToken;
    } catch (err: any) {
      if (authServer) authServer.stop();
      logger.error(`Authentication error: ${err.message}`);
      process.exit(1);
    }
  }

  // 2. Fetch authenticated user profile
  let userInfo = getGlobalConfig().user;
  try {
    const authServiceUrl = options.authUrl || process.env.COM_AUTH_URL || "https://auth.myworkbeast.com/auth";
    const res = await fetch(`${authServiceUrl}/me`, {
      headers: {
        Authorization: `Bearer ${token}`,
        "Content-Type": "application/json",
      },
    });
    if (res.ok) {
      userInfo = await res.json();
    }
  } catch {}

  // 3. Persist token & config
  saveGlobalConfig({
    token,
    apiUrl: options.api,
    user: userInfo,
  });

  const displayUser = userInfo?.name || userInfo?.username || userInfo?.email || "Developer";

  logger.card("SESSION DETAILS", [
    { label: "Account", value: displayUser, color: colors.bold + colors.white },
    { label: "Backend API", value: options.api || getGlobalConfig().apiUrl || "https://base.myworkbeast.com" },
    { label: "Auth Provider", value: "Google (Center Auth)", color: colors.green },
    { label: "Status", value: "● Active & Authenticated", color: colors.emerald },
  ]);

  logger.nextSteps([
    { cmd: "com clone <viewId>", desc: "Download view source code to your machine" },
    { cmd: "com whoami", desc: "View current session and identity information" },
  ]);
}

export function logoutCommand() {
  saveGlobalConfig({ token: undefined, user: undefined });
  logger.hero();
  logger.success("Successfully logged out from Com.AI.VN CLI.");
  console.log(`    ${colors.darkGray}› To authenticate again: ${colors.cyan}com login${colors.reset}\n`);
}

export function whoamiCommand() {
  const config = getGlobalConfig();
  if (!config.token) {
    logger.hero();
    logger.warn(`No active login session found.`);
    console.log(`    ${colors.darkGray}› Authenticate by running: ${colors.bold}${colors.cyan}com login${colors.reset}\n`);
    return;
  }

  logger.hero();
  const maskedToken = `${config.token.slice(0, 8)}...${config.token.slice(-6)}`;
  const displayUser = config.user?.name || config.user?.username || config.user?.email || "Authenticated User";

  logger.card("CURRENT SESSION INFO", [
    { label: "User Account", value: displayUser, color: colors.bold + colors.white },
    { label: "Backend Host", value: config.apiUrl || "https://base.myworkbeast.com" },
    { label: "Token Session", value: maskedToken, color: colors.darkGray },
    { label: "Connection", value: "● Connected", color: colors.emerald },
  ]);
}
