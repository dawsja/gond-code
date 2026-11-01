# Gond Code — theme

The source of truth is `src/styles/theme.css`; this is a summary. shadcn's semantic tokens carry
the values, so shadcn components and ours pick them up without hard-coded colours. Contrast is
checked in `theme.test.ts`.

**Dark mode only:** no light theme and no theme switch. `<html>` always
carries `dark`, and the tint is `data-tint` on it (`src/lib/theme.ts`).

**Window chrome:** Windows' own dark title bar, dark before the window
shows so it never flashes white.

**Logo:** Gond Code's mark, a black rounded square with a faint light edge and the Gond bot's eyes as a
white `>` and `|` (`docs/gond-code.svg` for the UI, `docs/gond-code-1024.png` for Windows icons; pinned in `docs/brand.sha256`). Gond's cog
(`docs/gond.svg`) means Gond itself.

## Fonts and shape
- **Inter Variable** for UI, **JetBrains Mono Variable** for terminals and code
  (`@fontsource-variable/*`). Base letter-spacing −0.02em.
- `--radius` 0.5rem for controls; cards 16–24px; pills full. Nested corners are concentric
  (DESIGN.md, Identity).

## Surfaces (near-black)

| Token | Value | Use |
|---|---|---|
| `--background` | `#000000` | the floor, terminals |
| `--sidebar` | `#030303` | sidebar |
| `--card`, `--popover` | `#0e0e0e` | cards, menus, dialogs |
| `--raised` | `#141414` | raised controls |
| `--hover`, `--secondary`, `--muted` | `#1a1a1a` | hover, selected rows |
| `--accent` | `#1e1e1e` | accent surfaces |
| `--border` | `#1c1c1c` | borders (`--border-subtle` `#151515`) |
| `--edge`, `--input` | `#262626` | pane and field edges, scrollbar thumb |
| `--foreground` | `#ededed` | text |
| `--muted-foreground` | `#8a8a8a` | secondary text |
| `--faint` | `#5a5a5a` | meta text, idle dots |
| `--destructive` | `#e5484d` | destructive actions |

## Tones
Material-200 tones from Gond's dark theme carry state: `--tone-ok #a5d6a7` (done),
`--tone-warn #ffe082` (needs you), `--tone-bad #ef9a9a` (exited, errors), `--tone-info #90caf9`.
Accents: `--mint #81c995`, `--amber #fdd663`, `--sky #8ab4f8`, `--magenta #f28bca`,
`--grape #c58af9`, `--ember #cf6679`, `--ash #6e6e6e`.

## Tint
One accent colour the person picks in Settings → General (default blue). `--primary` is
`--tint-solid` (white text on it clears 4.5:1). Each tint sets five shades:

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
working spinner and tinted text.

## Terminal colours
The terminal palette (true black ground, saturated ANSI colours and readable brights in Gond's
tones) lives in `src/features/workspace/Terminal.tsx`. Programs that send 24-bit colour get it
as sent. The sign-in fallback terminal
(`src/features/terminal/XtermPane.tsx`) reads its colours from the theme.
