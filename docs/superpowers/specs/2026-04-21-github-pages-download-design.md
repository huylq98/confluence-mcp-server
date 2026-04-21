# GitHub Pages Landing Page — Design Spec

**Date:** 2026-04-21  
**Status:** Approved

---

## Goal

Publish a polished marketing/download page at `huylq33.github.io/confluence-mcp-server` so users have a clean, linkable entry point to download ConfluenceConnect and understand what it does — without needing to read the GitHub README.

---

## Architecture

| Concern | Decision |
|---|---|
| Source | `docs/index.html` on `master` — hand-authored HTML/CSS template |
| Version placeholder | `{{VERSION}}` in the template, replaced at deploy time |
| Output | `gh-pages` branch, root `index.html` with `{{VERSION}}` substituted |
| Deploy trigger | Push to `master` via GitHub Actions |
| GitHub Pages setting | Branch: `gh-pages`, folder: root `/` |

The `docs/` folder on `master` holds only the source template. No generated files land on `master`. The `gh-pages` branch is managed entirely by CI.

---

## GitHub Actions Workflow

File: `.github/workflows/pages.yml`

**Trigger:** `push` to `master`

**Steps:**
1. Checkout repository
2. Extract version from `Cargo.toml` using `grep` + `sed` (e.g. `0.2.1`)
3. Replace `{{VERSION}}` in `docs/index.html` → write to a temp output file
4. Force-push the output file (as `index.html`) to the `gh-pages` branch

No build step, no bundler, no Node — pure shell + git.

---

## Page Sections

### 1. Hero
- Background: `#0d1b2a` (dark navy)
- Product name: **Confluence Connect**
- Tagline: *Ask Claude about your Confluence pages*
- Download button: links to `https://github.com/huylq33/confluence-mcp-server/releases/latest/download/ConfluenceConnect.exe`
- Version badge: `v{{VERSION}}`
- SmartScreen note: one line — "Windows may show an unknown publisher warning — click More info → Run anyway"

### 2. How it works
- Background: white
- 3 numbered steps with emoji icons:
  1. 📥 Download `ConfluenceConnect.exe`
  2. 🔧 Run the wizard — enter your Confluence URL and a Personal Access Token
  3. 💬 Fully quit and reopen Claude Desktop — ask Claude about your pages

### 3. What you can do
- Background: `#f5f7fa` (light grey)
- 5 capability bullets:
  - Search pages with full-text or CQL queries
  - Fetch any page by URL, title, or ID
  - List all accessible Confluence spaces
  - Read page comments (inline and footer)
  - List file attachments with download links

### 4. Requirements
- Background: white
- Simple list:
  - Windows 10 or later
  - Claude Desktop (latest version)
  - Confluence Server or Data Center 7.x+

### 5. Footer
- Background: `#0d1b2a`
- MIT license link
- GitHub repository link
- "built by huylq33"

---

## Visual Language

| Element | Value |
|---|---|
| Hero background | `#0d1b2a` |
| Accent / button | `#4fc3f7` (light blue) |
| Button text | `#0d1b2a` |
| Body background | `#ffffff` |
| Alternate section | `#f5f7fa` |
| Font stack | `Inter, system-ui, sans-serif` (no external dependency) |
| No images | Page relies on copy, spacing, and icons only |

---

## Constraints

- No screenshots available — design must work with text and emoji/icon elements only
- No signing certificate — SmartScreen disclaimer is required
- Download link must point to GitHub Releases `latest` (not a hardcoded version URL)
- No external runtime dependencies in the workflow (no Node, no Python)
