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
  working, a question on screen ("Do you want to…", "[y/n]") means it needs you.
- **Panes.** Each pane is a terminal in your default shell. You start an agent by typing it
  there (Claude Code, Codex, Antigravity, Grok Build, opencode, Pi, GitHub Copilot, Cursor, or
  a custom agent); Gond sees it running and shows it on the pane and in the sidebar. Panes split right
  or down, resize, zoom, rename and restart. The right-click menu has Copy, Paste, Select all,
  Split right, Split down, Rename, Restart, Open folder, Zoom and Close.
- **Status bar.** A thin bar under the panes: on the left, where the focused pane is (the git
  repository and branch, or a plain folder, following a shell's `cd`); on the right, Gond Code's
  version.
- **New terminal (Ctrl+T).** A terminal in the default shell beside the focused pane, in its
  folder; Split right and Split down open one too.
- **No setup screen.** A new or empty session opens a terminal in the default shell from
  Settings, in the last folder you used (your home folder before any). Each terminal remembers
  its shell; closing a session's last terminal opens the same shell again.
- **Resume.** After Restart to update, panes come back as their shells, with their screen, in
  the folder they were in; you start the agent again with its own resume (`claude --continue`,
  `codex resume`).
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
    on the PC), speech to text, close to tray.
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
