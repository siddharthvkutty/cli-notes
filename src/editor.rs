//! Built-in text editor for note bodies. Set `NOTES_EDITOR` (e.g. `nvim`, `nano`, `code -w`)
//! to use an external editor instead.
use crate::tui::{hints, ACCENT, DIM};
use ratatui::{
    crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    layout::Constraint::{Length, Min},
    layout::Layout,
    style::{Style, Stylize},
    text::{Line, Span},
    widgets::{Block, BorderType, Padding, Paragraph},
    DefaultTerminal, Frame,
};
use std::{env, fs, io, io::Write, process::Command};

fn external() -> Option<String> {
    env::var("NOTES_EDITOR").ok().filter(|s| !s.trim().is_empty())
}

/// Edit from a plain shell command. `Ok(None)` means the user discarded their changes.
pub fn edit_cli(title: &str, initial: &str) -> io::Result<Option<String>> {
    if let Some(ed) = external() {
        return run_external(&ed, initial).map(Some);
    }
    let mut term = ratatui::init();
    let res = builtin(&mut term, title, initial);
    ratatui::restore();
    res
}

/// Edit from inside the interactive view.
pub fn edit_tui(term: &mut DefaultTerminal, title: &str, initial: &str) -> io::Result<Option<String>> {
    if let Some(ed) = external() {
        ratatui::restore();
        let res = run_external(&ed, initial);
        *term = ratatui::init();
        return res.map(Some);
    }
    builtin(term, title, initial)
}

fn run_external(ed: &str, initial: &str) -> io::Result<String> {
    let mut f = tempfile::Builder::new().prefix("note-").suffix(".md").tempfile()?;
    f.write_all(initial.as_bytes())?;
    // via sh so values like "code -w" work; the path arrives as $0
    let status = Command::new("sh").arg("-c").arg(format!("{ed} \"$0\"")).arg(f.path()).status()?;
    if !status.success() {
        return Err(io::Error::other(format!("editor “{ed}” exited with an error, nothing saved")));
    }
    Ok(fs::read_to_string(f.path())?.trim_end().to_string())
}

fn builtin(term: &mut DefaultTerminal, title: &str, initial: &str) -> io::Result<Option<String>> {
    let mut ed = Ed::new(initial);
    loop {
        term.draw(|f| ed.draw(f, title))?;
        if let Event::Key(k) = event::read()? {
            if k.kind == KeyEventKind::Press {
                if let Some(save) = ed.key(k) {
                    return Ok(save.then(|| ed.text()));
                }
            }
        }
    }
}

/// Start index (in chars) of every screen row of `line` when word-wrapped to `w` columns.
/// Rows break after the last space that fits; a word longer than a row is split. A row never
/// ends flush against the edge on the last row, so the cursor always has room past the final char.
fn breaks(line: &str, w: usize) -> Vec<usize> {
    let cs: Vec<char> = line.chars().collect();
    let mut out = vec![0];
    let mut s = 0;
    while cs.len() - s >= w {
        s = (s..s + w).rev().find(|&i| cs[i] == ' ').map_or(s + w, |i| i + 1);
        out.push(s);
    }
    out
}

/// Which screen row (index into `breaks`) holds char position `col`.
fn row_of(b: &[usize], col: usize) -> usize {
    b.iter().rposition(|&s| s <= col).unwrap_or(0)
}

/// Last cursor position on screen row `k` of a line with `n` chars.
fn row_end(b: &[usize], k: usize, n: usize) -> usize {
    b.get(k + 1).map_or(n, |&next| next - 1)
}

struct Ed {
    lines: Vec<String>,
    orig: String,
    row: usize,
    col: usize, // in chars
    top: usize, // first visible screen row
    w: usize,   // text width and height, set on every draw
    h: usize,
    confirm: bool, // "save changes?" prompt showing
}

// ponytail: wide chars (CJK, emoji) count as width 1, so the cursor can drift on lines containing
// them; no undo. Upgrade path for both: `tui-textarea`.
impl Ed {
    fn new(initial: &str) -> Self {
        let mut lines: Vec<String> = initial.lines().map(String::from).collect();
        if lines.is_empty() {
            lines.push(String::new());
        }
        Ed { lines, orig: initial.trim_end().into(), row: 0, col: 0, top: 0, w: 80, h: 20, confirm: false }
    }

    fn text(&self) -> String {
        self.lines.join("\n").trim_end().to_string()
    }

