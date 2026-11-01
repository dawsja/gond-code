# Gond Code — design

The design language and the screens as they are now. Palette, fonts and tokens:
[THEME.md](THEME.md). Where this file and the code disagree, fix this file.

## 1. Identity

- **Concentric corners:** a shape nested inside another keeps the same curve.
  Inner radius = outer radius − the padding between them (a pill inside a pill is a pill).
  Segmented controls, menus and tiles in cards follow it; the layout check's `concentric` rule
  fails a square highlight in a pill track.
- **Palette:** dark only, near-black surfaces (THEME.md). Gond's Material-200 tones
  (`--tone-ok/warn/bad/info`) carry state. **One tint** (the person's colour, blue by default)
  marks focus, the focused pane and primary buttons.
- **Fonts:** Inter Variable for UI, JetBrains Mono for terminals, ids and paths. Body 12–13.5px,
  meta 11.5px. Base letter-spacing −0.02em.
- **Shape:** cards 16–24px radius, panes 10px, inputs `rounded-xl`, buttons and badges full
  pills. Borders 1px; no drop shadows.
- **The Gond Code mark** is the file exactly: a black rounded square on white, with
  the Gond bot's eyes as a terminal prompt, a white `>` and `|` (`docs/gond-code.svg` for the UI, `docs/gond-code-1024.png` for Windows icons). Never recoloured, tinted, outlined or inverted. Gond's cog (`docs/gond.svg`)
  means Gond itself, not this app. The mark is in the tray, the installer, the app icon and
  Windows' title bar.
- **Brand icons:** every agent and model shows its real mark (`@lobehub/icons`, MIT; Pi's own
  `pi.svg`), colour variants where they exist, mono marks in `--foreground`. A custom agent
  without one gets a neutral glyph. Never hand-drawn logos.
- **Voice:** plain words. Buttons are named for the act ("Split right", "Restart session"),
  status is one or two words ("Needs you", "Working"), people and agents are named, never
  "the AI".
- **No stubs:** a control that doesn't work yet is hidden, never greyed out with "Not available
  yet".

## 2. The window

```
┌──────────────────────── Windows' own dark title bar ─────────────────────────┐
├──────────────┬───────────────────────────────────────────────────────────────┤
│ Sessions   + │ ┌─ ✦ Claude Code · Opus 5.5 · High ──── ● Working  × ─┐┌─────┐ │
│  Main        │ │                                                     ││     │ │
│  Review      │ │  (the agent's own CLI)                              ││     │ │
│              │ │                                                     │└─────┘ │
│ Agents     + │ │                                                     │┌─────┐ │
│ Needs you 1  │ └─────────────────────────────────────────────────────┘│     │ │
│  ✦ api  2m ! │                                                        └─────┘ │
│ Working 2    │                                                               │
│ Done 1       │                                                               │
│ ⚙ Settings   │                                                               │
└──────────────┴───────────────────────────────────────────────────────────────┘
```

- **Title bar:** Windows' own, dark from the first frame, titled "Gond Code" with the app icon.
  Windows gives Minimize / Maximize / Close, snap layouts and the system menu; we draw nothing
  in it.
- **Sidebar** (248px, `--sidebar`, right border):
  - **Sessions** with a + (New session): one row per session; the current one on `--hover`.
    Double-click or right-click → Rename; right-click → Close session.
  - **Agents** with a + (New agent, as Ctrl+T): every agent pane in every session, grouped in
    the order Needs you, Working, Done, Idle, Exited. Each group heading is the status in its
    tone with a count. Each row: brand icon, the pane's name, how long it has held that status
    ("2m"), a status icon (amber alert, spinning loader in tint ink, green check, grey dot, red
    x). Clicking a row opens its session and focuses the pane. A tooltip names pane · agent ·
    session.
  - At the foot: the app update button when a newer Gond Code is ready, then Settings.
- **Workspace:** the current session's panes. Every session's panes stay mounted (hidden when
  another is showing), so agents keep running.

## 3. Panes

- A pane is a card: 10px radius, 1px `--edge` border that turns `--tint-edge` when focused, on
  pure black. Panes are laid out flat, so splitting never remounts a terminal.
- **Header** (32px, 12px text): brand icon (or a terminal glyph for a shell), the pane's name
  (Rename edits it in place), its detail in `--faint` ("Opus 5.5 · High"), then the status
  (dot and word; amber for Needs you) and a close ×. Focused: a light tint wash; else `#070707`.
- **Terminal:** xterm.js on the WebGL renderer, JetBrains Mono, a palette in Gond's tones (true
  black ground, saturated ANSI colours, readable brights, 24-bit colour passed through).
