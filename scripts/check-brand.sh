#!/usr/bin/env bash
# Brand check: the logo files match their pinned hashes, and every app icon in
# src-tauri/icons is exactly what `bun run icons` makes from docs/gond-code-1024.png (Gond Code's
# own mark: a white circle with a black triangle, never the cog).
set -euo pipefail
cd "$(dirname "$0")/.."

grep -v '^#' docs/brand.sha256 | sha256sum --check --quiet || {
  echo "::error::A brand file in docs/ (gond-code.svg, gond-code-1024.png, gond.svg, gond-1024.png) changed. They must stay the official logos."
  echo "For an approved new logo, update docs/brand.sha256 in the same commit."
  exit 1
}

out=$(mktemp -d)
trap 'rm -rf "$out"' EXIT
bun x tauri icon docs/gond-code-1024.png -o "$out" >/dev/null 2>&1

bad=0
for f in src-tauri/icons/*; do
  name=$(basename "$f")
  # icon.icns is macOS only, and the generator writes it differently every run.
  [ "$name" = icon.icns ] && continue
  [ -d "$f" ] && continue
  if ! cmp -s "$f" "$out/$name"; then
    echo "::error file=$f::$name isn't generated from docs/gond-code-1024.png. Run \`bun run icons\` and commit the result."
    bad=1
  fi
done
exit $bad
