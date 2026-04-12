# Website Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Port the youwhatknow docs site from static HTML in `docs/` to an eigen-powered site in `website/`, preserving the full dark/neon aesthetic, expanding documentation, and deploying via GitHub Actions.

**Architecture:** Eigen static site generator with Jinja2 templates. Landing page as inline HTML template (custom-styled components). Docs page as single scrollable page with content loaded from `_data/docs.yaml` via `| markdown` filter. HTMX fragment navigation between pages.

**Tech Stack:** Eigen (Rust SSG), Jinja2 templates, HTMX, CSS custom properties, vanilla JS

**Spec:** `docs/superpowers/specs/2026-04-12-website-redesign.md`

**Eigen binary:** Available at `~/.cargo/bin/eigen`. Just use `eigen` directly.

---

## Milestone 1: Eigen Scaffolding (site config, layout, static assets)

### Task 1: Create site.toml and directory structure

**Files:**
- Create: `website/site.toml`
- Create: `website/CNAME`
- Create: `website/templates/` (directory)
- Create: `website/_data/` (directory)
- Create: `website/static/css/` (directory)
- Create: `website/static/js/` (directory)

- [ ] **Step 1: Create directory structure**

```bash
mkdir -p website/templates/_partials website/_data website/static/css website/static/js
```

- [ ] **Step 2: Create site.toml**

```toml
# website/site.toml
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

- [ ] **Step 3: Create CNAME**

```
youwhatknow.it
```

- [ ] **Step 4: Create nav data**

```yaml
# website/_data/nav.yaml
- label: Docs
  url: /docs.html

- label: GitHub
  url: https://github.com/wavefunk/youwhatknow
  external: true
```

- [ ] **Step 5: Commit**

```bash
git add website/site.toml website/CNAME website/_data/nav.yaml
git commit -m "feat(website): add eigen site config and nav data"
```

---

### Task 2: Port static assets (CSS, JS, favicon)

**Files:**
- Create: `website/static/css/style.css` (port from `docs/css/style.css`)
- Create: `website/static/js/main.js` (port from `docs/js/main.js` + terminal demo + sidebar tracking)
- Create: `website/static/favicon.svg` (copy from `docs/logo.svg`)

- [ ] **Step 1: Copy and extend CSS**

Copy `docs/css/style.css` to `website/static/css/style.css`. Then append these additions for the docs sidebar layout:

```css
/* ── 22. Docs sidebar layout ── */
.docs-layout {
    display: grid;
    grid-template-columns: 240px 1fr;
    gap: 3rem;
    max-width: 1100px;
    margin: 0 auto;
    padding: 0 2rem;
}

.sidebar {
    position: sticky;
    top: 2rem;
    max-height: calc(100vh - 4rem);
    overflow-y: auto;
    padding-right: 1rem;
    scrollbar-width: thin;
    scrollbar-color: var(--border) transparent;
}

.sidebar::-webkit-scrollbar { width: 4px; }
.sidebar::-webkit-scrollbar-track { background: transparent; }
.sidebar::-webkit-scrollbar-thumb { background: var(--border); border-radius: 2px; }

.sidebar-title {
    font-family: var(--display);
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.15em;
    color: var(--glow);
    margin-bottom: 1rem;
    padding-bottom: 0.5rem;
    border-bottom: 1px solid var(--border);
}

.sidebar-nav {
    list-style: none;
}

.sidebar-nav li {
    margin-bottom: 0.15rem;
}

.sidebar-nav a {
    display: block;
    font-size: 12.5px;
    color: var(--text-dim);
    padding: 0.3rem 0.75rem;
    border-left: 2px solid transparent;
    transition: all 0.15s;
}

.sidebar-nav a:hover {
    color: var(--text);
    text-decoration: none;
}

.sidebar-nav a.active {
    color: var(--glow);
    border-left-color: var(--glow);
    background: var(--glow-dim);
}

.sidebar-toggle {
    display: none;
    position: fixed;
    bottom: 1.5rem;
    right: 1.5rem;
    z-index: 1000;
    background: var(--bg-card);
    border: 1px solid var(--glow-dim);
    color: var(--glow);
    width: 44px;
    height: 44px;
    font-size: 18px;
    cursor: pointer;
    border-radius: 4px;
}

.sidebar-overlay {
    display: none;
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.6);
    z-index: 998;
}

/* Docs page body overrides */
body.docs-page nav { margin-bottom: 2rem; }

/* Markdown prose in doc sections */
.doc-content h2 {
    font-family: var(--serif);
    font-size: clamp(1.5rem, 3vw, 2rem);
    font-weight: 400;
    font-style: italic;
    color: #fff;
    margin-bottom: 1rem;
    line-height: 1.3;
}

.doc-content h3 {
    font-family: var(--display);
    font-size: 13px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: #fff;
    margin: 2rem 0 0.75rem;
}

.doc-content h4 {
    font-size: 13px;
    color: #fff;
    font-weight: 500;
    margin: 1.5rem 0 0.5rem;
}

