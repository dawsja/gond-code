# Gond Code — theme

The token source of truth is `src/styles/theme.css`; this is a summary. shadcn's semantic
light/dark tokens carry the values, so components pick them up without hard-coded colours.
`theme.test.ts` checks palette contrast; native caption and terminal rendering still need visual
review in Gond Code Dev, not just a DOM audit.

## Appearance and persistence

**System / Light / Dark**, default **System**, in Settings → General. System follows the OS's
`prefers-color-scheme` and responds to changes while the app is running; explicit Light or Dark
ignores OS changes. `src/lib/theme.ts` resolves the preference: `<html>` carries `dark` only for
the resolved dark palette, with the matching CSS `color-scheme`. The independent tint stays in
`data-tint` in either mode.

Rust's persisted **`settings.theme`** (`system`, `light`, `dark`) is authoritative. The UI changes
it through `settings.set` and follows the settings state tree. localStorage is **only a
first-paint mirror**, read before React mounts to avoid a flash, then replaced by the hydrated
Rust setting. It is never a second settings store or a reason to overwrite Rust on startup.
Missing, invalid or inaccessible browser storage falls back to System. An older settings store
without a theme also defaults to System.

**Window chrome:** Windows' own title bar follows the resolved appearance. Apply the saved
preference before showing the window, and keep native caption colours in sync with setting
changes and OS changes in System mode. The hidden window reads saved settings before `show()`;
Tauri config does not force a theme. Windows 11 captions use `#030303` / white text in dark and
paper white / `#202020` text in light; Windows 10 uses immersive dark mode without custom caption
colours. Native/WebView backgrounds match the resolved floor. A browser screenshot cannot verify
native chrome. The parser-blocking, same-origin `public/theme-init.js` respects `script-src 'self'`
without allowing inline scripts; its cache keys are `gond-code-theme` and `gond-code-tint`.

**Logo:** Gond Code's mark is an ink rounded square matching Gond and Gond Agent (outer
radius 300, ring radius 287), with a faint light edge and the Gond bot's eyes as a white `>`
and `|`. `docs/gond-code.svg` is the source; `docs/gond-code-1024.png` and Windows icons are
derived from it (pinned in `docs/brand.sha256`). It appears in OS chrome, not in the React UI.
Gond's cog (`docs/gond.svg`) means Gond itself. Never invert or recolour either mark for light mode.

## Fonts and shape

- **Inter Variable** for UI, **JetBrains Mono Variable** for code
  (`@fontsource-variable/*`). Terminals use a Nerd Font shipped in `src/assets/fonts`
  (JetBrains Mono by default, Hack or Fira Code), picked in Settings → General. Base letter-spacing −0.02em.
- `--radius` 0.5rem for controls; cards 16–24px; pills full. Nested corners are concentric
  (DESIGN.md, Identity).

## Surfaces

