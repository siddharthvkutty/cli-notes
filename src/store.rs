use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use std::{cmp::Reverse, env, fs, io, path::PathBuf};

#[derive(Serialize, Deserialize, Clone)]
pub struct Note {
    pub id: u32,
    pub title: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub pinned: bool,
    pub created: i64,
    pub updated: i64,
}

#[derive(Serialize, Deserialize, Default)]
pub struct Store {
    next_id: u32,
    pub notes: Vec<Note>,
    #[serde(skip)]
    pub path: PathBuf,
}

pub fn now() -> i64 {
    Local::now().timestamp()
}

pub fn fmt_time(ts: i64) -> String {
    DateTime::from_timestamp(ts, 0)
        .map(|d| d.with_timezone(&Local).format("%b %d, %H:%M").to_string())
        .unwrap_or_default()
}

/// "work, #Ideas  work" -> ["work", "ideas"]
pub fn parse_tags(s: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for t in s.split(|c: char| c == ',' || c.is_whitespace()) {
        let t = t.trim_start_matches('#').to_lowercase();
        if !t.is_empty() && !out.contains(&t) {
            out.push(t);
        }
    }
    out
}

impl Store {
    /// `$NOTES_FILE`, else `$XDG_DATA_HOME/notes/notes.json`, else `~/.local/share/notes/notes.json`.
    pub fn load() -> io::Result<Self> {
        let path = match env::var_os("NOTES_FILE") {
            Some(p) => PathBuf::from(p),
            None => env::var_os("XDG_DATA_HOME")
                .map(PathBuf::from)
                .or_else(|| env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
                .ok_or_else(|| io::Error::other("cannot locate data dir: set NOTES_FILE"))?
                .join("notes/notes.json"),
        };
        Self::load_from(path)
    }

    pub fn load_from(path: PathBuf) -> io::Result<Self> {
        let mut s = match fs::read_to_string(&path) {
            Ok(txt) => serde_json::from_str::<Store>(&txt).map_err(|e| {
                // Refuse to continue: saving over a file we couldn't parse would destroy it.
                io::Error::new(io::ErrorKind::InvalidData, format!("{} is not valid: {e}", path.display()))
            })?,
            Err(e) if e.kind() == io::ErrorKind::NotFound => Store::default(),
            Err(e) => return Err(e),
        };
        s.path = path;
        let max = s.notes.iter().map(|n| n.id).max().unwrap_or(0);
        s.next_id = s.next_id.max(max + 1);
        Ok(s)
    }

    /// Write to a sibling temp file then rename, so a crash never leaves a half-written file.
    pub fn save(&self) -> io::Result<()> {
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir)?;
        }
        let tmp = self.path.with_extension("json.tmp");
        fs::write(&tmp, serde_json::to_string_pretty(self)?)?;
        fs::rename(tmp, &self.path)
    }

    pub fn add(&mut self, title: String, body: String, tags: Vec<String>) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        let t = now();
        self.notes.push(Note { id, title, body, tags, pinned: false, created: t, updated: t });
        id
    }

    pub fn get(&self, id: u32) -> Option<&Note> {
        self.notes.iter().find(|n| n.id == id)
    }

    pub fn get_mut(&mut self, id: u32) -> Option<&mut Note> {
        self.notes.iter_mut().find(|n| n.id == id)
    }

    /// Apply an edit and bump `updated`.
    pub fn update(&mut self, id: u32, f: impl FnOnce(&mut Note)) {
        if let Some(n) = self.get_mut(id) {
            f(n);
            n.updated = now();
        }
    }

    pub fn remove(&mut self, id: u32) {
        self.notes.retain(|n| n.id != id);
    }

    /// Note ids matching `query` (title/body/tags) and `tag`, pinned first, then newest edit first.
    pub fn view(&self, query: &str, tag: Option<&str>) -> Vec<u32> {
        let q = query.to_lowercase();
        let mut v: Vec<&Note> = self
            .notes
            .iter()
            .filter(|n| tag.is_none_or(|t| n.tags.iter().any(|x| x.eq_ignore_ascii_case(t))))
            .filter(|n| {
                q.is_empty()
                    || n.title.to_lowercase().contains(&q)
                    || n.body.to_lowercase().contains(&q)
                    || n.tags.iter().any(|t| t.contains(&q))
            })
            .collect();
        v.sort_by_key(|n| (!n.pinned, Reverse(n.updated), Reverse(n.id)));
        v.iter().map(|n| n.id).collect()
    }

    /// CLI note reference: an id, an exact title, or a unique title fragment.
    pub fn resolve(&self, r: &str) -> Result<u32, String> {
        if let Ok(id) = r.parse::<u32>() {
            return self.get(id).map(|n| n.id).ok_or(format!("no note with id {id}"));
        }
        let q = r.to_lowercase();
        let hits: Vec<&Note> = self.notes.iter().filter(|n| n.title.to_lowercase().contains(&q)).collect();
        if let Some(n) = hits.iter().find(|n| n.title.to_lowercase() == q) {
            return Ok(n.id);
        }
        match hits.as_slice() {
            [n] => Ok(n.id),
            [] => Err(format!("no note matching “{r}”")),
            _ => Err(format!("“{r}” matches {} notes, use an id (see `notes list`)", hits.len())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_behaves() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("n/notes.json");
        let mut s = Store::load_from(path.clone()).unwrap();
        let a = s.add("Groceries".into(), "milk, eggs".into(), parse_tags("Home, #errands home"));
        let b = s.add("Ideas".into(), "build a notes app".into(), vec![]);
        assert_eq!(s.notes[0].tags, ["home", "errands"]);

        // search hits body and tags; pinned floats to the top
        assert_eq!(s.view("eggs", None), [a]);
        assert_eq!(s.view("", Some("errands")), [a]);
        s.get_mut(a).unwrap().pinned = true;
        assert_eq!(s.view("", None), [a, b]);

        // resolve by id, exact title, fragment; ambiguous/missing fail
        assert_eq!(s.resolve("2"), Ok(b));
        assert_eq!(s.resolve("groc"), Ok(a));
        assert!(s.resolve("9").is_err() && s.resolve("zzz").is_err());

        // roundtrip; removed ids are never reused
        s.remove(b);
        s.save().unwrap();
        let mut s2 = Store::load_from(path.clone()).unwrap();
        assert_eq!(s2.notes.len(), 1);
        assert_eq!(s2.add("x".into(), "".into(), vec![]), 3);

        // corrupt file is an error, not a silent reset
        fs::write(&path, "{nope").unwrap();
        assert!(Store::load_from(path).is_err());
    }
}
