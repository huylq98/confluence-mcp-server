const invoke = window.__TAURI__.core.invoke;
const openDialog = window.__TAURI__.dialog.open;

const $ = (id) => document.getElementById(id);
const $$ = (sel) => document.querySelectorAll(sel);

const statusEl = $("status");
const successPanel = $("success");
const successPath = $("success-path");

let state = "idle"; // idle → testing → tested → saving → done

function setStatus(kind, msg) {
  if (!msg) {
    statusEl.className = "status";
    statusEl.textContent = "";
    return;
  }
  // Re-trigger animation if we're replacing an existing status
  statusEl.classList.remove("visible");
  void statusEl.offsetWidth; // force reflow
  statusEl.className = `status visible ${kind}`;
  statusEl.textContent = msg;
}

function setStep(n, stateName) {
  $$(".step").forEach((el) => {
    const num = parseInt(el.dataset.step, 10);
    if (num < n) el.dataset.state = "done";
    else if (num === n) el.dataset.state = stateName;
    else el.dataset.state = "upcoming";
  });
}

// Segment (tab) switching
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

// Initial load — pull existing config if it's there
async function init() {
  try {
    const cfg = await invoke("load_existing_config");
    $("url").value = cfg.url || "";
    $("username").value = cfg.username || "";
    $("password").value = cfg.password || "";
    $("token").value = cfg.token || "";
    $("ssl-verify").checked = cfg.sslVerify ?? true;
    $("install-dir").value = cfg.installDir || "";

    if (cfg.token) {
      document.querySelector('[data-target="auth-token"]').click();
    } else if (cfg.username) {
      document.querySelector('[data-target="auth-basic"]').click();
    }

    if (cfg.confluenceConfigured) {
      setStatus(
        "info",
        "Existing configuration loaded. Adjust and save to update it."
      );
    }
  } catch (e) {
    setStatus("err", "Could not read existing config: " + e);
  }
}

// Folder picker
$("pick-dir").addEventListener("click", async () => {
  const picked = await openDialog({
    directory: true,
    defaultPath: $("install-dir").value || undefined,
  });
  if (picked) $("install-dir").value = picked;
});

// Test connection
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

// Save & finish
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
      },
    });

    if (result.success) {
      state = "done";
      setStep(3, "done");
      showSuccess(result);
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

// Remove
$("btn-remove").addEventListener("click", async () => {
  const ok = confirm(
    "Remove Confluence MCP from Claude Desktop?\n\n" +
      "This unregisters the MCP server entry and deletes the extracted server binary."
  );
  if (!ok) return;

  try {
    const result = await invoke("remove_config", {
      installDir: $("install-dir").value,
    });
    setStatus(result.success ? "ok" : "err", result.message);
  } catch (e) {
    setStatus("err", "Unexpected error: " + e);
  }
});

function showSuccess(result) {
  successPanel.classList.remove("hidden");
  successPanel.setAttribute("aria-hidden", "false");
  if (result && result.serverPath) {
    successPath.textContent = "Server installed at\n" + result.serverPath;
  }
}

// If the user edits any auth/URL input after a successful test,
// invalidate the tested state so they're forced to re-test.
["url", "username", "password", "token"].forEach((id) => {
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

// Enter submits the most advanced action available
$("wizard").addEventListener("keydown", (e) => {
  if (e.key !== "Enter" || e.target.tagName === "BUTTON") return;
  e.preventDefault();
  if (!$("btn-save").disabled) $("btn-save").click();
  else if (!$("btn-test").disabled) $("btn-test").click();
});

init();
