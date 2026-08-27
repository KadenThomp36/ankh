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

use std::cell::{RefCell, RefMut};
use std::collections::HashMap;
use std::rc::Rc;

use super::audio::Player;
use super::images::Images;
use super::keys::{format_seq, Key, Keymap, Match};
use super::theme::Theme;
use super::views::browser::BrowserView;
use super::views::decks::DecksView;
use super::views::help::HelpView;
use super::views::review::{ReviewView, Stage};
use super::views::stats::StatsView;
use crate::lua::{Config, Request, Runtime, Snapshot};
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
    // management
    Stats,
    StatsAll,
    DeckOptions,
    PromptNewDeck,
    PromptRenameDeck,
    ConfirmDeleteDeck,
    FsrsOptimize,
    /// A Lua function registered through `ankh.keymap.set`.
    Lua(usize),
}

impl Action {
    /// Parse the names used by `ankh.keymap.set` / `ankh.action`.
    pub fn from_name(s: &str) -> Option<Action> {
        let mut it = s.split_whitespace();
        let head = it.next()?;
        let arg = it.next();
        let num = || arg.and_then(|a| a.parse::<u8>().ok());
        Some(match head {
            "quit" => Action::Quit,
            "force_quit" => Action::ForceQuit,
            "down" => Action::Down,
            "up" => Action::Up,
            "top" => Action::Top,
            "bottom" => Action::Bottom,
            "half_down" => Action::HalfDown,
            "half_up" => Action::HalfUp,
            "expand" => Action::Expand,
            "collapse" => Action::Collapse,
            "toggle_fold" => Action::ToggleFold,
            "open" => Action::Open,
            "sync" => Action::Sync,
            "sync_download" => Action::SyncDownload,
            "sync_upload" => Action::SyncUpload,
            "refresh" => Action::Refresh,
            "command_mode" => Action::CommandMode,
            "help" => Action::Help,
            "clear_message" => Action::ClearMessage,
            "show_answer" => Action::ShowAnswer,
            "continue" => Action::Continue,
            "rate" => Action::Rate(match arg? {
                "again" | "1" => Rating::Again,
                "hard" | "2" => Rating::Hard,
                "good" | "3" => Rating::Good,
                "easy" | "4" => Rating::Easy,
                _ => return None,
            }),
            "undo" => Action::Undo,
            "bury" => Action::Bury,
            "suspend" => Action::Suspend,
            "toggle_mark" => Action::ToggleMark,
            "flag" => Action::Flag(num().filter(|n| *n <= 7)?),
            "replay" => Action::Replay,
            "unbury" => Action::Unbury,
            "back" => Action::Back,
            "scroll_down" => Action::ScrollDown,
            "scroll_up" => Action::ScrollUp,
            "toggle_hints" => Action::ToggleHints,
            "open_browser" => Action::OpenBrowser,
            "browse_deck" => Action::BrowseDeck,
            "insert_mode" => Action::InsertMode,
            "visual_mode" => Action::VisualMode,
            "clear_search" => Action::ClearSearch,
            "preview" => Action::Preview,
            "flip_preview" => Action::FlipPreview,
            "card_info" => Action::CardInfo,
            "toggle_suspend" => Action::ToggleSuspend,
            "bulk_bury" => Action::BulkBury,
            "bulk_flag" => Action::BulkFlag(num().filter(|n| *n <= 7)?),
            "bulk_mark" => Action::BulkMark,
            "prompt_tag" => Action::PromptTag,
            "prompt_untag" => Action::PromptUntag,
            "prompt_move" => Action::PromptMove,
            "prompt_due" => Action::PromptDue,
            "confirm_delete" => Action::ConfirmDelete,
            "confirm_forget" => Action::ConfirmForget,
            "cycle_sort" => Action::CycleSort,
            "reverse_sort" => Action::ReverseSort,
            "study_card" => Action::StudyCard,
            "edit_note" => Action::EditNote,
            "add_note" => Action::AddNote,
            "stats" => Action::Stats,
            "stats_all" => Action::StatsAll,
            "deck_options" => Action::DeckOptions,
            "new_deck" => Action::PromptNewDeck,
            "rename_deck" => Action::PromptRenameDeck,
            "delete_deck" => Action::ConfirmDeleteDeck,
            "fsrs_optimize" => Action::FsrsOptimize,
            _ => return None,
        })
    }

