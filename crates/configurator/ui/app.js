const invoke = window.__TAURI__.core.invoke;
const openDialog = window.__TAURI__.dialog.open;

const $ = (id) => document.getElementById(id);
const $$ = (sel) => document.querySelectorAll(sel);

const statusEl = $("status"); // setup-view status strip
const monitorStatusEl = $("monitor-status"); // monitor-view status strip

let state = "idle"; // idle → testing → tested → saving
let currentView = "setup";
let statusTimer = null;

/* ── Generic status helper (used on whichever view is active) ───────── */
function setStatus(kind, msg) {
  const el = currentView === "monitor" ? monitorStatusEl : statusEl;
  const other = currentView === "monitor" ? statusEl : monitorStatusEl;
  // Clear the other view's status so switching doesn't show stale messages
  other.className = "status";
  other.textContent = "";

  if (!msg) {
    el.className = "status";
    el.textContent = "";
    return;
  }
  el.classList.remove("visible");
  void el.offsetWidth; // re-trigger animation
  el.className = `status visible ${kind}`;
  el.textContent = msg;
}

/* ── View switching ─────────────────────────────────────────────────── */
function switchView(name) {
  if (name === currentView) return;
  // When navigating back to Setup, make sure the form is visible and the
  // "You're set." panel is hidden (covers the Edit-credentials round-trip).
  if (name === "setup") {
    $("youre-set").hidden = true;
    $("wizard").hidden = false;
  }
  $$(".view").forEach((v) => v.classList.toggle("active", v.id === `view-${name}`));
  $$(".view-tab").forEach((t) => {
    const on = t.dataset.view === name;
    t.classList.toggle("active", on);
    t.setAttribute("aria-selected", on ? "true" : "false");
  });
  currentView = name;
  // Clear stale status from the view we just left
  setStatus("", "");

  if (name === "monitor") {
    startStatusPolling();
    refreshMonitorStats();
    refreshAnalyzer();
  } else {
    stopStatusPolling();
  }
}

function enableMonitorTab(enabled) {
  const tab = document.querySelector('.view-tab[data-view="monitor"]');
  tab.disabled = !enabled;
  if (enabled) tab.removeAttribute("title");
  else tab.title = "Available after saving credentials";
}

$$(".view-tab").forEach((tab) => {
  tab.addEventListener("click", () => {
    if (tab.disabled) return;
    switchView(tab.dataset.view);
  });
});

/* ── Step progress (setup view) ─────────────────────────────────────── */
function setStep(n, stateName) {
  $$(".step").forEach((el) => {
    const num = parseInt(el.dataset.step, 10);
    if (num < n) el.dataset.state = "done";
    else if (num === n) el.dataset.state = stateName;
    else el.dataset.state = "upcoming";
  });
}

/* ── Auth segment (inside setup view) ───────────────────────────────── */
$$(".seg-btn").forEach((btn) => {
  btn.addEventListener("click", () => {
    $$(".seg-btn").forEach((b) => {
      b.classList.remove("active");
      b.setAttribute("aria-selected", "false");
    });
    $$(".seg-panel").forEach((p) => p.classList.remove("active"));
    btn.classList.add("active");
    btn.setAttribute("aria-selected", "true");
    $(btn.dataset.target).classList.add("active");
  });
});

/* ── URL validation badge + PAT deep-link ────────────────────────────── */
const urlInput = $("url");
const urlBadge = $("url-badge");
const patLink = $("pat-link");

function updateUrlBadge() {
  const v = urlInput.value.trim();
  if (!v) { urlBadge.dataset.state = "empty"; urlBadge.textContent = ""; return; }
  try {
    const u = new URL(v);
    if (u.protocol === "https:") { urlBadge.dataset.state = "ok";   urlBadge.textContent = "HTTPS ✓"; }
    else if (u.protocol === "http:") { urlBadge.dataset.state = "warn"; urlBadge.textContent = "HTTP ⚠"; }
    else { urlBadge.dataset.state = "bad"; urlBadge.textContent = "✗"; }
  } catch {
    urlBadge.dataset.state = "bad";
    urlBadge.textContent = "✗";
  }
}

function updatePatLink() {
  const v = urlInput.value.trim();
  try {
    const u = new URL(v);
    const base = `${u.protocol}//${u.host}`;
    patLink.href = `${base}/plugins/personalaccesstokens/usertokens.action`;
    patLink.setAttribute("aria-disabled", "false");
  } catch {
    patLink.href = "#";
    patLink.setAttribute("aria-disabled", "true");
  }
}

urlInput.addEventListener("input", () => { updateUrlBadge(); updatePatLink(); });
updateUrlBadge();
updatePatLink();

