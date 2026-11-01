# Contributing to Gond Code

Setup and the dev loop are in [README.md](README.md); the orientation and testing policy are in
[AGENTS.md](AGENTS.md).

## Rules

- Work happens on `main`. Keep it green.
- Build and test on Windows before pushing anything that touches the UI or the Rust side.
- Bun for JS (`bun install`, `bun run`, `bunx`), never npm. Commit `bun.lock` with any
  dependency change, or CI's frozen install fails.
- Windows-only code behind `cfg(windows)`; the Linux dev path keeps compiling so logic tests
  there.
- Child processes start through `crate::process::command`, so none opens a console window
  (`scripts/check-no-console.sh`).
- New data goes in a folder under `%APPDATA%\GondCode` or `%LOCALAPPDATA%\GondCode`, never a
  root of its own, so uninstall can remove it. Reach them through `appdata::roaming()` and
  `appdata::local()`, never a written-out path, so Gond Code Dev keeps its own (`GondCodeDev`). Migrations are numbered SQL files in
  `src-tauri/src/store/migrations`; the next free number is 0021.
- Icons are generated from `docs/gond-code-1024.png` with `bun run icons`; CI checks they match.
- No screenshots in CI.

## Real-agent tests

These are `#[ignore]`d and install, sign in or download for real. Run one with
`cargo test --lib <name> -- --ignored --nocapture` in `src-tauri`: `real_install`,
`real_update`, `real_uninstall`, `real_installs_without_winget`,
`real_winget_download_reports_progress`, `real_codex_repairs_after_its_files_are_removed`
(`GOND_REAL_HARNESS`, `GOND_REAL_PLAN`), `real_signin_reaches_the_link`,
`real_own_sign_ins_are_found`, `real_plan_usage`, `real_speech_round_trip` (downloads the 731 MB
model; `GOND_SPEECH_WAV` for a recording). A shell started before an install doesn't see the new
agent: read the Machine and User PATH from the registry first. Unit tests must not depend on
what's installed.

## CI

- `ci.yml`, on every push to `main`:
  - `checks` (Ubuntu): brand check, no Gemini CLI, no console windows, frozen install,
    typecheck, lint, build, vitest, cargo test, `cargo clippy --all-targets -- -D warnings`.
  - `windows`: vitest, cargo test and the NSIS installer as the `gond-code-windows-nsis`
    artifact.
  - `install fallback`, `winget progress` and `smoke` (install, launch, uninstall) on Windows.
- `layout.yml` audits every screen for layout problems and fails on any finding.
- `install-check.yml` and `signin-check.yml` run real agent installs, uninstalls and sign-ins.

## Releases

- A `v*` tag runs `release.yml`. It builds the installer, drafts a GitHub Release in
  `dawsja/gond-code` (a prerelease when the tag has a `-`), and `scripts/publish-release.sh` adds
  `latest.json` for the updater. The notes come from `docs/releases/<version>.md`
  ([docs/releases/README.md](docs/releases/README.md)).
- A person publishes the draft. Installed apps read `latest.json` from the latest published
  release, so drafts and prereleases never reach them.
- The updater signing key is a repo secret; never print it. The installer is checked against the
  public key in `src-tauri/tauri.conf.json`. Code signing isn't set up yet.
- Never move a tag. If a release candidate fails, cut the next number.

## Gotchas

- **CRLF on a Windows checkout:** plain `prettier --check .` and `scripts/check-brand.sh` fail
  locally on line endings only. Use `bun x prettier --check --end-of-line auto <files>`; CI on
  Linux is the judge.
- **One-line note:** `check-no-gemini-cli.sh` allows one "Gemini CLI … not supported" note, on
  one line. Don't rewrap it.
- **A running `gond-code.exe` locks the debug build:** close it first, or use `cargo check --lib`.
- **ConPTY:** hidden terminals send `ESC[6n` and must get `ESC[1;1R` back, or winget and
  background sign-ins hang. ConPTY repaints only changed cells, so parsed progress must only move
  forward.
- **Speech builds for a CPU floor** (`src-tauri/.cargo/config.toml`, `TRANSCRIBE_CMAKE_ARGS`):
  AVX2, FMA, F16C and BMI2, with AVX-512 off. It needs CMake on PATH (winget's `Kitware.CMake`
  isn't on PATH by default).
- **Locks** are module statics and not re-entrant: drop one before calling a helper that takes
  it.
