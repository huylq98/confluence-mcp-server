# GitHub Pages Landing Page Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Publish a polished marketing/download page at `huylq33.github.io/confluence-mcp-server` that auto-deploys on every push to `master`.

**Architecture:** A hand-authored `docs/index.html` template containing a `{{VERSION}}` placeholder lives on `master`. A GitHub Actions workflow triggers on push to `master`, extracts the workspace version from `Cargo.toml`, substitutes the placeholder, and force-pushes the rendered file to the `gh-pages` branch. GitHub Pages is pointed at the root of `gh-pages`.

**Tech Stack:** HTML, CSS, GitHub Actions, `sed`, `peaceiris/actions-gh-pages@v4`

---

## File Map

| Action | Path | Responsibility |
|---|---|---|
| Create | `docs/index.html` | Marketing page template — contains `{{VERSION}}` placeholder |
| Create | `.github/workflows/pages.yml` | Deploy workflow — extracts version, substitutes, pushes to `gh-pages` |

---

## Task 1: Create the HTML template

**Files:**
- Create: `docs/index.html`

- [ ] **Step 1: Create `docs/index.html`**

Write this file exactly:

```html
<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>Confluence Connect — Ask Claude about your Confluence pages</title>
  <style>
    * { box-sizing: border-box; margin: 0; padding: 0; }
    body { font-family: Inter, system-ui, -apple-system, sans-serif; color: #1a2332; line-height: 1.6; }

    /* Hero */
    .hero { background: #0d1b2a; color: #fff; padding: 80px 24px; text-align: center; }
    .hero h1 { font-size: 2.8rem; font-weight: 700; letter-spacing: -0.5px; margin-bottom: 16px; }
    .hero .tagline { font-size: 1.25rem; color: #a8c4d8; margin-bottom: 40px; }
    .download-btn {
      display: inline-block;
      background: #4fc3f7;
      color: #0d1b2a;
      font-size: 1.1rem;
      font-weight: 700;
      padding: 16px 36px;
      border-radius: 8px;
      text-decoration: none;
      transition: background 0.2s;
    }
    .download-btn:hover { background: #81d4fa; }
    .version-badge { margin-top: 14px; font-size: 0.85rem; color: #7aa8c0; }
    .smartscreen-note { margin-top: 10px; font-size: 0.8rem; color: #5a7a8f; }

    /* Sections */
    section { padding: 72px 24px; }
    .container { max-width: 860px; margin: 0 auto; }
    section h2 { font-size: 1.8rem; font-weight: 700; margin-bottom: 40px; text-align: center; color: #0d1b2a; }

    /* Steps */
    .steps { display: flex; gap: 32px; justify-content: center; flex-wrap: wrap; }
    .step { flex: 1; min-width: 200px; max-width: 260px; text-align: center; }
    .step-icon { font-size: 2.5rem; margin-bottom: 16px; }
    .step-num { font-size: 0.75rem; font-weight: 700; text-transform: uppercase; letter-spacing: 1px; color: #4fc3f7; margin-bottom: 8px; }
    .step h3 { font-size: 1rem; font-weight: 600; margin-bottom: 8px; }
    .step p { font-size: 0.9rem; color: #4a5568; }

    /* Capabilities */
    .alt { background: #f5f7fa; }
    .capabilities { list-style: none; max-width: 560px; margin: 0 auto; }
    .capabilities li { padding: 12px 0; font-size: 1rem; border-bottom: 1px solid #e2e8f0; display: flex; gap: 12px; align-items: flex-start; }
    .capabilities li:last-child { border-bottom: none; }
    .capabilities li::before { content: "→"; color: #4fc3f7; font-weight: 700; flex-shrink: 0; margin-top: 1px; }

    /* Requirements */
    .requirements { list-style: none; max-width: 400px; margin: 0 auto; }
    .requirements li { padding: 10px 0; font-size: 0.95rem; border-bottom: 1px solid #e2e8f0; display: flex; gap: 10px; }
    .requirements li:last-child { border-bottom: none; }
    .requirements li::before { content: "✓"; color: #4fc3f7; font-weight: 700; }

    /* Footer */
    footer { background: #0d1b2a; color: #7aa8c0; text-align: center; padding: 40px 24px; font-size: 0.875rem; }
    footer a { color: #4fc3f7; text-decoration: none; }
    footer a:hover { text-decoration: underline; }
    footer .sep { margin: 0 10px; }
  </style>
</head>
<body>

<header class="hero">
  <div class="container">
    <h1>Confluence Connect</h1>
    <p class="tagline">Ask Claude about your Confluence pages</p>
    <a class="download-btn" href="https://github.com/huylq33/confluence-mcp-server/releases/latest/download/ConfluenceConnect.exe">
      &#8203;⬇ Download for Windows
    </a>
    <p class="version-badge">v{{VERSION}}</p>
    <p class="smartscreen-note">Windows may show an unknown publisher warning — click <strong>More info → Run anyway</strong></p>
  </div>
</header>

<section>
  <div class="container">
    <h2>How it works</h2>
    <div class="steps">
      <div class="step">
        <div class="step-icon">📥</div>
        <div class="step-num">Step 1</div>
        <h3>Download</h3>
        <p>Download <code>ConfluenceConnect.exe</code> and run it. No installer, no runtime required.</p>
      </div>
      <div class="step">
        <div class="step-icon">🔧</div>
        <div class="step-num">Step 2</div>
        <h3>Configure</h3>
        <p>Enter your Confluence URL and a Personal Access Token (or username and password). Click <strong>Test connection</strong>, then <strong>Save &amp; finish</strong>.</p>
      </div>
      <div class="step">
        <div class="step-icon">💬</div>
        <div class="step-num">Step 3</div>
        <h3>Ask Claude</h3>
        <p>Fully quit Claude Desktop and reopen it. Claude can now read your Confluence pages.</p>
      </div>
    </div>
  </div>
</section>

<section class="alt">
  <div class="container">
    <h2>What you can do</h2>
    <ul class="capabilities">
      <li>Search pages with full-text or CQL queries across all spaces</li>
      <li>Fetch any page by URL, title, or numeric ID</li>
      <li>List all accessible Confluence spaces</li>
      <li>Read page comments — inline and footer</li>
      <li>List file attachments with download links</li>
    </ul>
  </div>
</section>

<section>
  <div class="container">
    <h2>Requirements</h2>
    <ul class="requirements">
      <li>Windows 10 or later</li>
      <li>Claude Desktop (latest version)</li>
      <li>Confluence Server or Data Center 7.x+</li>
    </ul>
  </div>
</section>

<footer>
  <p>
    <a href="https://github.com/huylq33/confluence-mcp-server/blob/master/LICENSE">MIT License</a>
    <span class="sep">·</span>
    <a href="https://github.com/huylq33/confluence-mcp-server">GitHub</a>
    <span class="sep">·</span>
    built by huylq33
  </p>
</footer>

</body>
</html>
```