/* ── Initial load ───────────────────────────────────────────────────── */
async function init() {
  try {
    const cfg = await invoke("load_existing_config");
    $("url").value = cfg.url || "";
    updateUrlBadge();
    updatePatLink();
    $("username").value = cfg.username || "";
    $("password").value = cfg.password || "";
    $("token").value = cfg.token || "";
    $("ssl-verify").checked = cfg.sslVerify ?? true;
    $("install-dir").value = cfg.installDir || "";

    // Proxy: use saved value; otherwise prefill from system-detected proxy
    // (without committing it until the user hits Test/Save).
    const savedProxy = cfg.proxyUrl || "";
    const detectedProxy = cfg.detectedProxyUrl || "";
    const detectedKind = cfg.detectedProxyKind || "";
    $("proxy-url").value = savedProxy || detectedProxy;
    const hint = $("proxy-hint");
    if (!savedProxy && detectedProxy) {
      const label = detectedKind === "pac" ? "PAC script URL" : "static proxy";
      hint.textContent = `Detected from Windows (${label}): ${detectedProxy}. Edit or clear if incorrect.`;
    } else if (!savedProxy && detectedKind === "wpad") {
      hint.textContent =
        "Windows is set to auto-detect a proxy (WPAD). Paste your company's .pac URL if you know it — otherwise leave blank.";
    } else if (savedProxy) {
      hint.textContent = "Saved proxy. Clear the field to go direct. Accepts http://host:port or a .pac URL.";
    }

    if (cfg.token) {
      document.querySelector('[data-target="auth-token"]').click();
    } else if (cfg.username) {
      document.querySelector('[data-target="auth-basic"]').click();
    }

    if (cfg.confluenceConfigured) {
      enableMonitorTab(true);
      paintMonitorDetails(cfg);
      switchView("monitor");
    }
  } catch (e) {
    setStatus("err", "Could not read existing config: " + e);
  }
}

function paintMonitorDetails(cfg) {
  $("detail-url").textContent = cfg.url || "—";
  $("detail-auth").textContent = cfg.token
    ? "Personal access token"
    : cfg.username
      ? `User — ${cfg.username}`
      : "—";
  $("detail-path").textContent = cfg.installDir || "—";
}

/* ── Folder picker ─────────────────────────────────────────────────── */
$("pick-dir").addEventListener("click", async () => {
  const picked = await openDialog({
    directory: true,
    defaultPath: $("install-dir").value || undefined,
  });
  if (picked) $("install-dir").value = picked;
});

/* ── Test connection ───────────────────────────────────────────────── */
$("btn-test").addEventListener("click", async () => {
  state = "testing";
  setStep(2, "active");
  setStatus("info", "Testing connection…");
  $("btn-test").disabled = true;

  try {
    const result = await invoke("test_connection", {
      args: {
        url: $("url").value.trim(),
        username: $("username").value,
        password: $("password").value,
        token: $("token").value,
        sslVerify: $("ssl-verify").checked,
        proxyUrl: $("proxy-url").value.trim(),
      },
    });

    if (result.success) {
      state = "tested";
      setStep(3, "active");
      setStatus("ok", result.message);
      $("btn-save").disabled = false;
    } else {
      state = "idle";
      setStep(1, "active");
      setStatus("err", result.message);
      $("btn-save").disabled = true;
    }
  } catch (e) {
    state = "idle";
    setStep(1, "active");
    setStatus("err", "Unexpected error: " + e);
    $("btn-save").disabled = true;
  } finally {
    $("btn-test").disabled = false;
  }
});

/* ── Save & finish ─────────────────────────────────────────────────── */
$("btn-save").addEventListener("click", async () => {
  state = "saving";
  setStatus("info", "Writing configuration…");
  $("btn-save").disabled = true;
  $("btn-test").disabled = true;

  try {
    const result = await invoke("save_config", {
      args: {
        url: $("url").value.trim(),
        username: $("username").value,
        password: $("password").value,
        token: $("token").value,
        sslVerify: $("ssl-verify").checked,
        installDir: $("install-dir").value,
        proxyUrl: $("proxy-url").value.trim(),
      },
    });

    if (result.success) {
      state = "idle";
      setStep(3, "done");
      // Prepare the monitor view in the background, but let the user
      // read the "You're set." checklist before advancing.
      const latest = await invoke("load_existing_config");
      enableMonitorTab(true);
      paintMonitorDetails(latest);
      setStatus("ok", result.message);
      $("wizard").hidden = true;
      $("youre-set").hidden = false;
    } else {
      state = "tested";
      setStatus("err", result.message);
      $("btn-save").disabled = false;
      $("btn-test").disabled = false;
    }
  } catch (e) {
    state = "tested";
    setStatus("err", "Unexpected error: " + e);
    $("btn-save").disabled = false;
    $("btn-test").disabled = false;
  }
});