Light follows [Gond's public globals](https://github.com/dawsja/gond/blob/main/apps/web/app/globals.css):
linen floor, paper panels, mist hover surfaces, fog borders, carbon text and graphite secondary
text. Dark preserves Gond Code's near-black palette, not Gond's generic Material-dark surfaces.
Code keeps tinted primary actions in both modes, even though Gond's light `--primary` is neutral.

| Token | Light | Dark | Use |
|---|---|---|---|
| `--background` | `#f5f3ee` | `#000000` | floor, terminals |
| `--sidebar` | `#ffffff` | `#030303` | sidebar |
| `--card`, `--popover` | `#ffffff` | `#0e0e0e` | cards, menus, dialogs |
| `--raised` | `#ffffff` | `#141414` | raised controls |
| `--pane-header` | `#ebe8e1` | `#070707` | unfocused pane headers |
| `--hover`, `--secondary`, `--muted` | `#ebe8e1` | `#1a1a1a` | hover, selected rows |
| `--accent` | `#ebe8e1` | `#1e1e1e` | accent surfaces |
| `--border` | `#d6d2c9` | `#1c1c1c` | borders |
| `--border-subtle` | `#ebe8e1` | `#151515` | quiet dividers |
| `--edge`, `--input` | `#c9c5bb` | `#262626` | pane and field edges, scrollbar thumb |
| `--foreground` | `#1b1a18` | `#ededed` | text |
| `--muted-foreground` | `#524f4a` | `#8a8a8a` | secondary text |
| `--faint` | `#6b6863` | `#5a5a5a` | meta text, idle dots |
| `--ring` | `#1b1a18` | `#5c5c5c` | focus rings |
| `--destructive` | `#c2301a` | `#e5484d` | destructive actions |

Use semantic tokens for surfaces, text, hover/selected rows, focus, scrollbars and status. A
literal near-black surface or white text in a shared component is not a light-mode fallback.

## Tones

Gond's deeper light tones and Material-200 dark tones carry state; words/icons accompany colour.

| Token | Light | Dark | State |
|---|---|---|---|
| `--tone-ok` | `#1a7d38` | `#a5d6a7` | done |
| `--tone-warn` | `#8a5a00` | `#ffe082` | needs you |
| `--tone-bad` | `#c2301a` | `#ef9a9a` | exited, errors |
| `--tone-info` | `#2266c4` | `#90caf9` | information |

| Accent | Light | Dark |
|---|---|---|
| `--mint` | `#1a7d38` | `#81c995` |
| `--amber` | `#8a5a00` | `#fdd663` |
| `--sky` | `#2266c4` | `#8ab4f8` |
| `--magenta` | `#b8307f` | `#f28bca` |
| `--grape` | `#8250df` | `#c58af9` |
| `--ember` | `#c2301a` | `#cf6679` |
| `--ash` | `#6b6863` | `#6e6e6e` |

## Tint

One accent colour the person picks in Settings → General (default blue), independent of
appearance. `--primary` is `--tint-solid` with white text. Each tint sets five shades; light
values below are the **exact light tint values from Gond's public globals**, not the dark
shades reused on a pale background. Contrast tests protect white on solid and ink on wash
(at least 4.5:1).

### Light

| Tint | `--tint` | `--tint-solid` | `--tint-wash` | `--tint-edge` | `--tint-ink` |
|---|---|---|---|---|---|
| blue | `#2f6fd6` | `#2c62ba` | `#d6dbe0` | `#a9b8cd` | `#29559d` |
| purple | `#7650d8` | `#6848bb` | `#ded7e0` | `#bdafce` | `#5b409e` |
| pink | `#cf3d85` | `#b43875` | `#e8d5d7` | `#d6aab7` | `#993364` |
| red | `#d03a3a` | `#b53535` | `#e8d5cf` | `#d6a9a2` | `#9a3030` |
| orange | `#e04e0a` | `#c2460c` | `#ead7c9` | `#dbae94` | `#9f3d0f` |
| amber | `#d98a08` | `#9e670d` | `#e9dec9` | `#d9bf94` | `#83580f` |
| green | `#189c47` | `#19863f` | `#d4e0d0` | `#a3c4a5` | `#196d36` |

### Dark (unchanged)

| Tint | `--tint` | `--tint-solid` | `--tint-wash` | `--tint-edge` | `--tint-ink` |
|---|---|---|---|---|---|
| blue | `#2f6fd6` | `#2b61b9` | `#252f40` | `#3d5780` | `#8db0e8` |
| purple | `#7650d8` | `#6747ba` | `#302a40` | `#5a4b81` | `#b49fea` |
| pink | `#cf3d85` | `#b33774` | `#3f2733` | `#7d4360` | `#e594bc` |
| red | `#d03a3a` | `#b33434` | `#3f2727` | `#7e4242` | `#e59393` |
| orange | `#e04e0a` | `#c1450b` | `#412a1f` | `#844a2f` | `#ee9e78` |
| amber | `#d98a08` | `#9f670b` | `#40331f` | `#81622e` | `#eabf77` |
| green | `#189c47` | `#17853e` | `#213629` | `#346947` | `#80c99a` |

`--tint-edge` outlines the focused pane, `--tint-wash` tints its header, `--tint-ink` is the
working spinner and tinted text. Focus, selected rows and primary actions use the same tint
in both modes; the shades change with the resolved palette.

## Terminal colours

**`src/lib/terminalTheme.ts` is the source** for both light and dark xterm palettes,
used by workspace terminals. Any future terminal consumer must use the same palette (there is
no separate sign-in terminal in this checkout).
Light uses linen with carbon foreground and darker ANSI colours; dark retains true black,
saturated ANSI colours and readable brights. Both define foreground, background, cursor,
selection and all 16 ANSI colours. Programs that send explicit 24-bit colour get it as sent;
Gond does not recolour their output, so an agent's own theme may still need changing in its CLI.

Light terminal palette (normal / bright): black `#1b1a18` / `#524f4a`, red `#c2301a` /
`#b53535`, green `#1a7d38` / `#19863f`, yellow `#8a5a00` / `#9e670d`, blue `#2266c4` /
`#2f6fd6`, magenta `#923eaf` / `#b8307f`, cyan `#147580` / `#20838c`, white `#6b6863` /
`#77736c`. Foreground/cursor are carbon `#1b1a18` on linen `#f5f3ee`; active/inactive selections
are `#c8d7ed` / `#dddcd7`. Normal text clears 4.5:1, every ANSI colour clears 3:1 on the ground.

Changing appearance updates mounted xterm instances' theme options in place, including panes
in hidden sessions. Do not recreate the terminal, clear scrollback/selection, reopen PTYs or
restart agents to apply a palette. Native caption updates are likewise independent of PTYs.

## Checking both palettes

The mock supports `?theme=light` and `?theme=dark`. `scripts/layout-check.ts` runs every configured
screen and size in both modes, with the same audit and zero-findings bar. `--screenshots` adds
one PNG per screen×theme×size to `--out` for optional visual review. The DOM audit does not
validate contrast in terminal canvases, native Windows caption colours, or arbitrary truecolour
output. Review those in Gond Code Dev, including a live System-mode OS change and switching
appearance with running agents; do not claim that review was done merely because layout passed.
