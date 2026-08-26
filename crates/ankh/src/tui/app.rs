//! Application state and the event loop.

use std::time::{Duration, Instant};

use ankh_core::engine::{SyncHandle, SyncOp};
use ankh_core::{AuthStore, Credentials, Engine, Error, Paths, Result, SyncOptions, SyncOutcome, SyncProgress};
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::layout::{Alignment, Constraint, Flex, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ratatui::{DefaultTerminal, Frame};

use super::keys::{format_seq, Key, Keymap, Match};
use super::theme::Theme;
use super::views::decks::DecksView;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Quit,
    ForceQuit,
    Down,
    Up,
    Top,
    Bottom,
    HalfDown,
    HalfUp,
    Expand,
    Collapse,
    ToggleFold,
    Open,
    Sync,
    SyncDownload,
    SyncUpload,
    Refresh,
    CommandMode,
    Help,
    ClearMessage,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Command,
}

impl Mode {
    fn label(&self) -> &'static str {
        match self {
            Mode::Normal => "NORMAL",
            Mode::Command => "COMMAND",
        }
    }
}

#[derive(Debug, Clone)]
enum Message {
    Info(String),
    Error(String),
}

/// A blocking question with single-key answers, e.g. the full-sync prompt.
struct Prompt {
    title: String,
    body: Vec<String>,
    choices: Vec<(char, String, Action)>,
}

struct SyncState {
    handle: SyncHandle,
    started: Instant,
    quit_after: bool,
}

pub struct App {
    engine: Engine,
    creds: Option<Credentials>,
    theme: Theme,
    mode: Mode,
    decks: DecksView,
    keymap: Keymap<Action>,
    pending: Vec<Key>,
    count: Option<usize>,
    pending_since: Option<Instant>,
    cmdline: String,
    message: Option<(Message, Instant)>,
    prompt: Option<Prompt>,
    sync: Option<SyncState>,
    last_sync: Option<Instant>,
    show_help: bool,
    should_quit: bool,
    tick: u64,
}

const TIMEOUTLEN: Duration = Duration::from_millis(1000);

impl App {
    pub fn new(paths: Paths) -> Result<Self> {
        let store = AuthStore::new(&paths.profile);
        let creds = store.load()?;
        let engine = Engine::open(paths)?;
        let mut app = App {
            engine,
            creds,
            theme: Theme::tokyonight(),
            mode: Mode::Normal,
            decks: DecksView::default(),
            keymap: default_keymap(),
            pending: Vec::new(),
            count: None,
            pending_since: None,
            cmdline: String::new(),
            message: None,
            prompt: None,
            sync: None,
            last_sync: None,
            show_help: false,
            should_quit: false,
            tick: 0,
        };
        app.refresh();
        Ok(app)
    }

    pub fn run(mut self, mut terminal: DefaultTerminal) -> anyhow::Result<()> {
        // Sync on launch (when logged in). Never blocks the UI.
        if self.creds.is_some() {
            self.start_sync(SyncOp::Normal, false);
        } else {
            self.info("not logged in — run `ankh login` in a shell to enable sync");
        }
        while !self.should_quit {
            terminal.draw(|f| self.draw(f))?;
            if event::poll(Duration::from_millis(50))? {
                match event::read()? {
                    Event::Key(k) if k.kind != KeyEventKind::Release => self.on_key(Key::from_event(k)),
                    Event::Resize(_, _) => {}
                    _ => {}
                }
            }
            self.on_tick();
        }
        self.engine.close()?;
        Ok(())
    }

    // ----- state helpers ---------------------------------------------------

    fn info(&mut self, s: impl Into<String>) {
        self.message = Some((Message::Info(s.into()), Instant::now()));
    }

    fn error(&mut self, s: impl Into<String>) {
        self.message = Some((Message::Error(s.into()), Instant::now()));
    }

    fn refresh(&mut self) {
        match self.engine.deck_tree() {
            Ok(t) => self.decks.set_tree(t),
            Err(Error::Busy) => {}
            Err(e) => self.error(e.to_string()),
        }
    }

    fn start_sync(&mut self, op: SyncOp, quit_after: bool) {
        if self.sync.is_some() {
            self.info("a sync is already running");
            return;
        }
        let Some(creds) = self.creds.clone() else {
            self.error("not logged in — run `ankh login`");
            return;
        };
        match self.engine.sync_in_background(creds, op, SyncOptions { media: true }) {
            Ok(handle) => self.sync = Some(SyncState { handle, started: Instant::now(), quit_after }),
            Err(e) => self.error(e.to_string()),
        }
    }

