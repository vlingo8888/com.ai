import { saveGlobalConfig, getGlobalConfig } from "../core/config";
import { logger, colors } from "../core/logger";

export interface LoginOptions {
  token?: string;
  api?: string;
  authUrl?: string;
}

const PAGE_CSS = `
  *{box-sizing:border-box;margin:0;padding:0}
  :root{--bg:#fafafa;--card:#fff;--border:#eaeaea;--fg:#171717;--muted:#666;--subtle:#8f8f8f;--ok:#0a7d33;--okbg:#e9f9ee}
  html,body{height:100%}
  body{font-family:"Geist",-apple-system,BlinkMacSystemFont,"Segoe UI",Roboto,"Helvetica Neue",Arial,sans-serif;background:var(--bg);color:var(--fg);-webkit-font-smoothing:antialiased;display:flex;flex-direction:column;min-height:100vh}
  header{height:64px;display:flex;align-items:center;padding:0 24px;border-bottom:1px solid var(--border);background:var(--card)}
  .brand{display:flex;align-items:center;gap:10px;font-weight:600;font-size:15px;letter-spacing:-.01em}
  .logo{width:22px;height:22px;border-radius:6px;background:var(--fg);color:#fff;display:grid;place-items:center;font-size:12px;font-weight:700}
  main{flex:1;display:grid;place-items:center;padding:32px 16px}
  .card{width:100%;max-width:420px;background:var(--card);border:1px solid var(--border);border-radius:12px;padding:40px 32px 32px;box-shadow:0 1px 2px rgba(0,0,0,.04);text-align:center;animation:in .35s ease-out}
  @keyframes in{from{opacity:0;transform:translateY(6px)}to{opacity:1;transform:none}}
  .check{width:48px;height:48px;border-radius:50%;background:var(--okbg);display:grid;place-items:center;margin:0 auto 20px}
  h1{font-size:22px;font-weight:600;letter-spacing:-.02em;margin-bottom:8px}
  p{color:var(--muted);font-size:14px;line-height:1.6}
  .user{margin-top:24px;display:flex;align-items:center;gap:12px;padding:12px;border:1px solid var(--border);border-radius:8px;text-align:left}
  .avatar{width:32px;height:32px;border-radius:50%;background:#f2f2f2;display:grid;place-items:center;font-size:13px;font-weight:600;color:var(--muted);overflow:hidden;flex:none}
  .avatar img{width:100%;height:100%;object-fit:cover}
  .user b{display:block;font-size:14px;font-weight:500}
  .user span{display:block;font-size:13px;color:var(--subtle);overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
  .hint{margin-top:24px;padding-top:20px;border-top:1px solid var(--border);font-size:13px;color:var(--subtle)}
  kbd{font-family:"Geist Mono",ui-monospace,SFMono-Regular,Menlo,monospace;font-size:12px;background:#f5f5f5;border:1px solid var(--border);border-radius:4px;padding:1px 6px;color:var(--fg)}
  .spin{width:20px;height:20px;border:2px solid var(--border);border-top-color:var(--fg);border-radius:50%;animation:r .7s linear infinite;margin:0 auto 16px}
  @keyframes r{to{transform:rotate(360deg)}}
  footer{padding:20px;text-align:center;font-size:12px;color:var(--subtle)}
`;

function esc(v: unknown): string {
  return String(v ?? "").replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]!));
}

function pageShell(title: string, body: string): string {
  return `<!DOCTYPE html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><title>${title}</title><link rel="preconnect" href="https://fonts.googleapis.com"><link href="https://fonts.googleapis.com/css2?family=Geist:wght@400;500;600;700&family=Geist+Mono&display=swap" rel="stylesheet"><style>${PAGE_CSS}</style></head><body><header><div class="brand"><div class="logo">C</div>com.ai.vn</div></header><main>${body}</main><footer>© com.ai.vn · Center Auth</footer></body></html>`;
}

function successPage(user: any): string {
  const name = user?.name || user?.displayName || "";
  const email = user?.email || "";
  const pic = user?.picture || user?.avatar || user?.avatarUrl || "";
  const initial = esc((name || email || "?").trim().charAt(0).toUpperCase());
  const userBlock = name || email
    ? `<div class="user"><div class="avatar">${pic ? `<img src="${esc(pic)}" alt="" referrerpolicy="no-referrer">` : initial}</div><div style="min-width:0">${name ? `<b>${esc(name)}</b>` : ""}${email ? `<span>${esc(email)}</span>` : ""}</div></div>`
    : "";
  return pageShell(
    "Signed in · com.ai.vn",
    `<div class="card"><div class="check"><svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="#0a7d33" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><path d="M20 6 9 17l-5-5"/></svg></div><h1>You're signed in</h1><p>The com CLI is now connected to your account.<br>You can close this tab and return to your terminal.</p>${userBlock}<div class="hint">Next, run <kbd>com create</kbd> in your terminal</div></div>`
  );
}

function processingPage(): string {
  return pageShell(
    "Signing in · com.ai.vn",
    `<div class="card"><div class="spin"></div><h1>Signing you in…</h1><p>Verifying your session. This only takes a moment.</p></div><script>const h=location.hash.substring(1);if(h){const q=new URLSearchParams(h);const t=q.get('token')||q.get('access_token')||q.get('jwt');if(t)location.href='/callback?token='+encodeURIComponent(t);}</script>`
  );
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
              return new Response(successPage(receivedUser),
                { headers: { "Content-Type": "text/html; charset=utf-8" } }
              );
            }

            // Fallback for OAuth hash fragments (#token=... or #access_token=...)
            return new Response(processingPage(),
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
        // `start` is a cmd.exe builtin, not an executable; the empty "" is the window title
        const openArgs =
          process.platform === "darwin"
            ? ["open", targetLoginUrl]
            : process.platform === "win32"
            ? ["cmd", "/c", "start", "", targetLoginUrl]
            : ["xdg-open", targetLoginUrl];
        Bun.spawn(openArgs);
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
