#!/usr/bin/env bash
# Gemini CLI is removed: Google models run through agy only. Fails on any mention
# of the Gemini CLI harness in code, design or docs, except one "not supported" note in the docs.
# "Gemini" model names and the Gemini brand icon (via agy) stay allowed.
set -euo pipefail
cd "$(dirname "$0")/.."

hits=$(grep -rniE 'gemini[ _-]?cli' src src-tauri/src docs 2>/dev/null || true)
notes=$(grep -E '^docs/(HARNESSES|PRODUCT)\.md:[0-9]+:.*not supported' <<<"$hits" || true)
rest=$(grep -vxF -f <(printf '%s\n' "$notes") <<<"$hits" || true)
[ -z "$notes" ] && rest=$hits

if [ "$(grep -c . <<<"$notes")" -gt 1 ]; then
  echo "::error::More than one 'Gemini CLI not supported' note; keep a single one (HARNESSES.md or PRODUCT.md)."
  echo "$notes"
  exit 1
fi
if [ -n "$rest" ]; then
  while IFS= read -r line; do
    file=${line%%:*}; rest_=${line#*:}; ln=${rest_%%:*}
    echo "::error file=$file,line=$ln::Gemini CLI was removed; use agy for Google models."
  done <<<"$rest"
  echo "$rest"
  exit 1
fi
echo "No Gemini CLI references (allowed note: ${notes:-none})"
