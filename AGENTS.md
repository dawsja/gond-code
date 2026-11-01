# Working on Gond Code

How agents (Claude Code, Codex and the rest) work in this repo. `CLAUDE.md` points here.

## Orientation

Gond Code is a Tauri 2 Windows app that hosts coding agents in a terminal workspace: sessions of
split terminal panes, each running an agent's own CLI or a plain shell. Read
[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) first.

- **Frontend:** `src/` (React 19, TypeScript, Vite, Tailwind). `src/lib/ipc.ts` is the only
  place that calls Tauri.
- **Rust:** `src-tauri/src/`. Every action is a command in the registry (`api/`).
- **Tooling:** Bun only (`bun install`, `bun run`, `bunx`), never npm. Rust stable.
- **Run it in dev:** `bun install`, then `bun run tauri dev`, which opens **Gond Code Dev**
  beside the installed app (see "Dev and prod" below). `bun run dev` is the UI alone on mock data.
- **Checks:** `bun run typecheck`, `bun run lint`, `bun run test`, and
  `cargo clippy --all-targets -- -D warnings` in `src-tauri`.
- **Commands:** a new or changed command needs its row in `docs/AGENT-API.md` (same name and
  risk), and `src/lib/commands.generated.json` regenerated with
  `UPDATE_COMMANDS=1 cargo test --lib commands_json` in `src-tauri`.
- Windows-only code goes behind `#[cfg(windows)]`; everything else must compile and test on
  Linux. Logic lives outside `commands/` so it tests without Tauri.
- Don't add back what the product left out (docs/PRODUCT.md, "Not in the product").

## Dev and prod

You are probably running inside Gond Code: the installed app (**prod**) is where the user codes,
and your terminal is one of its panes. Leave it alone. Work in **Gond Code Dev**, the separate
app every debug build is (`src-tauri/src/channel.rs`; README, "Dev and prod side by side").

- **Start Dev:** `bun run tauri dev`, as a background task (it keeps running and rebuilds on
  changes). There's nothing to configure: a debug build is Dev. It opens its own window, titled
  "Gond Code Dev", with an amber dot on its icon.
- **Stop Dev:** stop that background task, or use the tray's "Quit Gond Code Dev". Dev's X hides
  it to the tray, like prod's. To stop it by process, match on its path:
  `Get-Process gond-code | Where-Object Path -like "*\src-tauri\target\*" | Stop-Process`.
- **Never stop or restart prod.** Prod runs from `%LOCALAPPDATA%\Gond Code\gond-code.exe`. Never
  run `Stop-Process -Name gond-code`, `taskkill /im gond-code.exe` or anything else that matches
  by name: both apps share it, and killing prod kills your own terminal.
- **Never touch prod's data:** `%APPDATA%\GondCode` and `%LOCALAPPDATA%\GondCode`. Dev's data is
  in `%APPDATA%\GondCodeDev` and `%LOCALAPPDATA%\GondCodeDev`. Delete those to reset Dev.
- **Don't run a release build to try something.** A release build is prod: same identity and
  data, so it hands over to the running app or writes your real sessions. For a fast Dev build,
  use `bun x tauri build --no-bundle --features dev`. Never bundle that build or run an
  installer locally: it installs over prod.
- **Dev shares the PC.** An agent that Dev installs, updates or uninstalls (Settings → Agents) is
  installed, updated or uninstalled for prod too. Don't uninstall agents from Dev unless asked.
- **Dev doesn't update itself.** Settings → Updates says so, and `app.update.*` answers `invalid`.
- **Ports:** `tauri dev` serves the UI on 1420. While Dev is running, start the mock UI on
  another port: `bunx vite --port 1477`.
- **A running Dev locks `target\debug\gond-code.exe`:** stop Dev before a standalone
  `cargo build`. `cargo check`, `cargo test --lib` and `cargo clippy` are fine while it runs.

## Testing policy

Prioritize development velocity without sacrificing meaningful correctness.

- Do not generate tests automatically for every function or change.
- Add tests when they protect important behavior, prevent realistic regressions, or validate
  meaningful edge cases.
- Prefer a small number of high-value behavioral tests over exhaustive variations.
- Avoid redundant tests, excessive mocking, and tests that merely duplicate implementation details.
- During development, run only tests relevant to modified code.
- Do not repeatedly execute the full test suite after minor edits.
- Run the full relevant suite before finalizing substantial changes.
- Never modify or delete existing tests simply to make a failing implementation pass.
- Keep tests deterministic, fast, and maintainable.
- Prioritize working, correct software over test counts and coverage percentages.

Running only the relevant tests:

- Frontend: `bunx vitest run src/features/workspace` (a folder or file).
- Rust: `cargo test --lib <name-or-module>` in `src-tauri`, e.g. `cargo test --lib runner`.
- Full suites before finishing a substantial change: `bun run test` (vitest, then cargo test),
  `bun run typecheck`, `bun run lint`, and `cargo clippy -- -D warnings` in `src-tauri`.