    fn on_tick(&mut self) {
        self.tick = self.tick.wrapping_add(1);
        if let Some((_, at)) = &self.message {
            if at.elapsed() > Duration::from_secs(6) {
                self.message = None;
            }
        }
        if let Some(since) = self.pending_since {
            if since.elapsed() > TIMEOUTLEN && !self.pending.is_empty() {
                // neovim's timeoutlen: give up on the sequence.
                self.pending.clear();
                self.pending_since = None;
            }
        }
        if self.sync.as_ref().map(|s| s.handle.is_finished()).unwrap_or(false) {
            let SyncState { handle, quit_after, .. } = self.sync.take().unwrap();
            match self.engine.finish_background(handle) {
                Ok((report, creds)) => {
                    if self.creds.as_ref() != Some(&creds) {
                        let _ = AuthStore::new(&self.engine.paths().profile).save(&creds);
                        self.creds = Some(creds);
                    }
                    self.last_sync = Some(Instant::now());
                    match report.outcome {
                        SyncOutcome::NoChanges => self.info("already in sync"),
                        SyncOutcome::Synced => self.info("synced"),
                        SyncOutcome::FullDownloaded => self.info("downloaded collection from AnkiWeb"),
                        SyncOutcome::FullUploaded => self.info("uploaded collection to AnkiWeb"),
                        SyncOutcome::FullSyncRequired { upload_ok, download_ok } => {
                            if download_ok && self.engine.is_pristine().unwrap_or(false) {
                                self.info("empty local collection — downloading from AnkiWeb");
                                self.start_sync(SyncOp::FullDownload, quit_after);
                                return;
                            }
                            self.prompt = Some(full_sync_prompt(upload_ok, download_ok));
                        }
                    }
                    if !report.server_message.is_empty() {
                        self.info(format!("AnkiWeb: {}", report.server_message));
                    }
                }
                Err(e) => self.error(format!("sync failed: {e}")),
            }
            self.refresh();
            if quit_after {
                self.should_quit = true;
            }
        }
    }

    // ----- input -------------------------------------------------------------

    fn on_key(&mut self, key: Key) {
        if let Some(p) = &self.prompt {
            if let KeyCode::Char(c) = key.code {
                if let Some((_, _, action)) = p.choices.iter().find(|(k, _, _)| *k == c) {
                    let action = *action;
                    self.prompt = None;
                    self.dispatch(action);
                }
            } else if key.code == KeyCode::Esc {
                self.prompt = None;
            }
            return;
        }
        if self.show_help {
            self.show_help = false;
            return;
        }
        match self.mode {
            Mode::Command => self.on_key_command(key),
            Mode::Normal => self.on_key_normal(key),
        }
    }

    fn on_key_normal(&mut self, key: Key) {
        // Count prefix: digits before a sequence (but `0` alone is a motion).
        if self.pending.is_empty() {
            if let KeyCode::Char(c @ '0'..='9') = key.code {
                if key.mods.is_empty() && !(c == '0' && self.count.is_none()) {
                    let d = c.to_digit(10).unwrap() as usize;
                    self.count = Some(self.count.unwrap_or(0).saturating_mul(10).saturating_add(d).min(9999));
                    return;
                }
            }
        }
        if key.code == KeyCode::Esc && (!self.pending.is_empty() || self.count.is_some()) {
            self.pending.clear();
            self.count = None;
            self.pending_since = None;
            return;
        }
        self.pending.push(key);
        self.pending_since = Some(Instant::now());
        match self.keymap.lookup(&self.pending) {
            Match::Exact(b) => {
                let action = b.action;
                self.pending.clear();
                self.pending_since = None;
                let n = self.count.take().unwrap_or(1);
                for _ in 0..n.min(500) {
                    self.dispatch(action);
                }
            }
            Match::Prefix(_) => {}
            Match::None => {
                self.pending.clear();
                self.pending_since = None;
                self.count = None;
            }
        }
    }

    fn on_key_command(&mut self, key: Key) {
        match key.code {
            KeyCode::Esc => {
                self.mode = Mode::Normal;
                self.cmdline.clear();
            }
            KeyCode::Enter => {
                let cmd = std::mem::take(&mut self.cmdline);
                self.mode = Mode::Normal;
                self.run_command(cmd.trim());
            }
            KeyCode::Backspace => {
                if self.cmdline.pop().is_none() {
                    self.mode = Mode::Normal;
                }
            }
            KeyCode::Char('u') if key.mods.contains(KeyModifiers::CONTROL) => self.cmdline.clear(),
            KeyCode::Char(c) if !key.mods.contains(KeyModifiers::CONTROL) => self.cmdline.push(c),
            _ => {}
        }
    }

