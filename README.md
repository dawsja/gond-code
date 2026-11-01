# Gond Code

A Windows app that hosts your coding agents in a terminal workspace. Each pane is a terminal in
your default shell; type an agent there (Claude Code, Codex, Antigravity, Grok Build, opencode,
Pi, GitHub Copilot or Cursor) and Gond Code sees it running. Panes are grouped into named
**Sessions**, and the sidebar lists every agent by what it's doing: needs you, working, done or
idle.

Gond Code doesn't sit between you and the agent. Each one runs as it would in your own terminal,
with its own permissions, tools and sign-in. Gond Code installs and updates the agents, lays out
their panes, and shows each agent's plan usage.

## Install

Download `Gond.Code_<version>_x64-setup.exe` from the latest
[release](https://github.com/dawsja/gond-code/releases/latest) and run it. It installs for your
user only; no admin rights needed. Windows 10 22H2 or 11, x64.

The installer isn't code-signed yet, so Windows SmartScreen may warn the first time: choose
**More info → Run anyway**. After that, Gond Code updates itself from Settings → Updates.

## What's in it

- **Sessions** of split terminal panes: split right or down, resize, zoom, rename, restart.
  Sessions are saved, so the app reopens as you left it.
- **Agents in the sidebar**, grouped by status read from each terminal: Needs you, Working,
  Done, Idle.
- **Agents from the terminal:** every pane is your default shell; type `claude`, `codex` or any
  installed agent to start it, and Gond shows it on the pane and in the sidebar.
- **Panes come back** after a restart or an update in the same shell and folder.
- **Fast terminals:** xterm.js on WebGL with full 24-bit colour; Rust runs the PTYs (ConPTY).
- **Settings:** General (colour, default shell, speech to text, close to tray), Usage (plan
  limits), Agents (install, update, uninstall), GitHub (Git and GitHub CLI install and sign-in),
  Updates.
- **Transcription** (speech to text), optional and local (NVIDIA Parakeet): Ctrl+Shift+Space types what you say
  into the focused pane.
- **Ctrl+Shift+P** lists every keyboard shortcut.

## Build from source

You need [Bun](https://bun.sh) and Rust stable ([rustup](https://rustup.rs)). Use Bun, not npm.

```sh
bun install
bun run tauri dev     # Gond Code Dev, with hot reload (see below)
bun run dev           # the UI alone in a browser, on mock data
bun run typecheck     # tsc --noEmit
bun run lint          # eslint, prettier --check, cargo fmt --check
bun run test          # vitest, then cargo test
bun run dist:win      # the NSIS installer, in src-tauri/target/release/bundle/nsis/
```

A local debug build: `bun x tauri build --debug --no-bundle`, then run
`src-tauri\target\debug\gond-code.exe`.

### Dev and prod side by side

You can work on Gond Code from inside Gond Code. Every debug build is **Gond Code Dev**, a
separate app from the Gond Code you installed:

|            | Gond Code (prod)                                  | Gond Code Dev                                           |
| ---------- | ------------------------------------------------- | ------------------------------------------------------- |
| Built by   | a release build (`bun run dist:win`)              | any debug build (`bun run tauri dev`)                   |
| Runs from  | `%LOCALAPPDATA%\Gond Code\gond-code.exe`          | `src-tauri\target\debug\gond-code.exe`                  |
| Identifier | `app.gond.code`                                   | `app.gond.code.dev`                                     |
| Data       | `%APPDATA%\GondCode`, `%LOCALAPPDATA%\GondCode`   | `%APPDATA%\GondCodeDev`, `%LOCALAPPDATA%\GondCodeDev`   |
| Looks like | "Gond Code"                                       | "Gond Code Dev", with an amber dot on its icon          |
| Updates    | itself, from Settings → Updates                   | never: pull and run it again                            |

They don't share sessions, settings, the database, the speech model, the window position or the
single-instance lock, so both run at once and Dev can't touch your real sessions. They do share
the PC: the agents either one installs, updates or uninstalls, and the folders their terminals
work in, are the same.

The channel is fixed at compile time (`src-tauri/src/channel.rs`), so a release build is always
prod. For an optimised Dev build, use `bun x tauri build --no-bundle --features dev`. Never
bundle it: the installer would install over prod.

On Windows, install the [Microsoft C++ Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/)
("Desktop development with C++") and Rust's MSVC toolchain (`rustup default stable-msvc`).
Speech to text builds transcribe.cpp from source, so CMake must be on PATH. WebView2 ships with
Windows 10 and 11.

Gond Code ships for Windows only, but the Rust side compiles and tests on Linux. Tauri needs
WebKitGTK 4.1 there:

```sh
# Debian / Ubuntu
sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev libsoup-3.0-dev \
  libjavascriptcoregtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev \
  libxdo-dev libssl-dev build-essential pkg-config
```

## Docs

- [docs/PRODUCT.md](docs/PRODUCT.md): what the app does and what it doesn't
- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md): how it's built
- [docs/HARNESSES.md](docs/HARNESSES.md): each agent's install, detection and plan usage
- [docs/AGENT-API.md](docs/AGENT-API.md): the command registry
- [docs/DESIGN.md](docs/DESIGN.md) and [docs/THEME.md](docs/THEME.md): design and theme
- [CONTRIBUTING.md](CONTRIBUTING.md) and [AGENTS.md](AGENTS.md): how to work on it
