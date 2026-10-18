// Com.AI.VN dev tools (dev only): one small floating button, like the Next.js dev indicator. It shows
// the HMR status and an error count; a tap opens a menu (Issues, Queries & Actions, route) and the
// Queries & Actions panel lists the SQL queries, Server Action / RPC calls and errors of the session
// (Ctrl+Shift+D). The button can be dragged: it snaps to the nearest side and remembers where it was
// left, so it never has to cover the page. Shared by the client-rendered runtime and the React Server
// Components router; the markup is added on demand so the server-rendered document stays exactly
// what React renders.

const ORIGIN = ((window.__NATA_CONFIG__ || {}).origin || "").replace(/\/$/, "");
const POSITION_KEY = "__nata_dev_indicator";
const HIDDEN_KEY = "__nata_dev_indicator_hidden";
const EDGE = 12;
const SIZE = 36;

const ICON = {
  logo: '<svg width="16" height="16" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M13.5 2 4 13.5h6.5L9.5 22 19 10.5h-6.5L13.5 2Z"/></svg>',
  issues: '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="12" cy="12" r="9"/><path d="M12 8v4M12 16h.01"/></svg>',
  queries: '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><ellipse cx="12" cy="5" rx="8" ry="3"/><path d="M4 5v14c0 1.7 3.6 3 8 3s8-1.3 8-3V5"/><path d="M4 12c0 1.7 3.6 3 8 3s8-1.3 8-3"/></svg>',
  route: '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M9 18 15 12 9 6"/></svg>',
  hide: '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M3 3l18 18"/><path d="M10.6 5.1A9.8 9.8 0 0 1 12 5c5 0 9 4.5 10 7a12 12 0 0 1-3 4.2M6.6 6.6A12 12 0 0 0 2 12c1 2.5 5 7 10 7a9.6 9.6 0 0 0 5.4-1.6"/><path d="M9.9 9.9a3 3 0 0 0 4.2 4.2"/></svg>',
  search: '<svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="11" cy="11" r="7"/><path d="m20 20-3.5-3.5"/></svg>',
  trash: '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M3 6h18M8 6V4h8v2M6 6l1 14h10l1-14"/></svg>',
  download: '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M12 4v11M7 10l5 5 5-5M5 20h14"/></svg>',
  expand: '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M15 3h6v6M9 21H3v-6M21 3l-7 7M3 21l7-7"/></svg>',
  collapse: '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M4 14h6v6M20 10h-6V4M14 10l7-7M3 21l7-7"/></svg>',
  close: '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M18 6 6 18M6 6l12 12"/></svg>',
};

