use crate::store::{fmt_time, parse_tags, Note, Store};
use crate::edit_text;
use ratatui::{
    crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    layout::{Constraint::{Length, Min, Percentage}, Flex, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, BorderType, Clear, List, ListItem, ListState, Paragraph, Wrap},
    DefaultTerminal, Frame,
};
use std::{env, io};

const ACCENT: Color = Color::Rgb(217, 119, 87);
const DIM: Color = Color::Rgb(128, 128, 128);
const TAG: Color = Color::Rgb(122, 162, 247);
const BAD: Color = Color::Rgb(224, 108, 117);
const OK: Color = Color::Rgb(152, 195, 121);

#[derive(PartialEq, Clone, Copy)]
enum Mode {
    Normal,
    Search,
    New,
    Rename,
    Tags,
    Confirm,
    Help,
}

enum Typing {
    Submit,
    Cancel,
    Going,
}

struct App {
    store: Store,
    ids: Vec<u32>,
    sel: usize,
    scroll: u16,
    query: String,
    mode: Mode,
    input: String,
    cur: usize, // cursor, in chars
    status: String,
    quit: bool,
}

pub fn run(store: Store) -> io::Result<()> {
    let mut term = ratatui::init();
    let mut app = App { store, ids: vec![], sel: 0, scroll: 0, query: String::new(), mode: Mode::Normal, input: String::new(), cur: 0, status: String::new(), quit: false };
    app.refresh(None);
    let res = app.run(&mut term);
    ratatui::restore();
    res
}

impl App {
    fn run(&mut self, term: &mut DefaultTerminal) -> io::Result<()> {
        while !self.quit {
            term.draw(|f| self.draw(f))?;
            if let Event::Key(k) = event::read()? {
                if k.kind == KeyEventKind::Press {
                    self.key(k, term)?;
                }
            }
        }
        Ok(())
    }

    // ---------- state helpers ----------

    fn cur_id(&self) -> Option<u32> {
        self.ids.get(self.sel).copied()
    }

    fn cur_note(&self) -> Option<&Note> {
        self.cur_id().and_then(|id| self.store.get(id))
    }

    /// Recompute the visible list, keeping `keep` (or the current note) selected if it's still visible.
    fn refresh(&mut self, keep: Option<u32>) {
        let want = keep.or_else(|| self.cur_id());
        self.ids = self.store.view(&self.query, None);
        self.sel = want
            .and_then(|id| self.ids.iter().position(|&i| i == id))
            .unwrap_or(self.sel.min(self.ids.len().saturating_sub(1)));
        self.scroll = 0;
    }

    fn go(&mut self, i: usize) {
        self.sel = i.min(self.ids.len().saturating_sub(1));
        self.scroll = 0;
    }

    fn save(&mut self, ok: &str) {
        self.status = match self.store.save() {
            Ok(()) => format!("✓ {ok}"),
            Err(e) => format!("✗ could not save: {e}"),
        };
    }

    fn change(&mut self, id: u32, ok: &str, f: impl FnOnce(&mut Note)) {
        self.store.update(id, f);
        self.save(ok);
        self.refresh(Some(id));
    }

    fn prompt(&mut self, mode: Mode, text: &str) {
        self.mode = mode;
        self.input = text.to_string();
        self.cur = text.chars().count();
    }

    fn edit_body(&mut self, term: &mut DefaultTerminal) {
        let Some(n) = self.cur_note() else { return };
        let (id, body) = (n.id, n.body.clone());
        ratatui::restore();
        let res = edit_text(&body);
        *term = ratatui::init();
        match res {
            Ok(new) if new != body => self.change(id, "Saved", |n| n.body = new),
            Ok(_) => {}
            Err(e) => self.status = format!("✗ {e}"),
        }
    }

    // ---------- input ----------

