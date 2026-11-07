# notes — documentation

A terminal notebook written in Rust. Run `notes` for a full-screen interactive view, or use short subcommands from the shell.

## Install

No Rust knowledge needed. Paste this into a terminal (macOS or Linux):

```sh
curl -fsSL https://github.com/siddharthvkutty/cli-notes/releases/latest/download/install.sh | bash
```

The installer:

1. Checks for `git` and a C compiler, and offers to install Rust (via the official rustup installer) if it's missing.
2. Clones the repo into `~/.cli-notes` and builds it (about a minute the first time).
3. Links the binary into `~/.cli-notes/bin` and adds that folder to your PATH in `~/.zshrc`, `~/.bashrc` (`~/.bash_profile` on macOS) or fish's `config.fish`, depending on your shell.
4. Tells you to open a new terminal, then `notes` just works.

**Uninstall:** run the same installer again. When it finds `~/.cli-notes` it asks whether to uninstall, removes the folder and the PATH line, then separately asks whether to delete your saved notes (default: keep them).

Overrides: `NOTES_REPO` (git URL to clone, for a fork or a different repo address) and `NOTES_HOME` (install folder).

Developers can skip all that: `cargo build --release` gives `target/release/notes`.

**Publishing the installer:** `release/` is git-ignored on purpose. `install.sh` is attached to a GitHub Release as an asset named exactly `install.sh`, which is what the `latest/download` link above serves. The release must be published (not a draft or pre-release). The installer itself clones the repo and builds from source, so you only need to re-upload it when the script changes, not when the app does.


## Interactive view

Run `notes` with no arguments.

```
╭ ✻ notes v0.1.0 ───────────────────────────────╮
│ Welcome back · 12 notes · 2 pinned · ~/.local… │
╰────────────────────────────────────────────────╯
╭ Notes ───────────╮╭ Preview ───────────────────╮
│▌★ Groceries      ││ Groceries                  │
│   Nov 07  #home  ││ #4 · created … · updated … │
│   Project plan   ││ - milk                     │
╰──────────────────╯╰────────────────────────────╯
 ↑↓ move  n new  e edit  r rename  t tags  ...
```

Left: your notes (pinned first, then most recently edited). Right: the selected note. Press `?` any time for the key list.

| Key | Action |
|---|---|
| `↑` `↓` / `j` `k` | Move selection |
| `g` / `G` | First / last note |
| `PgUp` `PgDn` | Scroll the preview |
| `n` | New note: type a title, press Enter, then write the body in the editor |
| `e` / `Enter` | Edit the body in the editor |
| `r` | Rename the title (inline, pre-filled) |
| `t` | Edit tags (comma or space separated) |
| `p` | Pin / unpin |
| `d` | Delete (asks `y` to confirm) |
| `/` | Search titles, bodies and tags, filtering live |
| `Esc` | Clear the search filter |
| `q` / `Ctrl-C` | Quit |

Title, tag and search prompts support `←` `→` `Home` `End` `Delete`, `Ctrl-A` / `Ctrl-E` (start / end), `Ctrl-U` (clear before cursor) and `Ctrl-W` (delete word).

Markdown lines starting with `#` are shown bold in the preview. Changes are saved to disk immediately.

## The editor

Writing or editing a body opens a built-in editor, with nano-style shortcuts shown along the bottom. Just type.

| Key | Action |
|---|---|
| `Ctrl-S` | Save and close |
| `Ctrl-X` / `Esc` | Close. If you changed anything it asks: `y` save, `n` discard, any other key keeps editing |
| Arrows, `Home` / `End`, `PgUp` / `PgDn` | Move (`Ctrl-A` / `Ctrl-E` also jump to line start / end) |
| `Enter`, `Backspace`, `Delete`, `Tab` | As you'd expect (`Tab` inserts 4 spaces) |

Long lines wrap at word boundaries, and a word too long for the screen is split. There is no undo yet.

**Prefer vim, nano, VS Code…?** Set `NOTES_EDITOR` and it is used instead, everywhere:

```sh
export NOTES_EDITOR=nvim          # or: nano, "code -w", "hx" …
```

## Commands

`notes -h` or `notes --help` prints the same overview; `notes <command> --help` gives details.

| Command | Alias | What it does |
|---|---|---|
| `notes` | | Open the interactive view |
| `notes add <TITLE>... [-b BODY] [-t TAGS]` | `a` | Create a note. Without `-b` it reads stdin if piped, otherwise opens the editor |
| `notes list [-t TAG] [-s TEXT]` | `ls` | List notes, optionally filtered by tag and/or search text |
| `notes show <NOTE>` | `s` | Print a note |
| `notes edit <NOTE>` | `e` | Edit the body in the editor |
| `notes rename <NOTE> <TITLE>...` | `mv` | Change the title |
| `notes tag <NOTE> <TAGS>... [-r]` | | Add tags, or remove them with `-r` |
| `notes pin <NOTE>` | | Toggle pinned |
| `notes delete <NOTE> [-y]` | `rm` | Delete, asking first unless `-y` |
| `notes path` | | Print the storage file location |

**`<NOTE>`** is an id (`3`), an exact title, or a unique piece of a title (`groc`). A purely numeric argument is always treated as an id. If a fragment matches several notes you get an error asking for the id.

Titles don't need quotes: `notes add Buy milk -t home` works. Bodies starting with `-` are fine: `notes add List -b "- one"`.

### Examples

```sh
notes add Groceries -t home            # opens the editor for the body
notes add Idea -b "ship it"
git log --oneline -5 | notes add "Recent commits" -t dev
notes ls -t home
notes ls -s milk
notes rename groc "Shopping list"
notes tag 3 work urgent
notes tag 3 urgent -r
notes pin 3
notes rm 3 -y
```

## Features at a glance

- Editable titles (CLI `rename`, TUI `r`) and editable bodies in a built-in editor
- Tags, pinning, live search across title, body and tags
- Created / updated timestamps; the most recently edited note rises to the top
- Pipe text straight into a note
- Delete always asks first (use `-y` to skip, required when not on a terminal)
- Colours only when output is a terminal, so `notes ls | grep …` stays clean

## Storage

All notes live in one human-readable JSON file:

1. `$NOTES_FILE` if set
2. else `$XDG_DATA_HOME/notes/notes.json`
3. else `~/.local/share/notes/notes.json`

Safety:
- Saves write a temp file then rename it over the real one, so a crash can't leave a half-written file.
- If the file can't be parsed, `notes` stops with an error rather than starting empty and overwriting it.
- Ids are never reused after a delete.

Back up or sync by copying that one file.

## Layout of the code

| File | Purpose |
|---|---|
| `src/main.rs` | CLI definition (clap) and subcommands |
| `src/editor.rs` | Built-in text editor (and the optional external-editor hook) |
| `src/store.rs` | `Note` / `Store`, JSON load/save, search and sort, lookup by id or title, unit test |
| `src/tui.rs` | The interactive view (ratatui) |
| `release/install.sh` | Installer / uninstaller (git-ignored, published as a release asset, see Install) |

Run the test with `cargo test`.

## Ideas not built yet

Shell completions (`clap_complete`), JSON export, a trash/undo for deletes, notebooks/folders, and undo in the editor.