    fn len(&self) -> usize {
        self.lines[self.row].chars().count()
    }

    fn at(&self) -> usize {
        self.lines[self.row].char_indices().nth(self.col).map_or(self.lines[self.row].len(), |(i, _)| i)
    }

    fn up(&mut self) {
        let b = breaks(&self.lines[self.row], self.w);
        let k = row_of(&b, self.col);
        let x = self.col - b[k];
        if k > 0 {
            self.col = (b[k - 1] + x).min(row_end(&b, k - 1, self.len()));
        } else if self.row > 0 {
            self.row -= 1;
            let b = breaks(&self.lines[self.row], self.w);
            self.col = (b[b.len() - 1] + x).min(self.len());
        }
    }

    fn down(&mut self) {
        let b = breaks(&self.lines[self.row], self.w);
        let k = row_of(&b, self.col);
        let x = self.col - b[k];
        if k + 1 < b.len() {
            self.col = (b[k + 1] + x).min(row_end(&b, k + 1, self.len()));
        } else if self.row + 1 < self.lines.len() {
            self.row += 1;
            let b = breaks(&self.lines[self.row], self.w);
            self.col = x.min(row_end(&b, 0, self.len()));
        }
    }

    fn exit(&mut self) -> Option<bool> {
        if self.text() == self.orig {
            return Some(false);
        }
        self.confirm = true;
        None
    }

    /// `Some(true)` = save and close, `Some(false)` = close without saving, `None` = keep going.
    fn key(&mut self, k: KeyEvent) -> Option<bool> {
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        if self.confirm {
            match k.code {
                KeyCode::Char('y' | 'Y') => return Some(true),
                KeyCode::Char('n' | 'N') => return Some(false),
                _ => self.confirm = false,
            }
            return None;
        }
        match k.code {
            KeyCode::Char('s') if ctrl => return Some(true),
            KeyCode::Char('x' | 'c') if ctrl => return self.exit(),
            KeyCode::Esc => return self.exit(),
            KeyCode::Up => self.up(),
            KeyCode::Down => self.down(),
            KeyCode::PageUp => (0..self.h).for_each(|_| self.up()),
            KeyCode::PageDown => (0..self.h).for_each(|_| self.down()),
            KeyCode::Left if self.col > 0 => self.col -= 1,
            KeyCode::Left if self.row > 0 => {
                self.row -= 1;
                self.col = self.len();
            }
            KeyCode::Right if self.col < self.len() => self.col += 1,
            KeyCode::Right if self.row + 1 < self.lines.len() => {
                self.row += 1;
                self.col = 0;
            }
            KeyCode::Home => self.col = 0,
            KeyCode::End => self.col = self.len(),
            KeyCode::Char('a') if ctrl => self.col = 0,
            KeyCode::Char('e') if ctrl => self.col = self.len(),
            KeyCode::Enter => {
                let at = self.at();
                let rest = self.lines[self.row].split_off(at);
                self.lines.insert(self.row + 1, rest);
                self.row += 1;
                self.col = 0;
            }
            KeyCode::Backspace if self.col > 0 => {
                self.col -= 1;
                let at = self.at();
                self.lines[self.row].remove(at);
            }
            KeyCode::Backspace if self.row > 0 => {
                let cur = self.lines.remove(self.row);
                self.row -= 1;
                self.col = self.len();
                self.lines[self.row].push_str(&cur);
            }
            KeyCode::Delete if self.col < self.len() => {
                let at = self.at();
                self.lines[self.row].remove(at);
            }
            KeyCode::Delete if self.row + 1 < self.lines.len() => {
                let next = self.lines.remove(self.row + 1);
                self.lines[self.row].push_str(&next);
            }
            KeyCode::Tab => {
                let at = self.at();
                self.lines[self.row].insert_str(at, "    ");
                self.col += 4;
            }
            KeyCode::Char(c) if !ctrl => {
                let at = self.at();
                self.lines[self.row].insert(at, c);
                self.col += 1;
            }
            _ => {}
        }
        None
    }

