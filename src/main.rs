use std::fs;
use std::io::{self, IsTerminal, Read, Write};
use std::path::PathBuf;
use std::process::{Command as ProcessCommand, ExitCode};

use chrono::{Local, NaiveDateTime};
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};

const ENV_DIR: &str = "NOTES_DIR";
const ENV_FILE: &str = "NOTES_FILE";

#[derive(Parser)]
#[command(
    name = "notes",
    version,
    about = "A simple terminal notes manager",
    long_about = "A simple terminal notes manager with persistent storage.\n\
                  Notes are stored as JSON in ~/.local/share/notes/notes.json\n\
                  (override with NOTES_DIR or NOTES_FILE)."
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// List all notes (default when no subcommand is given)
    List {
        /// Show the full content of every note
        #[arg(short, long)]
        full: bool,
    },
    /// Create a new note
    Add {
        /// Title of the note
        title: String,
        /// Note content. If omitted, content is read from stdin (or $EDITOR when run interactively).
        #[arg(trailing_var_arg = true)]
        content: Vec<String>,
    },
    /// Show a single note in full
    Show {
        /// Note id (see `notes list`)
        id: u64,
    },
    /// Delete one or more notes
    Delete {
        /// Note id(s) to delete
        ids: Vec<u64>,
        /// Do not ask for confirmation
        #[arg(short, long)]
        yes: bool,
    },
    /// Search notes by title or content
    Search {
        /// Text to look for
        query: String,
        /// Search is case-sensitive
        #[arg(short, long)]
        case_sensitive: bool,
    },
}

#[derive(Serialize, Deserialize, Clone)]
struct Note {
    id: u64,
    title: String,
    content: String,
    created_at: String,
}

#[derive(Serialize, Deserialize, Default)]
struct Store {
    next_id: u64,
    notes: Vec<Note>,
}

fn storage_path() -> PathBuf {
    if let Ok(p) = std::env::var(ENV_FILE) {
        return PathBuf::from(p);
    }
    if let Ok(dir) = std::env::var(ENV_DIR) {
        return PathBuf::from(dir).join("notes.json");
    }
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| {
            let home = std::env::var_os("HOME").unwrap_or_else(|| ".".into());
            PathBuf::from(home).join(".local/share")
        });
    base.join("notes").join("notes.json")
}

fn load() -> Result<Store, String> {
    let path = storage_path();
    if !path.exists() {
        return Ok(Store { next_id: 1, notes: Vec::new() });
    }
    let data = fs::read_to_string(&path)
        .map_err(|e| format!("failed to read {}: {e}", path.display()))?;
    if data.trim().is_empty() {
        return Ok(Store { next_id: 1, notes: Vec::new() });
    }
    serde_json::from_str(&data)
        .map_err(|e| format!("corrupt storage file {}: {e}", path.display()))
}

fn save(store: &Store) -> Result<(), String> {
    let path = storage_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create {}: {e}", parent.display()))?;
    }
    let json = serde_json::to_string_pretty(store)
        .map_err(|e| format!("failed to serialize notes: {e}"))?;
    // Write to a temp file then rename for atomic persistence.
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, json).map_err(|e| format!("failed to write {}: {e}", tmp.display()))?;
    fs::rename(&tmp, &path).map_err(|e| format!("failed to save {}: {e}", path.display()))?;
    Ok(())
}