.doc-content p {
    color: var(--text-dim);
    font-size: 13.5px;
    line-height: 1.8;
    margin-bottom: 1rem;
    max-width: 650px;
}

.doc-content p strong { color: var(--text); font-weight: 500; }

.doc-content ul, .doc-content ol {
    padding-left: 1.5rem;
    margin-bottom: 1rem;
}

.doc-content li {
    margin-bottom: 0.3rem;
    list-style-type: disc;
    color: var(--text-dim);
    font-size: 13.5px;
    line-height: 1.8;
}

.doc-content code {
    background: var(--glow-dim);
    color: var(--glow);
    padding: 0.15em 0.4em;
    font-size: 0.9em;
    font-family: var(--mono);
}

.doc-content pre {
    background: var(--bg-card);
    border: 1px solid var(--border);
    padding: 1.25rem;
    margin: 1rem 0 1.5rem;
    overflow-x: auto;
    font-size: 12.5px;
    line-height: 1.8;
    position: relative;
}

.doc-content pre code {
    background: none;
    color: var(--text);
    padding: 0;
    font-size: inherit;
}

.doc-content blockquote {
    background: #0d1a10;
    border: 1px solid #1a3d1f;
    border-left: 3px solid var(--glow);
    padding: 1rem 1.5rem;
    margin: 1.5rem 0;
    font-size: 13px;
    color: var(--glow);
    line-height: 1.7;
}

.doc-content table {
    width: 100%;
    border-collapse: collapse;
    font-size: 13px;
    margin: 1rem 0 1.5rem;
}

.doc-content th {
    text-align: left;
    font-family: var(--display);
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.1em;
    color: var(--text-dim);
    padding: 0.75rem 1rem;
    border-bottom: 2px solid var(--border);
}

.doc-content td {
    padding: 0.75rem 1rem;
    border-bottom: 1px solid var(--border);
    vertical-align: top;
    color: var(--text-dim);
    font-size: 12.5px;
    line-height: 1.6;
}

.doc-content td:first-child {
    font-weight: 500;
    color: var(--amber);
    white-space: nowrap;
}

.doc-content td code {
    font-size: 11px;
}

.doc-content hr {
    border: none;
    border-top: 1px solid var(--border);
    margin: 3rem 0;
}

.copy-btn {
    position: absolute;
    top: 0.5rem;
    right: 0.5rem;
    background: var(--bg);
    border: 1px solid var(--border);
    color: var(--text-dim);
    font-size: 10px;
    font-family: var(--mono);
    padding: 0.2em 0.5em;
    cursor: pointer;
    opacity: 0;
    transition: opacity 0.15s;
}

.doc-content pre:hover .copy-btn { opacity: 1; }
.copy-btn:hover { color: var(--glow); border-color: var(--glow-dim); }

/* ── 23. Responsive: sidebar ── */
@media (max-width: 1024px) {
    .docs-layout {
        grid-template-columns: 1fr;
    }

    .sidebar {
        position: fixed;
        top: 0;
        left: -280px;
        width: 260px;
        height: 100vh;
        max-height: 100vh;
        background: var(--bg);
        border-right: 1px solid var(--border);
        z-index: 999;
        padding: 2rem 1.5rem;
        transition: left 0.3s ease;
    }

    .sidebar.open { left: 0; }
    .sidebar-toggle { display: flex; align-items: center; justify-content: center; }
    .sidebar-overlay.open { display: block; }
}
```

- [ ] **Step 2: Create JS file**

Combine the intersection observer, terminal demo animation, and sidebar active tracking into `website/static/js/main.js`:

```javascript
// Reveal animations (intersection observer)
(function () {
  var observer = new IntersectionObserver(
    function (entries) {
      entries.forEach(function (entry) {
        if (entry.isIntersecting) {
          entry.target.classList.add("visible");
        }
      });
    },
    { threshold: 0.15 }
  );
  document.querySelectorAll(".reveal").forEach(function (el) {
    observer.observe(el);
  });
})();

