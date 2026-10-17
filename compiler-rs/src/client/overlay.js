// Com.AI.VN dev overlay: Next.js-style error reporting shared by the web shell and Zalo Mini Apps.
//
// Every error is classified before anything is shown:
//   build   - a module failed to compile           -> full-screen "Build Error", cannot be dismissed
//   runtime - the route could not render/mount     -> dialog opens automatically, dismissable
//   issue   - stray errors, rejections, network    -> only counted on the "Issues" badge
//   ignored - noise from dev tooling and browsers  -> never shown
// The overlay lives in a Shadow DOM so the app's CSS (Tailwind, globals.css) cannot restyle it.

const INTERNAL_URL = /\/_hmr|\/_nata\/(hmr_version|logs|client\.js|overlay\.js)|eruda|@tailwindcss\/browser|cdn\.jsdelivr\.net\/npm\/eruda/;
const NETWORK_MESSAGE = /^(Load failed|Failed to fetch|NetworkError when attempting to fetch resource\.?|Network request failed|The network connection was lost\.?)$/i;
const IGNORED_MESSAGE = /^Script error\.?$|ResizeObserver loop|^NEXT_REDIRECT|^NEXT_NOT_FOUND$/;
// React recovering from a server render that showed a Suspense fallback (the server uses renderToString
// until streaming SSR lands, ROADMAP P1.5); the client renders the content, nothing for the user to fix
const KNOWN_FRAMEWORK_RECOVERY = /server used "renderToString" which does not support Suspense/;

const issues = [];
const seen = new WeakSet();
let activeIndex = 0;
let dialogOpen = false;
let host = null;
let shadow = null;

function esc(value) {
  return String(value == null ? "" : value).replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));
}

function toError(value) {
  if (value instanceof Error) return value;
  if (value && typeof value === "object" && "message" in value) return value;
  return new Error(typeof value === "string" ? value : JSON.stringify(value));
}

// ---------- stack parsing ----------

const FRAME_RE = /^\s*(?:at\s+)?(?:(.*?)\s*\(|(.*?)@)?((?:https?|file):\/\/[^\s)]+?):(\d+):(\d+)\)?\s*$/;