$("btn-to-monitor").addEventListener("click", () => {
  $("youre-set").hidden = true;
  $("wizard").hidden = false;
  switchView("monitor");
});

/* ── Monitor actions ───────────────────────────────────────────────── */
$("btn-edit-creds").addEventListener("click", () => {
  switchView("setup");
});

$("btn-stop").addEventListener("click", async () => {
  try {
    const result = await invoke("stop_server");
    setStatus(result.success ? "ok" : "info", result.message);
    await refreshStatus();
  } catch (e) {
    setStatus("err", "Unexpected error: " + e);
  }
});

$("btn-remove").addEventListener("click", async () => {
  const ok = confirm(
    "Turn off Confluence MCP and remove it from Claude Desktop?\n\n" +
      "Stops any running instance, unregisters the MCP server entry, and deletes the extracted binary.\n\n" +
      "You must restart Claude Desktop afterwards for the change to take effect."
  );
  if (!ok) return;

  try {
    await invoke("stop_server").catch(() => null);
    const result = await invoke("remove_config", {
      installDir: $("install-dir").value,
    });
    if (result.success) {
      // Configuration no longer exists — hide the monitor tab and send user back to setup.
      enableMonitorTab(false);
      switchView("setup");
      setStatus("ok", result.message);
    } else {
      setStatus("err", result.message);
    }
  } catch (e) {
    setStatus("err", "Unexpected error: " + e);
  }
});

/* ── Status polling ─────────────────────────────────────────────────── */
const statusDot = $("status-dot");
const statusText = $("status-text");
const statusMeta = $("status-meta");
const stateWord = $("monitor-state-word");

async function refreshStatus() {
  try {
    const s = await invoke("server_status");
    if (s.running) {
      statusDot.dataset.state = "running";
      statusText.textContent = "Running";
      statusMeta.textContent = `PID ${s.pid} · ${s.memoryMb} MB`;
      stateWord.textContent = "running";
      stateWord.className = "running";
      $("btn-stop").disabled = false;
    } else {
      statusDot.dataset.state = "stopped";
      statusText.textContent = "Not running";
      statusMeta.textContent =
        "The MCP server is spawned by Claude Desktop on startup — quit and relaunch Claude Desktop to see it here.";
      stateWord.textContent = "stopped";
      stateWord.className = "stopped";
      $("btn-stop").disabled = true;
    }
  } catch (e) {
    statusDot.dataset.state = "unavailable";
    statusText.textContent = "Status unavailable";
    statusMeta.textContent = String(e);
    stateWord.textContent = "unavailable";
    stateWord.className = "unavailable";
    $("btn-stop").disabled = true;
  }
}

function startStatusPolling() {
  if (statusTimer) return;
  refreshStatus();
  refreshMonitorStats();
  refreshAnalyzer();
  statusTimer = setInterval(() => {
    refreshStatus();
    tickCounter += 1;
    if (tickCounter % 2 === 0) {
      refreshMonitorStats();
      refreshAnalyzer();
    }
  }, 3000);
}

function stopStatusPolling() {
  if (!statusTimer) return;
  clearInterval(statusTimer);
  statusTimer = null;
}

/* ── Monitor: stats rendering ───────────────────────────────────────── */
let tickCounter = 0;

async function refreshMonitorStats() {
  let s;
  try {
    s = await invoke("get_stats");
  } catch (_) {
    return; // not configured yet, silent no-op
  }
  $("today-calls").textContent = s.todayCalls;
  $("today-tokens").textContent = formatTokens(s.todayTokens);
  $("today-errors").textContent = s.todayErrors;

  renderTokenChart(s.sevenDayTokens);

  const errList = $("errors-list");
  errList.innerHTML = "";
  for (const e of s.recentErrors) {
    const li = document.createElement("li");
    const date = new Date(e.ts * 1000);
    const hhmm = date.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
    li.textContent = `${hhmm} · ${e.tool} · ${e.status} · ${e.message}`;
    errList.appendChild(li);
  }
  $("errors-summary").textContent = `Recent errors (${s.recentErrors.length})`;
}

function formatTokens(n) {
  if (n < 1000) return String(n);
  if (n < 1_000_000) return (n / 1000).toFixed(n < 10000 ? 1 : 0) + "k";
  return (n / 1_000_000).toFixed(1) + "M";
}