const STYLES = `
#_nata_dev_tools, #_nata_dev_tools * { box-sizing: border-box; }
#_nata_dev_tools { font: 13px/1.4 ui-sans-serif, system-ui, -apple-system, "Segoe UI", Roboto, sans-serif; color: #ededed; -webkit-font-smoothing: antialiased; }
#_nata_dev_tools button { font: inherit; color: inherit; }
#_nata_debug_pill { position: fixed; left: 0; top: 0; z-index: 2147483640; width: ${SIZE}px; height: ${SIZE}px; border-radius: 50%; display: flex; align-items: center; justify-content: center; background: #0a0a0a; color: #fafafa; border: 1px solid rgba(255,255,255,.16); box-shadow: 0 4px 16px rgba(0,0,0,.35); cursor: pointer; user-select: none; -webkit-user-select: none; touch-action: none; -webkit-tap-highlight-color: transparent; transition: transform .25s cubic-bezier(.2,.8,.2,1), opacity .15s ease; opacity: .92; }
#_nata_debug_pill:hover { opacity: 1; }
#_nata_debug_pill.dragging { transition: none; opacity: 1; cursor: grabbing; }
#_dev_status { position: absolute; right: 1px; bottom: 1px; width: 9px; height: 9px; border-radius: 50%; background: #10b981; border: 2px solid #0a0a0a; }
#_nata_pill_err { position: absolute; top: -5px; right: -5px; min-width: 18px; height: 18px; padding: 0 5px; border-radius: 9px; background: #e5484d; color: #fff; font-size: 11px; font-weight: 600; line-height: 18px; text-align: center; font-variant-numeric: tabular-nums; display: none; }
._nd-menu { position: fixed; z-index: 2147483641; width: 248px; padding: 6px; background: #0a0a0a; border: 1px solid #2e2e2e; border-radius: 12px; box-shadow: 0 16px 40px rgba(0,0,0,.45); display: none; }
._nd-menu.open { display: block; }
._nd-item { width: 100%; display: flex; align-items: center; gap: 10px; height: 34px; padding: 0 10px; background: transparent; border: 0; border-radius: 8px; cursor: pointer; text-align: left; color: #ededed; }
._nd-item:hover, ._nd-item:focus-visible { background: #1a1a1a; outline: none; }
._nd-item svg { color: #a1a1a1; flex-shrink: 0; }
._nd-item span:first-of-type { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
._nd-item[disabled] { cursor: default; }
._nd-item[disabled]:hover { background: transparent; }
._nd-meta { color: #a1a1a1; font: 12px ui-monospace, SFMono-Regular, Menlo, monospace; max-width: 120px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
._nd-count { min-width: 20px; height: 20px; padding: 0 6px; border-radius: 10px; background: #1f1f1f; color: #a1a1a1; font-size: 11px; font-weight: 600; line-height: 20px; text-align: center; font-variant-numeric: tabular-nums; }
._nd-count.err { background: rgba(229,72,77,.18); color: #ff6166; }
._nd-sep { height: 1px; margin: 6px 4px; background: #1f1f1f; }
._nd-foot { padding: 4px 10px 2px; color: #707070; font-size: 11px; }
._nd-foot kbd { font: 11px ui-monospace, SFMono-Regular, Menlo, monospace; color: #a1a1a1; }
#_nata_debug_drawer { position: fixed; z-index: 2147483642; display: none; flex-direction: column; width: min(760px, calc(100vw - 24px)); height: min(520px, 75vh); background: #0a0a0a; border: 1px solid #2e2e2e; border-radius: 12px; box-shadow: 0 24px 64px rgba(0,0,0,.55); overflow: hidden; color: #ededed; }
#_nata_debug_drawer.max { height: min(900px, calc(100vh - 24px)); }
._nd-head { display: flex; align-items: center; gap: 8px; padding: 8px 8px 8px 14px; border-bottom: 1px solid #1f1f1f; flex-wrap: wrap; }
._nd-title { font-weight: 600; font-size: 13px; color: #fafafa; margin-right: 4px; }
._nd-tabs { display: flex; gap: 2px; padding: 2px; background: #111; border: 1px solid #1f1f1f; border-radius: 8px; }
._nd-tabs button { height: 24px; padding: 0 9px; border: 0; border-radius: 6px; background: transparent; color: #a1a1a1; font-size: 12px; cursor: pointer; white-space: nowrap; }
._nd-tabs button.active { background: #262626; color: #fafafa; }
._nd-tabs button span { color: #707070; margin-left: 3px; font-variant-numeric: tabular-nums; }
._nd-search { position: relative; flex: 1; min-width: 140px; }
._nd-search svg { position: absolute; left: 9px; top: 50%; transform: translateY(-50%); color: #707070; }
._nd-search input { width: 100%; height: 28px; padding: 0 10px 0 28px; background: #111; border: 1px solid #1f1f1f; border-radius: 8px; color: #ededed; font: inherit; font-size: 12px; outline: none; }
._nd-search input:focus { border-color: #3a3a3a; }
._nd-icon { width: 28px; height: 28px; display: inline-flex; align-items: center; justify-content: center; border: 0; border-radius: 6px; background: transparent; color: #a1a1a1; cursor: pointer; flex-shrink: 0; }
._nd-icon:hover { background: #1a1a1a; color: #fafafa; }
._nd-actions { display: flex; gap: 2px; margin-left: auto; }
#_dbg_logs_container { flex: 1; overflow-y: auto; padding: 10px; display: flex; flex-direction: column; gap: 6px; min-height: 0; }
@media (max-width: 640px) {
  #_nata_debug_drawer { left: 8px !important; right: 8px !important; top: auto !important; bottom: calc(8px + env(safe-area-inset-bottom, 0px)) !important; width: auto; height: 70vh; }
  #_nata_debug_drawer.max { height: calc(100vh - 16px - env(safe-area-inset-top, 0px)); }
  ._nd-head { padding: 8px; gap: 6px; }
  ._nd-title { display: none; }
  ._nd-tabs { flex: 1; min-width: 0; overflow-x: auto; scrollbar-width: none; }
  ._nd-tabs button { padding: 0 7px; }
  ._nd-search { order: 10; flex-basis: 100%; }
  ._nd-hide-sm { display: none !important; }
}
`;

const MARKUP = `<style>${STYLES}</style>
  <div id="_nata_debug_pill" role="button" tabindex="0" aria-haspopup="menu" aria-label="Open Com.AI.VN dev tools" title="Dev tools · drag to move">
    ${ICON.logo}
    <div id="_dev_status"></div>
    <span id="_nata_pill_err">0</span>
  </div>

  <div class="_nd-menu" id="_nata_dev_menu" role="menu">
    <button class="_nd-item" role="menuitem" data-action="issues">${ICON.issues}<span>Issues</span><span class="_nd-count" id="_nd_issue_count">0</span></button>
    <button class="_nd-item" role="menuitem" data-action="queries">${ICON.queries}<span>Queries &amp; Actions</span><span class="_nd-count" id="_nd_log_count">0</span></button>
    <button class="_nd-item" role="menuitem" disabled>${ICON.route}<span>Route</span><span class="_nd-meta" id="_nd_route">/</span></button>
    <div class="_nd-sep"></div>
    <button class="_nd-item" role="menuitem" data-action="hide">${ICON.hide}<span>Hide for this session</span></button>
    <div class="_nd-foot"><kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>D</kbd> queries · drag the button to move it</div>
  </div>

  <div id="_nata_debug_drawer" role="dialog" aria-label="Queries & Actions">
    <div class="_nd-head">
      <span class="_nd-title">Queries &amp; Actions</span>
      <div class="_nd-tabs" id="_dbg_tabs">
        <button id="_tab_btn_all" class="active" onclick="window.__NATA_SET_TAB__('all')">All<span id="_tab_cnt_all">0</span></button>
        <button id="_tab_btn_sql" onclick="window.__NATA_SET_TAB__('sql')">SQL<span id="_tab_cnt_sql">0</span></button>
        <button id="_tab_btn_rpc" onclick="window.__NATA_SET_TAB__('rpc')">Actions<span id="_tab_cnt_rpc">0</span></button>
        <button id="_tab_btn_error" onclick="window.__NATA_SET_TAB__('error')">Errors<span id="_tab_cnt_err">0</span></button>
      </div>
      <div class="_nd-search">${ICON.search}<input id="_dbg_search" oninput="window.__NATA_FILTER_LOGS__()" type="text" placeholder="Search SQL, action, params… (Ctrl+K)" /></div>
      <div class="_nd-actions">
        <button class="_nd-icon" onclick="window.__NATA_CLEAR_LOGS__()" title="Clear">${ICON.trash}</button>
        <button class="_nd-icon" onclick="window.__NATA_EXPORT_LOGS__()" title="Export as JSON">${ICON.download}</button>
        <button class="_nd-icon" id="_dbg_expand_btn" onclick="window.__NATA_TOGGLE_EXPAND__()" title="Toggle height">${ICON.expand}</button>
        <button class="_nd-icon" onclick="window.__NATA_TOGGLE_DEBUGGER__()" title="Close (Esc)">${ICON.close}</button>
      </div>
    </div>
    <div id="_dbg_logs_container"></div>
  </div>`;

