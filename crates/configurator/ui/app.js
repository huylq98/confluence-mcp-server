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

/* ── Initial load ───────────────────────────────────────────────────── */
async function init() {
  try {
    const cfg = await invoke("load_existing_config");
    $("url").value = cfg.url || "";
    $("username").value = cfg.username || "";
    $("password").value = cfg.password || "";
    $("token").value = cfg.token || "";
    $("ssl-verify").checked = cfg.sslVerify ?? true;
    $("install-dir").value = cfg.installDir || "";

    // Proxy: use saved value; otherwise prefill from system-detected proxy
    // (without committing it until the user hits Test/Save).
    const savedProxy = cfg.proxyUrl || "";
    const detectedProxy = cfg.detectedProxyUrl || "";
    $("proxy-url").value = savedProxy || detectedProxy;
    const hint = $("proxy-hint");
    if (!savedProxy && detectedProxy) {
      hint.textContent = `Detected from Windows system proxy: ${detectedProxy}. Edit or clear if incorrect.`;
    } else if (savedProxy) {
      hint.textContent = "Saved proxy. Clear the field to go direct.";
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
      // Repaint the monitor view with the new config and switch to it.
      const latest = await invoke("load_existing_config");
      enableMonitorTab(true);
      paintMonitorDetails(latest);
      switchView("monitor");
      setStatus("ok", result.message);
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
  statusTimer = setInterval(refreshStatus, 3000);
}

function stopStatusPolling() {
  if (!statusTimer) return;
  clearInterval(statusTimer);
  statusTimer = null;
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