function parseStack(stack) {
  if (!stack) return [];
  const frames = [];
  for (const line of String(stack).split("\n")) {
    const m = line.match(FRAME_RE);
    if (!m) continue;
    const url = m[3];
    const fn = (m[1] || m[2] || "").replace(/^async\s+/, "").trim() || "<anonymous>";
    const bundle = url.match(/\/_bundle\/([^?#]+)/);
    frames.push({
      fn,
      url,
      file: bundle ? bundle[1] : null,
      line: Number(m[4]),
      column: Number(m[5]),
      ignored: !bundle,
    });
  }
  return frames;
}

// ---------- classification & hints (Vietnamese) ----------

function describe(err) {
  const message = String(err.message || err);
  const name = err.name || "Error";

  const req = err.__nataRequest;
  if (req || (name === "TypeError" && NETWORK_MESSAGE.test(message))) {
    const target = req ? `${req.method} ${req.url}` : "";
    return {
      label: "Network Error",
      title: target ? `Failed to fetch ${target}` : `${name}: ${message}`,
      hint: req
        ? `Request tới <code>${esc(req.url)}</code> không kết nối được (${esc(message)}). Kiểm tra dev server / tunnel còn chạy không, URL backend (<code>COM_BACKEND_URL</code>) đúng chưa, và CORS nếu gọi sang domain khác. Lỗi này không làm hỏng trang nên chỉ hiện ở badge Issues.`
        : "Một request mạng bị lỗi nhưng trình duyệt không cho biết URL (thường do request bị chặn hoặc tunnel mất kết nối). Mở tab Network (Eruda trên Zalo) để xem request đỏ.",
    };
  }

  const exportMatch = message.match(/does not provide an export named '([^']+)'/i) || message.match(/Importing binding name '([^']+)' is not found/i);
  if (exportMatch) {
    const missing = exportMatch[1];
    const mod = (message.match(/module '([^']+)'/) || [])[1];
    return {
      label: "Missing Export",
      title: message,
      hint: `${mod ? `<code>${esc(mod.replace(/^.*\/_bundle\//, ""))}</code>` : "Module"} không export <code>${esc(missing)}</code>. Kiểm tra tên export (vd <code>DatePicker</code> thay vì <code>DateTimePicker</code>), hoặc dùng <code>import ${esc(missing)} from "…"</code> nếu đó là export mặc định.`,
    };
  }

  const notFound = message.match(/(?:Module not found|Cannot find module):\s*([^\s,]+)/i);
  if (notFound) {
    const mod = notFound[1].replace(/^\/_bundle\//, "");
    return {
      label: "Module Not Found",
      title: `Module not found: Can't resolve '${mod}'`,
      hint: `Không tìm thấy <code>${esc(mod)}</code>. Kiểm tra file <code>${esc(mod)}.tsx</code> hoặc <code>${esc(mod)}/index.tsx</code> có tồn tại không, và alias <code>@/</code> trong tsconfig.`,
    };
  }

  if (/Importing a module script failed|Failed to fetch dynamically imported module|error loading dynamically imported module/i.test(message)) {
    return {
      label: "Failed to Load Module",
      title: message,
      hint: "Trình duyệt không tải được một module (dev server, tunnel hoặc CDN esm.sh không phản hồi). Thử tải lại; nếu đang chạy trong Zalo, kiểm tra tunnel còn hoạt động.",
    };
  }

  const nullMatch = message.match(/Cannot read propert(?:y|ies) of (null|undefined) \(reading '([^']+)'\)|(null|undefined) is not an object \(evaluating '([^']+)'\)/i);
  if (nullMatch) {
    const kind = nullMatch[1] || nullMatch[3];
    const prop = nullMatch[2] || nullMatch[4];
    return {
      label: null,
      title: `${name}: ${message}`,
      hint: `Giá trị đang là <code>${esc(kind)}</code> khi truy cập <code>${esc(prop)}</code>. Dùng optional chaining (<code>item?.field</code>) hoặc giá trị mặc định trước khi đọc.`,
    };
  }

  if (/Rendered (more|fewer) hooks|Invalid hook call/i.test(message)) {
    return {
      label: null,
      title: `${name}: ${message}`,
      hint: "Hooks phải được gọi theo cùng thứ tự ở mọi lần render: không gọi hook trong if/vòng lặp hoặc sau <code>return</code> sớm.",
    };
  }

  return { label: null, title: `${name}: ${message}`, hint: null };
}

function isIgnored(err, source) {
  const message = String(err && (err.message || err));
  if (IGNORED_MESSAGE.test(message) || KNOWN_FRAMEWORK_RECOVERY.test(message)) return true;
  if (err && err.name === "AbortError") return true;
  if (err && err.__nataInternal) return true;
  if (err && err.__nataRequest && INTERNAL_URL.test(err.__nataRequest.url)) return true;
  // Errors raised entirely inside dev tooling scripts (Eruda, Tailwind browser build)
  if (source === "window-error" && err && err.stack) {
    const frames = parseStack(err.stack);
    if (frames.length && frames.every((f) => INTERNAL_URL.test(f.url))) return true;
  }
  return false;
}

// ---------- public API ----------

/**
 * Records an error. `kind` is "build" | "runtime" | "issue"; `source` identifies global handlers
 * ("window-error" | "rejection" | "console") so tooling noise can be filtered.
 */
export function reportIssue(value, kind = "issue", source = null) {
  if (value == null) return;
  const err = toError(value);
  if (typeof err === "object") {
    if (seen.has(err)) return;
    seen.add(err);
  }
  if (err.__nataBuildError) {
    reportBuildError(err.__nataBuildError);
    return;
  }
  if (isIgnored(err, source)) return;

  const info = describe(err);
  const frames = parseStack(err.stack);
  const top = frames.find((f) => !f.ignored);
  const effectiveKind = info.label === "Network Error" ? "issue" : kind;
  const label = info.label || (effectiveKind === "runtime" ? "Unhandled Runtime Error" : source === "console" ? "Console Error" : "Runtime Error");
  const key = `${effectiveKind}|${info.title}|${top ? `${top.file}:${top.line}` : ""}`;

  const existing = issues.find((i) => i.key === key);
  if (existing) {
    existing.count++;
  } else {
    issues.push({ key, kind: effectiveKind, label, title: info.title, hint: info.hint, frames, top, frame: null, count: 1 });
    if (top) loadCompiledFrame(issues[issues.length - 1]);
  }

  if (effectiveKind === "runtime") {
    activeIndex = issues.findIndex((i) => i.key === key);
    dialogOpen = true;
  }
  render();
}

/** Called by the module that replaces a file which failed to compile (see `build_error_module`). */
export function reportBuildError(payload) {
  const title = payload.message || "Syntax error";
  const key = `build|${payload.file}|${payload.line}|${title}`;
  if (!issues.some((i) => i.key === key)) {
    // A file has at most one build error at a time
    for (let i = issues.length - 1; i >= 0; i--) {
      if (issues[i].kind === "build" && issues[i].file === payload.file) issues.splice(i, 1);
    }
    issues.unshift({
      key,
      kind: "build",
      label: "Build Error",
      title,
      file: payload.file,
      location: payload.line ? `./${payload.file}:${payload.line}:${payload.column}` : `./${payload.file}`,
      frame: payload.frame || "",
      hint: "Sửa lỗi cú pháp ở vị trí trên rồi lưu file, trang sẽ tự cập nhật.",
      frames: [],
      count: 1,
    });
  }
  activeIndex = 0;
  dialogOpen = true;
  render();
}

/** After a successful hot update: build and runtime errors are stale, issues stay on the badge. */
export function clearBlockingErrors() {
  let changed = false;
  for (let i = issues.length - 1; i >= 0; i--) {
    if (issues[i].kind === "build" || issues[i].kind === "runtime") {
      issues.splice(i, 1);
      changed = true;
    }
  }
  if (changed) {
    activeIndex = 0;
    if (!issues.length) dialogOpen = false;
    render();
  }
}

/** Marks a request so its failure is attributed to the URL (and dev-tool requests are ignored). */
export function annotateRequestError(err, method, url) {
  if (err && typeof err === "object") {
    try {
      err.__nataRequest = { method, url };
      if (INTERNAL_URL.test(url)) err.__nataInternal = true;
    } catch {}
  }
  return err;
}

// ---------- compiled code frames ----------

const sourceCache = new Map();

async function loadCompiledFrame(issue) {
  const { top } = issue;
  try {
    let text = sourceCache.get(top.url);
    if (text === undefined) {
      const res = await fetch(top.url.split("?")[0], { cache: "force-cache" });
      text = res.ok ? await res.text() : "";
      sourceCache.set(top.url, text);
    }
    const lines = text.split("\n");
    if (!lines[top.line - 1]) return;
    const start = Math.max(0, top.line - 3);
    const end = Math.min(lines.length, top.line + 2);
    const width = String(end).length;
    let out = "";
    for (let n = start + 1; n <= end; n++) {
      const marker = n === top.line ? ">" : " ";
      out += `${marker} ${String(n).padStart(width)} | ${lines[n - 1]}\n`;
      if (n === top.line) out += `  ${" ".repeat(width)} | ${" ".repeat(Math.max(0, top.column - 1))}^\n`;
    }
    issue.frame = out;
    issue.frameCompiled = true;
    render();
  } catch {}
}

// ---------- rendering ----------

const STYLES = `
:host { all: initial; }
* { box-sizing: border-box; }
.badge { position: fixed; left: 16px; bottom: calc(16px + env(safe-area-inset-bottom, 0px)); z-index: 2147483646; display: flex; align-items: center; gap: 8px; height: 36px; padding: 0 14px 0 6px; border-radius: 999px; background: #0a0a0a; color: #fafafa; border: 1px solid #2e2e2e; box-shadow: 0 8px 24px rgba(0,0,0,.35); font: 500 13px/1 ui-sans-serif, system-ui, -apple-system, "Segoe UI", sans-serif; cursor: pointer; user-select: none; }
.badge .count { display: inline-flex; align-items: center; justify-content: center; min-width: 24px; height: 24px; padding: 0 7px; border-radius: 999px; background: #e5484d; color: #fff; font-weight: 600; }
.backdrop { position: fixed; inset: 0; z-index: 2147483647; background: rgba(0,0,0,.55); backdrop-filter: blur(2px); display: flex; align-items: flex-start; justify-content: center; padding: max(48px, env(safe-area-inset-top, 0px)) 16px 24px; overflow: auto; font: 14px/1.5 ui-sans-serif, system-ui, -apple-system, "Segoe UI", sans-serif; }
.dialog { width: 100%; max-width: 960px; background: #0a0a0a; color: #ededed; border: 1px solid #2e2e2e; border-radius: 12px; box-shadow: 0 24px 64px rgba(0,0,0,.6); overflow: hidden; }
.dialog.build { border-top: 4px solid #e5484d; }
.header { display: flex; align-items: center; justify-content: space-between; gap: 12px; padding: 12px 16px; border-bottom: 1px solid #1f1f1f; }
.pager { display: flex; align-items: center; gap: 6px; color: #a1a1a1; font-size: 13px; font-variant-numeric: tabular-nums; }
.pager button, .close, .copy { background: #1a1a1a; color: #ededed; border: 1px solid #2e2e2e; border-radius: 6px; height: 26px; min-width: 26px; padding: 0 8px; cursor: pointer; font: inherit; }
.pager button:disabled { opacity: .35; cursor: default; }
.actions { display: flex; gap: 8px; }
.body { padding: 20px 20px 24px; }
.label { display: inline-block; color: #ff6166; font-weight: 600; font-size: 14px; margin-bottom: 8px; }
.label .times { color: #a1a1a1; font-weight: 400; margin-left: 6px; }
.title { margin: 0 0 12px; font: 600 17px/1.45 ui-monospace, SFMono-Regular, Menlo, Consolas, monospace; color: #fafafa; white-space: pre-wrap; word-break: break-word; }
.location { font: 13px ui-monospace, SFMono-Regular, Menlo, monospace; color: #a1a1a1; margin-bottom: 8px; }
.location b { color: #ededed; font-weight: 500; }
pre.frame { margin: 0 0 16px; padding: 12px 14px; background: #111; border: 1px solid #1f1f1f; border-radius: 8px; overflow: auto; font: 12.5px/1.6 ui-monospace, SFMono-Regular, Menlo, monospace; color: #a1a1a1; }
pre.frame .hl { color: #fafafa; background: rgba(229,72,77,.12); display: block; }
pre.frame .caret { color: #ff6166; display: block; }
.hint { margin: 0 0 16px; padding: 12px 14px; border-radius: 8px; background: rgba(59,130,246,.08); border: 1px solid rgba(59,130,246,.25); color: #c7dcff; font-size: 13.5px; }
.hint code, .hint b { color: #fff; }
.hint code { font: 12.5px ui-monospace, SFMono-Regular, Menlo, monospace; background: rgba(255,255,255,.08); padding: 1px 5px; border-radius: 4px; }
details { border-top: 1px solid #1f1f1f; padding-top: 12px; }
summary { cursor: pointer; color: #ededed; font-weight: 600; font-size: 13px; }
.stack { margin-top: 8px; font: 12.5px/1.7 ui-monospace, SFMono-Regular, Menlo, monospace; }
.stack .fn { color: #ededed; }
.stack .file { color: #a1a1a1; }
.stack .ign { opacity: .45; }
.footer { padding: 10px 20px; border-top: 1px solid #1f1f1f; color: #707070; font-size: 12px; }
@media (max-width: 640px) { .backdrop { padding: 16px 8px; } .body { padding: 16px; } .title { font-size: 15px; } }
`;

function ensureHost() {
  if (host && document.body && document.body.contains(host)) return;
  if (!document.body) return;
  host = document.createElement("nata-dev-overlay");
  shadow = host.attachShadow({ mode: "open" });
  document.body.appendChild(host);
}

function renderFrame(frame) {
  return frame
    .replace(/\n$/, "")
    .split("\n")
    .map((line) => {
      if (line.startsWith(">")) return `<span class="hl">${esc(line)}</span>`;
      if (/^\s+\|\s*\^$/.test(line)) return `<span class="caret">${esc(line)}</span>`;
      return esc(line);
    })
    .join("\n");
}

function render() {
  ensureHost();
  if (!shadow) {
    document.addEventListener("DOMContentLoaded", render, { once: true });
    return;
  }
  if (!issues.length) {
    shadow.innerHTML = "";
    return;
  }
  activeIndex = Math.min(activeIndex, issues.length - 1);
  const hasBuild = issues.some((i) => i.kind === "build");
  const showDialog = dialogOpen || hasBuild;
  let html = `<style>${STYLES}</style>`;

  if (!showDialog) {
    const total = issues.reduce((n, i) => n + i.count, 0);
    html += `<div class="badge" part="badge" role="button" aria-label="Open issues"><span class="count">${total}</span>${total === 1 ? "Issue" : "Issues"}</div>`;
    shadow.innerHTML = html;
    shadow.querySelector(".badge").onclick = () => { dialogOpen = true; render(); };
    return;
  }

  const issue = hasBuild ? issues.find((i) => i.kind === "build") : issues[activeIndex];
  const index = issues.indexOf(issue);
  const loc = issue.location || (issue.top ? `./${issue.top.file} (${issue.top.line}:${issue.top.column})` : "");
  const stackRows = (issue.frames || [])
    .map((f) => `<div class="${f.ignored ? "ign" : ""}"><span class="fn">${esc(f.fn)}</span> <span class="file">${esc(f.file ? `./${f.file}` : f.url.replace(/^https?:\/\/[^/]+/, ""))} (${f.line}:${f.column})</span></div>`)
    .join("");
  const visibleFrames = (issue.frames || []).filter((f) => !f.ignored).length;

  html += `<div class="backdrop">
    <div class="dialog ${issue.kind === "build" ? "build" : ""}" role="dialog" aria-modal="true">
      <div class="header">
        <div class="pager">
          ${hasBuild ? "" : `<button class="prev" ${index <= 0 ? "disabled" : ""} aria-label="Previous">‹</button>
          <button class="next" ${index >= issues.length - 1 ? "disabled" : ""} aria-label="Next">›</button>`}
          <span>${hasBuild ? "1 of 1" : `${index + 1} of ${issues.length}`}${issue.kind === "issue" ? " issue" : " error"}${issues.length > 1 && !hasBuild ? "s" : ""}</span>
        </div>
        <div class="actions">
          <button class="copy">Copy</button>
          ${issue.kind === "build" ? "" : `<button class="close" aria-label="Close">✕</button>`}
        </div>
      </div>
      <div class="body">
        <div class="label">${esc(issue.label)}${issue.count > 1 ? `<span class="times">×${issue.count}</span>` : ""}</div>
        <h1 class="title">${esc(issue.title)}</h1>
        ${loc ? `<div class="location"><b>${esc(loc)}</b>${issue.frameCompiled ? " · compiled output" : ""}</div>` : ""}
        ${issue.frame ? `<pre class="frame">${renderFrame(issue.frame)}</pre>` : ""}
        ${issue.hint ? `<div class="hint">💡 ${issue.hint}</div>` : ""}
        ${stackRows ? `<details ${visibleFrames ? "open" : ""}><summary>Call Stack (${(issue.frames || []).length})</summary><div class="stack">${stackRows}</div></details>` : ""}
      </div>
      <div class="footer">${issue.kind === "build" ? "This error occurred during the build process and can only be dismissed by fixing the error." : "Esc to close · ← → to switch"}</div>
    </div>
  </div>`;
  shadow.innerHTML = html;

  const q = (sel) => shadow.querySelector(sel);
  if (q(".prev")) q(".prev").onclick = () => { activeIndex = Math.max(0, index - 1); render(); };
  if (q(".next")) q(".next").onclick = () => { activeIndex = Math.min(issues.length - 1, index + 1); render(); };
  if (q(".close")) q(".close").onclick = () => { dialogOpen = false; render(); };
  q(".backdrop").onclick = (e) => { if (e.target === q(".backdrop") && issue.kind !== "build") { dialogOpen = false; render(); } };
  q(".copy").onclick = () => {
    const text = [issue.label, issue.title, loc, issue.frame, (issue.frames || []).map((f) => `  at ${f.fn} (${f.url}:${f.line}:${f.column})`).join("\n")].filter(Boolean).join("\n");
    try { navigator.clipboard.writeText(text); q(".copy").textContent = "Copied"; } catch {}
  };
}

// ---------- global wiring ----------

/**
 * Attributes network failures to their URL so "TypeError: Load failed" says which request broke,
 * and lists them on the Issues badge.
 * Idempotent; called again after tools that replace `window.fetch` (Eruda) so the wrapper stays outermost.
 */
export function instrumentFetch() {
  const current = window.fetch;
  if (typeof current !== "function" || current.__nataInstrumented) return;
  const instrumented = function (input, init) {
    const url = typeof input === "string" ? input : (input && input.url) || String(input);
    const method = ((init && init.method) || (input && input.method) || "GET").toUpperCase();
    return Promise.resolve(current.call(this, input, init)).catch((err) => {
      annotateRequestError(err, method, url);
      // Reported here rather than via `unhandledrejection`: browsers drop that event for scripts loaded
      // cross-origin, which is how the Zalo container loads the app. Network errors only reach the badge.
      if (!err || !err.__nataInternal) setTimeout(() => reportIssue(err, "issue", "fetch"), 0);
      throw err;
    });
  };
  instrumented.__nataInstrumented = true;
  window.fetch = instrumented;
}

function install() {
  if (window.__NATA_OVERLAY_INSTALLED__) return;
  window.__NATA_OVERLAY_INSTALLED__ = true;
  window.__NATA_BUILD_ERROR__ = reportBuildError;
  window.__NATA_REPORT_ISSUE__ = reportIssue;

  window.addEventListener("error", (e) => {
    // Failed <script>/<link> loads fire error events without an Error object; they are not app crashes
    if (!e.error && e.target && e.target !== window) return;
    reportIssue(e.error || e.message, "issue", "window-error");
  });
  window.addEventListener("unhandledrejection", (e) => reportIssue(e.reason, "issue", "rejection"));

  instrumentFetch();
  window.__NATA_INSTRUMENT_FETCH__ = instrumentFetch;

  window.addEventListener("keydown", (e) => {
    if (!issues.length) return;
    const hasBuild = issues.some((i) => i.kind === "build");
    if (e.key === "Escape" && dialogOpen && !hasBuild) { dialogOpen = false; render(); }
    if (dialogOpen && !hasBuild && e.key === "ArrowLeft") { activeIndex = Math.max(0, activeIndex - 1); render(); }
    if (dialogOpen && !hasBuild && e.key === "ArrowRight") { activeIndex = Math.min(issues.length - 1, activeIndex + 1); render(); }
  });
}

install();
