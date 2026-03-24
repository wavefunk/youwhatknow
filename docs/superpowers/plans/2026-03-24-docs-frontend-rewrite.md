# Docs & Frontend Rewrite Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rewrite the youwhatknow landing page and docs to reflect the actual deny-first behavior, add missing features, and extract shared CSS/JS into common files.

**Architecture:** Two static HTML pages sharing a common CSS design system and JS utilities. The existing dark terminal aesthetic, color scheme, and font stack are preserved. Content is completely rewritten to match the actual codebase behavior.

**Tech Stack:** HTML, CSS, vanilla JS. Hosted on GitHub Pages at youwhatknow.it.

**Spec:** `docs/superpowers/specs/2026-03-24-docs-frontend-rewrite-design.md`

---

## File Map

| Action | File | Responsibility |
|--------|------|---------------|
| Create | `docs/css/style.css` | Shared design system: variables, reset, typography, nav, footer, code blocks, terminal, sections, callouts, config tables, animations, responsive, overlays |
| Create | `docs/js/main.js` | Shared JS: IntersectionObserver for `.reveal` elements |
| Rewrite | `docs/index.html` | Landing page: hero, terminal demo, problem section, how it works, eviction, architecture, benefits, setup, footer |
| Rewrite | `docs/docs.html` | Reference docs: installation, quickstart, CLI, hook behavior, eviction, config, endpoints, indexing, storage, env vars, lifecycle, nix |
| Keep | `docs/logo.svg` | Unchanged |
| Keep | `docs/CNAME` | Unchanged |

---

### Task 1: Create shared CSS design system

**Files:**
- Create: `docs/css/style.css`

This is the foundation. Extract and consolidate all CSS from the existing `index.html` and `docs.html` into one file. Both existing files use identical CSS variables, font imports, body styles, and many shared components.

- [ ] **Step 1: Create `docs/css/` directory**

```bash
mkdir -p docs/css
```

- [ ] **Step 2: Write `docs/css/style.css`**

The CSS should contain these sections in order:

1. **CSS Variables** — `:root` block with all color, font, and spacing variables. Exact values from existing files:
   - `--bg: #0a0a0c`, `--bg-card: #111114`, `--glow: #39ff85`, `--glow-dim: #39ff8533`, `--glow-mid: #39ff8566`
   - `--amber: #ffb830`, `--amber-dim: #ffb83033`, `--red: #ff4f4f`
   - `--text: #c8c8d0`, `--text-dim: #5a5a6e`, `--border: #1e1e28`
   - `--mono: 'JetBrains Mono', 'Courier New', monospace`
   - `--display: 'Space Mono', monospace`
   - `--serif: 'Instrument Serif', Georgia, serif`

2. **Reset & Body** — `* { margin: 0; padding: 0; box-sizing: border-box; }`, html scroll-behavior, body background/color/font/line-height/overflow-x.

3. **Overlays** — `body::after` (scanline) and `body::before` (noise texture). Exact values from existing index.html.

4. **Base typography** — `a` styles, `.container` (max-width 900px for landing, but use 900px as default — docs overrides to 800px via a class or its own rule), `.muted`, `.muted strong`.

5. **Nav** — `nav`, `.nav-logo`, `.nav-links`. Used by docs page, landing page has its own hero nav.

6. **Section labels** — `.section-label` with `::after` line.

7. **Headings** — `h2` (serif italic), `h3` (display uppercase).

8. **Hero** — `.hero`, `.hero-badge`, `.hero-badge-dot`, `.hero h1`, `.hero h1 em`, `.hero-tagline`, `.hero-sub`, `.hero-sub strong`, `.hero-cta`.

9. **Terminal** — `.terminal-section`, `.terminal-label`, `.terminal`, `.terminal-bar`, `.terminal-dot` (.r, .y, .g), `.terminal-bar-title`, `.terminal-body`, `.term-line`, `.t-prompt`, `.t-cmd`, `.t-file`, `.t-warn`, `.t-ok`, `.t-dim`, `.t-bold`, `.cursor-blink`.

10. **Conversation bubbles** — `.convo`, `.bubble`, `.bubble-claude`, `.bubble-you`, `.bubble-ywk`, `.bubble-who`.

11. **Flow steps** — `.flow`, `.flow-step`, `.flow-num`, `.flow-step h3/p/code`, `.flow-aside`.

12. **Architecture diagram** — `.arch-diagram`, `.arch-row`, `.arch-box`, `.arch-box.primary`, `.arch-box.secondary`, `.arch-arrow-down`.

13. **Benefits grid** — `.benefits-grid`, `.benefit`, `.benefit-icon`, `.benefit h3`, `.benefit p`.

