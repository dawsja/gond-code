# Release notes

One file per release, named for its version without the `v`: `0.1.0.md`. The release workflow
uses it verbatim as the GitHub Release notes, and `scripts/publish-release.sh` copies it into
`latest.json`, whose notes the app shows in Settings → Updates. A release candidate
(`v0.1.0-rc.2`) uses its own file (`0.1.0-rc.2.md`) if there is one, otherwise its release's
(`0.1.0.md`). With neither, GitHub generates notes covering what changed since the previous
tag.

Write for the person installing Gond Code, not for the team: a short intro line, then a few
grouped bullets of what's new, then known issues. No internal names, file paths or commands.

The Updates tab shows the notes as plain text, not rendered Markdown, so keep the Markdown
light: `##` headings and `-` bullets read fine either way; avoid bold, links and tables.

```md
Gond Code 0.2.0 adds …

## New
- …

## Fixed
- …

## Known issues
- …
```
