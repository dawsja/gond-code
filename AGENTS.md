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
- **Run it in dev:** `bun install`, then `bun run tauri dev`. `bun run dev` is the UI alone on
  mock data.
- **Checks:** `bun run typecheck`, `bun run lint`, `bun run test`, and
  `cargo clippy --all-targets -- -D warnings` in `src-tauri`.
- **Commands:** a new or changed command needs its row in `docs/AGENT-API.md` (same name and
  risk), and `src/lib/commands.generated.json` regenerated with
  `UPDATE_COMMANDS=1 cargo test --lib commands_json` in `src-tauri`.
- Windows-only code goes behind `#[cfg(windows)]`; everything else must compile and test on
  Linux. Logic lives outside `commands/` so it tests without Tauri.
- Don't add back what the product left out (docs/PRODUCT.md, "Not in the product").

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