// ---------- indicator position (drag, snap to a side, remembered) ----------

function storage(kind) {
  try { return kind === "session" ? window.sessionStorage : window.localStorage; } catch { return null; }
}

function loadPosition() {
  try {
    const saved = JSON.parse(storage("local")?.getItem(POSITION_KEY) || "null");
    if (saved && (saved.side === "left" || saved.side === "right") && typeof saved.y === "number") return saved;
  } catch {}
  // Default like Next.js: bottom left, clear of the home indicator
  return { side: "left", y: 1 };
}

let position = loadPosition();

function safeInset(name) {
  const probe = document.createElement("div");
  probe.style.cssText = `position:fixed;visibility:hidden;padding-top:env(safe-area-inset-${name},0px)`;
  document.body.appendChild(probe);
  const value = parseFloat(getComputedStyle(probe).paddingTop) || 0;
  probe.remove();
  return value;
}

let insets = null;
function bounds() {
  if (!insets) insets = { top: safeInset("top"), bottom: safeInset("bottom") };
  const minY = EDGE + insets.top;
  const maxY = window.innerHeight - SIZE - EDGE - insets.bottom;
  return { minY, maxY: Math.max(minY, maxY) };
}

/** Pixel coordinates of the indicator for the saved side and relative height (0 = top, 1 = bottom) */
function coordinates() {
  const { minY, maxY } = bounds();
  const x = position.side === "left" ? EDGE : window.innerWidth - SIZE - EDGE;
  return { x, y: minY + (maxY - minY) * Math.min(1, Math.max(0, position.y)) };
}

function placeIndicator() {
  const pill = document.getElementById("_nata_debug_pill");
  if (!pill) return;
  const { x, y } = coordinates();
  pill.style.transform = `translate(${x}px, ${y}px)`;
  placeFloating();
}

/** The menu and the panel open next to the indicator, on the side of the screen it sits on */
function placeFloating() {
  const { x, y } = coordinates();
  const below = y < window.innerHeight / 2;
  const menu = document.getElementById("_nata_dev_menu");
  const drawer = document.getElementById("_nata_debug_drawer");
  for (const el of [menu, drawer]) {
    if (!el) continue;
    el.style.left = position.side === "left" ? `${EDGE}px` : "auto";
    el.style.right = position.side === "right" ? `${EDGE}px` : "auto";
    if (el === menu) {
      el.style.top = below ? `${y + SIZE + 8}px` : "auto";
      el.style.bottom = below ? "auto" : `${window.innerHeight - y + 8}px`;
    } else {
      el.style.top = below ? `${EDGE + (insets ? insets.top : 0)}px` : "auto";
      el.style.bottom = below ? "auto" : `${EDGE + (insets ? insets.bottom : 0)}px`;
    }
  }
}

function enableDrag(pill) {
  let start = null;
  let moved = false;

  pill.addEventListener("pointerdown", (e) => {
    if (e.button !== 0) return;
    const { x, y } = coordinates();
    start = { px: e.clientX, py: e.clientY, x, y };
    moved = false;
    pill.setPointerCapture(e.pointerId);
  });

  pill.addEventListener("pointermove", (e) => {
    if (!start) return;
    const dx = e.clientX - start.px;
    const dy = e.clientY - start.py;
    if (!moved && Math.hypot(dx, dy) < 5) return;
    if (!moved) {
      moved = true;
      pill.classList.add("dragging");
      closeMenu();
    }
    const x = Math.min(window.innerWidth - SIZE, Math.max(0, start.x + dx));
    const y = Math.min(window.innerHeight - SIZE, Math.max(0, start.y + dy));
    pill.style.transform = `translate(${x}px, ${y}px)`;
  });

  const end = (e) => {
    if (!start) return;
    const wasDrag = moved;
    if (wasDrag) {
      const { minY, maxY } = bounds();
      const x = start.x + (e.clientX - start.px);
      const y = start.y + (e.clientY - start.py);
      position = {
        side: x + SIZE / 2 < window.innerWidth / 2 ? "left" : "right",
        y: maxY > minY ? Math.min(1, Math.max(0, (y - minY) / (maxY - minY))) : 1,
      };
      try { storage("local")?.setItem(POSITION_KEY, JSON.stringify(position)); } catch {}
      pill.classList.remove("dragging");
      placeIndicator();
    }
    start = null;
    if (!wasDrag && e.type === "pointerup") toggleMenu();
  };
  pill.addEventListener("pointerup", end);
  pill.addEventListener("pointercancel", end);
  pill.addEventListener("keydown", (e) => {
    if (e.key === "Enter" || e.key === " ") { e.preventDefault(); toggleMenu(); }
  });
}

