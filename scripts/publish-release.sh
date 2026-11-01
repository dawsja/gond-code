#!/usr/bin/env bash
# Adds the updater manifest (latest.json) to one tag's GitHub Release in this repo, next to
# the installer the release job uploaded. Installed apps read it from the latest published
# release, so drafts and prereleases never reach them. Run by release.yml with release/ holding
# the build artifact. Env: TAG (v1.2.3[-rc.N]), REPO (owner/name), GH_TOKEN.
set -euo pipefail
version=${TAG#v}
shopt -s nullglob
exes=(release/*-setup.exe)
[ ${#exes[@]} -eq 1 ] || { echo "expected one installer in release/, found ${#exes[@]}"; exit 1; }
sig="${exes[0]}.sig"
if [ ! -f "$sig" ]; then
  echo "No updater signature (no TAURI_SIGNING_PRIVATE_KEY): no latest.json, so installed apps won't see $TAG."
  exit 0
fi

# GitHub turns spaces in asset names into dots, so the uploaded installer is at this name.
name="Gond.Code_${version}_x64-setup.exe"
notes_file=""
for f in "docs/releases/$version.md" "docs/releases/${version%%-*}.md"; do
  if [ -f "$f" ]; then notes_file=$f; break; fi
done
notes=${notes_file:+$(cat "$notes_file")}
[ -n "$notes" ] || notes="Gond Code $version"

out=$(mktemp -d)
jq -n --arg v "$version" --arg notes "$notes" --arg date "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
  --arg sig "$(cat "$sig")" \
  --arg url "https://github.com/$REPO/releases/download/$TAG/$name" \
  '{version: $v, notes: $notes, pub_date: $date,
    platforms: {"windows-x86_64": {signature: $sig, url: $url}}}' > "$out/latest.json"
gh release upload "$TAG" "$out/latest.json" --clobber --repo "$REPO"
echo "latest.json added to $TAG in $REPO (installed apps see it once the release is published)."