- **Keys:** Ctrl+T new agent; Ctrl+Shift+→ split right; Ctrl+Shift+↓ split down; Ctrl+Shift+Z
  zoom; Ctrl+Shift+W close; Ctrl+Shift+C / V copy and paste (Ctrl+C copies when there's a
  selection, else stops the program); Ctrl+Shift+Space dictation, Esc cancels it.
- **Shortcut list (Ctrl+Shift+P):** a command-menu-style dialog (`KeyHelp.tsx`) with a search
  field and the keys grouped as Panes, Terminal, Speech and Help. It only lists them.
- **Right-click menu:** Copy, Paste, Select all, Split right, Split down, Rename, Restart session (Restart
  terminal for a shell), Open folder, Zoom pane, Close pane.
- Dividers between panes drag to resize.

## 4. New agent (Ctrl+T)

A dialog: **Agent** (installed agents with brand icons, plus a plain terminal), **Sign in as**
(the agent's own sign-in, other accounts, or "Paste an API key…", saved to Credential Manager),
**Folder** (recent folders, or Browse), **Model** (each with its brand icon; one that thinks
opens its thinking levels beside it, and picking a level picks the model), then **Split right**
or **Split down** when the session already has panes. Enter starts it. The menus are Gond's own
(shadcn), never the browser's.

## 5. Empty session

No starter screen and no setup screen: a session with no panes opens one terminal in the
default shell (`AutoShell.tsx`), in the last folder used, or the home folder before any. Closing
a session's last terminal opens a fresh one in the same shell. Agents come from Ctrl+T or the
Agents +.

## 6. Settings

A section nav (General, Usage, Agents, GitHub, Updates) and the section; below 1100px the nav becomes a
row of tabs.
- **General:** Colour (tint swatches), Default shell, Speech to text (download with progress, or
  why this CPU can't), Closing the window (keep running in the tray).
- **Usage:** each account's plan windows as bars that fill as the plan is used (green, then
  amber, then red), with what's left and when it resets. No costs.
- **Agents:** a card per agent: version → latest, install source, Install / Update / Check, the
  sign-in actions (Use an API key, Sign in where supported) and a muted Uninstall that asks
  first: **Keep my settings and sign-ins** or **Remove everything**, listing what goes.
- **GitHub:** the `gh` accounts (Active badge, scopes, Switch to, Disconnect) and Add account:
  Sign in with browser (a large one-time code, Copy, Open GitHub, completes by itself) or Use a
  token (masked, piped to `gh`). Git and GitHub CLI install from here if missing.
- **Updates:** this version and Check for updates. When a newer one is ready: its version, its
  release notes, and **Restart to update**, which turns into a download meter with the percent.
  A note says when agents are working. The sidebar's update button opens this section.

## 7. Background work

Installs, updates, sign-ins and the speech download run as calm background jobs: no terminals,
no console windows, no raw output unless the person asks for details.
- The card that started a job shows a thin meter, the step in plain words ("Installing Git · 2 of
  3") and the time left; an agent install also shows Cancel. Moving on leaves a progress toast bottom-right with Hide; hiding never
  cancels.
- An error is one plain sentence with **Retry** and a quiet **Show details** (the captured log,
  read-only). A sign-in completes by itself when the agent or `gh` reports it signed in.

## 8. Windows citizen

- **Tray:** the mark and a menu (Show, Quit); a click brings the window forward. With "Keep
  running in the tray" on, the window's × hides to it so the agents keep working.
- **Single instance:** starting the app again brings the running window forward.
- **Mica Alt** on Windows 11 (`window.rs`) only where the UI leaves its background transparent;
  content surfaces stay solid near-black.
- **No web patterns:** no URLs or routes in the UI, native folder pickers, thin dark scrollbars,
  no page loads, window size and position remembered. Text selection is off for chrome and on
  for content.

## 9. Layout rules

- Window minimum 1024×720. When things don't fit, collapse a column (to tabs or a menu); never
  squeeze text below 12 characters.
- One spacing scale: 4 · 8 · 12 · 16 · 20 · 24 · 32 · 40.
- Every flex/grid child holding text has `min-w-0`. One-line fields truncate with an ellipsis
  and a tooltip with the full value. Labels and badges never wrap. Numbers are tabular.
- `scripts/layout-check.ts` (CI: `layout.yml`) audits every screen for overflow, narrow text
  boxes, concentric corners and the like, and fails on any finding.
- Every rendered entity carries `data-entity` (and `data-id` where it has one); controls carry
  `data-command` (a registry name) or `data-local` for UI-only actions; status is always words
  as well as colour.

## 10. Motion

Small and quick: menus and dialogs fade (≈140ms), the working loader spins, nothing else moves.
All motion honours `prefers-reduced-motion`.