// ---------- menu ----------

function issueCount() {
  return Number(window.__NATA_ISSUE_COUNT__ || 0);
}

function updateMenu() {
  const route = document.getElementById("_nd_route");
  if (route) {
    const loc = window.__NATA_LOCATION__ || window.location;
    route.textContent = loc.pathname || "/";
    route.title = loc.pathname || "/";
  }
  const issueEl = document.getElementById("_nd_issue_count");
  if (issueEl) {
    issueEl.textContent = String(issueCount());
    issueEl.classList.toggle("err", issueCount() > 0);
  }
  const logs = window.__NATA_LOGS__ || [];
  const logEl = document.getElementById("_nd_log_count");
  if (logEl) {
    logEl.textContent = String(logs.length);
    logEl.classList.toggle("err", logs.some((l) => l.status === "error"));
  }
}

function toggleMenu(force) {
  const menu = document.getElementById("_nata_dev_menu");
  if (!menu) return;
  const open = force !== undefined ? force : !menu.classList.contains("open");
  if (open) {
    if (window.__NATA_IS_DRAWER_OPEN__) window.__NATA_TOGGLE_DEBUGGER__();
    updateMenu();
    placeFloating();
  }
  menu.classList.toggle("open", open);
}

function closeMenu() {
  toggleMenu(false);
}

function setHidden(hidden) {
  const pill = document.getElementById("_nata_debug_pill");
  if (pill) pill.style.display = hidden ? "none" : "flex";
  try {
    if (hidden) storage("session")?.setItem(HIDDEN_KEY, "1");
    else storage("session")?.removeItem(HIDDEN_KEY);
  } catch {}
}

/** Adds the indicator, its menu and the panel to the page if the document does not have them yet */
export function mountDebugUi() {
  if (document.getElementById("_nata_debug_pill") || !document.body) return;
  const host = document.createElement("div");
  host.id = "_nata_dev_tools";
  host.innerHTML = MARKUP;
  document.body.appendChild(host);

  const pill = document.getElementById("_nata_debug_pill");
  enableDrag(pill);
  placeIndicator();
  window.addEventListener("resize", () => { insets = null; placeIndicator(); });

  document.getElementById("_nata_dev_menu").addEventListener("click", (e) => {
    const action = e.target.closest("[data-action]")?.getAttribute("data-action");
    if (!action) return;
    closeMenu();
    if (action === "issues") window.__NATA_OPEN_ISSUES__?.();
    if (action === "queries") window.__NATA_TOGGLE_DEBUGGER__();
    if (action === "hide") setHidden(true);
  });
  document.addEventListener("pointerdown", (e) => {
    if (!host.contains(e.target)) closeMenu();
  }, true);

  let hidden = false;
  try { hidden = storage("session")?.getItem(HIDDEN_KEY) === "1"; } catch {}
  if (hidden) setHidden(true);

  // The overlay hides its own "Issues" badge once this indicator exists and reports counts to it
  window.__NATA_DEV_INDICATOR__ = { update: updatePillBadges };
  window.dispatchEvent(new Event("nata:dev-indicator"));
  renderLogs();
}

// Neutral Dark Query & Action Debugger Engine
window.__NATA_LOGS__ = [];
window.__NATA_CURRENT_TAB__ = "all";
window.__NATA_EXPANDED_ITEMS__ = new Set();
window.__NATA_IS_DRAWER_OPEN__ = false;
window.__NATA_IS_MAXIMIZED__ = false;

function escapeHtml(str) {
  if (str === null || str === undefined) return '';
  return String(str)
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#039;');
}

function highlightSql(sql) {
  if (!sql) return '';
  const safe = escapeHtml(sql);
  const sqlRegex = /('(?:[^'\\]|\\.)*'|"(?:[^"\\]|\\.)*")|(\$\d+)|\b(SELECT|FROM|WHERE|AND|OR|NOT|IN|LIKE|ILIKE|BETWEEN|IS|NULL|JOIN|INNER\s+JOIN|LEFT\s+JOIN|RIGHT\s+JOIN|FULL\s+JOIN|CROSS\s+JOIN|ON|GROUP\s+BY|ORDER\s+BY|ASC|DESC|LIMIT|OFFSET|HAVING|UNION|ALL|AS|DISTINCT|CASE|WHEN|THEN|ELSE|END|INSERT\s+INTO|VALUES|UPDATE|SET|DELETE\s+FROM|DELETE|RETURNING|CREATE\s+TABLE|ALTER\s+TABLE|DROP\s+TABLE|BEGIN|COMMIT|ROLLBACK|COUNT|SUM|AVG|MIN|MAX|EXISTS|COALESCE)\b|\b(\d+(?:\.\d+)?)\b/gi;
  return safe.replace(sqlRegex, function (match, str, param, kw, num) {
    if (str) return '<span style="color:#34d399;">' + str + '</span>';
    if (param) return '<span style="color:#c084fc;font-weight:600;">' + param + '</span>';
    if (kw) return '<span style="color:#38bdf8;font-weight:600;">' + kw + '</span>';
    if (num) return '<span style="color:#fbbf24;">' + num + '</span>';
    return match;
  });
}