// Terminal demo animation (only on landing page)
(function () {
  var terminal = document.getElementById("terminal-demo");
  if (!terminal) return;

  var lines = [
    { type: "cmd", prompt: "claude", cmd: "Read", file: "src/server.rs" },
    { type: "msg", text: "  623 lines.", cls: "t-dim" },
    { type: "msg", text: "", cls: "t-dim" },
    { type: "msg", text: "  youwhatknow: denied.", cls: "t-ok t-bold" },
    {
      type: "msg",
      text: "  src/server.rs (623 lines) \u2014 Axum server with activity tracking",
      cls: "t-ok",
    },
    {
      type: "msg",
      text: "  Public: create_router, start_server, AppState",
      cls: "t-ok",
    },
    {
      type: "msg",
      text: "  If this summary is sufficient, do not read the file.",
      cls: "t-ok",
    },
    { type: "msg", text: "", cls: "t-dim" },
    { type: "cmd", prompt: "claude", cmd: "Read", file: "src/config.rs" },
    {
      type: "msg",
      text: "  Claude moves on. Summary was enough.",
      cls: "t-dim",
    },
    { type: "msg", text: "", cls: "t-dim" },
    { type: "cmd", prompt: "claude", cmd: "Read", file: "src/server.rs" },
    { type: "msg", text: "  Second read \u2014 allowed through.", cls: "t-dim" },
    { type: "msg", text: "", cls: "t-dim" },
    { type: "cmd", prompt: "claude", cmd: "Read", file: "src/server.rs" },
    { type: "msg", text: "  Third time. Really?", cls: "t-warn" },
    {
      type: "msg",
      text: "  read 3x this session \u2014 consider offset/limit",
      cls: "t-ok",
    },
  ];

  var lineIndex = 0;

  function addLine() {
    if (lineIndex >= lines.length) {
      var cursor = document.createElement("span");
      cursor.className = "cursor-blink";
      if (terminal.lastElementChild) {
        terminal.lastElementChild.appendChild(cursor);
      }
      return;
    }

    var line = lines[lineIndex];
    var el = document.createElement("div");
    el.className = "term-line";

    if (line.type === "cmd") {
      var prompt = document.createElement("span");
      prompt.className = "t-prompt";
      prompt.textContent = line.prompt;
      var cmd = document.createElement("span");
      cmd.className = "t-cmd";
      cmd.textContent = line.cmd;
      var file = document.createElement("span");
      file.className = "t-file";
      file.textContent = line.file;
      el.appendChild(prompt);
      el.appendChild(document.createTextNode(" "));
      el.appendChild(cmd);
      el.appendChild(document.createTextNode(" "));
      el.appendChild(file);
    } else {
      var span = document.createElement("span");
      span.className = line.cls;
      span.textContent = line.text;
      el.appendChild(span);
    }

    terminal.appendChild(el);
    lineIndex++;

    var delay = line.text === "" ? 500 : lineIndex > 10 ? 150 : 300;
    setTimeout(addLine, delay);
  }

  setTimeout(addLine, 1000);
})();

// Sidebar active section tracking (only on docs page)
(function () {
  var sidebarLinks = document.querySelectorAll(".sidebar-nav a");
  if (!sidebarLinks.length) return;

  var sections = [];
  sidebarLinks.forEach(function (link) {
    var href = link.getAttribute("href");
    if (href && href.startsWith("#")) {
      var section = document.getElementById(href.slice(1));
      if (section) sections.push({ el: section, link: link });
    }
  });

  var observer = new IntersectionObserver(
    function (entries) {
      entries.forEach(function (entry) {
        var match = sections.find(function (s) {
          return s.el === entry.target;
        });
        if (match) {
          if (entry.isIntersecting) {
            sidebarLinks.forEach(function (l) {
              l.classList.remove("active");
            });
            match.link.classList.add("active");
          }
        }
      });
    },
    { rootMargin: "-20% 0px -70% 0px" }
  );

  sections.forEach(function (s) {
    observer.observe(s.el);
  });
})();

// Sidebar toggle (mobile)
function toggleSidebar() {
  document.getElementById("sidebar").classList.toggle("open");
  document.getElementById("sidebar-overlay").classList.toggle("open");
}
function closeSidebar() {
  document.getElementById("sidebar").classList.remove("open");
  document.getElementById("sidebar-overlay").classList.remove("open");
}

// Copy buttons for code blocks in docs
document.addEventListener("DOMContentLoaded", function () {
  document.querySelectorAll(".doc-content pre").forEach(function (pre) {
    var btn = document.createElement("button");
    btn.className = "copy-btn";
    btn.textContent = "copy";
    btn.addEventListener("click", function () {
      var code = pre.querySelector("code");
      navigator.clipboard.writeText(code ? code.textContent : pre.textContent);
      btn.textContent = "copied";
      setTimeout(function () {
        btn.textContent = "copy";
      }, 1500);
    });
    pre.appendChild(btn);
  });
});
```

- [ ] **Step 3: Copy favicon**

```bash
cp -f docs/logo.svg website/static/favicon.svg
```

- [ ] **Step 4: Commit**

```bash
git add website/static/
git commit -m "feat(website): add CSS design system, JS, and favicon"
```

---

### Task 3: Create base layout and partials

**Files:**
- Create: `website/templates/_base.html`
- Create: `website/templates/_partials/nav.html`
- Create: `website/templates/_partials/footer.html`
- Create: `website/templates/_partials/effects.html`

- [ ] **Step 1: Create `_base.html`**

```html
<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>{% block title %}{{ site.name }}{% endblock %}</title>
  <link rel="icon" href="/favicon.svg" type="image/svg+xml">
  <link rel="preconnect" href="https://fonts.googleapis.com">
  <link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
  <link href="https://fonts.googleapis.com/css2?family=JetBrains+Mono:wght@300;400;500;700&family=Space+Mono:wght@400;700&family=Instrument+Serif:ital@0;1&display=swap" rel="stylesheet">
  <link rel="stylesheet" href="{{ asset('/css/style.css') }}">
  <script src="https://unpkg.com/htmx.org@2.0.4" defer></script>