    fn key(&mut self, k: KeyEvent, term: &mut DefaultTerminal) -> io::Result<()> {
        self.status.clear();
        if k.modifiers.contains(KeyModifiers::CONTROL) && k.code == KeyCode::Char('c') {
            self.quit = true;
            return Ok(());
        }
        match self.mode {
            Mode::Normal => self.normal(k, term),
            Mode::Help => self.mode = Mode::Normal,
            Mode::Confirm => {
                if let (KeyCode::Char('y' | 'Y'), Some(id)) = (k.code, self.cur_id()) {
                    self.store.remove(id);
                    self.save("Deleted");
                    self.refresh(None);
                }
                self.mode = Mode::Normal;
            }
            _ => match self.type_in(k) {
                Typing::Submit => self.submit(term),
                Typing::Cancel => {
                    if self.mode == Mode::Search {
                        self.query.clear();
                        self.refresh(None);
                    }
                    self.mode = Mode::Normal;
                }
                Typing::Going => {
                    if self.mode == Mode::Search {
                        self.query = self.input.clone();
                        self.refresh(None);
                    }
                }
            },
        }
        Ok(())
    }

    fn normal(&mut self, k: KeyEvent, term: &mut DefaultTerminal) {
        let last = self.ids.len().saturating_sub(1);
        match k.code {
            KeyCode::Char('q') => self.quit = true,
            KeyCode::Esc if !self.query.is_empty() => {
                self.query.clear();
                self.refresh(None);
            }
            KeyCode::Down | KeyCode::Char('j') => self.go(self.sel + 1),
            KeyCode::Up | KeyCode::Char('k') => self.go(self.sel.saturating_sub(1)),
            KeyCode::Home | KeyCode::Char('g') => self.go(0),
            KeyCode::End | KeyCode::Char('G') => self.go(last),
            KeyCode::PageDown => self.scroll = self.scroll.saturating_add(5),
            KeyCode::PageUp => self.scroll = self.scroll.saturating_sub(5),
            KeyCode::Char('n') => self.prompt(Mode::New, ""),
            KeyCode::Char('/') => self.prompt(Mode::Search, &self.query.clone()),
            KeyCode::Char('?') => self.mode = Mode::Help,
            _ if self.ids.is_empty() => {}
            KeyCode::Enter | KeyCode::Char('e') => self.edit_body(term),
            KeyCode::Char('r') => self.prompt(Mode::Rename, &self.cur_note().unwrap().title.clone()),
            KeyCode::Char('t') => self.prompt(Mode::Tags, &self.cur_note().unwrap().tags.join(", ")),
            KeyCode::Char('d') => self.mode = Mode::Confirm,
            KeyCode::Char('p') => {
                let id = self.cur_id().unwrap();
                let n = self.store.get_mut(id).unwrap();
                n.pinned = !n.pinned;
                let msg = if n.pinned { "Pinned" } else { "Unpinned" };
                self.save(msg);
                self.refresh(Some(id));
            }
            _ => {}
        }
    }

    fn submit(&mut self, term: &mut DefaultTerminal) {
        let text = self.input.trim().to_string();
        let mode = std::mem::replace(&mut self.mode, Mode::Normal);
        let id = self.cur_id();
        match (mode, id) {
            (Mode::New, _) => {
                let title = if text.is_empty() { "Untitled".into() } else { text };
                let id = self.store.add(title, String::new(), vec![]);
                self.save("Created");
                self.refresh(Some(id));
                self.edit_body(term); // straight into writing the body
            }
            (Mode::Rename, Some(id)) if !text.is_empty() => self.change(id, "Renamed", |n| n.title = text),
            (Mode::Tags, Some(id)) => self.change(id, "Tags updated", |n| n.tags = parse_tags(&text)),
            _ => {} // search: keep the filter
        }
    }

    /// Single-line editor with a movable cursor.
    fn type_in(&mut self, k: KeyEvent) -> Typing {
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        let chars: Vec<char> = self.input.chars().collect();
        let n = chars.len();
        let cut = |from: usize, to: usize| chars[..from].iter().chain(&chars[to..]).collect::<String>();
        match k.code {
            KeyCode::Enter => return Typing::Submit,
            KeyCode::Esc => return Typing::Cancel,
            KeyCode::Left => self.cur = self.cur.saturating_sub(1),
            KeyCode::Right => self.cur = (self.cur + 1).min(n),
            KeyCode::Home => self.cur = 0,
            KeyCode::End => self.cur = n,
            KeyCode::Char('a') if ctrl => self.cur = 0,
            KeyCode::Char('e') if ctrl => self.cur = n,
            KeyCode::Char('u') if ctrl => {
                self.input = cut(0, self.cur);
                self.cur = 0;
            }
            KeyCode::Char('w') if ctrl => {
                let mut s = self.cur;
                while s > 0 && chars[s - 1] == ' ' {
                    s -= 1;
                }
                while s > 0 && chars[s - 1] != ' ' {
                    s -= 1;
                }
                self.input = cut(s, self.cur);
                self.cur = s;
            }
            KeyCode::Backspace if self.cur > 0 => {
                self.input = cut(self.cur - 1, self.cur);
                self.cur -= 1;
            }
            KeyCode::Delete if self.cur < n => self.input = cut(self.cur, self.cur + 1),
            KeyCode::Char(c) if !ctrl => {
                let at = self.byte(self.cur);
                self.input.insert(at, c);
                self.cur += 1;
            }
            _ => {}
        }
        Typing::Going
    }

