<h1 align="center">✻ notes</h1>

<p align="center">A tidy notebook for your terminal.<br>Fast, keyboard-driven, and one small binary.</p>

```
╭ ✻ notes v0.1.0 ─────────────────────────────────────────────────────╮
│ Welcome back · 3 notes · 1 pinned · ~/.local/share/notes/notes.json │
╰─────────────────────────────────────────────────────────────────────╯
╭ Notes ─────────────────╮╭ Preview ──────────────────────────────────╮
│▌★ Groceries            ││ Groceries                                 │
│   Nov 07, 13:12 #home  ││ #4 · created Nov 07 · updated Nov 07      │
│   Project plan         ││ #home                                     │
│   Nov 07, 13:12 #work  ││                                           │
│   Better idea          ││ - milk                                    │
│   Nov 07, 13:12 #fun   ││ - eggs                                    │
╰────────────────────────╯╰───────────────────────────────────────────╯
 ↑↓ move  n new  e edit  r rename  t tags  p pin  d delete  / search
```

## Install

macOS or Linux. No Rust knowledge needed; the installer sets up everything it needs.

```sh
curl -fsSL https://github.com/siddharthvkutty/cli-notes/releases/latest/download/install.sh | bash
```

Open a new terminal and run `notes`. To uninstall, run the same command again.

## Use

Type `notes` to open the app. Press `?` inside for every key.

Or stay in the shell:

```sh
notes add Groceries -t home        # body opens in the built-in editor
notes ls                           # list notes
notes show groc                    # print one (id, title, or part of a title)
notes rename groc "Shopping list"
notes rm 3
```

Pipe anything in: `git log --oneline -5 | notes add "Recent commits"`

Run `notes -h` for all commands.

## Features

- Editable titles and bodies, with a built-in editor (or bring your own via `NOTES_EDITOR`)
- Tags, pinning, and live search across titles, text and tags
- Plain JSON storage, saved safely and instantly
- Asks before it deletes anything

More detail in [DOCS.md](DOCS.md).