</head>
<body class="{% block body_class %}{% endblock %}" hx-boost="true">
  {% include "_partials/nav.html" %}
  {% include "_partials/effects.html" %}
  <main id="content">
    {% block content %}{% endblock %}
  </main>
  {% include "_partials/footer.html" %}
  <script src="{{ asset('/js/main.js') }}"></script>
</body>
</html>
```

- [ ] **Step 2: Create `_partials/nav.html`**

```html
<nav>
  <div class="container">
    <a {{ link_to("/index.html") }} class="nav-logo">youwhat(k)now</a>
    <div class="nav-links">
      {% for item in nav %}
        {% if item.external %}
        <a href="{{ item.url }}" target="_blank" rel="noopener">{{ item.label }}</a>
        {% else %}
        <a {{ link_to(item.url) }}>{{ item.label }}</a>
        {% endif %}
      {% endfor %}
    </div>
  </div>
</nav>
```

- [ ] **Step 3: Create `_partials/footer.html`**

```html
<footer>
  <div class="container">
    <div class="footer-logo">youwhat(k)now</div>
    <p class="footer-links">
      <a {{ link_to("/docs.html") }}>Docs</a>
      <a href="https://github.com/wavefunk/youwhatknow" target="_blank" rel="noopener">GitHub</a>
    </p>
    <p>Built with Rust, Tokio, Axum, and tree-sitter.</p>
    <p class="footer-quip">"No Claude was harmed in the making of this tool. Just mildly inconvenienced."</p>
  </div>
