# Website Redesign: Eigen-Based Landing Page & Docs

**Date:** 2026-04-12
**Status:** Approved

## Overview

Port the youwhatknow documentation site from static HTML (`docs/`) to an eigen-powered site (`website/`). Preserve the existing dark/neon aesthetic in full — including all animations, visual effects, and interactive elements. Expand and revise the documentation content. Deploy via GitHub Actions to `youwhatknow.it`.

## Site Structure

```
website/
├── site.toml
├── CNAME
├── templates/
│   ├── _base.html
│   ├── index.html
│   ├── docs.html
│   └── _partials/
│       ├── nav.html
│       ├── footer.html
│       ├── effects.html
│       └── terminal.html
├── _data/
│   ├── nav.yaml
│   └── docs.yaml
├── static/
│   ├── css/
│   │   └── style.css
│   ├── js/
│   │   └── main.js
│   └── favicon.svg
└── dist/                    # gitignored
```

## Configuration (`site.toml`)

```toml
[site]
name = "youwhatknow"
base_url = "https://youwhatknow.it"

[site.seo]
title = "youwhatknow — Claude Code hook server"
description = "Stop Claude from re-reading files. A hook server that injects file summaries and tracks repeat reads per session."
og_type = "website"

[build]
fragments = true
fragment_dir = "_fragments"
content_block = "content"
minify = true

[build.critical_css]
enabled = true
max_inline_size = 50000
preload_full = true

[build.hints]
enabled = true
auto_detect_hero = true
prefetch_links = true

[build.content_hash]
enabled = true

[build.bundling]
enabled = true
css = true
js = true
tree_shake_css = true

[sitemap]
enabled = true

[robots]
enabled = true
sitemap = true
[[robots.rules]]
user_agent = "*"
allow = ["/"]
```

## Pages

### Landing Page (`index.html`)

Direct port of current `docs/index.html` as an eigen template extending `_base.html`. All content inline HTML (not markdown) due to heavy custom styling.

**Sections in order:**

1. **Hero** — badge with animated pulse dot, logo SVG with glow drop-shadow, "you what (k)now?" title with green-highlighted "(k)now", tagline, CTA button, nav links
2. **Terminal Demo** — styled terminal window (red/yellow/green dots) with animated line-by-line output showing youwhatknow intercepting file reads. Animation script in `static/js/main.js`
3. **The Problem** — conversation bubbles with accent-colored borders (Claude=amber, You=red, youwhatknow=green)
4. **How It Works** — 6-step flow with large serif numbers, code snippets, italic asides
5. **Working Set Eviction** — eviction threshold explanation
6. **Architecture** — ASCII diagram (Claude Sessions → HTTP hooks → daemon → indexes → tooling)
7. **Benefits** — 3×2 responsive grid of benefit cards with emoji icons
8. **Setup** — install commands, expandable details for manual hook config and config.toml

### Docs Page (`docs.html`)

Single scrollable page. Two-column layout: sticky sidebar TOC (260px) + content area.

**Sidebar:**
- Lists all section titles as anchor links (`#installation`, `#cli-reference`, etc.)
- Active section highlighted on scroll via intersection observer
- Collapses to hamburger on mobile (≤1024px)

**Content:** Loaded from `_data/docs.yaml`, each section rendered via `| markdown` filter:

```jinja
{% for section in docs %}
<section id="{{ section.slug }}" class="doc-section">
  <h2>{{ section.title }}</h2>
  {{ section.content | markdown }}
</section>
{% endfor %}
```

**Documentation sections** (revised and expanded):

1. **Installation** — curl installer, Nix flake, cargo install
2. **Quickstart** — one-command `youwhatknow setup`, what happens behind the scenes
3. **How It Works** — hook system overview, read tracking lifecycle, thresholds
4. **CLI Reference** — all subcommands (setup, status, summary, init, reset, serve, reindex, logs, restart, prime) with flags and examples
5. **Hook Behavior** — PreToolUse deny/allow logic, SessionStart project map injection, threshold mechanics
6. **Working Set Eviction** — eviction mechanics, configurable thresholds
7. **Session Management** — per-session tracking, session cleanup, activity tracking
8. **Daemon Configuration** — system-wide `~/.config/youwhatknow/config.toml`, all options with defaults
9. **Project Configuration** — per-project `.claude/youwhatknow.toml`, all options
10. **API Endpoints** — all HTTP endpoints with request/response examples
11. **Indexing & Symbols** — file discovery via git, tree-sitter symbol extraction, supported languages, Claude-generated descriptions, incremental re-indexing
12. **Storage Format** — TOML summary file structure, project-summary.toml and per-area files
13. **Environment Variables** — all config overrides
14. **Daemon Lifecycle** — PID files, startup, graceful shutdown, idle timeout
15. **Nix Integration** — flake usage, dev shell hooks
16. **Troubleshooting** — common issues, checking logs, restart, reindex

## Shared Components

### `_base.html`

Master layout template:

```html
<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <!-- Google Fonts: JetBrains Mono, Space Mono, Instrument Serif -->
  <link rel="stylesheet" href="{{ asset('css/style.css') }}">
  <script src="https://unpkg.com/htmx.org@2.0.4"></script>
  <link rel="icon" type="image/svg+xml" href="{{ asset('favicon.svg') }}">
  {% block head %}{% endblock %}
</head>
<body hx-boost="true">
  {% include "_partials/nav.html" %}
  {% include "_partials/effects.html" %}
  <main id="content">
    {% block content %}{% endblock %}
  </main>
  {% include "_partials/footer.html" %}
  <script src="{{ asset('js/main.js') }}"></script>
</body>
</html>
```

