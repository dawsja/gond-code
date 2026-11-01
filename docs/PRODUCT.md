# Gond Code — product

## One line
A Windows app that hosts coding agents in a terminal workspace: named Sessions of split terminal
panes, each running an agent's own interactive CLI or a plain shell, and a sidebar that shows
every agent by what it's doing.

## What's in it
- **Sessions.** Named groups of panes. Add, rename and close them from the sidebar. They are
  saved to `sessions.json` beside the database, so the app reopens as you left it.
- **Agents.** The sidebar lists every agent pane in every session, grouped by status: **Needs
  you**, **Working**, **Done**, **Idle** (and Exited), with how long it has been in that status.
  Gond reads the status from each terminal's output, as you would: output still coming means
  working, a question on screen ("Do you want to…", "[y/n]") means it needs you. Under its name,
  an agent with a plan shows the window it will run out of first ("Weekly 80% left · resets Tue
  3 AM"), amber at 10% or less and red when it's used up, and Gond warns once as it gets low and
  once when it runs out. While one runs, Gond reads its plan when it starts and every 5 minutes.
- **Panes.** Each pane is a terminal in your default shell. You start an agent by typing it
  there (Claude Code, Codex, Antigravity, Grok Build, opencode, Pi, GitHub Copilot, Cursor, or
  a custom agent); Gond sees it running and shows it on the pane and in the sidebar. Panes split right
  or down, resize, zoom, rename and restart. The right-click menu has Copy, Paste, Select all,
  Split right, Split down, Rename, Restart, Open folder, Zoom and Close.
- **Search every pane (Ctrl+Shift+F).** A search box over what every terminal in every session
  printed (its scrollback, replays included), newest first, grouped by pane and session.
  Choosing a match opens its session, focuses the pane, scrolls to the line and selects it.
- **`file:line` links.** A path a pane prints (`src/pty/host.rs:199:16`, `main.c(12,5)`,
  `./docs/PRODUCT.md`) is a link when it is a real file, from the folder the pane is in. A
  click opens it at the line in the editor picked in Settings → General → Open files in (VS
  Code, Cursor, Windsurf or Zed, whichever are installed), else the file's default app. This is
  the one way into an editor: Gond has no editor or Explorer panels.
- **Drag and drop.** Files and folders dragged over a pane light it up ("Drop to insert the
  path"); dropping types their paths in, as a paste would, the way the pane's shell writes a
  path (`/c/…` in Git Bash, `/mnt/c/…` in WSL, quoted when needed). An agent there reads them;
  a picture's path is how the agents attach one. A file in the temp folder (a screenshot
  dragged from the Snipping Tool, a file dragged out of a zip or a mail) is copied to Gond's
  data folder first and kept a week, so it's still there when the agent reads it.
- **Status bar.** A thin bar under the panes: on the left, where the focused pane is (the git
  repository and branch, or a plain folder, following a shell's `cd`); on the right, Gond Code's
  version.
- **New terminal (Ctrl+T).** A terminal in the default shell beside the focused pane, in its
  folder; Split right and Split down open one too.
- **No setup screen.** A new or empty session opens a terminal in the default shell from
  Settings, in the last folder you used (your home folder before any). Each terminal remembers
  its shell; closing a session's last terminal opens the same shell again.
- **Agents outlive the app.** The terminals run in the terminal host, a background process, so
  quitting, Restart to update or a crash leaves each shell and the agent in it working. When
  Gond Code opens, every pane picks its terminal back up and replays what it printed while you
  were away. Settings → General → Quitting can close them on quit instead; the tray's "Quit and
  close all terminals" always does. Only a restart of the PC ends them: then panes come back as
  their shells, with their last screen, in the folder they were in.
- **Terminals.** xterm.js on the WebGL renderer with full 24-bit colour; Rust runs the PTYs
  (ConPTY).
- **Keys.** Ctrl+T new terminal; Ctrl+Shift+→ / ↓ split right / down; Ctrl+Shift+Z zoom;
  Ctrl+Shift+W close pane; Ctrl+Shift+C / V copy and paste (Ctrl+C copies when there's a
  selection); Ctrl+Shift+Space speech to text, Esc cancels it. Ctrl+Shift+P opens a searchable
  list of them.
- **Transcription** (optional, speech to text). A local NVIDIA Parakeet model, downloaded when you turn it on
  in Settings. The text is typed into the focused pane for you to send.
- **Settings.**
  - **General:** colour, default shell (PowerShell, Command Prompt, Git Bash, WSL; whichever are
    on the PC), terminal font (JetBrains Mono, Hack or Fira Code Nerd Font; nothing custom), speech
    to text, close to tray.
  - **Usage:** each installed agent's plan limits, as the agent reports them for its own sign-in
    (session, weekly, monthly windows). Gond does not count spend.
  - **Agents:** install, update and uninstall each agent CLI. A running install can be
    cancelled. Each agent signs in inside its own CLI.
  - **Plugins:** what Gond Code connects to, one card each. GitHub: Git and the GitHub CLI
    install from here if missing; sign in through `gh`.
  - **Updates:** this version, Check for updates, download progress and Restart to update.
    The app also checks every 6 hours; an update installs only when you click.

## Principles
- **Gond adds nothing between you and the agent.** Each agent's own permissions, tools, skills
  and MCP servers apply. Gond passes no permission flags of its own.
- **A Windows app, never a web app:** Windows' own dark title bar, single instance, remembered
  window position, tray icon, native pickers. The browser dev mode is a developer tool and never
  ships.
- **Local-first.** Agents run on your machine with your own CLIs and sign-ins. No Gond Code
  server, no telemetry.
- **Dark mode only**, near-black, one tint colour (docs/THEME.md).
- **No stubs.** No greyed-out "Not available yet" items: a control is hidden until it works.

## Not in the product
No chat, no MCP server, no skills or connectors pages, no permission modes, no helper agents,
worktrees or checkpoints, no approvals, no event log, no spend tracking or caps, no taskbar jump
list and no command-line copy of the app on PATH.

## Stack
- Tauri 2 (Rust) + React 19 + TypeScript + Vite + Tailwind 4 + shadcn-style components.
- Bun for JS tooling. `portable-pty` (ConPTY) for terminals, xterm.js in the UI.
- Local state in SQLite (`rusqlite`) under `%APPDATA%\GondCode`; secrets in Windows Credential
  Manager.
- Installer: NSIS, per user, built on GitHub Actions. Updates through the Tauri updater from
  GitHub Releases on `dawsja/gond-code`, signature-checked.

## Platform
Windows 10 22H2+ / 11, x64. macOS and Linux are out of scope (Linux is a dev path only).