</footer>
```

- [ ] **Step 4: Create `_partials/effects.html`**

The scanline overlay is handled by CSS pseudo-elements on `body` and `body.landing`, so this partial only needs the fractal noise comment. However, since the noise texture is also CSS-based (via data URI in `body.landing::before`), this partial can be empty or contain a comment. The effects are purely CSS.

Actually — the effects are entirely in CSS already (`body::after` for scanlines, `body.landing::before` for noise). No separate partial needed. Create an empty placeholder for future use:

```html
{# Visual effects (scanlines, noise) are handled via CSS pseudo-elements on body #}
```

- [ ] **Step 5: Commit**

```bash
git add website/templates/
git commit -m "feat(website): add base layout and partials"
```

---

### Task 4: Create landing page template

**Files:**
- Create: `website/templates/index.html`

- [ ] **Step 1: Create `index.html`**

Port the entire `docs/index.html` body content into an eigen template. The content between `<body>` and `</body>` becomes the `{% block content %}` of the template, minus the nav/footer (now in partials).

```html
---
data:
  nav:
    file: "nav.yaml"
seo:
  title: "youwhatknow — Claude, buddy, you already read that file"
  description: "Stop Claude from re-reading files. A hook server that injects file summaries and tracks repeat reads per session."
  og_type: "website"
---
{% extends "_base.html" %}

{% block title %}youwhatknow &mdash; Claude, buddy, you already read that file{% endblock %}
{% block body_class %}landing{% endblock %}

{% block content %}
<!-- ── HERO ── -->
<section class="hero">
    <div class="container">
        <div class="hero-badge">
            <span class="hero-badge-dot"></span> A Claude Code hook server
        </div>
        <img src="/favicon.svg" alt="!?" width="120" height="120" class="hero-logo">
        <h1><em>you what (k)now?</em></h1>
        <p class="hero-tagline">"Wait, are you reading that file again?"</p>
        <p class="hero-sub">You know the look. Claude's halfway through a task and decides to read <strong>src/main.rs</strong> for the fifth time. 847 lines. Again. You stare at your token counter and whisper <strong>"you what now?"</strong><br><br>Instead of 2000 lines of code, Claude gets a summary. If it still wants the full file, it has to ask twice.</p>
        <a href="#setup" class="hero-cta">Make it stop &#8594;</a>
        <div class="hero-links">
            <a {{ link_to("/docs.html") }}>Docs</a>
            <a href="https://github.com/wavefunk/youwhatknow" target="_blank" rel="noopener">GitHub</a>
        </div>
    </div>
</section>

<!-- ── TERMINAL DEMO ── -->
<div class="terminal-section">
    <div class="container">
        <p class="terminal-label">A typical Tuesday with Claude (dramatized only slightly)</p>
        <div class="terminal">
            <div class="terminal-bar">
                <div class="terminal-dot r"></div>
                <div class="terminal-dot y"></div>
                <div class="terminal-dot g"></div>
                <span class="terminal-bar-title">claude &mdash; very busy reading the same file</span>
            </div>
            <div class="terminal-body" id="terminal-demo"></div>
        </div>
    </div>
</div>

<!-- ── THE PROBLEM ── -->
<section class="reveal">
    <div class="container">
        <div class="section-label">The problem, illustrated</div>
        <h2>We need to talk about Claude's reading habits.</h2>
        <p class="muted">We love Claude. Claude is great. But Claude has a problem. It reads files like someone who opens the fridge every 10 minutes hoping new food appeared.</p>
        <div class="convo">
            <div class="bubble bubble-claude">
                <div class="bubble-who">Claude</div>
                Let me read src/main.rs to understand the project structure.
            </div>
            <div class="bubble bubble-claude">
                <div class="bubble-who">Claude (30 seconds later)</div>
                I should check src/main.rs to see how the server starts.
            </div>
            <div class="bubble bubble-you">
                <div class="bubble-who">You</div>
                you literally just read that
            </div>
            <div class="bubble bubble-ywk">
                <div class="bubble-who">youwhatknow</div>
                denied. src/main.rs (847 lines) &mdash; Entry point, server startup. Public: main().<br>
                If this is enough, don't read the file. Read again if you need it.
            </div>
            <div class="bubble bubble-claude">
                <div class="bubble-who">Claude</div>
                ...okay, the summary is enough. Thanks.
            </div>
        </div>
    </div>
</section>

<!-- ── HOW IT WORKS ── -->
<section class="reveal">
    <div class="container">
        <div class="section-label">How it works</div>
        <h2>A polite but firm intervention.</h2>
        <p class="muted">youwhatknow sits between Claude and the filesystem. "You can have more if you want, but here's what's on your plate already."</p>
        <div class="flow">
            <div class="flow-step">
                <div class="flow-num">1</div>
                <h3>Claude reaches for a file</h3>
                <p>Claude Code fires a PreToolUse hook before every Read. youwhatknow gets the file path and session ID via HTTP before the read happens.</p>
                <code>POST /hook/pre-read</code>
            </div>
            <div class="flow-step">
                <div class="flow-num">2</div>
                <h3>Small files get a free pass</h3>
                <p>Files with 30 lines or fewer are waved through without intervention. Targeted reads with offset/limit also pass.</p>
                <code>line_threshold = 30</code>
                <aside>No point summarizing a 12-line config file.</aside>
            </div>
            <div class="flow-step">
                <div class="flow-num">3</div>
                <h3>First read: denied with a summary</h3>
                <p>Instead of 2000 lines of code, Claude sees: file description, public symbols, line-range map. "If this is sufficient, do not read the file. Read again for the full file."</p>
                <code>deny + summary</code>
                <aside>Think of it as the back-cover blurb instead of reading the whole book.</aside>
            </div>
            <div class="flow-step">
                <div class="flow-num">4</div>
                <h3>Second read: allowed clean</h3>
                <p>Claude asked twice, so it genuinely needs the file. Goes through with no context injection, no nudge, no friction.</p>
            </div>
            <div class="flow-step">
                <div class="flow-num">5</div>
                <h3>Repeat offenders get nudged</h3>
                <p>Third read and beyond: allowed, but with a reminder. "This file has been read 3x this session. Consider using offset/limit for targeted reads."</p>
                <code>read 3x this session</code>
                <aside>Tough love. But the kind that saves your token budget.</aside>
            </div>
            <div class="flow-step">
                <div class="flow-num">6</div>
                <h3>Day-one orientation</h3>
                <p>On SessionStart, Claude gets a full project map injected automatically. No more "let me explore the codebase" spirals.</p>
                <code>POST /hook/session-start</code>
                <aside>Like giving the new hire a map instead of letting them wander the building.</aside>
            </div>
        </div>
    </div>
</section>

<!-- ── WORKING SET EVICTION ── -->
<section class="reveal">
    <div class="container">
        <div class="section-label">Working set</div>
        <h2>Files fade. Context stays fresh.</h2>
        <p class="muted">After more than 40 other file reads, a file's read count resets to zero. Next read shows the summary again. No stale state, no manual cleanup &mdash; the working set stays current automatically.</p>
        <p class="muted">Configurable via <code>eviction_threshold</code> in your project config. Or use <code>youwhatknow reset &lt;path&gt;</code> to do it manually.</p>
    </div>
</section>

<!-- ── ARCHITECTURE ── -->
<section class="reveal">
    <div class="container">
        <div class="section-label">Architecture</div>
        <h2>One daemon. All projects.</h2>
        <div class="arch-diagram">
            <div class="arch-row">
                <div class="arch-box secondary">Claude Session A</div>
                <div class="arch-box secondary">Claude Session B</div>
                <div class="arch-box secondary">Subagent</div>
            </div>
            <div class="arch-arrow-down">&darr; HTTP hooks &darr;</div>
            <div class="arch-row">
                <div class="arch-box primary">youwhatknow &mdash; localhost:7849</div>
            </div>
            <div class="arch-arrow-down">&darr; routes by cwd &darr;</div>
            <div class="arch-row">
                <div class="arch-box secondary">Project A Index</div>
                <div class="arch-box secondary">Project B Index</div>
                <div class="arch-box secondary">Project C Index</div>
            </div>
            <div class="arch-arrow-down">&darr;</div>
            <div class="arch-row">
                <div class="arch-box secondary">tree-sitter &bull; haiku descriptions &bull; TOML</div>
            </div>
        </div>
    </div>
</section>

<!-- ── BENEFITS ── -->
<section class="reveal">
    <div class="container">
        <div class="section-label">Why bother</div>
        <h2>Six reasons to stop the madness.</h2>
        <div class="benefits-grid">
            <div class="benefit">
                <div class="benefit-icon glow">&lt;/&gt;</div>
                <h3>Summary first, file second</h3>
                <p>Claude gets description, symbols, and line ranges. Has to ask twice for the full file. Less context waste.</p>
            </div>
            <div class="benefit">
                <div class="benefit-icon amber">&#8634;</div>
                <h3>Working set eviction</h3>
                <p>After 41+ intervening file reads, stale counts reset automatically. No manual cleanup.</p>
            </div>
            <div class="benefit">
                <div class="benefit-icon red">&#8635;</div>
                <h3>"You already read that"</h3>
                <p>Per-session tracking nudges Claude on 3rd+ reads to use offset/limit.</p>
            </div>
            <div class="benefit">
                <div class="benefit-icon glow">&empty;</div>
                <h3>Stupid simple setup</h3>
                <p>One command: <code>youwhatknow setup</code>. Hooks, daemon, indexing &mdash; all handled. No YAML nightmares.</p>
            </div>
            <div class="benefit">
                <div class="benefit-icon amber">&#9638;</div>
                <h3>All projects, one process</h3>
                <p>The daemon loads project indexes lazily. First request for a new project? Indexed in the background. Zero waiting.</p>
            </div>
            <div class="benefit">
                <div class="benefit-icon red">&#10005;</div>
                <h3>Invisible when off</h3>
                <p>Daemon not running? Claude works normally. HTTP hooks fail silently. It's there when you want it, gone when you don't.</p>
            </div>
        </div>
    </div>
</section>

<!-- ── SETUP ── -->
<section class="reveal" id="setup">
    <div class="container">
        <div class="section-label">Get started</div>
        <h2>Three commands. Maybe two.</h2>

        <div class="setup-block">
            <div class="setup-header">install <span class="setup-tag">pick one</span></div>
            <pre class="setup-code"><code>$ curl --proto '=https' --tlsv1.2 -LsSf https://github.com/wavefunk/youwhatknow/releases/latest/download/youwhatknow-installer.sh | sh

# Or: nix flake (see docs)
# Or: build from source
$ cargo build --release</code></pre>
        </div>

        <div class="setup-block">
            <div class="setup-header">terminal <span class="setup-tag">the whole thing</span></div>
            <pre class="setup-code"><code>$ cd your-project
$ youwhatknow setup

# Creates .claude/ and .claude/summaries/
# Merges hook config into .claude/settings.local.json
# Starts the daemon if not already running
# Triggers initial project indexing
# That's genuinely it.</code></pre>
        </div>

        <div class="setup-block">
            <div class="setup-header">terminal <span class="setup-tag">variants &amp; commands</span></div>
            <pre class="setup-code"><code>$ youwhatknow setup --shared    # writes to .claude/settings.json (team-shared)
$ youwhatknow setup --no-index  # skip initial indexing
$ youwhatknow status            # daemon uptime, active sessions, projects
$ youwhatknow summary src/main.rs # preview a file's summary
$ youwhatknow reset src/main.rs   # reset read count for a file</code></pre>
        </div>

        <details>
            <summary>Manual hook setup</summary>
            <p>If you prefer to configure hooks manually, add this to <code>.claude/settings.local.json</code>:</p>
            <pre class="setup-code"><code>{
  "hooks": {
    "PreToolUse": [
      {
        "matcher": "Read",
        "url": "http://localhost:7849/hook/pre-read"
      }
    ],
    "PostToolUse": [
      {
        "matcher": "Read",
        "url": "http://localhost:7849/hook/post-read"
      }
    ],
    "SessionStart": [
      {
        "url": "http://localhost:7849/hook/session-start"
      }
    ]
  }
}</code></pre>
        </details>

        <details>
            <summary>Optional config.toml</summary>
            <pre class="setup-code"><code># ~/.config/youwhatknow/config.toml
# All settings have sensible defaults.
port = 7849
session_timeout_minutes = 60
idle_shutdown_minutes = 30</code></pre>
        </details>
    </div>
</section>
{% endblock %}
```

- [ ] **Step 2: Verify build**

```bash
cd /home/nambiar/projects/wavefunk/youwhatknow
eigen build
```

Expected: Build succeeds, `website/dist/index.html` exists.

- [ ] **Step 3: Verify dev server**

```bash
eigen dev
```

Open `http://localhost:3000` in browser. Landing page should render with full aesthetic — neon green, animations, terminal demo, all sections.

- [ ] **Step 4: Commit**

```bash
git add website/templates/index.html
git commit -m "feat(website): add landing page template"
```

---

## Milestone 2: Documentation Page

### Task 5: Write docs.yaml with all documentation content

**Files:**
- Create: `website/_data/docs.yaml`

This is the largest task. All 16 documentation sections, written as markdown in YAML. Content is verified against the source code (all config values, CLI flags, endpoints, thresholds match).

- [ ] **Step 1: Write `_data/docs.yaml`**

Create `website/_data/docs.yaml` with all 16 sections. Each entry has `title`, `slug`, and `content` (markdown). The content is revised and expanded from the current `docs/docs.html`.

The sections are:

1. **Installation** (`slug: installation`) — curl installer, Nix flake input, cargo build. Mention Rust 2024 edition requirement.

2. **Quickstart** (`slug: quickstart`) — `youwhatknow setup` one-liner, what it does (creates dirs, merges hooks, starts daemon, indexes). `--shared` and `--no-index` flags. Daemon runs on localhost:7849, lazy loading, idle shutdown. Callout: daemon not running = Claude works normally.

3. **How It Works** (`slug: how-it-works`) — Hook system overview. PreToolUse fires before Read. Small files pass through. First read = deny with summary. Second = allow clean. Third+ = allow with nudge. SessionStart injects project map. Explain the read count lifecycle.

4. **CLI Reference** (`slug: cli-reference`) — All 10 subcommands with flags:
   - `serve` (default, no args)
   - `setup` (`--shared`, `--local`, `--no-index`)
   - `init` (internal, called by hook)
   - `status` (`--json`)
   - `summary <path>`
   - `reindex` (`--full`, `--json`)
   - `reset <path>` (`--session`)
   - `logs` (`-f/--follow`, `-n/--lines`)
   - `restart`
   - `prime`

5. **Hook Behavior** (`slug: hook-behavior`) — PreToolUse/Read: early exits (no tool_input, outside project, no summary, targeted read, under threshold). Read count 1=deny, 2=allow, 3+=nudge. What Claude sees: formatted summary example, nudge example, session start example. SessionStart: project map injection, indexing-in-progress note.

6. **Working Set Eviction** (`slug: eviction`) — Per-session monotonic sequence counter. `current_seq - file_last_seq > eviction_threshold` triggers reset. Default 40. Keeps working set current automatically. Manual reset via CLI.

7. **Session Management** (`slug: session-management`) — Per-session read count tracking. Session ID set via `YOUWHATKNOW_SESSION` env var. Session cleanup after `session_timeout_minutes` (default 60). Activity tracking per session.

8. **Daemon Configuration** (`slug: daemon-config`) — `~/.config/youwhatknow/config.toml`. Three fields: `port` (7849), `session_timeout_minutes` (60), `idle_shutdown_minutes` (30). Table of all fields with defaults and descriptions.

9. **Project Configuration** (`slug: project-config`) — `.claude/youwhatknow.toml`. Six fields: `summary_path`, `max_file_size_kb`, `line_threshold`, `ignored_patterns`, `max_concurrent_batches`, `eviction_threshold`. Table with defaults. Built-in ignore patterns list.

10. **API Endpoints** (`slug: api-endpoints`) — All 7 endpoints: POST `/hook/pre-read`, POST `/hook/session-start`, POST `/hook/summary`, POST `/hook/reset-read`, POST `/reindex`, GET `/health`, GET `/status`. Request/response details for each.

11. **Indexing & Symbols** (`slug: indexing`) — File discovery via `git ls-files`. Symbol extraction via tree-sitter: Rust, TypeScript, JavaScript, Python, Go with what each extracts. Description generation: batched Claude Haiku CLI calls (first 100 lines + symbols, batches of 15). Fallback to filename + symbols. Incremental re-indexing via `git diff` against stored commit hash in `.last-run`.

12. **Storage Format** (`slug: storage`) — TOML files in `.claude/summaries/`. Per-folder summary structure (generated, description, files map). Project summary structure (generated, last_commit, folders map). Folder key conversion (`src/indexer` → `src--indexer.toml`). Atomic writes.

13. **Environment Variables** (`slug: env-vars`) — `YOUWHATKNOW_PORT`, `YOUWHATKNOW_SESSION_TIMEOUT_MINUTES`, `YOUWHATKNOW_IDLE_SHUTDOWN_MINUTES`. Env vars override config file. Example usage.

14. **Daemon Lifecycle** (`slug: lifecycle`) — PID file at `~/.local/share/youwhatknow/youwhatknow.pid`. Starting: just run `youwhatknow`. Stopping: Ctrl+C/SIGTERM, idle timeout, manual kill. Multi-project: one daemon, lazy loading by cwd. Worktree sharing: resolves to git root.

15. **Nix Integration** (`slug: nix`) — Flake input, devShell packages, optional shell hook (auto-starts daemon). Shell hook behavior: checks PID, only starts if both `claude` and `youwhatknow` on PATH. Build caching: source filtering.

16. **Troubleshooting** (`slug: troubleshooting`) — Common issues: daemon not starting (port in use), summaries not appearing (check index, reindex), hooks not firing (check settings.json). Diagnostic commands: `youwhatknow status`, `youwhatknow logs -f`, `youwhatknow reindex --full`.

Each section's `content` field contains full markdown. Write the actual markdown content — do not use placeholders.

- [ ] **Step 2: Commit**

```bash
git add website/_data/docs.yaml
git commit -m "feat(website): add comprehensive documentation content"
```

---

### Task 6: Create docs page template

**Files:**
- Create: `website/templates/docs.html`

- [ ] **Step 1: Create `docs.html`**

```html
---
data:
  nav:
    file: "nav.yaml"
  docs:
    file: "docs.yaml"
seo:
  title: "Documentation — youwhatknow"
  description: "Everything you need to install, configure, and understand youwhatknow."
---
{% extends "_base.html" %}

{% block title %}youwhatknow &mdash; docs{% endblock %}
{% block body_class %}docs-page{% endblock %}

{% block content %}
<div class="container" style="margin-bottom: 2rem;">
    <div class="page-header">
        <h1>Documentation</h1>
        <p>Everything you need to install, configure, and understand youwhatknow.</p>
    </div>
</div>

<div class="docs-layout">
    <div id="sidebar-overlay" class="sidebar-overlay" onclick="closeSidebar()"></div>
    <aside id="sidebar" class="sidebar">
        <div class="sidebar-title">On this page</div>
        <ul class="sidebar-nav">
            {% for section in docs %}
            <li><a href="#{{ section.slug }}">{{ section.title }}</a></li>
            {% endfor %}
        </ul>
    </aside>

    <div class="doc-content">
        {% for section in docs %}
        <section id="{{ section.slug }}" class="doc-section">
            {{ section.content | markdown }}
        </section>
        {% endfor %}
    </div>
</div>

<button class="sidebar-toggle" onclick="toggleSidebar()" aria-label="Toggle table of contents">&#9776;</button>
{% endblock %}
```

- [ ] **Step 2: Verify build**

```bash
cd /home/nambiar/projects/wavefunk/youwhatknow
eigen build
```

Expected: Build succeeds, both `website/dist/index.html` and `website/dist/docs.html` exist.

- [ ] **Step 3: Verify dev server**

```bash
eigen dev
```

Check:
- `http://localhost:3000/docs.html` renders with sidebar and all 16 sections
- Sidebar links scroll to correct sections
- Active section highlighting works
- Mobile sidebar toggle works (resize browser)
- Navigation between landing page and docs works via HTMX
- Copy buttons appear on code blocks on hover

- [ ] **Step 4: Commit**

```bash
git add website/templates/docs.html
git commit -m "feat(website): add docs page template with sidebar"
```

---

## Milestone 3: Deployment & Cleanup

### Task 7: Add GitHub Actions workflow

**Files:**
- Create: `.github/workflows/pages.yml`

- [ ] **Step 1: Create workflow file**

```yaml
name: Deploy website

on:
  push:
    branches: [master]
    paths: ['website/**']
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
        run: cp -f website/CNAME website/dist/CNAME

      - uses: actions/configure-pages@v5
      - uses: actions/upload-pages-artifact@v3
        with:
          path: website/dist
      - uses: actions/deploy-pages@v4
        id: deployment
```

- [ ] **Step 2: Commit**

```bash
git add .github/workflows/pages.yml
git commit -m "ci: add GitHub Actions workflow for website deployment"
```

---

### Task 8: Update .gitignore and clean up old docs

**Files:**
- Modify: `.gitignore`
- Delete: `docs/index.html`, `docs/docs.html`, `docs/css/`, `docs/js/`, `docs/logo.svg`, `docs/CNAME`
- Keep: `docs/superpowers/`

- [ ] **Step 1: Add website/dist to .gitignore**

Add to `.gitignore`:

```
# Eigen build output
website/dist/
website/.eigen_cache/
```

- [ ] **Step 2: Delete old docs files**

```bash
rm -f docs/index.html docs/docs.html docs/logo.svg docs/CNAME
rm -rf docs/css docs/js
```

Verify `docs/superpowers/` still exists:

```bash
ls docs/superpowers/
```

- [ ] **Step 3: Commit**

```bash
git add .gitignore docs/
git commit -m "chore: remove old static docs site, update .gitignore for eigen"
```

---

### Task 9: Final verification

- [ ] **Step 1: Full build from clean state**

```bash
cd /home/nambiar/projects/wavefunk/youwhatknow
rm -rf website/dist website/.eigen_cache
eigen build
```

Expected: Clean build succeeds.

- [ ] **Step 2: Verify all output files**

```bash
ls website/dist/
```

Expected files: `index.html`, `docs.html`, `sitemap.xml`, `robots.txt`, `favicon.svg`, `css/`, `js/`, `_fragments/`

- [ ] **Step 3: Dev server smoke test**

```bash
eigen dev
```

Check:
- Landing page: all sections, animations, terminal demo
- Docs page: all 16 sections, sidebar, active tracking
- Navigation between pages
- Mobile responsiveness

- [ ] **Step 4: Verify GitHub Pages note**

Remind user: GitHub Pages source must be switched from "Deploy from a branch" to "GitHub Actions" in repository settings.