### `_partials/nav.html`

- Sticky top bar, `#0a0a0c` background, 1px bottom border
- Left: logo SVG linked to home
- Right: "Docs" link (active state on docs page), "GitHub" external link
- HTMX-boosted links for fragment navigation

### `_partials/footer.html`

- Logo, Docs link, GitHub link
- "Built with Rust + Tokio" tech note
- Quip: "No Claude was harmed in the making of this tool. Just mildly inconvenienced."

### `_partials/effects.html`

- Scanline overlay: fixed-position div with repeating-linear-gradient, pointer-events: none
- Fractal noise: inline SVG filter definition applied via CSS

### `_partials/terminal.html`

- Terminal demo component: styled window with red/yellow/green title-bar dots
- Animation container populated by JS

## CSS Design System (`static/css/style.css`)

Ported from current `docs/css/style.css` (864 lines).

**Preserved verbatim:**
- All CSS custom properties (colors, spacing, typography)
- Color palette: `#0a0a0c` bg, `#111114` card, `#39ff85` neon green, `#ffb830` amber, `#ff4f4f` red
- Font stack: JetBrains Mono 300/400/500/700, Space Mono 400/700, Instrument Serif italic/normal
- All component styles: hero, terminal, bubbles, steps, benefits grid, code blocks, callouts, config tables
- All animations: `fadeSlideUp`, `pulse`, `termFadeIn`, `blink`, reveal transitions
- Scanline and fractal noise overlay styles
- Responsive breakpoints at 900px and 600px

**Added:**
- Docs sidebar: sticky positioning, anchor link list, active state highlight (neon green left border), mobile hamburger toggle
- Docs two-column layout: sidebar (260px) + content area grid
- Markdown prose styles for `.doc-content`: headings, lists, inline code, tables, blockquotes, nested code blocks
- Sidebar responsive: collapses to off-screen drawer at ≤1024px
- Additional breakpoint at 1024px for sidebar collapse

**No new colors, fonts, or aesthetic changes.**

## JavaScript (`static/js/main.js`)

Combined from current `docs/js/main.js` and inline scripts:

- **Intersection observer** for `.reveal` class fade-slide-up animations (15% threshold)
- **Terminal demo animation** — generates lines with staggered setTimeout delays (300–500ms)
- **Sidebar active tracking** — intersection observer on `.doc-section` elements, highlights corresponding sidebar link
- **Sidebar mobile toggle** — hamburger open/close

## Data Files

### `_data/nav.yaml`

```yaml
- label: Docs
  url: /docs

- label: GitHub
  url: https://github.com/wavefunk/youwhatknow
  external: true
```

### `_data/docs.yaml`

Array of documentation sections. Each entry:

```yaml
- title: Installation
  slug: installation
  content: |
    Three ways to install youwhatknow:

    ### curl installer
    ```bash
    curl -fsSL https://youwhatknow.it/install.sh | bash
    ```
    ...
```

Content is revised and expanded from the current docs page. Each section's markdown is verified against the current source code for accuracy.

## GitHub Actions Deployment

**`.github/workflows/pages.yml`:**

```yaml
name: Deploy website

on:
  push:
    branches: [master]
    paths: [website/**]
  workflow_dispatch:

permissions:
  contents: read
  pages: write
  id-token: write

concurrency:
  group: pages
  cancel-in-progress: false

jobs:
  deploy:
    runs-on: ubuntu-latest
    environment:
      name: github-pages
      url: ${{ steps.deployment.outputs.page_url }}
    steps:
      - uses: actions/checkout@v4

      - name: Install eigen
        run: |
          curl -fsSL https://github.com/wavefunk/eigen/releases/latest/download/eigen-x86_64-unknown-linux-gnu.tar.gz | tar xz
          sudo mv eigen /usr/local/bin/

      - name: Build site
        run: eigen build
        working-directory: website

      - name: Add CNAME
        run: cp website/CNAME website/dist/CNAME

      - uses: actions/configure-pages@v5
      - uses: actions/upload-pages-artifact@v3
        with:
          path: website/dist
      - uses: actions/deploy-pages@v4
        id: deployment
```

GitHub Pages source must be configured to "GitHub Actions" (not "Deploy from a branch").

## Migration & Cleanup

**Build order:**
1. Create `website/` with all eigen files
2. Verify `eigen build` succeeds
3. Verify `eigen dev` serves correctly
4. Add GitHub Actions workflow
5. Delete `docs/index.html`, `docs/docs.html`, `docs/css/`, `docs/js/`, `docs/logo.svg`, `docs/CNAME`
6. Keep `docs/superpowers/` (design specs and plans — unrelated to website)
7. Update `.gitignore` for `website/dist/`

**GitHub Pages reconfiguration:** Switch from "deploy from branch" to "GitHub Actions" deployment source. Brief downtime possible during switchover.

## What's Not Changing

- The Rust source code
- `docs/superpowers/` directory
- `.github/workflows/release.yml` (cargo-dist)
- Any development tooling (justfile, flake.nix, etc.)