function renderTokenChart(days) {
  const svg = $("chart-tokens");
  const axis = $("chart-axis");
  while (svg.firstChild) svg.removeChild(svg.firstChild);
  axis.innerHTML = "";

  const max = Math.max(1, ...days.map((d) => d.tokens));
  const total = days.reduce((a, d) => a + d.tokens, 0);
  $("tokens-total").textContent = formatTokens(total);

  const W = 300, H = 90, pad = 12;
  const slot = (W - 2 * pad) / days.length;
  const barW = Math.max(2, slot * 0.7);
  const NS = "http://www.w3.org/2000/svg";

  days.forEach((d, i) => {
    const h = (d.tokens / max) * (H - 20);
    const x = pad + i * slot + (slot - barW) / 2;
    const y = H - 12 - h;
    const rect = document.createElementNS(NS, "rect");
    rect.setAttribute("x", x);
    rect.setAttribute("y", y);
    rect.setAttribute("width", barW);
    rect.setAttribute("height", Math.max(1, h));
    rect.setAttribute("rx", 2);
    rect.setAttribute("fill", "#60a5fa");
    if (d.tokens === 0) rect.setAttribute("opacity", "0.2");
    svg.appendChild(rect);

    const dow = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
    const dayLabel = document.createElement("span");
    const dt = new Date(d.date + "T00:00:00");
    dayLabel.textContent = dow[dt.getDay()];
    axis.appendChild(dayLabel);
  });

  // baseline
  const line = document.createElementNS(NS, "line");
  line.setAttribute("x1", 0); line.setAttribute("x2", W);
  line.setAttribute("y1", H - 12); line.setAttribute("y2", H - 12);
  line.setAttribute("stroke", "rgba(100,100,120,0.4)");
  line.setAttribute("stroke-width", "1");
  svg.appendChild(line);
}

/* ── Invalidate verified state when inputs change ──────────────────── */
["url", "username", "password", "token", "proxy-url"].forEach((id) => {
  $(id).addEventListener("input", () => {
    if (state === "tested") {
      state = "idle";
      setStep(1, "active");
      setStatus("", "");
      $("btn-save").disabled = true;
    }
  });
});

$("ssl-verify").addEventListener("change", () => {
  if (state === "tested") {
    state = "idle";
    setStep(1, "active");
    setStatus("", "");
    $("btn-save").disabled = true;
  }
});

/* ── Enter submits the most advanced available action ──────────────── */
$("wizard").addEventListener("keydown", (e) => {
  if (e.key !== "Enter" || e.target.tagName === "BUTTON") return;
  e.preventDefault();
  if (!$("btn-save").disabled) $("btn-save").click();
  else if (!$("btn-test").disabled) $("btn-test").click();
});

init();

/* ── Monitor: Test live ─────────────────────────────────────────────── */
$("btn-test-live").addEventListener("click", async () => {
  const btn = $("btn-test-live");
  btn.disabled = true;
  setStatus("info", "Testing live connection…");
  try {
    const r = await invoke("test_live_connection");
    if (r.success) setStatus("ok", r.message);
    else setStatus("err", r.message);
  } catch (e) {
    setStatus("err", String(e));
  } finally {
    btn.disabled = false;
  }
});

/* ── Monitor: analyzer sidebar ──────────────────────────────────────── */
async function refreshAnalyzer() {
  let tips = [];
  try {
    tips = await invoke("get_recommendations");
  } catch (_) {
    return;
  }
  const list = $("analyzer-list");
  const empty = $("analyzer-empty");
  list.innerHTML = "";
  if (tips.length === 0) {
    empty.hidden = false;
    return;
  }
  empty.hidden = true;
  for (const t of tips) {
    const li = document.createElement("li");
    const title = document.createElement("div");
    title.className = "tip-title";
    title.textContent = t.title;
    const detail = document.createElement("div");
    detail.className = "tip-detail";
    detail.textContent = t.detail;
    li.appendChild(title);
    li.appendChild(detail);
    list.appendChild(li);
  }
}

/* ── Monitor: Copy diagnostics ──────────────────────────────────────── */
$("btn-copy-diag").addEventListener("click", async () => {
  try {
    const msg = await invoke("copy_diagnostics");
    setStatus("ok", msg);
  } catch (e) {
    setStatus("err", String(e));
  }
});

/* ── Monitor: Open Claude log ───────────────────────────────────────── */
$("open-claude-log").addEventListener("click", async (ev) => {
  ev.preventDefault();
  try {
    await invoke("open_claude_log");
  } catch (e) {
    setStatus("err", String(e));
  }
});