    pub const NAMES: &'static [&'static str] = &[
        "quit",
        "force_quit",
        "down",
        "up",
        "top",
        "bottom",
        "half_down",
        "half_up",
        "expand",
        "collapse",
        "toggle_fold",
        "open",
        "sync",
        "sync_download",
        "sync_upload",
        "refresh",
        "command_mode",
        "help",
        "clear_message",
        "show_answer",
        "continue",
        "rate again|hard|good|easy",
        "undo",
        "bury",
        "suspend",
        "toggle_mark",
        "flag 0-7",
        "replay",
        "unbury",
        "back",
        "scroll_down",
        "scroll_up",
        "toggle_hints",
        "open_browser",
        "browse_deck",
        "insert_mode",
        "visual_mode",
        "clear_search",
        "preview",
        "flip_preview",
        "card_info",
        "toggle_suspend",
        "bulk_bury",
        "bulk_flag 0-7",
        "bulk_mark",
        "prompt_tag",
        "prompt_untag",
        "prompt_move",
        "prompt_due",
        "confirm_delete",
        "confirm_forget",
        "cycle_sort",
        "reverse_sort",
        "study_card",
        "edit_note",
        "add_note",
    ];
}

/// Something to do with the terminal released (run `$EDITOR`).
enum EditRequest {
    Existing { note_id: i64 },
    New { deck: String },
    Options { deck: ankh_core::DeckId },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum View {
    Decks,
    Review,
    Browser,
    Stats,
    Help,
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
    NewDeck,
    RenameDeck(ankh_core::DeckId, String),
    ConfirmDeleteDeck(ankh_core::DeckId, String, u32),
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
            PromptKind::NewDeck => "new deck (Parent::Child nests): ".into(),
            PromptKind::RenameDeck(_, _) => "rename deck to: ".into(),
            PromptKind::ConfirmDeleteDeck(_, name, n) => format!("delete {name} and its {n} cards? (y/N) "),
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
    /// Shared with the Lua runtime; never hold the borrow across a call
    /// into another `App` method (they borrow it too).
    engine: Rc<RefCell<Engine>>,
    creds: Option<Credentials>,
    theme: Theme,
    mode: Mode,
    view: View,
    decks: DecksView,
    review: Option<ReviewView>,
    browser: Option<BrowserView>,
    stats: Option<StatsView>,
    help: Option<HelpView>,
    audio: Player,
    images: Images,
    keymaps: HashMap<View, Keymap<Action>>,
    pending: Vec<Key>,
    count: Option<usize>,
    pending_since: Option<Instant>,
    /// An exact match that is waiting in case a longer sequence follows
    /// (`<Space>s` stats vs `<Space>ss` sync); fires on timeout.
    pending_exact: Option<Action>,
    cmdline: String,
    message: Option<(Message, Instant)>,
    prompt: Option<Prompt>,
    sync: Option<SyncState>,
    last_sync: Option<Instant>,
    show_help: bool,
    should_quit: bool,
    tick: u64,
    edit_request: Option<EditRequest>,
    lua: Runtime,
    config: Config,
}

impl App {
    pub fn new(paths: Paths, images: Images) -> Result<Self> {
        let store = AuthStore::new(&paths.profile);
        let creds = store.load()?;
        let engine = Rc::new(RefCell::new(Engine::open(paths)?));
        let init_lua = engine.borrow().paths().init_lua();
        let lua = Runtime::new(engine.clone(), Some(init_lua));
        let config = lua.config();
        let lua_errors = lua.errors();
        let mut theme = Theme::by_name(&config.theme).unwrap_or_else(Theme::tokyonight);
        theme.dark = match config.background.as_str() {
            "light" => false,
            "dark" => true,
            _ => super::theme::terminal_is_dark(),
        };
        let mut audio = Player::new();
        audio.enabled = config.audio_autoplay;
        let (maps, deletions) = lua.keymaps();
        let keymaps = compose_keymaps(&maps, &deletions);
        let mut app = App {
            engine,
            creds,
            theme,
            mode: Mode::Normal,
            view: View::Decks,
            decks: DecksView::default(),
            review: None,
            browser: None,
            stats: None,
            help: None,
            audio,
            images,
            keymaps,
            pending: Vec::new(),
            count: None,
            pending_since: None,
            pending_exact: None,
            cmdline: String::new(),
            message: None,
            prompt: None,
            sync: None,
            last_sync: None,
            show_help: false,
            should_quit: false,
            tick: 0,
            edit_request: None,
            lua,
            config,
        };
        app.refresh();
        if Theme::by_name(&app.config.theme).is_none() {
            app.error(format!("unknown theme {:?} (have: {})", app.config.theme, Theme::NAMES.join(", ")));
        }
        for e in lua_errors {
            app.error(format!("config: {e}"));
        }
        app.emit("startup", |_| Ok(mlua::Value::Nil));
        if !app.audio.available() {
            app.info("no audio player found — install mpv for card audio");
        }
        tracing::info!(images = app.images.protocol_name(), "image protocol");
        Ok(app)
    }