    fn byte(&self, char_idx: usize) -> usize {
        self.input.char_indices().nth(char_idx).map_or(self.input.len(), |(i, _)| i)
    }

    // ---------- drawing ----------

    fn draw(&self, f: &mut Frame) {
        let typing = matches!(self.mode, Mode::Search | Mode::New | Mode::Rename | Mode::Tags);
        let [head, main, input, foot] =
            Layout::vertical([Length(3), Min(3), Length(if typing { 3 } else { 0 }), Length(1)]).areas(f.area());

        // header
        let pinned = self.store.notes.iter().filter(|n| n.pinned).count();
        let path = self.store.path.display().to_string();
        let path = match env::var("HOME") {
            Ok(h) if path.starts_with(&h) => path.replacen(&h, "~", 1),
            _ => path,
        };
        let title = Line::from(vec![
            Span::styled(" ✻ ", Style::new().fg(ACCENT)),
            Span::styled("notes ", Style::new().bold()),
            Span::styled(format!("v{} ", env!("CARGO_PKG_VERSION")), Style::new().fg(DIM)),
        ]);
        let info = Line::from(vec![
            Span::raw(" Welcome back · "),
            Span::styled(format!("{} notes", self.store.notes.len()), Style::new().bold()),
            Span::styled(format!(" · {pinned} pinned · {path}"), Style::new().fg(DIM)),
        ]);
        f.render_widget(Paragraph::new(info).block(self.frame(title, ACCENT)), head);

        // list | preview
        let [left, right] = Layout::horizontal([Percentage(38), Percentage(62)]).areas(main);
        self.draw_list(f, left);
        self.draw_preview(f, right);

        // input box
        if typing {
            let label = match self.mode {
                Mode::New => "New note · title",
                Mode::Rename => "Rename",
                Mode::Tags => "Tags · comma separated",
                _ => "Search",
            };
            let line = Line::from(vec![Span::styled("› ", Style::new().fg(ACCENT)), Span::raw(self.input.as_str())]);
            f.render_widget(Paragraph::new(line).block(self.frame(format!(" {label} ").into(), ACCENT)), input);
            let w = Line::from(&self.input[..self.byte(self.cur)]).width() as u16;
            f.set_cursor_position((input.x + 3 + w, input.y + 1));
        }

        // footer
        f.render_widget(Paragraph::new(self.footer()), foot);

        if self.mode == Mode::Help {
            self.draw_help(f);
        }
    }