function syntaxHighlightJson(json) {
  if (json === undefined || json === null) return '<span style="color:#71717a;">null</span>';
  let str = '';
  try {
    str = JSON.stringify(json, null, 2);
  } catch (e) {
    str = String(json);
  }
  const safe = escapeHtml(str);
  return safe.replace(/("(\\u[a-zA-Z0-9]{4}|\\[^u]|[^\\"])*"(\s*:)?|\b(true|false|null)\b|-?\d+(?:\.\d*)?(?:[eE][+\-]?\d+)?)/g, function (match) {
    let cls = 'color:#fbbf24;';
    if (/^"/.test(match)) {
      if (/:$/.test(match)) {
        cls = 'color:#93c5fd;font-weight:500;';
      } else {
        cls = 'color:#34d399;';
      }
    } else if (/true|false/.test(match)) {
      cls = 'color:#c084fc;font-weight:600;';
    } else if (/null/.test(match)) {
      cls = 'color:#71717a;';
    }
    return '<span style="' + cls + '">' + match + '</span>';
  });
}

window.__NATA_COPY__ = function(text, btnId) {
  navigator.clipboard.writeText(text);
  const btn = document.getElementById(btnId);
  if (btn) {
    const originalText = btn.innerHTML;
    btn.innerHTML = '<span style="color:#34d399;">✓ Copied!</span>';
    setTimeout(() => { btn.innerHTML = originalText; }, 1400);
  }
};

window.__NATA_TOGGLE_ITEM__ = function(id) {
  if (window.__NATA_EXPANDED_ITEMS__.has(id)) {
    window.__NATA_EXPANDED_ITEMS__.delete(id);
  } else {
    window.__NATA_EXPANDED_ITEMS__.add(id);
  }
  renderLogs();
};

window.__NATA_TOGGLE_DEBUGGER__ = function() {
  const drawer = document.getElementById('_nata_debug_drawer');
  if (!drawer) return;
  window.__NATA_IS_DRAWER_OPEN__ = !window.__NATA_IS_DRAWER_OPEN__;
  drawer.style.display = window.__NATA_IS_DRAWER_OPEN__ ? 'flex' : 'none';
  if (window.__NATA_IS_DRAWER_OPEN__) {
    closeMenu();
    placeFloating();
    renderLogs();
    setTimeout(() => {
      const searchInput = document.getElementById('_dbg_search');
      if (searchInput) searchInput.focus();
    }, 50);
  }
};

window.__NATA_TOGGLE_EXPAND__ = function() {
  const drawer = document.getElementById('_nata_debug_drawer');
  const btn = document.getElementById('_dbg_expand_btn');
  if (!drawer) return;
  window.__NATA_IS_MAXIMIZED__ = !window.__NATA_IS_MAXIMIZED__;
  drawer.classList.toggle('max', window.__NATA_IS_MAXIMIZED__);
  if (btn) btn.innerHTML = window.__NATA_IS_MAXIMIZED__ ? ICON.collapse : ICON.expand;
};

window.__NATA_SET_TAB__ = function(tabName) {
  window.__NATA_CURRENT_TAB__ = tabName;
  ['all', 'sql', 'rpc', 'error'].forEach(t => {
    const el = document.getElementById('_tab_btn_' + t);
    if (el) el.classList.toggle('active', t === tabName);
  });
  renderLogs();
};

window.__NATA_FILTER_LOGS__ = function() {
  renderLogs();
};

window.__NATA_CLEAR_LOGS__ = async function() {
  window.__NATA_LOGS__ = [];
  window.__NATA_EXPANDED_ITEMS__.clear();
  try {
    await fetch(ORIGIN + '/_nata/logs', { method: 'DELETE' });
  } catch (e) {}
  renderLogs();
};

window.__NATA_EXPORT_LOGS__ = function() {
  const dataStr = 'data:text/json;charset=utf-8,' + encodeURIComponent(JSON.stringify(window.__NATA_LOGS__, null, 2));
  const downloadAnchor = document.createElement('a');
  downloadAnchor.setAttribute('href', dataStr);
  downloadAnchor.setAttribute('download', 'nata-query-logs-' + Date.now() + '.json');
  document.body.appendChild(downloadAnchor);
  downloadAnchor.click();
  downloadAnchor.remove();
};

/** Red count on the indicator: overlay issues plus failed queries / actions */
function updatePillBadges() {
  const errors = (window.__NATA_LOGS__ || []).filter(l => l.status === 'error').length + issueCount();
  const pillErr = document.getElementById('_nata_pill_err');
  if (pillErr) {
    pillErr.style.display = errors > 0 ? 'block' : 'none';
    pillErr.textContent = errors > 99 ? '99+' : String(errors);
  }
  updateMenu();
}