    fn run_command(&mut self, cmd: &str) {
        let mut parts = cmd.split_whitespace();
        let Some(head) = parts.next() else { return };
        let arg = parts.next();
        match (head, arg) {
            ("q" | "quit", _) => self.dispatch(Action::Quit),
            ("q!" | "quit!" | "qa!", _) => self.dispatch(Action::ForceQuit),
            ("sync" | "s", None) => self.dispatch(Action::Sync),
            ("sync", Some("download" | "down" | "pull")) => self.dispatch(Action::SyncDownload),
            ("sync", Some("upload" | "up" | "push")) => self.dispatch(Action::SyncUpload),
            ("refresh" | "r", _) => self.dispatch(Action::Refresh),
            ("help" | "h", _) => self.dispatch(Action::Help),
            _ => self.error(format!("not a command: {cmd}")),
        }
    }

    fn dispatch(&mut self, action: Action) {
        match action {
            Action::Quit => {
                if self.creds.is_some() && self.sync.is_none() {
                    self.start_sync(SyncOp::Normal, true);
                } else if self.sync.is_some() {
                    self.sync.as_mut().unwrap().quit_after = true;
                } else {
                    self.should_quit = true;
                }
            }
            Action::ForceQuit => self.should_quit = true,
            Action::Down => self.decks.move_by(1),
            Action::Up => self.decks.move_by(-1),
            Action::Top => self.decks.go_top(),
            Action::Bottom => self.decks.go_bottom(),
            Action::HalfDown => self.decks.move_by(10),
            Action::HalfUp => self.decks.move_by(-10),
            Action::Expand => self.decks.expand(),
            Action::Collapse => self.decks.collapse(),
            Action::ToggleFold => self.decks.toggle_fold(),
            Action::Open => {
                if let Some(d) = self.decks.selected_deck() {
                    let name = d.full_name.clone();
                    self.info(format!("review of “{name}” lands in the next milestone"));
                }
            }
            Action::Sync => self.start_sync(SyncOp::Normal, false),
            Action::SyncDownload => self.start_sync(SyncOp::FullDownload, false),
            Action::SyncUpload => self.start_sync(SyncOp::FullUpload, false),
            Action::Refresh => {
                self.refresh();
                self.info("refreshed");
            }
            Action::CommandMode => {
                self.mode = Mode::Command;
                self.cmdline.clear();
            }
            Action::Help => self.show_help = !self.show_help,
            Action::ClearMessage => self.message = None,
        }
    }

    // ----- drawing -------------------------------------------------------------

    fn draw(&mut self, f: &mut Frame) {
        let theme = self.theme.clone();
        let area = f.area();
        f.render_widget(Block::default().style(theme.base()), area);
        let chunks = Layout::vertical([Constraint::Min(3), Constraint::Length(1), Constraint::Length(1)]).split(area);

        self.decks.draw(f, chunks[0], &theme, self.prompt.is_none());
        self.draw_statusline(f, chunks[1], &theme);
        self.draw_cmdline(f, chunks[2], &theme);

        if let Match::Prefix(next) = self.keymap.lookup(&self.pending) {
            if !self.pending.is_empty() {
                self.draw_which_key(f, area, &theme, next);
            }
        }
        if self.sync.is_some() {
            self.draw_sync_overlay(f, area, &theme);
        }
        if let Some(p) = &self.prompt {
            draw_prompt(f, area, &theme, p);
        }
        if self.show_help {
            self.draw_help(f, area, &theme);
        }
    }

