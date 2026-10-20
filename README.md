# notes

A small CLI notes manager written in Rust. Notes are stored persistently as
JSON in `~/.local/share/notes/notes.json` (respects `XDG_DATA_HOME`).

## Build & install

```sh
./install.sh              # builds the release binary, installs to ~/.local/bin, adds it to PATH
```

The script is idempotent (safe to re-run) and supports:

```sh
./install.sh --prefix DIR   # install somewhere else
./install.sh --no-path      # do not edit shell rc files
./install.sh --uninstall    # remove the binary and the PATH block it added
```

After installing, open a new terminal (or `source ~/.bashrc`) and run `notes`.

## Usage

```
notes                 # list notes (default command)
notes list --full     # list with full content instead of preview
notes add "title" body text here     # create a note
printf 'multi\nline' | notes add "title"   # content from stdin
notes show 3          # print note #3 in full
notes delete 3        # delete (asks for confirmation)
notes delete 3 4 --yes            # delete several, no prompt
notes search keyword  # search titles and content (case-insensitive)
notes --help          # full command reference
```

## Storage

- File: `~/.local/share/notes/notes.json`
- Override location with the `NOTES_DIR` (directory) or `NOTES_FILE` (full path) env vars.
- Writes are atomic (temp file + rename), so the file is never left half-written.