fn now() -> String {
    Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

fn parse_created(s: &str) -> Option<NaiveDateTime> {
    NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S").ok()
}

fn floor_char_boundary(s: &str, mut i: usize) -> usize {
    if i >= s.len() {
        return s.len();
    }
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

fn truncate_chars(s: &str, max: usize) -> String {
    let mut out = String::new();
    let mut count = 0;
    for ch in s.chars() {
        if count >= max {
            out.push('…');
            break;
        }
        if ch == '\n' {
            out.push(' ');
        } else {
            out.push(ch);
        }
        count += 1;
    }
    out
}

fn find_index(store: &Store, id: u64) -> Option<usize> {
    store.notes.iter().position(|n| n.id == id)
}

fn print_list(store: &Store, full: bool) -> Result<(), String> {
    if store.notes.is_empty() {
        println!("No notes yet. Create one with: notes add \"my title\"");
        return Ok(());
    }
    let mut notes: Vec<&Note> = store.notes.iter().collect();
    notes.sort_by(|a, b| {
        parse_created(&b.created_at)
            .unwrap_or_default()
            .cmp(&parse_created(&a.created_at).unwrap_or_default())
            .then(b.id.cmp(&a.id))
    });
    for n in notes {
        println!(
            "{}  {}  {}",
            n.id,
            n.created_at,
            n.title
        );
        if full {
            for line in n.content.lines() {
                println!("      {line}");
            }
        } else {
            let preview = truncate_chars(&n.content, 70);
            if !preview.is_empty() {
                println!("      {preview}");
            }
        }
        println!();
    }
    Ok(())
}

fn read_editor(initial: &str) -> Result<String, String> {
    let editor = std::env::var("EDITOR")
        .or_else(|_| std::env::var("VISUAL"))
        .unwrap_or_else(|_| "vi".to_string());
    let tmp = std::env::temp_dir().join(format!("notes-{}.txt", std::process::id()));
    fs::write(&tmp, initial).map_err(|e| format!("failed to create temp file: {e}"))?;
    let status = ProcessCommand::new(&editor)
        .arg(&tmp)
        .status()
        .map_err(|e| format!("failed to launch editor '{editor}': {e}"))?;
    let body = fs::read_to_string(&tmp);
    let _ = fs::remove_file(&tmp);
    if !status.success() {
        return Err(format!("editor '{editor}' exited with {status}"));
    }
    body.map_err(|e| format!("failed to read edited content: {e}"))
}

fn read_content(args: &[String]) -> Result<String, String> {
    if !args.is_empty() {
        return Ok(args.join(" "));
    }
    if !io::stdin().is_terminal() {
        let mut buf = String::new();
        io::stdin()
            .read_to_string(&mut buf)
            .map_err(|e| format!("failed to read stdin: {e}"))?;
        return Ok(buf);
    }
    // Interactive with no content given: open the editor, or fall back to
    // reading lines until EOF (Ctrl-D).
    if std::env::var("EDITOR").is_ok() || std::env::var("VISUAL").is_ok() {
        return read_editor("");
    }
    eprintln!("Enter note content, finish with Ctrl-D:");
    let mut buf = String::new();
    io::stdin()
        .read_to_string(&mut buf)
        .map_err(|e| format!("failed to read stdin: {e}"))?;
    Ok(buf)
}

fn confirm(prompt: &str) -> Result<bool, String> {
    print!("{prompt} [y/N] ");
    io::stdout().flush().ok();
    let mut line = String::new();
    io::stdin()
        .read_line(&mut line)
        .map_err(|e| format!("failed to read input: {e}"))?;
    let answer = line.trim().to_ascii_lowercase();
    Ok(answer == "y" || answer == "yes")
}

fn run(cli: Cli) -> Result<(), String> {
    match cli.command.unwrap_or(Command::List { full: false }) {
        Command::List { full } => {
            let store = load()?;
            print_list(&store, full)
        }
        Command::Add { title, content } => {
            let title = title.trim();
            if title.is_empty() {
                return Err("title must not be empty".into());
            }
            let content = read_content(&content)?;
            let mut store = load()?;
            let note = Note {
                id: store.next_id,
                title: title.to_string(),
                content: content.trim_end().to_string(),
                created_at: now(),
            };
            store.next_id += 1;
            store.notes.push(note.clone());
            save(&store)?;
            println!("Added note #{}: {}", note.id, note.title);
            Ok(())
        }
        Command::Show { id } => {
            let store = load()?;
            let idx = find_index(&store, id).ok_or_else(|| format!("no note with id {id}"))?;
            let n = &store.notes[idx];
            println!("{}", n.title);
            println!("id: {}  created: {}", n.id, n.created_at);
            println!("{}", "-".repeat(40));
            println!("{}", n.content);
            Ok(())
        }
        Command::Delete { ids, yes } => {
            if ids.is_empty() {
                return Err("specify at least one note id to delete".into());
            }
            let mut store = load()?;
            for id in &ids {
                if find_index(&store, *id).is_none() {
                    return Err(format!("no note with id {id}"));
                }
            }
            if !yes {
                let prompt = if ids.len() == 1 {
                    let n = &store.notes[find_index(&store, ids[0]).unwrap()];
                    format!("Delete note #{} '{}'?", n.id, n.title)
                } else {
                    format!("Delete {} notes?", ids.len())
                };
                if !confirm(&prompt)? {
                    println!("Aborted.");
                    return Ok(());
                }
            }
            store.notes.retain(|n| !ids.contains(&n.id));
            save(&store)?;
            println!(
                "Deleted {} note{}.",
                ids.len(),
                if ids.len() == 1 { "" } else { "s" }
            );
            Ok(())
        }
        Command::Search { query, case_sensitive } => {
            let store = load()?;
            let q = if case_sensitive { query.clone() } else { query.to_lowercase() };
            let hits: Vec<&Note> = store
                .notes
                .iter()
                .filter(|n| {
                    let hay = if case_sensitive {
                        format!("{} {}", n.title, n.content)
                    } else {
                        format!("{} {}", n.title, n.content).to_lowercase()
                    };
                    hay.contains(&q)
                })
                .collect();
            if hits.is_empty() {
                println!("No notes matched {:?}.", query);
                return Ok(());
            }
            for n in hits {
                println!("{}  {}  {}", n.id, n.created_at, n.title);
                let haystack = if case_sensitive {
                    n.content.clone()
                } else {
                    n.content.to_lowercase()
                };
                if let Some(pos) = haystack.find(&q) {
                    let start = floor_char_boundary(&n.content, pos.saturating_sub(30));
                    let snippet = truncate_chars(&n.content[start..], 70);
                    println!("      ...{snippet}");
                }
                println!();
            }
            Ok(())
        }
    }
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