    fn draw_statusline(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let mode = self.mode.label();
        let mut left = vec![Span::styled(format!(" {mode} "), theme.mode_pill(mode)), Span::raw(" ")];
        if let Some(c) = self.count {
            left.push(Span::styled(c.to_string(), theme.accent()));
        }
        if !self.pending.is_empty() {
            left.push(Span::styled(format_seq(&self.pending), theme.accent()));
        }
        let user = self.creds.as_ref().map(|c| c.username.as_str()).unwrap_or("not logged in");
        let sync = match (&self.sync, self.last_sync) {
            (Some(_), _) => "syncing…".to_string(),
            (None, Some(t)) => format!("synced {}", humanize(t.elapsed())),
            (None, None) => "never synced".to_string(),
        };
        let right = Line::from(vec![
            Span::styled(user, theme.muted()),
            Span::styled("  ·  ", theme.muted()),
            Span::styled(sync, theme.muted()),
            Span::styled("  ·  ", theme.muted()),
            Span::styled("? help ", theme.muted()),
        ])
        .alignment(Alignment::Right);
        let bar = Style::default().bg(theme.bg_alt);
        f.render_widget(Block::default().style(bar), area);
        f.render_widget(Paragraph::new(Line::from(left)).style(bar), area);
        f.render_widget(Paragraph::new(right).style(bar), area);
    }

    fn draw_cmdline(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let line = match (&self.mode, &self.message) {
            (Mode::Command, _) => Line::from(vec![Span::styled(":", theme.accent()), Span::raw(self.cmdline.clone())]),
            (_, Some((Message::Error(m), _))) => {
                Line::from(Span::styled(format!(" {m}"), Style::default().fg(theme.error)))
            }
            (_, Some((Message::Info(m), _))) => Line::from(Span::styled(format!(" {m}"), theme.muted())),
            _ => Line::default(),
        };
        f.render_widget(Paragraph::new(line), area);
        if self.mode == Mode::Command {
            f.set_cursor_position((area.x + 1 + self.cmdline.chars().count() as u16, area.y));
        }
    }

    fn draw_which_key(&self, f: &mut Frame, area: Rect, theme: &Theme, next: Vec<(String, &str)>) {
        let rows = next.len() as u16 + 2;
        let width = next.iter().map(|(k, d)| k.len() + d.len() + 5).max().unwrap_or(20).max(24) as u16;
        let rect = Rect {
            x: area.right().saturating_sub(width + 1),
            y: area.bottom().saturating_sub(rows + 2),
            width: width.min(area.width),
            height: rows.min(area.height),
        };
        let lines: Vec<Line> = next
            .iter()
            .map(|(k, d)| {
                Line::from(vec![
                    Span::styled(format!(" {k:<4}"), theme.accent()),
                    Span::styled("→ ", theme.muted()),
                    Span::raw(d.to_string()),
                ])
            })
            .collect();
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(theme.border())
            .title(Span::styled(format!(" {} ", format_seq(&self.pending)), theme.title()))
            .style(Style::default().bg(theme.bg_alt));
        f.render_widget(Clear, rect);
        f.render_widget(Paragraph::new(lines).block(block), rect);
    }

    fn draw_sync_overlay(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        const SPIN: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
        let spinner = SPIN[(self.tick / 2) as usize % SPIN.len()];
        let elapsed = self.sync.as_ref().map(|s| s.started.elapsed()).unwrap_or_default();
        let detail = match self.engine.sync_progress() {
            SyncProgress::Idle | SyncProgress::Connecting => "connecting to AnkiWeb".to_string(),
            SyncProgress::Collection { stage, added, removed } => {
                format!("{stage} · {added} changed, {removed} removed")
            }
            SyncProgress::Full { transferred, total } if total > 0 => {
                format!(
                    "transferring {} / {} ({}%)",
                    human_bytes(transferred),
                    human_bytes(total),
                    transferred * 100 / total
                )
            }
            SyncProgress::Full { transferred, .. } => format!("transferring {}", human_bytes(transferred)),
            SyncProgress::Media { checked, downloaded, uploaded } => {
                format!("media · checked {checked}, ↓{downloaded} ↑{uploaded}")
            }
        };
        let rect = centered(area, 52, 5);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(theme.accent())
            .title(Span::styled(" sync ", theme.title()))
            .style(Style::default().bg(theme.bg_alt));
        let lines = vec![
            Line::from(vec![Span::styled(format!(" {spinner} "), theme.accent()), Span::raw(detail)]),
            Line::from(Span::styled(format!("   {}s · q to quit when done", elapsed.as_secs()), theme.muted())),
        ];
        f.render_widget(Clear, rect);
        f.render_widget(Paragraph::new(lines).block(block).wrap(Wrap { trim: true }), rect);
    }

