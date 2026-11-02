# agents/

One file per agent Gond Code supports. Everything about an agent that its vendor can change
lives here: how it's installed, updated and uninstalled, where it puts itself, how Gond sees it
running, how its conversations resume, what its prompts look like, where its plan usage comes
from. The code (`src-tauri/src/agents/spec.rs` and what reads it) is the general machinery.

The files are built into the app (and into `gond-remote`, the terminal host for WSL and SSH
machines). Nothing is read from disk at run time, and nothing is downloaded: a fix to a file
ships in the next app update, signed like the rest of it.

## When an agent stops working

Its CLI changed. Find what changed (its changelog, its install script, where it now writes its
files, a prompt's new words), change the agent's file, then:

```sh
cd src-tauri && cargo test --lib spec    # every file reads, its patterns match its examples
```

and try it in Gond Code Dev (`bun run tauri dev`). Most fixes are a line in one file. When the
file can't say it (a new kind of store, a title in a new format), the code grows a reader and the
file names it.

## Adding an agent

1. `agents/<id>.toml`, from the nearest existing agent's.
2. Its line in `FILES` in `src-tauri/src/agents/spec.rs` (a test checks the two match).
3. Its icon in `src/components/brand/`.
4. Its rows in `docs/HARNESSES.md`.

## The keys

Paths use `/` and start from `~` (your user folder), `%LOCALAPPDATA%`, `%APPDATA%`, or
`$VAR` (an environment variable the agent itself reads; unset, the next path in the list is
used). Every key but `id`, `name`, `command`, `icon` and `changelog` is optional. An unknown key
is an error, so a typo doesn't pass silently.

```toml
id = "kimi"                 # Gond's id for it; the file's name
name = "Kimi Code"          # as shown
command = "kimi"            # what you type to start it
icon = "kimi"               # its brand icon
changelog = "https://…"     # linked from its card
usage = "kimi"              # its plan usage reader (below); none: no card in Settings → Usage
```

### `[install]`: the ways to install it, best first

| Key | |
|---|---|
| `winget` | winget package id. Tried first. |
| `script` | The vendor's PowerShell script (`irm <url> \| iex`). After winget. |
| `script_args` | Arguments for the script (`-NonInteractive`); it then runs as a script block. |
| `script_bin` | Where the script puts the program. Gond adds it to the user PATH afterwards. |
| `script_first` | `true`: the script before winget (when winget's download is out of reach). |
| `npm` | Its npm package, installed with `bun add -g`. Last. |
| `npm_needs_node` | `true`: the package runs on Node, so a Bun install brings Node too. |
| `requires` | Programs it needs whichever way it's installed: `"git"`, `"node"`. |
| `platform_packages` | The per-platform npm packages its package pulls in (`"devin-win32-x64"`), uninstalled with it. |

### `[update]`

| Key | |
|---|---|
| `self` | Its own update command's arguments (`["update"]`), for a script install. Without it, an update runs the script again. A winget or Bun install updates through winget or Bun. |
| `latest` | Where its latest version is published, checked every 6 hours while it's installed: `{ npm = "<package>" }`, `{ github = "<owner/repo>" }` (latest release), `{ script = "<url>" }` (an install script that sets `$version = '…'`), or `{ text = "<url>" }` (a file holding only the version). None: no update check. |

### `[detect]`: finding it

| Key | |
|---|---|
| `dirs` | Folders its installers put it in, looked in before PATH has them. |
| `script_install` | Pieces of a program's path (lowercase, `/`) that mean its script installed it: `"/.kimi-code/"`. |
| `markers` | For an agent that's a runtime running a script (Node running `@minimax-ai/code`): folder names in the runtime's path or arguments that mean it's this agent. |
| `version_file` | `{ path = "…", key = "…" }`: a JSON file its install writes its version in, from the program's folder. Read instead of running `--version`. |
| `cwd_from_child` | `true`: its program is a launcher, so the folder it works in is its first child's. |

### `[uninstall]`: what Uninstall removes

Winget and Bun installs are removed through winget and Bun. These are its script install's files.

| Key | |
|---|---|
| `program` | Its program files. The first one is the program: it marks a script install. |
| `path` | PATH entries its script added. |
| `shared_path` | A PATH entry it shares with other tools (`~/.local/bin`): removed only once nothing else is in it. |
| `settings` | Its settings, sign-ins and history, removed only with "Remove everything". |
| `background` | A program it leaves running (`cline.exe`): stopped before an update or uninstall. |

### `[resume]`: picking its conversation back up after a restart

| Key | |
|---|---|
| `command` | The resume command; `{id}` is the conversation: `"kimi --session {id}"`. |
| `flags` | The flags that name a conversation on its command line: `["--session", "-S"]`. |
| `word` | Or the word before it (Codex's `codex resume <id>`). |
| `home` | Where its store is: the first of these that's set or exists. |
| `store` | Where it keeps its conversations, under `home`. One of: |

- `{ kind = "sqlite", file = "…", query = "…" }`: a SQLite file, and a query giving each
  conversation's id, folder and last change (seconds, milliseconds or a date), in that order.
- `{ kind = "jsonl-head", dir = "…", type = "…", id = "…", cwd = "…" }`: JSONL files a folder
  or two down `dir`, whose first line has `"type"` equal to `type`, and the conversation's id
  and folder under the keys `id` and `cwd`.
- `{ kind = "reader", name = "…" }`: a store like no other, read by its own code in
  `src-tauri/src/agents/resume.rs`: `claude`, `codex`, `copilot`, `cursor`, `grok`, `agy`,
  `kimi`.

### `[status]`: Working, Needs you, Idle

| Key | |
|---|---|
| `title` | A reader for the terminal title it sets, when its title says what it's doing (`src/features/workspace/status.ts`): `claude`, `codex`, `minimax`. |
| `prompts` | Its prompts' own words, as regular expressions (any case, a line at a time). On screen, they mean it's waiting on you. Words many agents share (`[y/n]`, `❯ 1.`, "Esc to cancel") are already known. Use single quotes, so `\` stays as written. |
| `examples` | Lines its prompts show, copied from the screen. The tests check each matches one of `prompts`, so a pattern that stops matching is caught. |
| `scan` | Where it asks: `"bottom"` (the last rows and the cursor's) or `"screen"` (anywhere, the default). |
| `paste` | The key it pastes an image on: `"ctrl+v"` (the default) or `"alt+v"`. |

An agent that reports its own state on OSC 7501 (MiniMax Code, Devin) needs nothing here for it.

### Readers

`usage`, a `reader` store and a status `title` name code written for one agent's format. The
names the code has are in `spec.rs` (`USAGE_READERS`, `RESUME_READERS`, `TITLE_READERS`), and
the tests reject any other.