export function renderLogs() {
  const container = document.getElementById('_dbg_logs_container');
  if (!container) return;

  const searchInput = document.getElementById('_dbg_search');
  const query = (searchInput ? searchInput.value : '').trim().toLowerCase();
  const tab = window.__NATA_CURRENT_TAB__ || 'all';

  // Update tab counter numbers
  const allCount = window.__NATA_LOGS__.length;
  const sqlCount = window.__NATA_LOGS__.filter(l => l.type === 'sql').length;
  const rpcCount = window.__NATA_LOGS__.filter(l => l.type === 'rpc').length;
  const errCount = window.__NATA_LOGS__.filter(l => l.status === 'error').length;

  const tabCntAll = document.getElementById('_tab_cnt_all');
  const tabCntSql = document.getElementById('_tab_cnt_sql');
  const tabCntRpc = document.getElementById('_tab_cnt_rpc');
  const tabCntErr = document.getElementById('_tab_cnt_err');
  if (tabCntAll) tabCntAll.textContent = String(allCount);
  if (tabCntSql) tabCntSql.textContent = String(sqlCount);
  if (tabCntRpc) tabCntRpc.textContent = String(rpcCount);
  if (tabCntErr) tabCntErr.textContent = String(errCount);

  updatePillBadges();

  // Filter by tab
  let filtered = window.__NATA_LOGS__.filter(item => {
    if (tab === 'sql') return item.type === 'sql';
    if (tab === 'rpc') return item.type === 'rpc';
    if (tab === 'error') return item.status === 'error';
    return true;
  });

  // Filter by search query
  if (query) {
    filtered = filtered.filter(item => {
      const str = JSON.stringify(item).toLowerCase();
      return str.includes(query);
    });
  }

  if (filtered.length === 0) {
    container.innerHTML = '<div style="display:flex;flex-direction:column;align-items:center;justify-content:center;height:100%;min-height:200px;gap:6px;user-select:none;text-align:center;padding:0 16px;">' +
      '<div style="font-size:13px;font-weight:500;color:#ededed;">' + (query ? 'No matching entries' : 'No queries or actions yet') + '</div>' +
      '<div style="font-size:12px;color:#707070;">SQL queries and Server Actions run by this page show up here live.</div>' +
    '</div>';
    return;
  }

  let html = '';
  for (let i = filtered.length - 1; i >= 0; i--) {
    const item = filtered[i];
    const isExpanded = window.__NATA_EXPANDED_ITEMS__.has(item.id);
    const isError = item.status === 'error';
    const isSql = item.type === 'sql';
    const isRpc = item.type === 'rpc';
    const duration = Number(item.duration_ms || 0);

    // Latency color styling
    let latColor = '#34d399';
    let latBg = 'rgba(16,185,129,0.12)';
    let latBorder = 'rgba(16,185,129,0.25)';
    if (duration >= 200 || isError) {
      latColor = '#f43f5e';
      latBg = 'rgba(244,63,94,0.12)';
      latBorder = 'rgba(244,63,94,0.25)';
    } else if (duration >= 50) {
      latColor = '#fbbf24';
      latBg = 'rgba(245,158,11,0.12)';
      latBorder = 'rgba(245,158,11,0.25)';
    }

    // Type Badge
    let typeBadge = '';
    if (isError) {
      typeBadge = '<span style="background:rgba(244,63,94,0.15);color:#fda4af;border:1px solid rgba(244,63,94,0.3);padding:2px 7px;border-radius:6px;font-size:10px;font-weight:700;">ERROR</span>';
    } else if (isSql) {
      typeBadge = '<span style="background:rgba(56,189,248,0.12);color:#38bdf8;border:1px solid rgba(56,189,248,0.25);padding:2px 7px;border-radius:6px;font-size:10px;font-weight:700;">SQL</span>';
    } else {
      typeBadge = '<span style="background:rgba(168,85,247,0.12);color:#c084fc;border:1px solid rgba(168,85,247,0.25);padding:2px 7px;border-radius:6px;font-size:10px;font-weight:700;">RPC</span>';
    }

    // Summary Title
    let summaryTitle = '';
    if (isSql) {
      const cleanSql = (item.sql || '').trim().replace(/\s+/g, ' ');
      summaryTitle = highlightSql(cleanSql.length > 120 ? cleanSql.slice(0, 120) + '...' : cleanSql);
    } else if (isRpc) {
      summaryTitle = '<span style="color:#e4e4e7;font-weight:600;">' + escapeHtml(item.module) + '</span> ' +
        '<span style="color:#71717a;">›</span> ' +
        '<span style="color:#38bdf8;font-weight:600;">' + escapeHtml(item.action) + '()</span>';
    }

    // Meta info (row count or nested queries)
    let metaInfo = '';
    if (isSql) {
      metaInfo = '<span class="_nd-hide-sm" style="color:#a1a1aa;font-size:11px;">' + (item.rowCount !== undefined ? item.rowCount + ' row(s)' : 'query') + '</span>';
    } else if (isRpc) {
      const qLen = Array.isArray(item.queries) ? item.queries.length : 0;
      metaInfo = '<span class="_nd-hide-sm" style="color:#a1a1aa;font-size:11px;">' + (qLen > 0 ? qLen + ' SQL query(s)' : 'action') + '</span>';
    }

    const timeStr = item.timestamp ? new Date(item.timestamp).toLocaleTimeString() : '';

    // Card header
    html += '<div style="background:#111;border:1px solid ' + (isExpanded ? '#3a3a3a' : '#1f1f1f') + ';border-radius:8px;overflow:hidden;flex-shrink:0;transition:border-color 0.15s ease;">';
    html += '<div onclick="window.__NATA_TOGGLE_ITEM__(\'' + item.id + '\')" style="padding:10px 14px;cursor:pointer;display:flex;align-items:center;justify-content:space-between;gap:12px;user-select:none;min-height:42px;box-sizing:border-box;background:' + (isExpanded ? '#18181b' : 'transparent') + ';">';
    html += '<div style="display:flex;align-items:center;gap:10px;flex:1;overflow:hidden;min-width:0;">';
    html += '<span style="font-size:10px;color:#71717a;transform:rotate(' + (isExpanded ? '90deg' : '0deg') + ');transition:transform 0.15s ease;flex-shrink:0;">▶</span>';
    html += typeBadge;
    html += '<div style="font-family:ui-monospace,SFMono-Regular,Menlo,monospace;font-size:12px;color:#f4f4f5;white-space:nowrap;overflow:hidden;text-overflow:ellipsis;flex:1;min-width:0;">' + summaryTitle + '</div>';
    html += '</div>';
    html += '<div style="display:flex;align-items:center;gap:10px;flex-shrink:0;">';
    html += metaInfo;
    html += '<span style="background:' + latBg + ';color:' + latColor + ';border:1px solid ' + latBorder + ';padding:2px 8px;border-radius:6px;font-size:11px;font-family:ui-monospace,monospace;font-weight:600;">' + duration + 'ms</span>';
    html += '<span class="_nd-hide-sm" style="color:#71717a;font-size:11px;font-family:ui-monospace,monospace;">' + timeStr + '</span>';
    html += '</div>';
    html += '</div>';

    // Card expanded body
    if (isExpanded) {
      html += '<div style="padding:14px;border-top:1px solid #27272a;display:flex;flex-direction:column;gap:12px;background:#09090b;">';

      // 1. SQL Query Box
      if (isSql && item.sql) {
        const copyBtnId = '_btn_copy_sql_' + item.id;
        const rawSqlEscaped = escapeHtml(item.sql).replace(/'/g, "\\'");
        html += '<div style="display:flex;flex-direction:column;gap:6px;">';
        html += '<div style="display:flex;align-items:center;justify-content:space-between;">';
        html += '<span style="font-size:11px;font-weight:600;color:#94a3b8;text-transform:uppercase;letter-spacing:0.5px;">SQL Statement</span>';
        html += '<button id="' + copyBtnId + '" onclick="window.__NATA_COPY__(\'' + rawSqlEscaped + '\', \'' + copyBtnId + '\')" style="background:#18181b;border:1px solid #27272a;color:#e4e4e7;padding:3px 8px;border-radius:6px;font-size:11px;cursor:pointer;">Copy SQL</button>';
        html += '</div>';
        html += '<pre style="margin:0;background:#121215;padding:12px;border-radius:8px;border:1px solid #27272a;font-family:ui-monospace,SFMono-Regular,Menlo,monospace;font-size:12px;line-height:1.5;color:#f4f4f5;white-space:pre-wrap;word-break:break-all;overflow-x:auto;">' + highlightSql(item.sql) + '</pre>';
        html += '</div>';
      }

      // 2. Parameters / Arguments Box
      const params = isSql ? item.parameters : item.parameters;
      if (params !== undefined && params !== null && (Array.isArray(params) ? params.length > 0 : Object.keys(params).length > 0)) {
        const copyBtnId = '_btn_copy_param_' + item.id;
        const rawParamsEscaped = escapeHtml(JSON.stringify(params, null, 2)).replace(/'/g, "\\'");
        html += '<div style="display:flex;flex-direction:column;gap:6px;">';
        html += '<div style="display:flex;align-items:center;justify-content:space-between;">';
        html += '<span style="font-size:11px;font-weight:600;color:#94a3b8;text-transform:uppercase;letter-spacing:0.5px;">Parameters / Arguments</span>';
        html += '<button id="' + copyBtnId + '" onclick="window.__NATA_COPY__(\'' + rawParamsEscaped + '\', \'' + copyBtnId + '\')" style="background:#18181b;border:1px solid #27272a;color:#e4e4e7;padding:3px 8px;border-radius:6px;font-size:11px;cursor:pointer;">Copy Params</button>';
        html += '</div>';
        html += '<pre style="margin:0;background:#121215;padding:12px;border-radius:8px;border:1px solid #27272a;font-family:ui-monospace,SFMono-Regular,Menlo,monospace;font-size:12px;line-height:1.5;color:#f4f4f5;white-space:pre-wrap;word-break:break-all;overflow-x:auto;">' + syntaxHighlightJson(params) + '</pre>';
        html += '</div>';
      }

      // 3. Response / Result Payload Box
      if (item.response !== undefined && item.response !== null) {
        const copyBtnId = '_btn_copy_res_' + item.id;
        const rawResEscaped = escapeHtml(JSON.stringify(item.response, null, 2)).replace(/'/g, "\\'");
        const isArr = Array.isArray(item.response);
        const countLabel = isArr ? ' (' + item.response.length + ' rows)' : '';
        html += '<div style="display:flex;flex-direction:column;gap:6px;">';
        html += '<div style="display:flex;align-items:center;justify-content:space-between;">';
        html += '<span style="font-size:11px;font-weight:600;color:#94a3b8;text-transform:uppercase;letter-spacing:0.5px;">Response / Result' + countLabel + '</span>';
        html += '<button id="' + copyBtnId + '" onclick="window.__NATA_COPY__(\'' + rawResEscaped + '\', \'' + copyBtnId + '\')" style="background:#18181b;border:1px solid #27272a;color:#e4e4e7;padding:3px 8px;border-radius:6px;font-size:11px;cursor:pointer;">Copy Response</button>';
        html += '</div>';
        html += '<pre style="margin:0;background:#121215;padding:12px;border-radius:8px;border:1px solid #27272a;font-family:ui-monospace,SFMono-Regular,Menlo,monospace;font-size:12px;line-height:1.5;color:#f4f4f5;white-space:pre-wrap;word-break:break-all;max-height:260px;overflow-y:auto;">' + syntaxHighlightJson(item.response) + '</pre>';
        html += '</div>';
      }

      // 4. Nested SQL Queries (for RPC actions)
      if (isRpc && Array.isArray(item.queries) && item.queries.length > 0) {
        html += '<div style="display:flex;flex-direction:column;gap:6px;">';
        html += '<span style="font-size:11px;font-weight:600;color:#38bdf8;text-transform:uppercase;letter-spacing:0.5px;">Executed SQL Queries (' + item.queries.length + ')</span>';
        html += '<div style="display:flex;flex-direction:column;gap:6px;">';
        for (const q of item.queries) {
          html += '<div style="background:#18181b;padding:8px 12px;border-radius:6px;border:1px solid #27272a;font-family:ui-monospace,SFMono-Regular,Menlo,monospace;font-size:11px;">';
          html += '<div style="display:flex;justify-content:space-between;margin-bottom:4px;">';
          html += '<span style="color:#38bdf8;font-weight:600;">SQL</span>';
          html += '<span style="color:#34d399;font-weight:600;">' + (q.duration_ms || 0) + 'ms</span>';
          html += '</div>';
          html += '<div style="color:#f4f4f5;white-space:pre-wrap;word-break:break-all;">' + highlightSql(q.sql || '') + '</div>';
          if (q.parameters && q.parameters.length > 0) {
            html += '<div style="color:#a1a1aa;margin-top:4px;font-size:10px;">Params: ' + escapeHtml(JSON.stringify(q.parameters)) + '</div>';
          }
          html += '</div>';
        }
        html += '</div>';
        html += '</div>';
      }

      // 5. Error Info Box (if status === 'error')
      if (isError && item.error) {
        html += '<div style="background:rgba(244,63,94,0.1);border:1px solid rgba(244,63,94,0.3);border-radius:8px;padding:12px;color:#fda4af;font-size:12px;">';
        html += '<div style="font-weight:700;color:#f43f5e;margin-bottom:4px;">Error Details</div>';
        html += '<pre style="margin:0;font-family:ui-monospace,monospace;white-space:pre-wrap;word-break:break-all;color:#fca5a5;">' + escapeHtml(item.error) + '</pre>';
        html += '</div>';
      }

      html += '</div>';
    }

    html += '</div>';
  }

  container.innerHTML = html;
}

// Global keyboard shortcuts for DevTools
window.addEventListener('keydown', (e) => {
  // Ctrl+Shift+D or Cmd+Shift+D to toggle debugger
  if ((e.ctrlKey || e.metaKey) && e.shiftKey && (e.key === 'D' || e.key === 'd')) {
    e.preventDefault();
    setHidden(false);
    window.__NATA_TOGGLE_DEBUGGER__();
  }
  // Escape closes the menu, then the panel
  if (e.key === 'Escape') {
    const menu = document.getElementById('_nata_dev_menu');
    if (menu && menu.classList.contains('open')) closeMenu();
    else if (window.__NATA_IS_DRAWER_OPEN__) window.__NATA_TOGGLE_DEBUGGER__();
  }
  // / or Ctrl+K / Cmd+K to focus search input when open
  if ((e.key === '/' || ((e.ctrlKey || e.metaKey) && (e.key === 'k' || e.key === 'K'))) && window.__NATA_IS_DRAWER_OPEN__) {
    const searchInput = document.getElementById('_dbg_search');
    if (searchInput && document.activeElement !== searchInput) {
      e.preventDefault();
      searchInput.focus();
      searchInput.select();
    }
  }
});

// Fetch initial logs on mount
fetch(ORIGIN + '/_nata/logs').then(r => r.json()).then(data => {
  if (data && Array.isArray(data.logs)) {
    window.__NATA_LOGS__ = data.logs;
    renderLogs();
  }
}).catch(() => {});

