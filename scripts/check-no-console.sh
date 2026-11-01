#!/usr/bin/env bash
# No visible console windows: every child process starts through crate::process::command,
# which sets CREATE_NO_WINDOW on Windows. Fails on any other Command::new in the app's Rust code
# (test modules, after #[cfg(test)], are exempt).
set -euo pipefail
cd "$(dirname "$0")/.."
bad=0
while IFS= read -r f; do
  [ "$f" = src-tauri/src/process.rs ] && continue
  # Test-only module (#266 stage 2): it starts windowed children on purpose, to prove its probe.
  [ "$f" = src-tauri/src/no_console_e2e.rs ] && continue
  hits=$(awk '/#\[cfg\(test\)\]/ { exit } /Command::new\(/ { print FILENAME ":" FNR ": " $0 }' "$f")
  if [ -n "$hits" ]; then
    while IFS= read -r h; do
      echo "::error file=${h%%:*},line=$(cut -d: -f2 <<<"$h")::Start child processes with crate::process::command (no console window, #266), not Command::new."
    done <<<"$hits"
    echo "$hits"
    bad=1
  fi
done < <(grep -rl --include='*.rs' -E 'Command::new\(' src-tauri/src || true)
[ $bad = 0 ] && echo "Every child process starts through crate::process::command"
exit $bad
