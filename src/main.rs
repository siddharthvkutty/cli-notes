mod editor;
mod store;
mod tui;

use clap::{Parser, Subcommand};
use std::io::{self, IsTerminal, Read, Write};
use store::{fmt_time, parse_tags, Store};

#[derive(Parser)]
#[command(
    name = "notes",
    version,
    about = "A tidy terminal notebook. Run with no arguments to open the interactive view.",
    after_help = "\
NOTE can be an id (`3`), an exact title, or a unique piece of a title.

Examples:
  notes                              open the interactive view
  notes add Groceries -t home        new note, body written in the editor
  notes add Idea -b \"ship it\"        new note with an inline body
  echo \"draft\" | notes add Piped     body from stdin
  notes ls -s milk                   search titles, bodies and tags
  notes rename groc \"Shopping list\"  retitle a note
  notes tag 3 work urgent            add tags (-r to remove)

Data lives in ~/.local/share/notes/notes.json (override with $NOTES_FILE)."
)]
struct Cli {
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Create a note; opens the editor for the body unless -b or stdin is given
    #[command(visible_alias = "a")]
    Add {
        /// Note title (several words are fine, no quotes needed)
        #[arg(required = true)]
        title: Vec<String>,
        /// Body text, skips the editor
        #[arg(short, long, allow_hyphen_values = true)]
        body: Option<String>,
        /// Tag(s): -t work -t urgent, or -t work,urgent
        #[arg(short, long, value_delimiter = ',')]
        tag: Vec<String>,
    },
    /// List notes, pinned first then most recently edited
    #[command(visible_alias = "ls")]
    List {
        /// Only notes with this tag
        #[arg(short, long)]
        tag: Option<String>,
        /// Only notes containing this text (title, body or tags)
        #[arg(short, long)]
        search: Option<String>,
    },
    /// Print a note
    #[command(visible_alias = "s")]
    Show { note: String },
    /// Edit a note's body in the built-in editor
    #[command(visible_alias = "e")]
    Edit { note: String },
    /// Change a note's title
    #[command(visible_alias = "mv")]
    Rename {
        note: String,
        #[arg(required = true)]
        title: Vec<String>,
    },
    /// Add tags to a note (or remove them with -r)
    Tag {
        note: String,
        #[arg(required = true)]
        tags: Vec<String>,
        /// Remove the tags instead of adding
        #[arg(short, long)]
        remove: bool,
    },
    /// Pin or unpin a note
    Pin { note: String },
    /// Delete a note
    #[command(visible_alias = "rm")]
    Delete {
        note: String,
        /// Don't ask for confirmation
        #[arg(short, long)]
        yes: bool,
    },
    /// Print where notes are stored
    Path,
}

fn paint(on: bool, code: &str, s: &str) -> String {
    if on { format!("\x1b[{code}m{s}\x1b[0m") } else { s.to_string() }
}

fn main() {
    if let Err(e) = run() {
        eprintln!("notes: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let mut s = Store::load()?;
    let tty = io::stdout().is_terminal();
    let Some(cmd) = cli.cmd else { return Ok(tui::run(s)?) };

    match cmd {
        Cmd::Add { title, body, tag } => {
            let body = match body {
                Some(b) => b,
                None if !io::stdin().is_terminal() => {
                    let mut b = String::new();
                    io::stdin().read_to_string(&mut b)?;
                    b.trim_end().to_string()
                }
                None => match editor::edit_cli(&title.join(" "), "")? {
                    Some(b) => b,
                    None => return Ok(println!("Cancelled, no note added")),
                },
            };
            let id = s.add(title.join(" "), body, parse_tags(&tag.join(",")));
            s.save()?;
            println!("Added note {id}");
        }
        Cmd::List { tag, search } => {
            let ids = s.view(search.as_deref().unwrap_or(""), tag.as_deref());
            if ids.is_empty() {
                println!("No notes. Try `notes add \"Title\"`, or run `notes` for the interactive view.");
            }
            let w = ids.iter().filter_map(|&i| s.get(i)).map(|n| n.title.chars().count()).max().unwrap_or(0).min(40);
            for n in ids.iter().filter_map(|&i| s.get(i)) {
                let tags: String = n.tags.iter().map(|t| format!(" #{t}")).collect();
                println!(
                    "{} {} {}  {}{}",
                    paint(tty, "2", &format!("{:>3}", n.id)),
                    paint(tty, "38;5;173", if n.pinned { "★" } else { " " }),
                    paint(tty, "1", &format!("{:<w$}", n.title)),
                    paint(tty, "2", &fmt_time(n.updated)),
                    paint(tty, "34", &tags),
                );
            }
        }
        Cmd::Show { note } => {
            let n = s.get(s.resolve(&note)?).unwrap();
            let tags: String = n.tags.iter().map(|t| format!(" · #{t}")).collect();
            println!("{}{}", paint(tty, "1;38;5;173", &n.title), if n.pinned { " ★" } else { "" });
            println!("{}", paint(tty, "2", &format!("#{} · updated {}{tags}", n.id, fmt_time(n.updated))));
            if !n.body.is_empty() {
                println!("\n{}", n.body);
            }
        }
        Cmd::Edit { note } => {
            let id = s.resolve(&note)?;
            let old = s.get(id).unwrap().body.clone();
            let title = s.get(id).unwrap().title.clone();
            match editor::edit_cli(&title, &old)? {
                Some(new) if new != old => {
                    s.update(id, |n| n.body = new);
                    s.save()?;
                    println!("Saved note {id}");
                }
                _ => println!("No changes"),
            }
        }
        Cmd::Rename { note, title } => {
            let id = s.resolve(&note)?;
            s.update(id, |n| n.title = title.join(" "));
            s.save()?;
            println!("Renamed note {id}");
        }
        Cmd::Tag { note, tags, remove } => {
            let id = s.resolve(&note)?;
            let tags = parse_tags(&tags.join(","));
            s.update(id, |n| {
                n.tags.retain(|t| !tags.contains(t));
                if !remove {
                    n.tags.extend(tags);
                }
            });
            s.save()?;
            println!("Tags now: {}", s.get(id).unwrap().tags.join(", "));
        }
        Cmd::Pin { note } => {
            let n = s.get_mut(s.resolve(&note)?).unwrap();
            n.pinned = !n.pinned;
            println!("{} note {}", if n.pinned { "Pinned" } else { "Unpinned" }, n.id);
            s.save()?;
        }
        Cmd::Delete { note, yes } => {
            let id = s.resolve(&note)?;
            let title = s.get(id).unwrap().title.clone();
            if !yes {
                if !io::stdin().is_terminal() {
                    return Err("refusing to delete without confirmation; pass -y".into());
                }
                print!("Delete “{title}”? [y/N] ");
                io::stdout().flush()?;
                let mut a = String::new();
                io::stdin().read_line(&mut a)?;
                if !a.trim().eq_ignore_ascii_case("y") {
                    return Ok(println!("Kept"));
                }
            }
            s.remove(id);
            s.save()?;
            println!("Deleted “{title}”");
        }
        Cmd::Path => println!("{}", s.path.display()),
    }
    Ok(())
}