    pub fn run(mut self, mut terminal: DefaultTerminal) -> anyhow::Result<()> {
        // Sync on launch (when logged in). Never blocks the UI.
        if self.creds.is_none() {
            self.info("not logged in — run `ankh login` in a shell to enable sync");
        } else if self.config.sync_on_launch {
            self.start_sync(SyncOp::Normal, false);
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
        if let Ok(cell) = Rc::try_unwrap(self.engine) {
            cell.into_inner().close()?;
        }
        Ok(())
    }

    // ----- state helpers ---------------------------------------------------

    fn eng(&self) -> RefMut<'_, Engine> {
        self.engine.borrow_mut()
    }

    fn info(&mut self, s: impl Into<String>) {
        self.message = Some((Message::Info(s.into()), Instant::now()));
    }

    fn error(&mut self, s: impl Into<String>) {
        self.message = Some((Message::Error(s.into()), Instant::now()));
    }

    fn refresh(&mut self) {
        let tree = self.eng().deck_tree();
        match tree {
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
        let media = self.config.sync_media;
        let started = self.eng().sync_in_background(creds, op, SyncOptions { media });
        match started {
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
            if since.elapsed() > Duration::from_millis(self.config.timeoutlen_ms) && !self.pending.is_empty() {
                // neovim's timeoutlen: fire the shorter mapping if there was
                // one, otherwise give up on the sequence.
                self.pending.clear();
                self.pending_since = None;
                if let Some(action) = self.pending_exact.take() {
                    let n = self.count.take().unwrap_or(1);
                    for _ in 0..n.min(500) {
                        self.dispatch(action);
                    }
                }
            }
        }
        if self.sync.as_ref().map(|s| s.handle.is_finished()).unwrap_or(false) {
            let SyncState { handle, quit_after, .. } = self.sync.take().unwrap();
            let finished = self.eng().finish_background(handle);
            match finished {
                Ok((report, creds)) => {
                    if self.creds.as_ref() != Some(&creds) {
                        let _ = AuthStore::new(&self.eng().paths().profile).save(&creds);
                        self.creds = Some(creds);
                    }
                    self.last_sync = Some(Instant::now());
                    match report.outcome {
                        SyncOutcome::NoChanges => self.info("already in sync"),
                        SyncOutcome::Synced => self.info("synced"),
                        SyncOutcome::FullDownloaded => self.info("downloaded collection from AnkiWeb"),
                        SyncOutcome::FullUploaded => self.info("uploaded collection to AnkiWeb"),
                        SyncOutcome::FullSyncRequired { upload_ok, download_ok } => {
                            if download_ok && self.eng().is_pristine().unwrap_or(false) {
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
            self.emit("sync_done", |_| Ok(mlua::Value::Nil));
            if quit_after {
                self.should_quit = true;
            }
        }
    }

    fn run_editor(&mut self, req: EditRequest) -> std::result::Result<String, String> {
        use crate::editor;
        match req {
            EditRequest::Existing { note_id } => {
                let doc = self.eng().note_doc(note_id).map_err(|e| e.to_string())?;
                let text = ankh_core::notefile::write(&[doc]);
                let Some(edited) = editor::edit_text(&text, &format!("note-{note_id}")).map_err(|e| e.to_string())?
                else {
                    return Ok("no changes".into());
                };
                let r = editor::save_note_file(&mut self.eng(), &edited).map_err(|e| e.to_string())?;
                self.after_edit();
                Ok(format!("saved note {note_id}{}", if r.updated == 1 { "" } else { " (+ more)" }))
            }
            EditRequest::Options { deck } => {
                let (opts, info) = self.eng().deck_options(deck).map_err(|e| e.to_string())?;
                let text = opts.to_toml(&info);
                let Some(edited) = editor::edit_text(&text, "options").map_err(|e| e.to_string())? else {
                    return Ok("no changes".into());
                };
                let new = ankh_core::DeckOptions::from_toml(&edited).map_err(|e| e.to_string())?;
                self.eng().save_deck_options(deck, &new).map_err(|e| e.to_string())?;
                self.refresh();
                Ok(format!("saved preset {:?}", new.preset))
            }
            EditRequest::New { deck } => {
                let text = editor::new_note_template(&mut self.eng(), None, &deck).map_err(|e| e.to_string())?;
                let Some(edited) = editor::edit_text(&text, "new").map_err(|e| e.to_string())? else {
                    return Ok("cancelled".into());
                };
                let body = editor::strip_leading_comments(&edited);
                if editor::is_blank(&body) {
                    return Ok("cancelled (empty note)".into());
                }
                let r = editor::save_note_file(&mut self.eng(), &body).map_err(|e| e.to_string())?;
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
                let fresh = self.eng().next_card();
                if let Some(rv) = self.review.as_mut() {
                    if let Some(card) = rv.card.clone() {
                        if let Ok(Some(fresh)) = fresh {
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
                let engine = self.engine.clone();
                if let Some(b) = self.browser.as_mut() {
                    b.refresh(&mut engine.borrow_mut());
                }
            }
            View::Decks | View::Stats | View::Help => self.refresh(),
        }
    }

    // ----- lua -----------------------------------------------------------------

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            view: match self.view {
                View::Decks => "decks",
                View::Review => "review",
                View::Browser => "browser",
                View::Stats => "stats",
                View::Help => "help",
            }
            .into(),
            mode: self.mode.label().to_lowercase(),
            card: self.review.as_ref().and_then(|r| r.card.clone()),
            answer_shown: self.review.as_ref().map(|r| r.answer_shown()).unwrap_or(false),
            deck: match self.view {
                View::Decks => self.decks.selected_deck().map(|d| d.full_name.clone()),
                View::Review => self.review.as_ref().map(|r| r.deck_name.clone()),
                View::Browser => self.browser.as_ref().and_then(|b| b.current().map(|r| r.deck.clone())),
                View::Stats | View::Help => None,
            },
        }
    }

    /// Fire a Lua event, then act on whatever the handlers asked for.
    fn emit(&mut self, event: &str, payload: impl FnOnce(&mlua::Lua) -> mlua::Result<mlua::Value>) {
        self.lua.set_snapshot(self.snapshot());
        self.lua.emit(event, payload);
        self.drain_requests();
    }

    fn drain_requests(&mut self) {
        for r in self.lua.take_requests() {
            match r {
                Request::Action(a) => self.dispatch(a),
                Request::Command(c) => self.run_command(&c),
                Request::Notify(m) => self.info(m),
                Request::Error(m) => self.error(m),
                Request::Browse(q) => self.open_browser(q),
            }
        }
    }

    fn emit_card(&mut self, event: &str, extra: Option<(&'static str, String)>) {
        let Some(card) = self.review.as_ref().and_then(|r| r.card.clone()) else { return };
        let lua_card = self.lua.card_table(&card);
        self.emit(event, move |_| {
            let t = lua_card?;
            if let Some((k, v)) = extra {
                t.set(k, v)?;
            }
            Ok(mlua::Value::Table(t))
        });
    }

    fn undo_result(&self) -> Result<Option<String>> {
        self.eng().undo()
    }

    fn unbury_result(&self) -> Result<()> {
        self.eng().unbury_current_deck()
    }

    fn keymap(&self) -> &Keymap<Action> {
        static EMPTY: std::sync::OnceLock<Keymap<Action>> = std::sync::OnceLock::new();
        self.keymaps.get(&self.view).unwrap_or_else(|| EMPTY.get_or_init(Keymap::default))
    }

    // ----- review flow -------------------------------------------------------

    fn open_deck(&mut self) {
        let Some(d) = self.decks.selected_deck() else { return };
        let (id, name) = (d.id, d.full_name.clone());
        let selected = self.eng().select_deck(id);
        if let Err(e) = selected {
            self.error(e.to_string());
            return;
        }
        let mut rv = ReviewView::new(name, self.eng().paths().media_folder());
        rv.session_started = Instant::now();
        self.review = Some(rv);
        self.view = View::Review;
        self.advance();
    }

    /// Load the next card (or the "done" screen) into the review view.
    fn advance(&mut self) {
        let next = self.eng().next_card();
        match next {
            Ok(Some(card)) => {
                let av = card.question_av.clone();
                if let Some(rv) = self.review.as_mut() {
                    rv.show_card(card);
                }
                self.play(&av);
                self.emit_card("card_shown", None);
            }
            Ok(None) => {
                self.audio.stop();
                let congrats = self.eng().congrats();
                match congrats {
                    Ok(c) => {
                        if let Some(rv) = self.review.as_mut() {
                            rv.finish(c);
                        }
                        self.emit("review_done", |_| Ok(mlua::Value::Nil));
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
                Av::File { name } => Some(self.eng().media_path(name)),
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
        self.emit_card("card_answered", Some(("rating", rating.label().to_string())));
        let answered = self.eng().answer(&card, rating, taken);
        match answered {
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
        let engine = self.engine.clone();
        let Some(b) = self.browser.as_mut() else {
            self.mode = Mode::Normal;
            return;
        };
        match key.code {
            KeyCode::Esc => self.mode = Mode::Normal,
            KeyCode::Enter => {
                b.run_search(&mut engine.borrow_mut());
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
                if matches!(
                    kind,
                    PromptKind::ConfirmDelete(_) | PromptKind::ConfirmForget(_) | PromptKind::ConfirmDeleteDeck(..)
                ) {
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
        match kind {
            PromptKind::NewDeck => {
                if text.is_empty() {
                    return;
                }
                let r = self.eng().create_deck(text);
                match r {
                    Ok(_) => {
                        self.info(format!("created {text}"));
                        self.refresh();
                    }
                    Err(e) => self.error(e.to_string()),
                }
                return;
            }
            PromptKind::RenameDeck(id, old) => {
                if text.is_empty() || text == old {
                    return;
                }
                let r = self.eng().rename_deck(id, text);
                match r {
                    Ok(()) => {
                        self.info(format!("renamed {old} → {text}"));
                        self.refresh();
                    }
                    Err(e) => self.error(e.to_string()),
                }
                return;
            }
            PromptKind::ConfirmDeleteDeck(id, name, _) => {
                let r = self.eng().delete_deck(id);
                match r {
                    Ok(n) => {
                        self.info(format!("deleted {name} ({n} cards)"));
                        self.refresh();
                    }
                    Err(e) => self.error(e.to_string()),
                }
                return;
            }
            _ => {}
        }
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
                    let nids = self.eng().note_ids_for_cards(&cids)?;
                    let n = self.eng().add_tags(&nids, text)?;
                    format!("tagged {n} note{}", plural(n))
                }
                PromptKind::RemoveTag => {
                    if text.is_empty() {
                        return Ok("no tags given".into());
                    }
                    let nids = self.eng().note_ids_for_cards(&cids)?;
                    let n = self.eng().remove_tags(&nids, text)?;
                    format!("untagged {n} note{}", plural(n))
                }
                PromptKind::MoveDeck => {
                    if text.is_empty() {
                        return Ok("no deck given".into());
                    }
                    let Some(id) = self.eng().deck_id_by_name(text, false)? else {
                        return Err(anyhow::anyhow!("no deck named {text:?} (create it first)").into());
                    };
                    let n = self.eng().move_cards(&cids, id)?;
                    format!("moved {n} card{} to {text}", plural(n))
                }
                PromptKind::SetDue => {
                    if text.is_empty() {
                        return Ok("no due date given".into());
                    }
                    self.eng().set_due(&cids, text)?;
                    format!("set due date on {} card{}", cids.len(), plural(cids.len()))
                }
                PromptKind::ConfirmDelete(_) => {
                    let nids = self.eng().note_ids_for_cards(&cids)?;
                    let n = self.eng().delete_notes(&nids)?;
                    format!("deleted {n} note{}", plural(n))
                }
                PromptKind::ConfirmForget(_) => {
                    self.eng().forget_cards(&cids)?;
                    format!("reset {} card{} to new", cids.len(), plural(cids.len()))
                }
                _ => unreachable!("deck prompts handled above"),
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
        let deck_prompt =
            matches!(kind, PromptKind::NewDeck | PromptKind::RenameDeck(..) | PromptKind::ConfirmDeleteDeck(..));
        if self.view != View::Browser && !deck_prompt {
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
        let engine = self.engine.clone();
        if let Some(b) = self.browser.as_mut() {
            b.anchor = None;
            b.refresh(&mut engine.borrow_mut());
        }
    }

    fn bulk(&mut self, f: impl FnOnce(&mut Engine, &[i64]) -> Result<String>) {
        let Some(b) = self.browser.as_ref() else { return };
        let cids = b.targets();
        if cids.is_empty() {
            return;
        }
        let r = f(&mut self.eng(), &cids);
        match r {
            Ok(msg) => {
                self.info(msg);
                self.after_bulk();
            }
            Err(e) => self.error(e.to_string()),
        }
    }

    fn open_browser(&mut self, query: String) {
        let mut b = BrowserView::new(query);
        b.run_search(&mut self.eng());
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
            self.pending_exact = None;
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
        let (exact, longer) = match self.keymap().lookup(&self.pending) {
            Match::Exact(b) => (Some(b.action), !b.nowait && self.keymap().has_longer(&self.pending)),
            Match::Prefix(_) => (None, true),
            Match::None => (None, false),
        };
        match (exact, longer) {
            (Some(action), false) => {
                self.pending.clear();
                self.pending_since = None;
                self.pending_exact = None;
                let n = self.count.take().unwrap_or(1);
                for _ in 0..n.min(500) {
                    self.dispatch(action);
                }
            }
            (Some(action), true) => self.pending_exact = Some(action),
            (None, true) => {}
            (None, false) => {
                // Dead end. If a shorter mapping was waiting, it fires and the
                // key that broke the sequence is re-read on its own.
                self.pending.clear();
                self.pending_since = None;
                if let Some(action) = self.pending_exact.take() {
                    let n = self.count.take().unwrap_or(1);
                    for _ in 0..n.min(500) {
                        self.dispatch(action);
                    }
                    self.on_key_normal(key);
                } else {
                    self.count = None;
                }
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
            ("help" | "h", topic) => {
                self.help = Some(HelpView::new(topic));
                self.view = View::Help;
            }
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
                    let engine = self.engine.clone();
                    if let Some(b) = self.browser.as_mut() {
                        b.sort = sb;
                        b.refresh(&mut engine.borrow_mut());
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
            ("stats", Some("all")) => self.dispatch(Action::StatsAll),
            ("stats", _) => self.dispatch(Action::Stats),
            ("options" | "o", _) => self.dispatch(Action::DeckOptions),
            ("fsrs", Some("optimize" | "optimise")) => self.dispatch(Action::FsrsOptimize),
            ("fsrs", Some("on" | "off")) => {
                let on = arg == Some("on");
                let deck = self.decks.selected_deck().map(|d| d.id);
                match deck {
                    Some(id) => {
                        let r = self.eng().set_fsrs_enabled(id, on);
                        match r {
                            Ok(()) => self.info(format!("FSRS {}", if on { "enabled" } else { "disabled" })),
                            Err(e) => self.error(e.to_string()),
                        }
                    }
                    None => self.info("select a deck first"),
                }
            }
            ("deck", Some("create" | "new")) => self.dispatch(Action::PromptNewDeck),
            ("deck", Some("rename")) => self.dispatch(Action::PromptRenameDeck),
            ("deck", Some("delete")) => self.dispatch(Action::ConfirmDeleteDeck),
            ("lua", _) => {
                let code = cmd.split_once(' ').map(|x| x.1).unwrap_or("");
                self.lua.set_snapshot(self.snapshot());
                match self.lua.eval(code) {
                    Ok(s) if s.is_empty() => {}
                    Ok(s) => self.info(s),
                    Err(e) => self.error(e),
                }
                self.drain_requests();
            }
            ("theme", Some(name)) => match Theme::by_name(name) {
                Some(t) => self.theme = t,
                None => self.error(format!("unknown theme {name:?} (have: {})", Theme::NAMES.join(", "))),
            },
            (name, _) if self.lua.has_command(name) => {
                let args = cmd.split_once(' ').map(|x| x.1).unwrap_or("").to_string();
                self.lua.set_snapshot(self.snapshot());
                self.lua.run_command(name, &args);
                self.drain_requests();
            }
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
                if self.view == View::Stats || self.view == View::Help {
                    self.view = View::Decks;
                    self.stats = None;
                    self.help = None;
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
                self.emit("quit", |_| Ok(mlua::Value::Nil));
                if self.creds.is_some() && self.sync.is_none() && self.config.sync_on_quit {
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
                View::Stats => {
                    if let Some(s) = self.stats.as_mut() {
                        s.scroll = s.scroll.saturating_add(1);
                    }
                }
                View::Help => {
                    if let Some(h) = self.help.as_mut() {
                        h.scroll = h.scroll.saturating_add(1);
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
                View::Stats => {
                    if let Some(s) = self.stats.as_mut() {
                        s.scroll = s.scroll.saturating_sub(1);
                    }
                }
                View::Help => {
                    if let Some(h) = self.help.as_mut() {
                        h.scroll = h.scroll.saturating_sub(1);
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
                View::Stats => {
                    if let Some(s) = self.stats.as_mut() {
                        s.scroll = 0;
                    }
                }
                View::Help => {
                    if let Some(h) = self.help.as_mut() {
                        h.scroll = 0;
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
                View::Stats => {
                    if let Some(s) = self.stats.as_mut() {
                        s.scroll = u16::MAX / 2;
                    }
                }
                View::Help => {
                    if let Some(h) = self.help.as_mut() {
                        h.scroll = u16::MAX / 2;
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
            Action::Undo => match self.undo_result() {
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
                app.eng().bury(c.card_id)?;
                Ok("card buried".into())
            }),
            Action::Suspend => self.with_current_card(|app, c| {
                app.eng().suspend(c.card_id)?;
                Ok("card suspended".into())
            }),
            Action::Flag(n) => {
                let Some(card) = self.review.as_ref().and_then(|r| r.card.clone()) else { return };
                let new = if card.flag == n { 0 } else { n };
                let r = self.eng().set_flag(card.card_id, new);
                match r {
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
                let r = self.eng().toggle_marked(card.note_id);
                match r {
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
                    // The desktop replays the question before the answer
                    // (deck option `replayq`, on by default); match that.
                    (Stage::Answer, Some(c)) => {
                        let mut av = c.question_av.clone();
                        av.extend(c.answer_av.iter().cloned());
                        av
                    }
                    (_, Some(c)) => c.question_av.clone(),
                    _ => vec![],
                };
                self.play(&av);
            }
            Action::Unbury => match self.unbury_result() {
                Ok(()) => {
                    self.info("unburied");
                    if self.view == View::Review {
                        self.advance();
                    }
                }
                Err(e) => self.error(e.to_string()),
            },
            Action::Back => match self.view {
                View::Stats | View::Help => {
                    self.view = View::Decks;
                    self.stats = None;
                    self.help = None;
                }
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
                    View::Browser | View::Stats | View::Help => String::new(),
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
                    View::Decks | View::Stats | View::Help => None,
                };
                let Some(id) = id else { return };
                if let Some(b) = self.browser.as_mut() {
                    if b.info.is_some() {
                        b.info = None;
                        return;
                    }
                }
                let r = self.eng().card_info(id);
                match r {
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
                let engine = self.engine.clone();
                if let Some(b) = self.browser.as_mut() {
                    b.cycle_sort(&mut engine.borrow_mut());
                }
            }
            Action::ReverseSort => {
                let engine = self.engine.clone();
                if let Some(b) = self.browser.as_mut() {
                    b.toggle_reverse(&mut engine.borrow_mut());
                }
            }
            Action::Stats | Action::StatsAll => {
                let (title, search) = if action == Action::StatsAll {
                    ("whole collection".to_string(), String::new())
                } else {
                    match self.view {
                        View::Decks => match self.decks.selected_deck() {
                            Some(d) => (d.full_name.clone(), format!("deck:\"{}\"", d.full_name)),
                            None => ("whole collection".into(), String::new()),
                        },
                        View::Review => match self.review.as_ref() {
                            Some(r) => (r.deck_name.clone(), format!("deck:\"{}\"", r.deck_name)),
                            None => ("whole collection".into(), String::new()),
                        },
                        View::Browser => match self.browser.as_ref() {
                            Some(b) if !b.query.is_empty() => (b.query.clone(), b.query.clone()),
                            _ => ("whole collection".into(), String::new()),
                        },
                        View::Stats | View::Help => return,
                    }
                };
                let r = self.eng().stats(&search, 365);
                match r {
                    Ok(st) => {
                        self.stats = Some(StatsView::new(title, st));
                        self.view = View::Stats;
                    }
                    Err(e) => self.error(e.to_string()),
                }
            }
            Action::DeckOptions => {
                let deck = match self.view {
                    View::Decks => self.decks.selected_deck().map(|d| d.id),
                    View::Review => self.review.as_ref().and_then(|r| r.card.as_ref().map(|c| c.deck_id)),
                    View::Browser => self.browser.as_ref().and_then(|b| b.current().map(|r| r.deck_id)),
                    View::Stats | View::Help => None,
                };
                match deck {
                    Some(id) => self.edit_request = Some(EditRequest::Options { deck: id }),
                    None => self.info("select a deck first"),
                }
            }
            Action::PromptNewDeck => self.prompt(PromptKind::NewDeck),
            Action::PromptRenameDeck => {
                if let Some(d) = self.decks.selected_deck() {
                    let (id, name) = (d.id, d.full_name.clone());
                    self.prompt(PromptKind::RenameDeck(id, name.clone()));
                    self.cmdline = name;
                }
            }
            Action::ConfirmDeleteDeck => {
                if let Some(d) = self.decks.selected_deck() {
                    let (id, name, n) = (d.id, d.full_name.clone(), d.total_with_children);
                    self.prompt(PromptKind::ConfirmDeleteDeck(id, name, n));
                }
            }
            Action::FsrsOptimize => {
                let deck = match self.view {
                    View::Decks => self.decks.selected_deck().map(|d| (d.id, d.full_name.clone())),
                    View::Review => {
                        self.review.as_ref().and_then(|r| r.card.as_ref().map(|c| (c.deck_id, r.deck_name.clone())))
                    }
                    _ => None,
                };
                let Some((id, name)) = deck else {
                    self.info("select a deck first");
                    return;
                };
                self.info(format!("optimising FSRS for {name}…"));
                let r = self.eng().fsrs_optimize(id);
                match r {
                    Ok((_, n)) => self.info(format!("FSRS parameters optimised for {name} from {n} reviews")),
                    Err(e) => self.error(e.to_string()),
                }
            }
            Action::Lua(idx) => {
                self.lua.set_snapshot(self.snapshot());
                self.lua.call_action(idx);
                self.drain_requests();
            }
            Action::EditNote => {
                let note_id = match self.view {
                    View::Review => self.review.as_ref().and_then(|r| r.card.as_ref().map(|c| c.note_id)),
                    View::Browser => self.browser.as_ref().and_then(|b| b.current().map(|r| r.note_id)),
                    View::Decks | View::Stats | View::Help => None,
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
                    View::Stats | View::Help => None,
                }
                .unwrap_or_else(|| "Default".into());
                self.edit_request = Some(EditRequest::New { deck });
            }
            Action::StudyCard => {
                // Open the current row's deck in review.
                let Some(row) = self.browser.as_ref().and_then(|b| b.current().cloned()) else { return };
                let selected = self.eng().select_deck(row.deck_id);
                if let Err(e) = selected {
                    self.error(e.to_string());
                    return;
                }
                let mut rv = ReviewView::new(row.deck, self.eng().paths().media_folder());
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
                let engine = self.engine.clone();
                let insert = self.mode == Mode::Insert;
                if let Some(b) = self.browser.as_mut() {
                    b.draw(f, chunks[0], &theme, &mut engine.borrow_mut(), insert);
                }
            }
            View::Stats => {
                if let Some(s) = self.stats.as_mut() {
                    s.draw(f, chunks[0], &theme);
                }
            }
            View::Help => {
                if let Some(h) = self.help.as_mut() {
                    h.draw(f, chunks[0], &theme);
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
        let detail = match self.eng().sync_progress() {
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
        lines.push(Line::from(Span::styled(" :lua CODE  :theme NAME  :edit  :add", theme.muted())));
        lines.push(Line::from(Span::styled(
            " :stats [all]  :options  :fsrs optimize|on|off  :deck create|rename|delete",
            theme.muted(),
        )));
        for (name, desc) in self.lua.command_names() {
            lines.push(Line::from(vec![
                Span::styled(format!(" :{name:<9}"), theme.accent()),
                Span::raw(desc.unwrap_or_default()),
            ]));
        }
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

/// Merge the Lua-defined "global" bindings under each view's own.
fn compose_keymaps(
    by_name: &HashMap<String, Keymap<Action>>,
    deletions: &crate::lua::Deletions,
) -> HashMap<View, Keymap<Action>> {
    let global = by_name.get("global").cloned().unwrap_or_default();
    let mut out = HashMap::new();
    for (view, name) in [
        (View::Decks, "decks"),
        (View::Review, "review"),
        (View::Browser, "browser"),
        (View::Stats, "stats"),
        (View::Help, "help"),
    ] {
        let mut km = global.clone();
        if let Some(own) = by_name.get(name) {
            km.extend(own);
        }
        for seq in deletions.get(name).into_iter().flatten() {
            km.unbind(seq);
        }
        out.insert(view, km);
    }
    out
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
