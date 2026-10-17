// Com.AI.VN query & action debugger (dev only): SQL queries, Server Action / RPC calls and errors of
// the current session, in a drawer toggled from the "Debug" pill (Ctrl+Shift+D). Shared by the
// client-rendered runtime and the React Server Components router; the markup is added on demand so
// the server-rendered document stays exactly what React renders.

const ORIGIN = ((window.__NATA_CONFIG__ || {}).origin || "").replace(/\/$/, "");

const MARKUP = `<div id="_nata_debug_pill" onclick="window.__NATA_TOGGLE_DEBUGGER__()" title="Toggle Query & Action Debugger (Ctrl+Shift+D)" style="position: fixed; bottom: 12px; right: 12px; z-index: 999990; display: flex; align-items: center; gap: 8px; padding: 6px 14px; background: #18181b; border: 1px solid #27272a; border-radius: 9999px; font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; font-size: 12px; color: #f4f4f5; cursor: pointer; user-select: none; box-shadow: 0 10px 25px rgba(0,0,0,0.6); transition: all 0.15s ease;">
    <div id="_dev_status" style="width: 8px; height: 8px; border-radius: 50%; background: #10b981; box-shadow: 0 0 8px rgba(16,185,129,0.5);"></div>
    <span style="font-weight: 600; letter-spacing: -0.2px;">⚡ Debug</span>
    <span id="_nata_pill_stats" style="color: #a1a1aa; font-size: 11px; background: #27272a; padding: 2px 8px; border-radius: 9999px;">0 logs</span>
    <span id="_nata_pill_err" style="display: none; color: #fda4af; background: rgba(244,63,94,0.25); border: 1px solid rgba(244,63,94,0.3); padding: 2px 8px; border-radius: 9999px; font-weight: 600;">0 err</span>
  </div>

  <div id="_nata_debug_drawer" style="display: none; position: fixed; bottom: 0; left: 0; right: 0; height: 480px; max-height: 85vh; background: #09090b; border-top: 1px solid #27272a; z-index: 999995; font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; color: #f4f4f5; box-shadow: 0 -12px 40px rgba(0,0,0,0.8); flex-direction: column; border-top-left-radius: 12px; border-top-right-radius: 12px; overflow: hidden;">
    <div style="padding: 10px 16px; border-bottom: 1px solid #27272a; display: flex; align-items: center; justify-content: space-between; background: #121215; gap: 12px; flex-wrap: wrap;">
      <div style="display: flex; align-items: center; gap: 10px;">
        <div style="display: flex; align-items: center; gap: 6px;">
          <span style="font-weight: 700; font-size: 13px; color: #fafafa; letter-spacing: -0.3px;">⚡ NATA Query Inspector</span>
          <span style="font-size: 10px; font-weight: 600; background: rgba(16,185,129,0.15); color: #34d399; border: 1px solid rgba(16,185,129,0.3); padding: 2px 6px; border-radius: 4px; text-transform: uppercase;">Live</span>
        </div>
        <div id="_dbg_tabs" style="display: flex; align-items: center; gap: 4px; background: #18181b; padding: 3px; border-radius: 8px; border: 1px solid #27272a;">
          <button id="_tab_btn_all" onclick="window.__NATA_SET_TAB__('all')" style="background: #27272a; color: #fafafa; border: none; padding: 4px 10px; border-radius: 6px; font-size: 11px; font-weight: 500; cursor: pointer;">All <span id="_tab_cnt_all" style="color: #a1a1aa; margin-left: 2px;">(0)</span></button>
          <button id="_tab_btn_sql" onclick="window.__NATA_SET_TAB__('sql')" style="background: transparent; color: #a1a1aa; border: none; padding: 4px 10px; border-radius: 6px; font-size: 11px; font-weight: 500; cursor: pointer;">SQL Queries <span id="_tab_cnt_sql" style="color: #71717a; margin-left: 2px;">(0)</span></button>
          <button id="_tab_btn_rpc" onclick="window.__NATA_SET_TAB__('rpc')" style="background: transparent; color: #a1a1aa; border: none; padding: 4px 10px; border-radius: 6px; font-size: 11px; font-weight: 500; cursor: pointer;">RPC Actions <span id="_tab_cnt_rpc" style="color: #71717a; margin-left: 2px;">(0)</span></button>
          <button id="_tab_btn_error" onclick="window.__NATA_SET_TAB__('error')" style="background: transparent; color: #a1a1aa; border: none; padding: 4px 10px; border-radius: 6px; font-size: 11px; font-weight: 500; cursor: pointer;">Errors <span id="_tab_cnt_err" style="color: #f43f5e; margin-left: 2px;">(0)</span></button>
        </div>
      </div>
      <div style="display: flex; align-items: center; gap: 8px; flex: 1; max-width: 420px; min-width: 200px;">
        <div style="position: relative; width: 100%;">
          <input id="_dbg_search" oninput="window.__NATA_FILTER_LOGS__()" type="text" placeholder="Search SQL, table, action, params, response... (Ctrl+K)" style="width: 100%; box-sizing: border-box; background: #18181b; border: 1px solid #27272a; border-radius: 8px; padding: 6px 10px 6px 28px; font-size: 12px; color: #f4f4f5; outline: none;" />
          <span style="position: absolute; left: 9px; top: 50%; transform: translateY(-50%); color: #71717a; font-size: 12px;">🔍</span>
        </div>
      </div>
      <div style="display: flex; align-items: center; gap: 6px;">
        <button onclick="window.__NATA_CLEAR_LOGS__()" title="Clear all logs" style="background: #18181b; border: 1px solid #27272a; color: #a1a1aa; padding: 5px 10px; border-radius: 6px; font-size: 11px; cursor: pointer; display: flex; align-items: center; gap: 4px;">🗑 Clear</button>
        <button onclick="window.__NATA_EXPORT_LOGS__()" title="Export logs as JSON" style="background: #18181b; border: 1px solid #27272a; color: #a1a1aa; padding: 5px 10px; border-radius: 6px; font-size: 11px; cursor: pointer; display: flex; align-items: center; gap: 4px;">💾 Export</button>
        <button id="_dbg_expand_btn" onclick="window.__NATA_TOGGLE_EXPAND__()" title="Toggle full height" style="background: #18181b; border: 1px solid #27272a; color: #a1a1aa; padding: 5px 9px; border-radius: 6px; font-size: 11px; cursor: pointer;">⤢</button>
        <button onclick="window.__NATA_TOGGLE_DEBUGGER__()" title="Close Debugger (Esc)" style="background: transparent; border: none; color: #71717a; padding: 4px 8px; font-size: 15px; cursor: pointer;">✕</button>
      </div>
    </div>
    <div id="_dbg_logs_container" style="flex: 1; overflow-y: auto; padding: 12px 16px; display: flex; flex-direction: column; gap: 8px; min-height: 0;"></div>
  </div>`;