14. **Setup / Code blocks** — `.setup-block`, `.setup-header`, `.setup-tag`, `.setup-code`. Also `.code-block`, `.code-header`, `.code-tag`, `.code-body` (used by docs page). Syntax classes: `.k` (amber), `.s` (glow), `.c` (dim italic), `.v` (text).

15. **Config tables** — `.config-table`, `.config-table th/td`, `.config-key`, `.config-default`, `.config-desc`.

16. **Endpoint cards** — `.endpoint`, `.endpoint-method` (.get, .post), `.endpoint-path`, `.endpoint p`.

17. **Callouts** — `.callout`, `.callout-warn`.

18. **Docs-specific** — `.page-header`, `.page-header h1/p`, `.toc`, `.toc-title`, `.toc ul/li`, `.doc-section`, `.doc-section p/strong`, `.file-tree`, `.file-tree .dir/.file/.desc`, `.lang-grid`, `.lang-card`, `.lang-card h4/.exts/p`.

19. **Footer** — `footer`, `.footer-logo`, `.footer-quip`.

20. **Animations** — `@keyframes fadeSlideUp`, `pulse`, `termFadeIn`, `blink`. `.reveal` class with opacity/transform transition. `.reveal.visible`.

21. **Responsive** — `@media (max-width: 900px)` for benefits grid, arch rows, bubbles. `@media (max-width: 600px)` for toc, lang-grid, config-table.

All values should be taken exactly from the existing HTML files. This is extraction, not redesign.

- [ ] **Step 3: Verify the file loads in a browser**

Open `docs/index.html` (even the old one) with the `<style>` block replaced by `<link rel="stylesheet" href="css/style.css">` — visual should be identical. (This is a sanity check, not a full test.)

- [ ] **Step 4: Commit**

```bash
git add docs/css/style.css
git commit -m "feat(docs): extract shared CSS design system"
```

---

### Task 2: Create shared JS

**Files:**
- Create: `docs/js/main.js`

- [ ] **Step 1: Create `docs/js/` directory**

```bash
mkdir -p docs/js
```

- [ ] **Step 2: Write `docs/js/main.js`**

Contains the IntersectionObserver logic used by both pages for `.reveal` elements:

```javascript
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
```

- [ ] **Step 3: Commit**

```bash
git add docs/js/main.js
git commit -m "feat(docs): add shared JS for scroll reveal"
```

---

### Task 3: Rewrite landing page (index.html)

**Files:**
- Rewrite: `docs/index.html`

**Reference:** Spec sections "Landing Page (index.html)" — hero, terminal demo, problem, how it works, eviction, architecture, benefits, setup, footer.

**Key content changes from old version:**
- Terminal demo: shows deny-first flow (deny → summary shown → Claude moves on → second read allowed → third read nudged)
- Problem bubbles: youwhatknow **denies** the read instead of just adding context
- How it works: 6 steps reflecting deny-first (was 5, described allow-with-context)
- New "Working Set Eviction" section
- Setup section: adds installation methods (installer script, nix, build from source) before `youwhatknow setup`
- Benefits grid: updated to reflect deny-first, adds eviction
- CLI commands: adds `reset`

- [ ] **Step 1: Write the complete `docs/index.html`**

Structure:
```html
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>youwhatknow — Claude, buddy, you already read that file</title>
    <link rel="preconnect" href="https://fonts.googleapis.com">
    <link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
    <link href="https://fonts.googleapis.com/css2?family=JetBrains+Mono:wght@300;400;500;700&family=Space+Mono:wght@400;700&family=Instrument+Serif:ital@0;1&display=swap" rel="stylesheet">
    <link rel="stylesheet" href="css/style.css">
</head>
<body>
    <!-- HERO section -->
    <!-- TERMINAL DEMO section -->
    <!-- THE PROBLEM section (conversation bubbles) -->
    <!-- HOW IT WORKS section (6 steps) -->
    <!-- WORKING SET EVICTION section -->
    <!-- ARCHITECTURE section -->
    <!-- BENEFITS section (6-item grid) -->
    <!-- SETUP section (install + setup + commands) -->
    <!-- FOOTER -->
    <script src="js/main.js"></script>
    <!-- inline terminal demo script -->
</body>
</html>
```

No `<style>` blocks. All styling from `css/style.css`.

**Hero:** Same structure as existing but with reframed sub copy per spec:
> "Instead of 2000 lines of code, Claude gets a summary. If it still wants the full file, it has to ask twice."

The fridge metaphor stays in the surrounding copy. The sub should lead with the core pitch above.