- [ ] **Step 2: Verify `{{VERSION}}` placeholder is present**

```bash
grep '{{VERSION}}' docs/index.html
```

Expected output: `    <p class="version-badge">v{{VERSION}}</p>`

- [ ] **Step 3: Open `docs/index.html` in a browser and visually verify**

Open the file directly (no server needed). Check:
- Hero section renders with dark navy background
- Download button is visible and teal-coloured
- Three steps render side by side
- Version badge shows the literal text `v{{VERSION}}` (placeholder, not substituted yet)
- Footer has correct links

- [ ] **Step 4: Commit**

```bash
git add docs/index.html
git commit -m "feat(pages): add landing page HTML template"
```

---

## Task 2: Create the GitHub Actions deploy workflow

**Files:**
- Create: `.github/workflows/pages.yml`

- [ ] **Step 1: Create `.github/workflows/pages.yml`**

Write this file exactly:

```yaml
name: Deploy GitHub Pages

on:
  push:
    branches: [master]

permissions:
  contents: write

jobs:
  deploy:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Extract version from Cargo.toml
        id: version
        run: |
          VERSION=$(grep '^version' Cargo.toml | head -1 | sed 's/version = "\(.*\)"/\1/')
          echo "version=$VERSION" >> $GITHUB_OUTPUT

      - name: Substitute version into page
        run: |
          mkdir -p _site
          sed "s/{{VERSION}}/${{ steps.version.outputs.version }}/g" docs/index.html > _site/index.html

      - name: Deploy to gh-pages
        uses: peaceiris/actions-gh-pages@v4
        with:
          github_token: ${{ secrets.GITHUB_TOKEN }}
          publish_dir: ./_site
          force_orphan: true
```

- [ ] **Step 2: Verify version extraction locally**

```bash
grep '^version' Cargo.toml | head -1 | sed 's/version = "\(.*\)"/\1/'
```

Expected output: `0.2.1`

- [ ] **Step 3: Verify placeholder substitution locally**

```bash
mkdir -p _site
VERSION=$(grep '^version' Cargo.toml | head -1 | sed 's/version = "\(.*\)"/\1/')
sed "s/{{VERSION}}/$VERSION/g" docs/index.html > _site/index.html
grep 'version-badge' _site/index.html
```

Expected output: `    <p class="version-badge">v0.2.1</p>`

- [ ] **Step 4: Clean up local test output**

```bash
rm -rf _site
```

- [ ] **Step 5: Commit and push**

```bash
git add .github/workflows/pages.yml
git commit -m "ci: add GitHub Pages deploy workflow"
git push origin master
```

---

## Task 3: Enable GitHub Pages on the repository (manual step)

**No files — this is done in the GitHub web UI.**

- [ ] **Step 1: Open repository Settings**

Go to `https://github.com/huylq33/confluence-mcp-server/settings/pages`

- [ ] **Step 2: Set the source**

Under **Build and deployment**:
- Source: **Deploy from a branch**
- Branch: **`gh-pages`**
- Folder: **`/ (root)`**

Click **Save**.

> Note: The `gh-pages` branch is created by the workflow in Task 2. If you do this step before the first push triggers the workflow, the branch won't exist yet — that's fine. Set it after the first workflow run completes.

- [ ] **Step 3: Verify the page is live**

Wait ~1 minute after the workflow run completes, then open:

```
https://huylq33.github.io/confluence-mcp-server/
```

Check:
- Version badge shows `v0.2.1` (not the placeholder `{{VERSION}}`)
- Download button link is `https://github.com/huylq33/confluence-mcp-server/releases/latest/download/ConfluenceConnect.exe`
- All three sections render correctly
- Footer links point to the correct GitHub URLs

---

## Task 4: Add `.superpowers/` to `.gitignore`

**Files:**
- Modify: `.gitignore`

- [ ] **Step 1: Check if `.gitignore` exists and add the entry**

```bash
echo '.superpowers/' >> .gitignore
git add .gitignore
git commit -m "chore: ignore .superpowers brainstorm output"
```

If `.gitignore` doesn't exist yet, this command creates it.