    fn draw(&mut self, f: &mut Frame, title: &str) {
        let [main, foot] = Layout::vertical([Min(3), Length(1)]).areas(f.area());
        let modified = if self.text() == self.orig { "" } else { " · modified" };
        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Style::new().fg(ACCENT))
            .padding(Padding::horizontal(1))
            .title(Line::from(vec![Span::styled(" ✻ Editing · ", Style::new().fg(ACCENT)), Span::raw(title.to_string()).bold(), Span::raw(" ")]))
            .title_bottom(Line::styled(format!(" Ln {}, Col {}{modified} ", self.row + 1, self.col + 1), Style::new().fg(DIM)).right_aligned());
        let inner = block.inner(main);
        self.w = (inner.width as usize).max(1);
        self.h = (inner.height as usize).max(1);

        let mut rows: Vec<String> = Vec::new();
        let mut y = 0; // screen row of the cursor
        let mut x = 0;
        for (i, l) in self.lines.iter().enumerate() {
            let b = breaks(l, self.w);
            let cs: Vec<char> = l.chars().collect();
            if i == self.row {
                let k = row_of(&b, self.col);
                (y, x) = (rows.len() + k, self.col - b[k]);
            }
            for k in 0..b.len() {
                rows.push(cs[b[k]..b.get(k + 1).copied().unwrap_or(cs.len())].iter().collect());
            }
        }
        if y < self.top {
            self.top = y;
        } else if y >= self.top + self.h {
            self.top = y + 1 - self.h;
        }

        let shown: Vec<Line> = rows.iter().skip(self.top).take(self.h).map(|r| Line::raw(r.as_str())).collect();
        f.render_widget(Paragraph::new(shown).block(block), main);
        f.set_cursor_position((inner.x + x as u16, inner.y + (y - self.top) as u16));

        let footer = if self.confirm {
            Line::from(vec![
                Span::styled(" Save changes? ", Style::new().bold()),
                Span::styled("y", Style::new().fg(ACCENT)),
                Span::styled(" save · ", Style::new().fg(DIM)),
                Span::styled("n", Style::new().fg(ACCENT)),
                Span::styled(" discard · any other key keeps editing", Style::new().fg(DIM)),
            ])
        } else {
            hints(&[("^S", "Save & close"), ("^X", "Exit"), ("←↑↓→", "Move"), ("PgUp/PgDn", "Page"), ("Home/End", "Line start/end")])
        };
        f.render_widget(Paragraph::new(footer), foot);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(e: &mut Ed, code: KeyCode) -> Option<bool> {
        e.key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    #[test]
    fn typing_and_navigation() {
        let mut e = Ed::new("");
        for c in "hello".chars() {
            press(&mut e, KeyCode::Char(c));
        }
        press(&mut e, KeyCode::Enter);
        press(&mut e, KeyCode::Char('x'));
        assert_eq!(e.text(), "hello\nx");
        press(&mut e, KeyCode::Backspace); // x gone
        press(&mut e, KeyCode::Backspace); // joins back onto "hello"
        assert_eq!((e.text().as_str(), e.row, e.col), ("hello", 0, 5));

        // word wrap: breaks after the last space that fits; long words split; cursor always fits
        assert_eq!(breaks("hello big world", 8), [0, 6, 10]); // "hello " / "big " / "world"
        assert_eq!(breaks("abcdefghij", 4), [0, 4, 8]);
        assert_eq!(breaks("", 4), [0]);
        let mut e = Ed::new("hello big world");
        e.w = 8;
        e.col = 12; // in "world", x=2
        press(&mut e, KeyCode::Up);
        assert_eq!(e.col, 8); // "big " row: x=2 -> 'g'
        press(&mut e, KeyCode::Up);
        assert_eq!(e.col, 2);
        press(&mut e, KeyCode::Down);
        press(&mut e, KeyCode::Down);
        assert_eq!(e.col, 12);

        // wrapped movement: width 4, "abcdefghij" is screen rows abcd / efgh / ij
        let mut e = Ed::new("abcdefghij\nxy");
        e.w = 4;
        e.col = 9;
        press(&mut e, KeyCode::Up);
        assert_eq!(e.col, 5);
        press(&mut e, KeyCode::Down);
        press(&mut e, KeyCode::Down); // past the last wrap row -> next line, same x
        assert_eq!((e.row, e.col), (1, 1));

        // unchanged -> exits straight away; edited -> asks first
        let mut e = Ed::new("a");
        assert_eq!(press(&mut e, KeyCode::Esc), Some(false));
        press(&mut e, KeyCode::Char('b'));
        assert_eq!(press(&mut e, KeyCode::Esc), None);
        assert_eq!(press(&mut e, KeyCode::Char('y')), Some(true));
    }
}