**Terminal demo script (inline):** Rewrite the line sequence to show the deny-first flow:
1. `claude Read src/server.rs` → dim: "623 lines"
2. Green bold: "youwhatknow: denied."
3. Green: "src/server.rs (623 lines) — Axum server with activity tracking"
4. Green: "Public: create_router, start_server, AppState"
5. Green: "If this summary is sufficient, do not read the file."
6. Blank line pause
7. `claude Read src/config.rs` → dim: "Claude moves on. Summary was enough."
8. `claude Read src/server.rs` → dim: "Second read — allowed through."
9. `claude Read src/server.rs` → warn: "Third time."
10. Green: "read 3x this session — consider offset/limit"

**Problem bubbles:** Full conversation flow (5 bubbles):
1. `.bubble-claude`: "Let me read src/main.rs to understand the project structure."
2. `.bubble-claude` (30 seconds later): "I should check src/main.rs to see how the server starts."
3. `.bubble-you`: "you literally just read that"
4. `.bubble-ywk` (youwhatknow): "denied. src/main.rs (847 lines) — Entry point, server startup. Public: main(). If this is enough, don't read the file. Read again if you need it."
5. `.bubble-claude`: "...okay, the summary is enough. Thanks."

**How it works:** 6 flow steps per spec (hook fires, small files pass, first read denied, second read clean, repeat nudge, session start orientation).

**Working set eviction:** New section between "How it works" and "Architecture". Brief explanation: after 41+ other file reads, count resets, summary shown again.

**Architecture:** Same HTML diagram structure as current. Still accurate.

**Benefits grid:** 6 items per spec.

**Setup:** Three blocks:
1. Install block (installer script, nix reference, build from source)
2. Setup block (`youwhatknow setup`, variants)
3. Commands block (`status`, `summary`, `reset`)
4. Collapsible: manual hook JSON
5. Collapsible: config.toml

**Footer:** Same.

- [ ] **Step 2: Open in browser, verify all sections render correctly**

Check: hero, terminal demo animation, conversation bubbles, flow steps, architecture diagram, benefits grid, setup blocks, footer. All should match the design aesthetic of the original.

- [ ] **Step 3: Commit**

```bash
git add docs/index.html
git commit -m "feat(docs): rewrite landing page with deny-first behavior"
```

---

### Task 4: Rewrite docs page (docs.html)

**Files:**
- Rewrite: `docs/docs.html`

**Reference:** Spec sections "Docs Page (docs.html)" — all 13 TOC sections.

**Key content changes from old version:**
- New "Installation" section before Quickstart
- CLI Reference: adds `reset` command, updates `summary` description, adds `serve` alias
- "Claude Code Hooks" → "Hook Behavior": completely rewritten for deny-first (count 1 = deny, count 2 = allow, count 3+ = nudge)
- "What Claude sees" examples: corrected to show actual format with `-- youwhatknow:` header, line-range map, and full instruction text
- New "Working Set Eviction" section
- Project Configuration: adds `max_concurrent_batches` and `eviction_threshold`
- API Endpoints: adds `/hook/summary` and `/hook/reset-read`, notes `/status` doesn't touch activity
- Sections from old docs that are still accurate (indexing, storage, env vars, lifecycle, nix) are carried over with their content unchanged

- [ ] **Step 1: Write the complete `docs/docs.html`**

Structure:
```html
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>youwhatknow — docs</title>
    <link rel="preconnect" href="https://fonts.googleapis.com">
    <link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
    <link href="https://fonts.googleapis.com/css2?family=JetBrains+Mono:wght@300;400;500;700&family=Space+Mono:wght@400;700&family=Instrument+Serif:ital@0;1&display=swap" rel="stylesheet">
    <link rel="stylesheet" href="css/style.css">
</head>
<body>
    <nav><!-- Home, Docs (active), GitHub --></nav>
    <div class="container">
        <!-- Page header -->
        <!-- TOC (13 items) -->
        <!-- Installation -->
        <!-- Quickstart -->
        <!-- CLI Reference -->
        <!-- Hook Behavior -->
        <!-- Working Set Eviction -->
        <!-- Daemon Configuration -->
        <!-- Project Configuration -->
        <!-- API Endpoints -->
        <!-- Indexing & Symbols -->
        <!-- Storage Format -->
        <!-- Environment Variables -->
        <!-- Lifecycle & PID -->
        <!-- Nix Integration -->
    </div>
    <footer><!-- same as landing --></footer>
    <script src="js/main.js"></script>
</body>
</html>
```

No `<style>` blocks. All styling from `css/style.css`.

**Installation section:** Three methods with code blocks — installer script one-liner, nix flake input, cargo build.

