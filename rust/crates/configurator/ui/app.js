const invoke = window.__TAURI__.core.invoke;
const openDialog = window.__TAURI__.dialog.open;

const $ = (id) => document.getElementById(id);
const statusEl = $("status");

function setStatus(kind, msg) {
  statusEl.className = "status " + kind;
  statusEl.textContent = msg;
}

// Tab switching
document.querySelectorAll(".tab").forEach(tab => {
  tab.addEventListener("click", () => {
    document.querySelectorAll(".tab").forEach(t => t.classList.remove("active"));
    document.querySelectorAll(".panel").forEach(p => p.classList.remove("active"));
    tab.classList.add("active");
    $(tab.dataset.target).classList.add("active");
  });
});

async function init() {
  try {
    const cfg = await invoke("load_existing_config");
    $("url").value = cfg.url;
    $("username").value = cfg.username;
    $("password").value = cfg.password;
    $("token").value = cfg.token;
    $("ssl-verify").checked = cfg.sslVerify;
    $("install-dir").value = cfg.installDir;
    if (cfg.token) {
      document.querySelector('[data-target="auth-token"]').click();
    } else if (cfg.username) {
      document.querySelector('[data-target="auth-basic"]').click();
    }
  } catch (e) {
    setStatus("err", "Failed to load existing config: " + e);
  }
}

$("pick-dir").addEventListener("click", async () => {
  const picked = await openDialog({ directory: true, defaultPath: $("install-dir").value });
  if (picked) $("install-dir").value = picked;
});

$("btn-test").addEventListener("click", async () => {
  setStatus("info", "Testing connection…");
  const result = await invoke("test_connection", {
    args: {
      url: $("url").value,
      username: $("username").value,
      password: $("password").value,
      token: $("token").value,
      sslVerify: $("ssl-verify").checked,
    }
  });
  setStatus(result.success ? "ok" : "err", result.message);
  $("btn-save").disabled = !result.success;
});

$("btn-save").addEventListener("click", async () => {
  setStatus("info", "Saving configuration…");
  const result = await invoke("save_config", {
    args: {
      url: $("url").value,
      username: $("username").value,
      password: $("password").value,
      token: $("token").value,
      sslVerify: $("ssl-verify").checked,
      installDir: $("install-dir").value,
    }
  });
  setStatus(result.success ? "ok" : "err", result.message);
});

$("btn-remove").addEventListener("click", async () => {
  if (!confirm("Remove Confluence MCP from Claude Desktop and delete the installed server binary?")) return;
  const result = await invoke("remove_config", { installDir: $("install-dir").value });
  setStatus(result.success ? "ok" : "err", result.message);
});

init();
