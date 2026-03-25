// State
let currentStep = 1;
let connectionResult = null;

// Wait for pywebview bridge
window.addEventListener("pywebviewready", async () => {
    await init();
});

async function init() {
    // Load existing config for pre-fill
    const existing = await pywebview.api.load_existing_config();
    if (existing.confluence_configured) {
        document.getElementById("url").value = existing.url || "";
        document.getElementById("ssl-verify").checked = existing.ssl_verify;

        if (existing.token) {
            setAuthMode("token");
            document.getElementById("token").value = existing.token;
        } else {
            setAuthMode("basic");
            document.getElementById("username").value = existing.username || "";
            document.getElementById("password").value = existing.password || "";
        }
    }
}

// --- Auth mode toggle ---
function setAuthMode(mode) {
    const basicBtn = document.getElementById("auth-basic-btn");
    const tokenBtn = document.getElementById("auth-token-btn");
    const basicFields = document.getElementById("basic-fields");
    const tokenFields = document.getElementById("token-fields");

    if (mode === "basic") {
        basicBtn.classList.add("active");
        tokenBtn.classList.remove("active");
        basicFields.style.display = "block";
        tokenFields.style.display = "none";
    } else {
        basicBtn.classList.remove("active");
        tokenBtn.classList.add("active");
        basicFields.style.display = "none";
        tokenFields.style.display = "block";
    }
}

// --- Step navigation ---
function goToStep(step) {
    currentStep = step;

    // Update step dots
    document.querySelectorAll(".step-dot").forEach((dot, i) => {
        const n = i + 1;
        dot.classList.remove("active", "done");
        if (n === currentStep) dot.classList.add("active");
        if (n < currentStep) dot.classList.add("done");
    });

    // Update step lines
    document.querySelectorAll(".step-line").forEach((line, i) => {
        line.classList.toggle("done", i + 1 < currentStep);
    });

    // Show/hide sections
    document.querySelectorAll(".section").forEach((sec) => {
        sec.classList.remove("active");
    });
    document.getElementById("step-" + step).classList.add("active");
}

// --- Test connection ---
async function testConnection() {
    const url = document.getElementById("url").value.trim();
    const username = document.getElementById("username").value.trim();
    const password = document.getElementById("password").value;
    const token = document.getElementById("token").value.trim();
    const sslVerify = document.getElementById("ssl-verify").checked;

    // Determine which auth mode is active
    const isToken = document.getElementById("auth-token-btn").classList.contains("active");

    goToStep(2);
    showTesting();

    const result = await pywebview.api.test_connection(
        url,
        isToken ? "" : username,
        isToken ? "" : password,
        isToken ? token : "",
        sslVerify
    );

    connectionResult = result;
    showTestResult(result);
}

function showTesting() {
    const container = document.getElementById("test-result");
    container.innerHTML = `
        <div class="spinner-container">
            <div class="spinner"></div>
            <div class="spinner-text">Testing connection to Confluence...</div>
        </div>
    `;
}

function showTestResult(result) {
    const container = document.getElementById("test-result");

    if (result.success) {
        let spacesHtml = "";
        if (result.spaces && result.spaces.length > 0) {
            spacesHtml = `
                <ul class="spaces-list">
                    ${result.spaces.map(s =>
                        `<li>${s.name} <span class="key">(${s.key})</span></li>`
                    ).join("")}
                </ul>
            `;
        }
        container.innerHTML = `
            <div class="status status-success">
                &#10003; ${result.message}
                ${spacesHtml}
            </div>
            <button class="btn btn-primary" onclick="goToStep(3); prepareStep3()">
                Next &rarr;
            </button>
        `;
    } else {
        container.innerHTML = `
            <div class="status status-error">
                ${result.message.replace(/\n/g, "<br>")}
            </div>
            <div class="btn-row">
                <button class="btn btn-secondary" onclick="goToStep(1)">
                    &larr; Back
                </button>
                <button class="btn btn-primary" onclick="testConnection()">
                    Try Again
                </button>
            </div>
        `;
    }
}

// --- Step 3: Save config ---
async function prepareStep3() {
    await updateConfigPreview();
}

async function updateConfigPreview() {
    const url = document.getElementById("url").value.trim();
    const username = document.getElementById("username").value.trim();
    const token = document.getElementById("token").value.trim();
    const sslVerify = document.getElementById("ssl-verify").checked;
    const isToken = document.getElementById("auth-token-btn").classList.contains("active");

    const exeInfo = await pywebview.api.get_exe_path();

    const env = { CONFLUENCE_URL: url, MCP_TRANSPORT: "stdio" };
    if (isToken && token) {
        env.CONFLUENCE_TOKEN = "********";
    } else {
        env.CONFLUENCE_USERNAME = username;
        env.CONFLUENCE_PASSWORD = "********";
    }
    if (!sslVerify) env.CONFLUENCE_SSL_VERIFY = "false";

    const entry = {
        command: exeInfo.command,
        args: exeInfo.args,
        env: env,
    };

    const preview = JSON.stringify({ mcpServers: { confluence: entry } }, null, 2);
    document.getElementById("config-preview").textContent = preview;
}

async function saveConfig() {
    const btn = document.getElementById("save-btn");
    btn.disabled = true;
    btn.textContent = "Saving...";

    const url = document.getElementById("url").value.trim();
    const username = document.getElementById("username").value.trim();
    const password = document.getElementById("password").value;
    const token = document.getElementById("token").value.trim();
    const sslVerify = document.getElementById("ssl-verify").checked;
    const isToken = document.getElementById("auth-token-btn").classList.contains("active");

    const result = await pywebview.api.save_config(
        url,
        isToken ? "" : username,
        isToken ? "" : password,
        isToken ? token : "",
        sslVerify
    );

    const container = document.getElementById("save-result");

    if (result.success) {
        container.innerHTML = `
            <div class="done-icon">&#10004;</div>
            <div class="status status-success">
                ${result.message}
            </div>
            <div class="done-path">
                Config file: ${result.config_path}
            </div>
            <div style="margin-top: 16px;">
                <button class="btn btn-primary" onclick="openClaudeDesktop()">
                    Open Claude Desktop
                </button>
            </div>
            <div style="margin-top: 10px;">
                <button class="btn btn-secondary" onclick="goToStep(1)">
                    &larr; Edit Settings
                </button>
            </div>
        `;
        btn.style.display = "none";
    } else {
        container.innerHTML = `
            <div class="status status-error">${result.message}</div>
        `;
        btn.disabled = false;
        btn.textContent = "Save Configuration";
    }
}

async function openClaudeDesktop() {
    await pywebview.api.open_claude_desktop();
}