    fn frame(&self, title: Line<'static>, color: Color) -> Block<'static> {
        Block::bordered().border_type(BorderType::Rounded).border_style(Style::new().fg(color)).title(title)
    }

    fn draw_list(&self, f: &mut Frame, area: ratatui::layout::Rect) {
        let label = if self.query.is_empty() { " Notes ".to_string() } else { format!(" Notes · “{}” ", self.query) };
        let block = self.frame(label.into(), DIM);
        if self.ids.is_empty() {
            let msg = if self.query.is_empty() { "No notes yet.\n\nPress n to write your first one." } else { "Nothing matches.\n\nEsc clears the search." };
            let p = Paragraph::new(msg).style(Style::new().fg(DIM)).centered().block(block);
            return f.render_widget(p, area);
        }
        let items: Vec<ListItem> = self
            .ids
            .iter()
            .filter_map(|&id| self.store.get(id))
            .map(|n| {
                let mut meta = vec![Span::styled(format!("  {}", fmt_time(n.updated)), Style::new().fg(DIM))];
                meta.extend(n.tags.iter().map(|t| Span::styled(format!(" #{t}"), Style::new().fg(TAG))));
                let pin = if n.pinned { "★ " } else { "  " };
                let head = Line::from(vec![Span::styled(pin, Style::new().fg(ACCENT)), Span::styled(n.title.as_str(), Style::new().bold())]);
                ListItem::new(Text::from(vec![head, Line::from(meta)]))
            })
            .collect();
        let list = List::new(items)
            .block(block)
            .highlight_symbol("▌")
            .highlight_style(Style::new().fg(ACCENT));
        f.render_stateful_widget(list, area, &mut ListState::default().with_selected(Some(self.sel)));
    }

    fn draw_preview(&self, f: &mut Frame, area: ratatui::layout::Rect) {
        let block = self.frame(" Preview ".into(), DIM);
        let Some(n) = self.cur_note() else { return f.render_widget(block, area) };
        let dim = Style::new().fg(DIM);
        let mut lines = vec![
            Line::styled(n.title.as_str(), Style::new().fg(ACCENT).bold()),
            Line::styled(format!("#{} · created {} · updated {}", n.id, fmt_time(n.created), fmt_time(n.updated)), dim),
        ];
        if !n.tags.is_empty() {
            lines.push(Line::styled(n.tags.iter().map(|t| format!("#{t}")).collect::<Vec<_>>().join(" "), Style::new().fg(TAG)));
        }
        lines.push(Line::raw(""));
        if n.body.is_empty() {
            lines.push(Line::styled("(empty — press e to write)", dim.add_modifier(Modifier::ITALIC)));
        }
        lines.extend(n.body.lines().map(|l| {
            if l.starts_with('#') { Line::styled(l, Style::new().bold()) } else { Line::raw(l) }
        }));
        let p = Paragraph::new(lines).wrap(Wrap { trim: false }).scroll((self.scroll, 0)).block(block);
        f.render_widget(p, area);
    }

    fn footer(&self) -> Line<'_> {
        if self.mode == Mode::Confirm {
            let t = self.cur_note().map_or("", |n| n.title.as_str());
            return Line::from(vec![
                Span::styled(format!(" Delete “{t}”? "), Style::new().fg(BAD).bold()),
                Span::styled("y", Style::new().fg(ACCENT)),
                Span::styled(" confirm · any other key cancels", Style::new().fg(DIM)),
            ]);
        }
        if !self.status.is_empty() {
            let c = if self.status.starts_with('✗') { BAD } else { OK };
            return Line::styled(format!(" {}", self.status), Style::new().fg(c));
        }
        let hints = match self.mode {
            Mode::Normal => &[("↑↓", "move"), ("n", "new"), ("e", "edit"), ("r", "rename"), ("t", "tags"), ("p", "pin"), ("d", "delete"), ("/", "search"), ("?", "help"), ("q", "quit")][..],
            Mode::Help => &[("any key", "close")][..],
            _ => &[("enter", "confirm"), ("esc", "cancel")][..],
        };
        let mut spans = vec![Span::raw(" ")];
        for (k, d) in hints {
            spans.push(Span::styled(*k, Style::new().fg(ACCENT)));
            spans.push(Span::styled(format!(" {d}  "), Style::new().fg(DIM)));
        }
        Line::from(spans)
    }

    fn draw_help(&self, f: &mut Frame) {
        let rows = [
            ("↑ ↓ / j k", "move"), ("g / G", "first / last"), ("PgUp PgDn", "scroll preview"),
            ("n", "new note"), ("e / enter", "edit body in $EDITOR"), ("r", "rename title"),
            ("t", "edit tags"), ("p", "pin / unpin"), ("d", "delete (asks first)"),
            ("/", "search titles, text, tags"), ("esc", "clear search"), ("q / ctrl-c", "quit"),
        ];
        let mut lines = vec![Line::raw("")];
        lines.extend(rows.iter().map(|(k, d)| Line::from(vec![Span::styled(format!("  {k:<12}"), Style::new().fg(ACCENT)), Span::raw(*d)])));
        let [v] = Layout::vertical([Length(lines.len() as u16 + 2)]).flex(Flex::Center).areas(f.area());
        let [area] = Layout::horizontal([Length(44)]).flex(Flex::Center).areas(v);
        f.render_widget(Clear, area);
        f.render_widget(Paragraph::new(lines).block(self.frame(" Keys ".into(), ACCENT)), area);
    }
}
