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

use std::collections::HashMap;

use super::audio::Player;
use super::images::Images;
use super::keys::{format_seq, Key, Keymap, Match};
use super::theme::Theme;
use super::views::browser::BrowserView;
use super::views::decks::DecksView;
use super::views::review::{ReviewView, Stage};
use ankh_core::{Av, Rating};

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
    // review
    ShowAnswer,
    /// Space/Enter: show the answer, or rate "good" if it's already shown.
    Continue,
    Rate(Rating),
    Undo,
    Bury,
    Suspend,
    ToggleMark,
    Flag(u8),
    Replay,
    Unbury,
    Back,
    ScrollDown,
    ScrollUp,
    ToggleHints,
    // browser
    OpenBrowser,
    BrowseDeck,
    InsertMode,
    VisualMode,
    ClearSearch,
    Preview,
    FlipPreview,
    CardInfo,
    ToggleSuspend,
    BulkBury,
    BulkFlag(u8),
    BulkMark,
    PromptTag,
    PromptUntag,
    PromptMove,
    PromptDue,
    ConfirmDelete,
    ConfirmForget,
    CycleSort,
    ReverseSort,
    StudyCard,
    EditNote,
    AddNote,
}

/// Something to do with the terminal released (run `$EDITOR`).
enum EditRequest {
    Existing { note_id: i64 },
    New { deck: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum View {
    Decks,
    Review,
    Browser,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Command,
    Insert,
    Visual,
    Prompt(PromptKind),
}

/// What a text prompt on the command line is for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromptKind {
    AddTag,
    RemoveTag,
    MoveDeck,
    SetDue,
    ConfirmDelete(usize),
    ConfirmForget(usize),
}

impl PromptKind {
    fn label(&self) -> String {
        match self {
            PromptKind::AddTag => "add tags: ".into(),
            PromptKind::RemoveTag => "remove tags: ".into(),
            PromptKind::MoveDeck => "move to deck: ".into(),
            PromptKind::SetDue => "due in days (0, 3, 1-7, 2!): ".into(),
            PromptKind::ConfirmDelete(n) => {
                format!("delete {n} note{} and all their cards? (y/N) ", if *n == 1 { "" } else { "s" })
            }
            PromptKind::ConfirmForget(n) => format!("reset {n} card{} to new? (y/N) ", if *n == 1 { "" } else { "s" }),
        }
    }
}

impl Mode {
    fn label(&self) -> &'static str {
        match self {
            Mode::Normal => "NORMAL",
            Mode::Command => "COMMAND",
            Mode::Insert => "INSERT",
            Mode::Visual => "VISUAL",
            Mode::Prompt(_) => "PROMPT",
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
    view: View,
    decks: DecksView,
    review: Option<ReviewView>,
    browser: Option<BrowserView>,
    audio: Player,
    images: Images,
    keymaps: HashMap<View, Keymap<Action>>,
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
    edit_request: Option<EditRequest>,
}

const TIMEOUTLEN: Duration = Duration::from_millis(1000);

impl App {
    pub fn new(paths: Paths, images: Images) -> Result<Self> {
        let store = AuthStore::new(&paths.profile);
        let creds = store.load()?;
        let engine = Engine::open(paths)?;
        let mut app = App {
            engine,
            creds,
            theme: Theme::tokyonight(),
            mode: Mode::Normal,
            view: View::Decks,
            decks: DecksView::default(),
            review: None,
            browser: None,
            audio: Player::new(),
            images,
            keymaps: default_keymaps(),
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
            edit_request: None,
        };
        app.refresh();
        if !app.audio.available() {
            app.info("no audio player found — install mpv for card audio");
        }
        tracing::info!(images = app.images.protocol_name(), "image protocol");
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
            if let Some(req) = self.edit_request.take() {
                // Give the terminal to the editor, then take it back.
                ratatui::restore();
                self.audio.stop();
                let outcome = self.run_editor(req);
                terminal = ratatui::init();
                match outcome {
                    Ok(msg) => self.info(msg),
                    Err(e) => self.error(e),
                }
            }
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

    fn run_editor(&mut self, req: EditRequest) -> std::result::Result<String, String> {
        use crate::editor;
        match req {
            EditRequest::Existing { note_id } => {
                let doc = self.engine.note_doc(note_id).map_err(|e| e.to_string())?;
                let text = ankh_core::notefile::write(&[doc]);
                let Some(edited) = editor::edit_text(&text, &format!("note-{note_id}")).map_err(|e| e.to_string())?
                else {
                    return Ok("no changes".into());
                };
                let r = editor::save_note_file(&mut self.engine, &edited).map_err(|e| e.to_string())?;
                self.after_edit();
                Ok(format!("saved note {note_id}{}", if r.updated == 1 { "" } else { " (+ more)" }))
            }
            EditRequest::New { deck } => {
                let text = editor::new_note_template(&mut self.engine, None, &deck).map_err(|e| e.to_string())?;
                let Some(edited) = editor::edit_text(&text, "new").map_err(|e| e.to_string())? else {
                    return Ok("cancelled".into());
                };
                let body = editor::strip_leading_comments(&edited);
                if editor::is_blank(&body) {
                    return Ok("cancelled (empty note)".into());
                }
                let r = editor::save_note_file(&mut self.engine, &body).map_err(|e| e.to_string())?;
                self.after_edit();
                Ok(format!("added {} note{}", r.added, plural(r.added)))
            }
        }
    }

    /// Refresh whatever view is showing after a note changed.
    fn after_edit(&mut self) {
        match self.view {
            View::Review => {
                // Re-render the current card with its new content.
                if let Some(rv) = self.review.as_mut() {
                    if let Some(card) = rv.card.clone() {
                        if let Ok(Some(fresh)) = self.engine.next_card() {
                            if fresh.card_id == card.card_id {
                                let shown = rv.answer_shown();
                                rv.show_card(fresh);
                                if shown {
                                    rv.show_answer();
                                }
                            }
                        }
                    }
                }
            }
            View::Browser => {
                if let Some(b) = self.browser.as_mut() {
                    b.refresh(&mut self.engine);
                }
            }
            View::Decks => self.refresh(),
        }
    }

    fn keymap(&self) -> &Keymap<Action> {
        &self.keymaps[&self.view]
    }

    // ----- review flow -------------------------------------------------------

    fn open_deck(&mut self) {
        let Some(d) = self.decks.selected_deck() else { return };
        let (id, name) = (d.id, d.full_name.clone());
        if let Err(e) = self.engine.select_deck(id) {
            self.error(e.to_string());
            return;
        }
        let mut rv = ReviewView::new(name, self.engine.paths().media_folder());
        rv.session_started = Instant::now();
        self.review = Some(rv);
        self.view = View::Review;
        self.advance();
    }

    /// Load the next card (or the "done" screen) into the review view.
    fn advance(&mut self) {
        match self.engine.next_card() {
            Ok(Some(card)) => {
                let av = card.question_av.clone();
                if let Some(rv) = self.review.as_mut() {
                    rv.show_card(card);
                }
                self.play(&av);
            }
            Ok(None) => {
                self.audio.stop();
                match self.engine.congrats() {
                    Ok(c) => {
                        if let Some(rv) = self.review.as_mut() {
                            rv.finish(c);
                        }
                    }
                    Err(e) => self.error(e.to_string()),
                }
            }
            Err(e) => self.error(e.to_string()),
        }
    }

    fn play(&mut self, av: &[Av]) {
        let files: Vec<_> = av
            .iter()
            .filter_map(|a| match a {
                Av::File { name } => Some(self.engine.media_path(name)),
                Av::Tts { .. } => None,
            })
            .collect();
        if !files.is_empty() {
            self.audio.play(&files);
        }
    }

    fn show_answer(&mut self) {
        let Some(rv) = self.review.as_mut() else { return };
        if rv.answer_shown() {
            return;
        }
        rv.show_answer();
        let av = rv.card.as_ref().map(|c| c.answer_av.clone()).unwrap_or_default();
        self.play(&av);
    }

    fn rate(&mut self, rating: Rating) {
        let Some(rv) = self.review.as_mut() else { return };
        if !rv.answer_shown() {
            return;
        }
        let Some(card) = rv.card.clone() else { return };
        let taken = rv.millis_taken();
        match self.engine.answer(&card, rating, taken) {
            Ok(()) => {
                if let Some(rv) = self.review.as_mut() {
                    rv.reviewed += 1;
                }
                self.advance();
            }
            Err(e) => self.error(e.to_string()),
        }
    }

    fn with_current_card(&mut self, f: impl FnOnce(&mut Self, &ankh_core::ReviewCard) -> Result<String>) {
        let Some(card) = self.review.as_ref().and_then(|r| r.card.clone()) else { return };
        match f(self, &card) {
            Ok(msg) => {
                self.info(msg);
                self.advance();
            }
            Err(e) => self.error(e.to_string()),
        }
    }

    fn leave_review(&mut self) {
        self.audio.stop();
        self.review = None;
        self.view = View::Decks;
        self.refresh();
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
        match self.mode.clone() {
            Mode::Command => self.on_key_command(key),
            Mode::Insert => self.on_key_insert(key),
            Mode::Prompt(kind) => self.on_key_prompt(key, kind),
            Mode::Normal | Mode::Visual => self.on_key_normal(key),
        }
    }

    fn on_key_insert(&mut self, key: Key) {
        let Some(b) = self.browser.as_mut() else {
            self.mode = Mode::Normal;
            return;
        };
        match key.code {
            KeyCode::Esc => self.mode = Mode::Normal,
            KeyCode::Enter => {
                b.run_search(&mut self.engine);
                self.mode = Mode::Normal;
            }
            KeyCode::Backspace => b.backspace(),
            KeyCode::Left => b.cursor_left(),
            KeyCode::Right => b.cursor_right(),
            KeyCode::Char('w') if key.mods.contains(KeyModifiers::CONTROL) => b.delete_word(),
            KeyCode::Char('u') if key.mods.contains(KeyModifiers::CONTROL) => b.clear_input(),
            KeyCode::Char(c) if !key.mods.contains(KeyModifiers::CONTROL) => b.insert_char(c),
            _ => {}
        }
    }

    fn on_key_prompt(&mut self, key: Key, kind: PromptKind) {
        match key.code {
            KeyCode::Esc => {
                self.mode = Mode::Normal;
                self.cmdline.clear();
            }
            KeyCode::Enter => {
                let text = std::mem::take(&mut self.cmdline);
                self.mode = Mode::Normal;
                self.finish_prompt(kind, text.trim());
            }
            KeyCode::Backspace => {
                self.cmdline.pop();
            }
            KeyCode::Char('u') if key.mods.contains(KeyModifiers::CONTROL) => self.cmdline.clear(),
            KeyCode::Char(c) if !key.mods.contains(KeyModifiers::CONTROL) => {
                // Confirmations take a single key.
                if matches!(kind, PromptKind::ConfirmDelete(_) | PromptKind::ConfirmForget(_)) {
                    self.mode = Mode::Normal;
                    self.cmdline.clear();
                    if c == 'y' || c == 'Y' {
                        self.finish_prompt(kind, "y");
                    } else {
                        self.info("cancelled");
                    }
                } else {
                    self.cmdline.push(c);
                }
            }
            _ => {}
        }
    }

    fn finish_prompt(&mut self, kind: PromptKind, text: &str) {
        let Some(b) = self.browser.as_ref() else { return };
        let cids = b.targets();
        if cids.is_empty() {
            return;
        }
        let res: Result<String> = (|| {
            Ok(match kind {
                PromptKind::AddTag => {
                    if text.is_empty() {
                        return Ok("no tags given".into());
                    }
                    let nids = self.engine.note_ids_for_cards(&cids)?;
                    let n = self.engine.add_tags(&nids, text)?;
                    format!("tagged {n} note{}", plural(n))
                }
                PromptKind::RemoveTag => {
                    if text.is_empty() {
                        return Ok("no tags given".into());
                    }
                    let nids = self.engine.note_ids_for_cards(&cids)?;
                    let n = self.engine.remove_tags(&nids, text)?;
                    format!("untagged {n} note{}", plural(n))
                }
                PromptKind::MoveDeck => {
                    if text.is_empty() {
                        return Ok("no deck given".into());
                    }
                    let Some(id) = self.engine.deck_id_by_name(text, false)? else {
                        return Err(anyhow::anyhow!("no deck named {text:?} (create it first)").into());
                    };
                    let n = self.engine.move_cards(&cids, id)?;
                    format!("moved {n} card{} to {text}", plural(n))
                }
                PromptKind::SetDue => {
                    if text.is_empty() {
                        return Ok("no due date given".into());
                    }
                    self.engine.set_due(&cids, text)?;
                    format!("set due date on {} card{}", cids.len(), plural(cids.len()))
                }
                PromptKind::ConfirmDelete(_) => {
                    let nids = self.engine.note_ids_for_cards(&cids)?;
                    let n = self.engine.delete_notes(&nids)?;
                    format!("deleted {n} note{}", plural(n))
                }
                PromptKind::ConfirmForget(_) => {
                    self.engine.forget_cards(&cids)?;
                    format!("reset {} card{} to new", cids.len(), plural(cids.len()))
                }
            })
        })();
        match res {
            Ok(msg) => {
                self.info(msg);
                self.after_bulk();
            }
            Err(e) => self.error(e.to_string()),
        }
    }

    fn prompt(&mut self, kind: PromptKind) {
        if self.view != View::Browser {
            return;
        }
        self.cmdline.clear();
        self.mode = Mode::Prompt(kind);
    }

    /// Refresh the browser (and leave visual mode) after a mutation.
    fn after_bulk(&mut self) {
        if self.mode == Mode::Visual {
            self.mode = Mode::Normal;
        }
        if let Some(b) = self.browser.as_mut() {
            b.anchor = None;
            b.refresh(&mut self.engine);
        }
    }

    fn bulk(&mut self, f: impl FnOnce(&mut Engine, &[i64]) -> Result<String>) {
        let Some(b) = self.browser.as_ref() else { return };
        let cids = b.targets();
        if cids.is_empty() {
            return;
        }
        match f(&mut self.engine, &cids) {
            Ok(msg) => {
                self.info(msg);
                self.after_bulk();
            }
            Err(e) => self.error(e.to_string()),
        }
    }

    fn open_browser(&mut self, query: String) {
        let mut b = BrowserView::new(query);
        b.run_search(&mut self.engine);
        self.browser = Some(b);
        self.view = View::Browser;
        self.mode = Mode::Normal;
    }

    fn on_key_normal(&mut self, key: Key) {
        // Count prefix: digits before a sequence (but `0` alone is a motion).
        // The review view binds digits to ratings, so no counts there.
        if self.pending.is_empty() && self.view != View::Review {
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
        if key.code == KeyCode::Esc && self.mode == Mode::Visual {
            self.mode = Mode::Normal;
            if let Some(b) = self.browser.as_mut() {
                b.anchor = None;
            }
            return;
        }
        self.pending.push(key);
        self.pending_since = Some(Instant::now());
        match self.keymap().lookup(&self.pending) {
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
            ("undo" | "u", _) => self.dispatch(Action::Undo),
            ("bury", _) => self.dispatch(Action::Bury),
            ("suspend", _) => self.dispatch(Action::Suspend),
            ("unbury", _) => self.dispatch(Action::Unbury),
            ("flag", Some(n)) => match n.parse::<u8>() {
                Ok(n) if n <= 7 => self.dispatch(Action::Flag(n)),
                _ => self.error("usage: :flag 0-7"),
            },
            ("audio", Some("off")) => {
                self.audio.enabled = false;
                self.audio.stop();
                self.info("audio off");
            }
            ("audio", Some("on")) => {
                self.audio.enabled = true;
                self.info("audio on");
            }
            ("browse" | "b" | "search", _) => {
                let rest = cmd.split_once(' ').map(|x| x.1).unwrap_or("").to_string();
                self.open_browser(rest);
            }
            ("sort", Some(col)) => match ankh_core::SortBy::parse(col) {
                Some(sb) => {
                    if let Some(b) = self.browser.as_mut() {
                        b.sort = sb;
                        b.refresh(&mut self.engine);
                    }
                }
                None => self.error(format!("unknown sort column {col:?}")),
            },
            ("tag", Some(_)) => {
                let rest = cmd.split_once(' ').map(|x| x.1).unwrap_or("").to_string();
                self.finish_prompt(PromptKind::AddTag, &rest);
            }
            ("untag", Some(_)) => {
                let rest = cmd.split_once(' ').map(|x| x.1).unwrap_or("").to_string();
                self.finish_prompt(PromptKind::RemoveTag, &rest);
            }
            ("move", Some(_)) => {
                let rest = cmd.split_once(' ').map(|x| x.1).unwrap_or("").to_string();
                self.finish_prompt(PromptKind::MoveDeck, &rest);
            }
            ("due", Some(d)) => self.finish_prompt(PromptKind::SetDue, d),
            ("delete", _) => self.dispatch(Action::ConfirmDelete),
            ("forget", _) => self.dispatch(Action::ConfirmForget),
            ("info", _) => self.dispatch(Action::CardInfo),
            ("edit" | "e", _) => self.dispatch(Action::EditNote),
            ("add" | "a", _) => self.dispatch(Action::AddNote),
            _ => self.error(format!("not a command: {cmd}")),
        }
    }

    fn dispatch(&mut self, action: Action) {
        match action {
            Action::Quit => {
                if self.view == View::Review {
                    self.leave_review();
                    return;
                }
                if self.view == View::Browser {
                    if self.browser.as_ref().map(|b| b.info.is_some()).unwrap_or(false) {
                        self.browser.as_mut().unwrap().info = None;
                        return;
                    }
                    self.view = View::Decks;
                    self.mode = Mode::Normal;
                    self.refresh();
                    return;
                }
                if self.creds.is_some() && self.sync.is_none() {
                    self.start_sync(SyncOp::Normal, true);
                } else if self.sync.is_some() {
                    self.sync.as_mut().unwrap().quit_after = true;
                } else {
                    self.should_quit = true;
                }
            }
            Action::ForceQuit => self.should_quit = true,
            Action::Down => match self.view {
                View::Decks => self.decks.move_by(1),
                View::Review => self.dispatch(Action::ScrollDown),
                View::Browser => {
                    if let Some(b) = self.browser.as_mut() {
                        b.move_by(1);
                    }
                }
            },
            Action::Up => match self.view {
                View::Decks => self.decks.move_by(-1),
                View::Review => self.dispatch(Action::ScrollUp),
                View::Browser => {
                    if let Some(b) = self.browser.as_mut() {
                        b.move_by(-1);
                    }
                }
            },
            Action::Top => match self.view {
                View::Decks => self.decks.go_top(),
                View::Review => {
                    if let Some(r) = self.review.as_mut() {
                        r.scroll = 0;
                    }
                }
                View::Browser => {
                    if let Some(b) = self.browser.as_mut() {
                        b.selected = 0;
                    }
                }
            },
            Action::Bottom => match self.view {
                View::Decks => self.decks.go_bottom(),
                View::Review => {
                    if let Some(r) = self.review.as_mut() {
                        r.scroll = u16::MAX / 2;
                    }
                }
                View::Browser => {
                    if let Some(b) = self.browser.as_mut() {
                        b.selected = b.ids.len().saturating_sub(1);
                    }
                }
            },
            Action::HalfDown => match self.view {
                View::Browser => {
                    if let Some(b) = self.browser.as_mut() {
                        b.move_by(15);
                    }
                }
                _ => self.decks.move_by(10),
            },
            Action::HalfUp => match self.view {
                View::Browser => {
                    if let Some(b) = self.browser.as_mut() {
                        b.move_by(-15);
                    }
                }
                _ => self.decks.move_by(-10),
            },
            Action::Expand => self.decks.expand(),
            Action::Collapse => self.decks.collapse(),
            Action::ToggleFold => self.decks.toggle_fold(),
            Action::Open => self.open_deck(),
            Action::ShowAnswer => self.show_answer(),
            Action::Continue => {
                let shown = self.review.as_ref().map(|r| r.answer_shown()).unwrap_or(false);
                if shown {
                    self.rate(Rating::Good);
                } else {
                    self.show_answer();
                }
            }
            Action::Rate(r) => {
                let shown = self.review.as_ref().map(|r| r.answer_shown()).unwrap_or(false);
                if shown {
                    self.rate(r);
                } else {
                    self.show_answer();
                }
            }
            Action::Undo => match self.engine.undo() {
                Ok(Some(what)) => {
                    self.info(format!("undid: {what}"));
                    if self.view == View::Review {
                        self.advance();
                    } else {
                        self.refresh();
                    }
                }
                Ok(None) => self.info("nothing to undo"),
                Err(e) => self.error(e.to_string()),
            },
            Action::Bury => self.with_current_card(|app, c| {
                app.engine.bury(c.card_id)?;
                Ok("card buried".into())
            }),
            Action::Suspend => self.with_current_card(|app, c| {
                app.engine.suspend(c.card_id)?;
                Ok("card suspended".into())
            }),
            Action::Flag(n) => {
                let Some(card) = self.review.as_ref().and_then(|r| r.card.clone()) else { return };
                let new = if card.flag == n { 0 } else { n };
                match self.engine.set_flag(card.card_id, new) {
                    Ok(()) => {
                        if let Some(c) = self.review.as_mut().and_then(|r| r.card.as_mut()) {
                            c.flag = new;
                        }
                    }
                    Err(e) => self.error(e.to_string()),
                }
            }
            Action::ToggleMark => {
                let Some(card) = self.review.as_ref().and_then(|r| r.card.clone()) else { return };
                match self.engine.toggle_marked(card.note_id) {
                    Ok(marked) => {
                        if let Some(c) = self.review.as_mut().and_then(|r| r.card.as_mut()) {
                            if marked {
                                c.tags.push("marked".into());
                            } else {
                                c.tags.retain(|t| !t.eq_ignore_ascii_case("marked"));
                            }
                        }
                    }
                    Err(e) => self.error(e.to_string()),
                }
            }
            Action::Replay => {
                let Some(rv) = self.review.as_ref() else { return };
                let av = match (&rv.stage, &rv.card) {
                    (Stage::Answer, Some(c)) => c.answer_av.clone(),
                    (_, Some(c)) => c.question_av.clone(),
                    _ => vec![],
                };
                self.play(&av);
            }
            Action::Unbury => match self.engine.unbury_current_deck() {
                Ok(()) => {
                    self.info("unburied");
                    if self.view == View::Review {
                        self.advance();
                    }
                }
                Err(e) => self.error(e.to_string()),
            },
            Action::Back => match self.view {
                View::Review => self.leave_review(),
                View::Browser => {
                    self.view = View::Decks;
                    self.mode = Mode::Normal;
                    self.refresh();
                }
                View::Decks => {}
            },
            Action::ScrollDown => {
                if let Some(r) = self.review.as_mut() {
                    r.scroll_by(1);
                }
            }
            Action::ScrollUp => {
                if let Some(r) = self.review.as_mut() {
                    r.scroll_by(-1);
                }
            }
            Action::ToggleHints => {
                if let Some(r) = self.review.as_mut() {
                    r.reveal_hints = !r.reveal_hints;
                    r.rerender();
                }
            }
            Action::OpenBrowser => {
                let q = self.browser.as_ref().map(|b| b.query.clone()).unwrap_or_default();
                self.open_browser(q);
            }
            Action::BrowseDeck => {
                let q = match self.view {
                    View::Decks => {
                        self.decks.selected_deck().map(|d| format!("deck:\"{}\"", d.full_name)).unwrap_or_default()
                    }
                    View::Review => {
                        self.review.as_ref().map(|r| format!("deck:\"{}\"", r.deck_name)).unwrap_or_default()
                    }
                    View::Browser => String::new(),
                };
                self.open_browser(q);
            }
            Action::InsertMode => {
                if self.view == View::Browser {
                    self.mode = Mode::Insert;
                }
            }
            Action::VisualMode => {
                if let Some(b) = self.browser.as_mut() {
                    if self.mode == Mode::Visual {
                        self.mode = Mode::Normal;
                        b.anchor = None;
                    } else {
                        self.mode = Mode::Visual;
                        b.anchor = Some(b.selected);
                    }
                }
            }
            Action::ClearSearch => {
                if let Some(b) = self.browser.as_mut() {
                    b.clear_input();
                    self.mode = Mode::Insert;
                }
            }
            Action::Preview => {
                if let Some(b) = self.browser.as_mut() {
                    b.preview = !b.preview;
                }
            }
            Action::FlipPreview => {
                if let Some(b) = self.browser.as_mut() {
                    b.preview_answer = !b.preview_answer;
                }
            }
            Action::CardInfo => {
                let id = match self.view {
                    View::Browser => self.browser.as_ref().and_then(|b| b.current_id()),
                    View::Review => self.review.as_ref().and_then(|r| r.card.as_ref().map(|c| c.card_id)),
                    View::Decks => None,
                };
                let Some(id) = id else { return };
                if let Some(b) = self.browser.as_mut() {
                    if b.info.is_some() {
                        b.info = None;
                        return;
                    }
                }
                match self.engine.card_info(id) {
                    Ok(info) => {
                        if let Some(b) = self.browser.as_mut() {
                            b.info = Some(info);
                        } else {
                            self.info(format!(
                                "{} · {}d interval · {} reviews · {} lapses{}",
                                info.deck,
                                info.interval_days,
                                info.reviews,
                                info.lapses,
                                info.stability.map(|s| format!(" · stability {s:.0}d")).unwrap_or_default()
                            ));
                        }
                    }
                    Err(e) => self.error(e.to_string()),
                }
            }
            Action::ToggleSuspend => self.bulk(|eng, cids| {
                let rows = eng.browser_rows(cids)?;
                let all_suspended = !rows.is_empty() && rows.iter().all(|r| r.state == ankh_core::CardState::Suspended);
                if all_suspended {
                    eng.unsuspend_cards(cids)?;
                    Ok(format!("unsuspended {} card{}", cids.len(), plural(cids.len())))
                } else {
                    let n = eng.suspend_cards(cids)?;
                    Ok(format!("suspended {n} card{}", plural(n)))
                }
            }),
            Action::BulkBury => self.bulk(|eng, cids| {
                let n = eng.bury_cards(cids)?;
                Ok(format!("buried {n} card{}", plural(n)))
            }),
            Action::BulkFlag(n) => self.bulk(move |eng, cids| {
                eng.flag_cards(cids, n)?;
                Ok(if n == 0 {
                    "flag cleared".to_string()
                } else {
                    format!("flagged {} card{}", cids.len(), plural(cids.len()))
                })
            }),
            Action::BulkMark => self.bulk(|eng, cids| {
                let nids = eng.note_ids_for_cards(cids)?;
                let rows = eng.browser_rows(cids)?;
                if rows.iter().all(|r| r.marked) {
                    eng.remove_tags(&nids, "marked")?;
                    Ok("unmarked".into())
                } else {
                    eng.add_tags(&nids, "marked")?;
                    Ok("marked".into())
                }
            }),
            Action::PromptTag => self.prompt(PromptKind::AddTag),
            Action::PromptUntag => self.prompt(PromptKind::RemoveTag),
            Action::PromptMove => self.prompt(PromptKind::MoveDeck),
            Action::PromptDue => self.prompt(PromptKind::SetDue),
            Action::ConfirmDelete => {
                let n = self.browser.as_ref().map(|b| b.targets().len()).unwrap_or(0);
                if n > 0 {
                    self.prompt(PromptKind::ConfirmDelete(n));
                }
            }
            Action::ConfirmForget => {
                let n = self.browser.as_ref().map(|b| b.targets().len()).unwrap_or(0);
                if n > 0 {
                    self.prompt(PromptKind::ConfirmForget(n));
                }
            }
            Action::CycleSort => {
                if let Some(b) = self.browser.as_mut() {
                    b.cycle_sort(&mut self.engine);
                }
            }
            Action::ReverseSort => {
                if let Some(b) = self.browser.as_mut() {
                    b.toggle_reverse(&mut self.engine);
                }
            }
            Action::EditNote => {
                let note_id = match self.view {
                    View::Review => self.review.as_ref().and_then(|r| r.card.as_ref().map(|c| c.note_id)),
                    View::Browser => self.browser.as_ref().and_then(|b| b.current().map(|r| r.note_id)),
                    View::Decks => None,
                };
                match note_id {
                    Some(id) => self.edit_request = Some(EditRequest::Existing { note_id: id }),
                    None => self.info("nothing to edit here"),
                }
            }
            Action::AddNote => {
                let deck = match self.view {
                    View::Decks => self.decks.selected_deck().map(|d| d.full_name.clone()),
                    View::Review => self.review.as_ref().map(|r| r.deck_name.clone()),
                    View::Browser => self.browser.as_ref().and_then(|b| b.current().map(|r| r.deck.clone())),
                }
                .unwrap_or_else(|| "Default".into());
                self.edit_request = Some(EditRequest::New { deck });
            }
            Action::StudyCard => {
                // Open the current row's deck in review.
                let Some(row) = self.browser.as_ref().and_then(|b| b.current().cloned()) else { return };
                if let Err(e) = self.engine.select_deck(row.deck_id) {
                    self.error(e.to_string());
                    return;
                }
                let mut rv = ReviewView::new(row.deck, self.engine.paths().media_folder());
                rv.session_started = Instant::now();
                self.review = Some(rv);
                self.view = View::Review;
                self.advance();
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

        match self.view {
            View::Decks => self.decks.draw(f, chunks[0], &theme, self.prompt.is_none()),
            View::Review => {
                if let Some(r) = self.review.as_mut() {
                    r.draw(f, chunks[0], &theme, &mut self.images);
                }
            }
            View::Browser => {
                if let Some(b) = self.browser.as_mut() {
                    b.draw(f, chunks[0], &theme, &mut self.engine, self.mode == Mode::Insert);
                }
            }
        }
        self.draw_statusline(f, chunks[1], &theme);
        self.draw_cmdline(f, chunks[2], &theme);

        if let Match::Prefix(next) = self.keymap().lookup(&self.pending) {
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
            (Mode::Prompt(k), _) => {
                Line::from(vec![Span::styled(k.label(), theme.accent()), Span::raw(self.cmdline.clone())])
            }
            (_, Some((Message::Error(m), _))) => {
                Line::from(Span::styled(format!(" {m}"), Style::default().fg(theme.error)))
            }
            (_, Some((Message::Info(m), _))) => Line::from(Span::styled(format!(" {m}"), theme.muted())),
            _ => Line::default(),
        };
        f.render_widget(Paragraph::new(line), area);
        match &self.mode {
            Mode::Command => f.set_cursor_position((area.x + 1 + self.cmdline.chars().count() as u16, area.y)),
            Mode::Prompt(k) => f.set_cursor_position((
                area.x + (k.label().chars().count() + self.cmdline.chars().count()) as u16,
                area.y,
            )),
            _ => {}
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
            .keymap()
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
        lines.push(Line::from(Span::styled(" :sync [download|upload]  :refresh  :undo  :q  :q!", theme.muted())));
        lines.push(Line::from(Span::styled(" :flag N  :bury  :suspend  :unbury  :audio on|off", theme.muted())));
        lines.push(Line::from(Span::styled(
            " :browse QUERY  :sort COL  :tag T  :untag T  :move DECK  :due N",
            theme.muted(),
        )));
        let rect = centered(area, 52, lines.len() as u16 + 2);
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

fn default_keymaps() -> HashMap<View, Keymap<Action>> {
    let mut global = Keymap::default();
    global
        .bind("j", Action::Down, "down")
        .bind("<Down>", Action::Down, "down")
        .bind("k", Action::Up, "up")
        .bind("<Up>", Action::Up, "up")
        .bind("gg", Action::Top, "top")
        .bind("G", Action::Bottom, "bottom")
        .bind("S", Action::Sync, "sync")
        .bind("<leader>ss", Action::Sync, "sync")
        .bind("<leader>sd", Action::SyncDownload, "sync: full download")
        .bind("<leader>su", Action::SyncUpload, "sync: full upload")
        .bind("u", Action::Undo, "undo")
        .bind(":", Action::CommandMode, "command line")
        .bind("?", Action::Help, "help")
        .bind("q", Action::Quit, "quit / back")
        .bind("ZZ", Action::Quit, "quit (syncs first)")
        .bind("ZQ", Action::ForceQuit, "quit without syncing")
        .bind("<Esc>", Action::ClearMessage, "clear message");

    let mut decks = global.clone();
    decks
        .bind("/", Action::BrowseDeck, "browse this deck")
        .bind("b", Action::OpenBrowser, "browser")
        .bind("a", Action::AddNote, "add note to deck")
        .bind("<C-d>", Action::HalfDown, "half page down")
        .bind("<C-u>", Action::HalfUp, "half page up")
        .bind("l", Action::Expand, "expand")
        .bind("h", Action::Collapse, "collapse / parent")
        .bind("za", Action::ToggleFold, "toggle fold")
        .bind("<CR>", Action::Open, "study deck")
        .bind("R", Action::Refresh, "refresh");

    let mut review = global.clone();
    review
        .bind("<Space>", Action::Continue, "show answer / good")
        .bind("<CR>", Action::Continue, "show answer / good")
        .bind("l", Action::ShowAnswer, "show answer")
        .bind("1", Action::Rate(Rating::Again), "again")
        .bind("2", Action::Rate(Rating::Hard), "hard")
        .bind("3", Action::Rate(Rating::Good), "good")
        .bind("4", Action::Rate(Rating::Easy), "easy")
        .bind("a", Action::Rate(Rating::Again), "again")
        .bind("h", Action::Rate(Rating::Hard), "hard")
        .bind("g", Action::Rate(Rating::Good), "good")
        .bind("e", Action::Rate(Rating::Easy), "easy")
        .bind("-", Action::Bury, "bury card")
        .bind("!", Action::Suspend, "suspend card")
        .bind("*", Action::ToggleMark, "mark / unmark note")
        .bind("r", Action::Replay, "replay audio")
        .bind("H", Action::ToggleHints, "reveal / hide hints")
        .bind("i", Action::CardInfo, "card info")
        .bind("/", Action::BrowseDeck, "browse this deck")
        .bind("<leader>e", Action::EditNote, "edit note in $EDITOR")
        .bind("<leader>a", Action::AddNote, "add note to deck")
        .bind("U", Action::Unbury, "unbury deck")
        .bind("<BS>", Action::Back, "back to decks")
        .bind("<C-d>", Action::ScrollDown, "scroll down")
        .bind("<C-u>", Action::ScrollUp, "scroll up")
        .bind("<leader>0", Action::Flag(0), "clear flag")
        .bind("<leader>1", Action::Flag(1), "flag red")
        .bind("<leader>2", Action::Flag(2), "flag orange")
        .bind("<leader>3", Action::Flag(3), "flag green")
        .bind("<leader>4", Action::Flag(4), "flag blue")
        .bind("<leader>5", Action::Flag(5), "flag pink")
        .bind("<leader>6", Action::Flag(6), "flag turquoise")
        .bind("<leader>7", Action::Flag(7), "flag purple");
    // `gg` conflicts with `g` (good); in review `g` wins and top/bottom use G only.
    review.unbind(&super::keys::parse_seq("gg").unwrap());

    let mut browser = global.clone();
    browser
        .bind("/", Action::InsertMode, "edit search")
        .bind("i", Action::InsertMode, "edit search")
        .bind("<C-l>", Action::ClearSearch, "new search")
        .bind("v", Action::VisualMode, "visual select")
        .bind("V", Action::VisualMode, "visual select")
        .bind("<C-d>", Action::HalfDown, "half page down")
        .bind("<C-u>", Action::HalfUp, "half page up")
        .bind("p", Action::Preview, "toggle preview")
        .bind("<Tab>", Action::FlipPreview, "preview question / answer")
        .bind("<CR>", Action::StudyCard, "study this card's deck")
        .bind("I", Action::CardInfo, "card info")
        .bind("e", Action::EditNote, "edit note in $EDITOR")
        .bind("a", Action::AddNote, "add note")
        .bind("!", Action::ToggleSuspend, "suspend / unsuspend")
        .bind("-", Action::BulkBury, "bury")
        .bind("*", Action::BulkMark, "mark / unmark")
        .bind("t", Action::PromptTag, "add tags")
        .bind("T", Action::PromptUntag, "remove tags")
        .bind("m", Action::PromptMove, "move to deck")
        .bind("d", Action::PromptDue, "set due date")
        .bind("D", Action::ConfirmDelete, "delete notes")
        .bind("F", Action::ConfirmForget, "forget (reset to new)")
        .bind("o", Action::CycleSort, "next sort column")
        .bind("O", Action::ReverseSort, "reverse sort")
        .bind("<BS>", Action::Back, "back to decks")
        .bind("<leader>0", Action::BulkFlag(0), "clear flag")
        .bind("<leader>1", Action::BulkFlag(1), "flag red")
        .bind("<leader>2", Action::BulkFlag(2), "flag orange")
        .bind("<leader>3", Action::BulkFlag(3), "flag green")
        .bind("<leader>4", Action::BulkFlag(4), "flag blue")
        .bind("<leader>5", Action::BulkFlag(5), "flag pink")
        .bind("<leader>6", Action::BulkFlag(6), "flag turquoise")
        .bind("<leader>7", Action::BulkFlag(7), "flag purple");

    HashMap::from([(View::Decks, decks), (View::Review, review), (View::Browser, browser)])
}

fn plural(n: usize) -> &'static str {
    if n == 1 {
        ""
    } else {
        "s"
    }
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