**CLI Reference:** Six subcommands per spec. Each with description, code block showing usage, and notes.

**Hook Behavior section content:**

Early exits list, then the three-state read count table:

| Read Count | Action | What Claude Sees |
|-----------|--------|-----------------|
| 1st | **Deny** | Summary with line ranges + "If this summary is sufficient, do not read the file." |
| 2nd | **Allow** (clean) | Nothing — file loads normally |
| 3rd+ | **Allow** (with nudge) | "This file has been read N times this session. Consider using offset/limit." |

Then "What Claude sees" code blocks with the exact output format:

**First read (deny):**
```
-- youwhatknow: src/server.rs --
src/server.rs (245 lines) — Axum server with activity tracking and idle shutdown
Public: create_router, start_server, AppState

  1-45    imports and types
  46-120  router setup and middleware
  121-200 hook endpoint handlers
  201-245 health and status endpoints

Read specific sections with offset/limit, or read again for the full file.
If this summary is sufficient, do not read the file. If you need the full file contents, read it again.
```

**Third+ read (allow with nudge):**
```
This file has been read 3 times this session. Consider using offset/limit for targeted reads.
```

**Session start:**
```
-- youwhatknow: project map --
src/ — Core hook server implementation with CLI, daemon, server, and indexing
  cli.rs — CLI command handlers for daemon and summary management
  config.rs — System and per-project configuration loading via Figment
  ...

-- youwhatknow: instructions --
Files over 30 lines show a summary on first read. Read again for the full file, or use offset/limit.
To preview any file without triggering a read: run `youwhatknow summary <path>` in the terminal.
```

Callout: "The deny is soft — Claude can always read the file by trying again. youwhatknow just makes it consider the summary first."

**Working Set Eviction:** Explanation per spec — sequence-based, threshold 40, `> not >=`, configurable.

**Daemon Configuration:** Table with port, session_timeout_minutes, idle_shutdown_minutes. Callout about not needing the file.

**Project Configuration:** Table with all 6 fields including new `max_concurrent_batches` and `eviction_threshold`. Built-in ignore patterns block.

**API Endpoints:** 7 endpoint cards including new `/hook/summary` and `/hook/reset-read`. Note on `/status` not touching activity.

**Remaining sections** (indexing, storage, env vars, lifecycle, nix): Carry over content from existing `docs/docs.html` — it's still accurate. Use the same HTML structure but reference `css/style.css` instead of inline styles.

- [ ] **Step 2: Open in browser, verify all sections render correctly**

Check: nav, TOC links, all 13 sections, code blocks, config tables, endpoint cards, callouts, footer.

- [ ] **Step 3: Commit**

```bash
git add docs/docs.html
git commit -m "feat(docs): rewrite docs page with accurate behavior and new sections"
```

---

### Task 5: Update README.md

**Files:**
- Modify: `README.md`

The README also describes the old allow-with-context behavior and is missing features. Update it to match.

- [ ] **Step 1: Update README content**

Key changes:
- "What this does" section: Rewrite item 1 to describe deny-first instead of "pre-read summaries". First read is denied with a summary; second read goes through.
- Add working set eviction to the feature list.
- Add `reset` to CLI commands section.
- Update "Setup — the easy way" to include installation methods (installer script first).
- Update the `summary` command description to note it primes the read count.

Keep the same tone and structure. Don't change the fridge joke or the overall README format.

- [ ] **Step 2: Commit**

```bash
git add README.md
git commit -m "docs: update README to reflect deny-first behavior and new features"
```

---

### Task 6: Final review and cleanup

- [ ] **Step 1: Delete no-longer-needed inline styles**

Verify neither `index.html` nor `docs.html` contain any `<style>` blocks.

- [ ] **Step 2: Cross-page navigation check**

Verify:
- Landing page "Docs" link → `docs.html`
- Landing page "GitHub" link → `https://github.com/wavefunk/youwhatknow`
- Docs page "Home" link → `index.html`
- Docs page "Docs" link → `docs.html` (active state)
- Docs page "GitHub" link → `https://github.com/wavefunk/youwhatknow`
- TOC links all have matching `id` anchors
- Footer links match on both pages

- [ ] **Step 3: Responsive check**

Verify the CSS includes all responsive breakpoints from the original:
- `@media (max-width: 900px)`: benefits grid → 1 column, arch rows → column, bubbles left-align
- `@media (max-width: 600px)`: TOC → 1 column, lang-grid → 1 column, config table smaller font

- [ ] **Step 4: Commit any fixes**

```bash
git add -A docs/
git commit -m "fix(docs): final review cleanup"
```