    fn draw_help(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let mut lines: Vec<Line> = self
            .keymap
            .bindings()
            .into_iter()
            .map(|b| {
                Line::from(vec![
                    Span::styled(format!(" {:<10}", format_seq(&b.seq)), theme.accent()),
                    Span::raw(b.desc.clone()),
                ])
            })
            .collect();
        lines.push(Line::default());
        lines.push(Line::from(Span::styled(" :sync [download|upload]  :refresh  :q  :q!", theme.muted())));
        let rect = centered(area, 48, lines.len() as u16 + 2);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(theme.border())
            .title(Span::styled(" keys ", theme.title()))
            .style(Style::default().bg(theme.bg_alt));
        f.render_widget(Clear, rect);
        f.render_widget(Paragraph::new(lines).block(block), rect);
    }
}

fn draw_prompt(f: &mut Frame, area: Rect, theme: &Theme, p: &Prompt) {
    let mut lines: Vec<Line> = p.body.iter().map(|s| Line::from(Span::raw(format!(" {s}")))).collect();
    lines.push(Line::default());
    for (k, label, _) in &p.choices {
        lines.push(Line::from(vec![
            Span::styled(format!("  {k}  "), theme.mode_pill("NORMAL")),
            Span::raw(format!(" {label}")),
        ]));
    }
    lines.push(Line::from(Span::styled("  Esc  decide later", theme.muted())));
    let rect = centered(area, 60, lines.len() as u16 + 2);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.warn))
        .title(Span::styled(format!(" {} ", p.title), Style::default().fg(theme.warn).add_modifier(Modifier::BOLD)))
        .style(Style::default().bg(theme.bg_alt));
    f.render_widget(Clear, rect);
    f.render_widget(Paragraph::new(lines).block(block).wrap(Wrap { trim: false }), rect);
}

fn full_sync_prompt(upload_ok: bool, download_ok: bool) -> Prompt {
    let mut choices = Vec::new();
    if download_ok {
        choices.push(('d', "Download — replace this collection with AnkiWeb's".to_string(), Action::SyncDownload));
    }
    if upload_ok {
        choices.push(('u', "Upload — replace AnkiWeb's collection with this one".to_string(), Action::SyncUpload));
    }
    Prompt {
        title: "full sync required".into(),
        body: vec![
            "The local collection and AnkiWeb have diverged in a way that".into(),
            "can't be merged. One side must overwrite the other.".into(),
        ],
        choices,
    }
}

fn default_keymap() -> Keymap<Action> {
    let mut k = Keymap::default();
    k.bind("j", Action::Down, "down")
        .bind("<Down>", Action::Down, "down")
        .bind("k", Action::Up, "up")
        .bind("<Up>", Action::Up, "up")
        .bind("gg", Action::Top, "first deck")
        .bind("G", Action::Bottom, "last deck")
        .bind("<C-d>", Action::HalfDown, "half page down")
        .bind("<C-u>", Action::HalfUp, "half page up")
        .bind("l", Action::Expand, "expand")
        .bind("h", Action::Collapse, "collapse / parent")
        .bind("za", Action::ToggleFold, "toggle fold")
        .bind("<CR>", Action::Open, "study deck")
        .bind("S", Action::Sync, "sync")
        .bind("<leader>ss", Action::Sync, "sync")
        .bind("<leader>sd", Action::SyncDownload, "sync: full download")
        .bind("<leader>su", Action::SyncUpload, "sync: full upload")
        .bind("R", Action::Refresh, "refresh")
        .bind(":", Action::CommandMode, "command line")
        .bind("?", Action::Help, "help")
        .bind("q", Action::Quit, "quit (syncs first)")
        .bind("ZZ", Action::Quit, "quit (syncs first)")
        .bind("ZQ", Action::ForceQuit, "quit without syncing")
        .bind("<Esc>", Action::ClearMessage, "clear message");
    k
}

fn centered(area: Rect, w: u16, h: u16) -> Rect {
    let [h_area] = Layout::horizontal([Constraint::Length(w.min(area.width))]).flex(Flex::Center).areas(area);
    let [rect] = Layout::vertical([Constraint::Length(h.min(area.height))]).flex(Flex::Center).areas(h_area);
    rect
}

fn humanize(d: Duration) -> String {
    let s = d.as_secs();
    if s < 5 {
        "just now".into()
    } else if s < 60 {
        format!("{s}s ago")
    } else if s < 3600 {
        format!("{}m ago", s / 60)
    } else {
        format!("{}h ago", s / 3600)
    }
}

fn human_bytes(n: usize) -> String {
    if n > 1 << 20 {
        format!("{:.1} MiB", n as f64 / (1 << 20) as f64)
    } else if n > 1 << 10 {
        format!("{} KiB", n >> 10)
    } else {
        format!("{n} B")
    }
}