/** Adds the pill and the drawer to the page if the document does not have them yet */
export function mountDebugUi() {
  if (document.getElementById("_nata_debug_pill")) return;
  const host = document.createElement("div");
  host.id = "_nata_dev_tools";
  host.innerHTML = MARKUP;
  document.body.appendChild(host);
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
  drawer.style.height = window.__NATA_IS_MAXIMIZED__ ? '85vh' : '460px';
  if (btn) btn.textContent = window.__NATA_IS_MAXIMIZED__ ? '⤓' : '⤢';
};

window.__NATA_SET_TAB__ = function(tabName) {
  window.__NATA_CURRENT_TAB__ = tabName;
  ['all', 'sql', 'rpc', 'error'].forEach(t => {
    const el = document.getElementById('_tab_btn_' + t);
    if (el) {
      if (t === tabName) {
        el.style.background = '#27272a';
        el.style.color = '#fafafa';
      } else {
        el.style.background = 'transparent';
        el.style.color = '#a1a1aa';
      }
    }
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

function updatePillBadges() {
  const total = window.__NATA_LOGS__.length;
  const errors = window.__NATA_LOGS__.filter(l => l.status === 'error').length;
  const pillStats = document.getElementById('_nata_pill_stats');
  const pillErr = document.getElementById('_nata_pill_err');
  if (pillStats) {
    pillStats.textContent = total + ' ' + (total === 1 ? 'log' : 'logs');
  }
  if (pillErr) {
    if (errors > 0) {
      pillErr.style.display = 'inline-block';
      pillErr.textContent = errors + ' err';
    } else {
      pillErr.style.display = 'none';
    }
  }
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
  if (tabCntAll) tabCntAll.textContent = '(' + allCount + ')';
  if (tabCntSql) tabCntSql.textContent = '(' + sqlCount + ')';
  if (tabCntRpc) tabCntRpc.textContent = '(' + rpcCount + ')';
  if (tabCntErr) tabCntErr.textContent = '(' + errCount + ')';

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
    container.innerHTML = '<div style="display:flex;flex-direction:column;align-items:center;justify-content:center;height:100%;min-height:220px;color:#71717a;gap:10px;user-select:none;">' +
      '<div style="font-size:28px;opacity:0.6;">⚡</div>' +
      '<div style="font-size:14px;font-weight:500;color:#a1a1aa;">No logs to display</div>' +
      '<div style="font-size:12px;color:#71717a;">Execute queries or server actions in your app to inspect them live.</div>' +
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
      metaInfo = '<span style="color:#a1a1aa;font-size:11px;">' + (item.rowCount !== undefined ? item.rowCount + ' row(s)' : 'query') + '</span>';
    } else if (isRpc) {
      const qLen = Array.isArray(item.queries) ? item.queries.length : 0;
      metaInfo = '<span style="color:#a1a1aa;font-size:11px;">' + (qLen > 0 ? qLen + ' SQL query(s)' : 'action') + '</span>';
    }

    const timeStr = item.timestamp ? new Date(item.timestamp).toLocaleTimeString() : '';

    // Card header
    html += '<div style="background:#121215;border:1px solid ' + (isExpanded ? '#3f3f46' : '#27272a') + ';border-radius:10px;overflow:hidden;flex-shrink:0;transition:border-color 0.15s ease;">';
    html += '<div onclick="window.__NATA_TOGGLE_ITEM__(\'' + item.id + '\')" style="padding:10px 14px;cursor:pointer;display:flex;align-items:center;justify-content:space-between;gap:12px;user-select:none;min-height:42px;box-sizing:border-box;background:' + (isExpanded ? '#18181b' : 'transparent') + ';">';
    html += '<div style="display:flex;align-items:center;gap:10px;flex:1;overflow:hidden;min-width:0;">';
    html += '<span style="font-size:10px;color:#71717a;transform:rotate(' + (isExpanded ? '90deg' : '0deg') + ');transition:transform 0.15s ease;flex-shrink:0;">▶</span>';
    html += typeBadge;
    html += '<div style="font-family:ui-monospace,SFMono-Regular,Menlo,monospace;font-size:12px;color:#f4f4f5;white-space:nowrap;overflow:hidden;text-overflow:ellipsis;flex:1;min-width:0;">' + summaryTitle + '</div>';
    html += '</div>';
    html += '<div style="display:flex;align-items:center;gap:10px;flex-shrink:0;">';
    html += metaInfo;
    html += '<span style="background:' + latBg + ';color:' + latColor + ';border:1px solid ' + latBorder + ';padding:2px 8px;border-radius:6px;font-size:11px;font-family:ui-monospace,monospace;font-weight:600;">' + duration + 'ms</span>';
    html += '<span style="color:#71717a;font-size:11px;font-family:ui-monospace,monospace;">' + timeStr + '</span>';
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
    window.__NATA_TOGGLE_DEBUGGER__();
  }
  // Escape to close debugger
  if (e.key === 'Escape' && window.__NATA_IS_DRAWER_OPEN__) {
    window.__NATA_TOGGLE_DEBUGGER__();
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

