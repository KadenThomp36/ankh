//! Searching and bulk operations — the card browser's engine.
//!
//! Search syntax is Anki's own (`deck:Korean is:due tag:leech "exact phrase"`),
//! parsed and executed by rslib. Rows are shaped here so the TUI and CLI show
//! the same columns.

use anki::card::CardId;
use anki::decks::DeckId as AnkiDeckId;
use anki::notes::NoteId;
use anki::search::SortMode;
use anki::text::html_to_text_line;
use anki_proto::scheduler::bury_or_suspend_cards_request::Mode as BuryMode;
use serde::Serialize;

use crate::decks::DeckId;
use crate::engine::Engine;
use crate::error::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SortBy {
    SortField,
    Deck,
    Due,
    Interval,
    Ease,
    Reps,
    Lapses,
    Created,
    Modified,
    Tags,
    Notetype,
}

impl SortBy {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.to_ascii_lowercase().as_str() {
            "field" | "sortfield" | "sort" => SortBy::SortField,
            "deck" => SortBy::Deck,
            "due" => SortBy::Due,
            "interval" | "ivl" => SortBy::Interval,
            "ease" => SortBy::Ease,
            "reps" | "reviews" => SortBy::Reps,
            "lapses" => SortBy::Lapses,
            "created" | "crt" | "added" => SortBy::Created,
            "modified" | "mod" => SortBy::Modified,
            "tags" => SortBy::Tags,
            "notetype" | "note" => SortBy::Notetype,
            _ => return None,
        })
    }

    pub fn label(self) -> &'static str {
        match self {
            SortBy::SortField => "field",
            SortBy::Deck => "deck",
            SortBy::Due => "due",
            SortBy::Interval => "interval",
            SortBy::Ease => "ease",
            SortBy::Reps => "reps",
            SortBy::Lapses => "lapses",
            SortBy::Created => "created",
            SortBy::Modified => "modified",
            SortBy::Tags => "tags",
            SortBy::Notetype => "notetype",
        }
    }

    fn column(self) -> anki::browser_table::Column {
        use anki::browser_table::Column as C;
        match self {
            SortBy::SortField => C::SortField,
            SortBy::Deck => C::Deck,
            SortBy::Due => C::Due,
            SortBy::Interval => C::Interval,
            SortBy::Ease => C::Ease,
            SortBy::Reps => C::Reps,
            SortBy::Lapses => C::Lapses,
            SortBy::Created => C::NoteCreation,
            SortBy::Modified => C::CardMod,
            SortBy::Tags => C::Tags,
            SortBy::Notetype => C::Notetype,
        }
    }

    pub const ALL: [SortBy; 11] = [
        SortBy::SortField,
        SortBy::Deck,
        SortBy::Due,
        SortBy::Interval,
        SortBy::Ease,
        SortBy::Reps,
        SortBy::Lapses,
        SortBy::Created,
        SortBy::Modified,
        SortBy::Tags,
        SortBy::Notetype,
    ];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CardState {
    New,
    Learning,
    Review,
    Suspended,
    Buried,
}

/// One row of the browser: enough to display and to act on.
#[derive(Debug, Clone, Serialize)]
pub struct BrowserRow {
    pub card_id: i64,
    pub note_id: i64,
    pub deck_id: DeckId,
    /// First field, HTML stripped, one line.
    pub sort_field: String,
    pub deck: String,
    pub notetype: String,
    pub template: String,
    pub state: CardState,
    /// Human due: `2026-09-01`, `in 3d`, `today`, `new #12`, `learning`.
    pub due: String,
    /// Days until due (negative = overdue); None for new/suspended.
    pub due_days: Option<i32>,
    pub interval_days: u32,
    pub ease: Option<f32>,
    pub stability: Option<f32>,
    pub difficulty: Option<f32>,
    pub reps: u32,
    pub lapses: u32,
    pub flag: u8,
    pub marked: bool,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CardInfo {
    pub card_id: i64,
    pub note_id: i64,
    pub deck: String,
    pub notetype: String,
    pub template: String,
    pub added: i64,
    pub first_review: Option<i64>,
    pub latest_review: Option<i64>,
    pub due_date: Option<i64>,
    pub interval_days: u32,
    pub ease: f32,
    pub reviews: u32,
    pub lapses: u32,
    pub average_secs: f32,
    pub total_secs: f32,
    pub stability: Option<f32>,
    pub difficulty: Option<f32>,
    pub retrievability: Option<f32>,
    pub preset: String,
    pub revlog: Vec<RevlogEntry>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RevlogEntry {
    pub time: i64,
    pub kind: String,
    pub button: u32,
    pub interval_secs: u32,
    pub taken_secs: f32,
}

impl Engine {
    /// Card ids matching an Anki search, sorted.
    pub fn search(&mut self, query: &str, sort: SortBy, reverse: bool) -> Result<Vec<i64>> {
        let col = self.col()?;
        let mode = SortMode::Builtin { column: sort.column(), reverse };
        let ids = col.search_cards(query, mode)?;
        Ok(ids.into_iter().map(|c| c.0).collect())
    }

    /// Build display rows for card ids (call with a page, not the whole result).
    pub fn browser_rows(&mut self, ids: &[i64]) -> Result<Vec<BrowserRow>> {
        let col = self.col()?;
        let timing = col.timing_today()?;
        let today = timing.days_elapsed as i64;
        let now = timing.now.0;
        let mut rows = Vec::with_capacity(ids.len());
        for &id in ids {
            let Some(card) = col.storage.get_card(CardId(id))? else { continue };
            let proto: anki_proto::cards::Card = card.into();
            let Some(note) = col.storage.get_note(NoteId(proto.note_id))? else { continue };
            let nt = col.get_notetype(note.notetype_id)?;
            let deck = col.get_deck(AnkiDeckId(proto.deck_id))?;
            let sort_field =
                note.fields().first().map(|f| html_to_text_line(f, false).into_owned()).unwrap_or_default();
            let (state, due, due_days) = describe_due(&proto, today, now);
            let marked = note.tags.iter().any(|t| t.eq_ignore_ascii_case("marked"));
            rows.push(BrowserRow {
                card_id: id,
                note_id: proto.note_id,
                deck_id: DeckId(proto.deck_id),
                sort_field,
                deck: deck.map(|d| d.human_name()).unwrap_or_default(),
                notetype: nt.as_ref().map(|n| n.name.clone()).unwrap_or_default(),
                template: nt
                    .as_ref()
                    .and_then(|n| n.templates.get(proto.template_idx as usize))
                    .map(|t| t.name.clone())
                    .unwrap_or_else(|| format!("card {}", proto.template_idx + 1)),
                state,
                due,
                due_days,
                interval_days: proto.interval,
                ease: if proto.ease_factor > 0 { Some(proto.ease_factor as f32 / 1000.0) } else { None },
                stability: proto.memory_state.map(|m| m.stability),
                difficulty: proto.memory_state.map(|m| m.difficulty),
                reps: proto.reps,
                lapses: proto.lapses,
                flag: proto.flags as u8,
                marked,
                tags: note.tags.clone(),
            });
        }
        Ok(rows)
    }

    pub fn note_ids_for_cards(&mut self, cids: &[i64]) -> Result<Vec<i64>> {
        let col = self.col()?;
        let mut out = Vec::new();
        for &c in cids {
            if let Some(card) = col.storage.get_card(CardId(c))? {
                let nid = card.note_id().0;
                if !out.contains(&nid) {
                    out.push(nid);
                }
            }
        }
        Ok(out)
    }

    pub fn card_info(&mut self, card_id: i64) -> Result<CardInfo> {
        let s = self.col()?.card_stats(CardId(card_id))?;
        Ok(CardInfo {
            card_id: s.card_id,
            note_id: s.note_id,
            deck: s.deck,
            notetype: s.notetype,
            template: s.card_type,
            added: s.added,
            first_review: s.first_review,
            latest_review: s.latest_review,
            due_date: s.due_date,
            interval_days: s.interval,
            ease: s.ease as f32 / 1000.0,
            reviews: s.reviews,
            lapses: s.lapses,
            average_secs: s.average_secs,
            total_secs: s.total_secs,
            stability: s.memory_state.map(|m| m.stability),
            difficulty: s.memory_state.map(|m| m.difficulty),
            retrievability: s.fsrs_retrievability,
            preset: s.preset,
            revlog: s
                .revlog
                .into_iter()
                .map(|r| RevlogEntry {
                    time: r.time,
                    kind: match r.review_kind {
                        0 => "learn",
                        1 => "review",
                        2 => "relearn",
                        3 => "filtered",
                        4 => "manual",
                        _ => "rescheduled",
                    }
                    .into(),
                    button: r.button_chosen,
                    interval_secs: r.interval,
                    taken_secs: r.taken_secs,
                })
                .collect(),
        })
    }

    /// Question/answer HTML + CSS for previewing a card outside the scheduler.
    pub fn render_card(&mut self, card_id: i64) -> Result<(String, String, String)> {
        use anki::card_rendering::strip_av_tags;
        let col = self.col()?;
        let r = col.render_existing_card(CardId(card_id), true, false)?;
        let join = |nodes: &[anki::template::RenderedNode]| -> String {
            nodes
                .iter()
                .map(|n| match n {
                    anki::template::RenderedNode::Text { text } => text.as_str(),
                    anki::template::RenderedNode::Replacement { current_text, .. } => current_text.as_str(),
                })
                .collect()
        };
        Ok((strip_av_tags(join(&r.qnodes)), strip_av_tags(join(&r.anodes)), r.css))
    }

    // ----- bulk operations -----------------------------------------------

    pub fn suspend_cards(&mut self, cids: &[i64]) -> Result<usize> {
        let ids: Vec<CardId> = cids.iter().map(|c| CardId(*c)).collect();
        Ok(self.col()?.bury_or_suspend_cards(&ids, BuryMode::Suspend)?.output)
    }

    pub fn bury_cards(&mut self, cids: &[i64]) -> Result<usize> {
        let ids: Vec<CardId> = cids.iter().map(|c| CardId(*c)).collect();
        Ok(self.col()?.bury_or_suspend_cards(&ids, BuryMode::BuryUser)?.output)
    }

    pub fn unsuspend_cards(&mut self, cids: &[i64]) -> Result<()> {
        let ids: Vec<CardId> = cids.iter().map(|c| CardId(*c)).collect();
        self.col()?.unbury_or_unsuspend_cards(&ids)?;
        Ok(())
    }

    pub fn flag_cards(&mut self, cids: &[i64], flag: u8) -> Result<usize> {
        let ids: Vec<CardId> = cids.iter().map(|c| CardId(*c)).collect();
        Ok(self.col()?.set_card_flag(&ids, flag as u32)?.output)
    }

    pub fn move_cards(&mut self, cids: &[i64], deck: DeckId) -> Result<usize> {
        let ids: Vec<CardId> = cids.iter().map(|c| CardId(*c)).collect();
        Ok(self.col()?.set_deck(&ids, AnkiDeckId(deck.0))?.output)
    }

    pub fn add_tags(&mut self, nids: &[i64], tags: &str) -> Result<usize> {
        let ids: Vec<NoteId> = nids.iter().map(|n| NoteId(*n)).collect();
        Ok(self.col()?.add_tags_to_notes(&ids, tags)?.output)
    }

    pub fn remove_tags(&mut self, nids: &[i64], tags: &str) -> Result<usize> {
        let ids: Vec<NoteId> = nids.iter().map(|n| NoteId(*n)).collect();
        Ok(self.col()?.remove_tags_from_notes(&ids, tags)?.output)
    }

    /// Delete notes (and all their cards).
    pub fn delete_notes(&mut self, nids: &[i64]) -> Result<usize> {
        let ids: Vec<NoteId> = nids.iter().map(|n| NoteId(*n)).collect();
        Ok(self.col()?.remove_notes(&ids)?.output)
    }

    /// "Forget": back to new, keeping the review history.
    pub fn forget_cards(&mut self, cids: &[i64]) -> Result<()> {
        let ids: Vec<CardId> = cids.iter().map(|c| CardId(*c)).collect();
        self.col()?.reschedule_cards_as_new(&ids, true, true, false, None)?;
        Ok(())
    }

    /// Set due date; `days` is Anki's spec: `0`, `3`, `1-7`, `2!`.
    pub fn set_due(&mut self, cids: &[i64], days: &str) -> Result<()> {
        let ids: Vec<CardId> = cids.iter().map(|c| CardId(*c)).collect();
        self.col()?.set_due_date(&ids, days, None)?;
        Ok(())
    }

    /// Deck id by human name (`Korean::Vocab`), creating it if asked.
    pub fn deck_id_by_name(&mut self, name: &str, create: bool) -> Result<Option<DeckId>> {
        let col = self.col()?;
        if let Some(id) = col.get_deck_id(name)? {
            return Ok(Some(DeckId(id.0)));
        }
        if create {
            let d = col.get_or_create_normal_deck(name)?;
            return Ok(Some(DeckId(d.id.0)));
        }
        Ok(None)
    }
}

fn describe_due(c: &anki_proto::cards::Card, today: i64, now: i64) -> (CardState, String, Option<i32>) {
    // queue: -3 user buried, -2 sched buried, -1 suspended, 0 new, 1 learn, 2 review, 3 day-learn, 4 preview
    match c.queue {
        -1 => (CardState::Suspended, "suspended".into(), None),
        -2 | -3 => (CardState::Buried, "buried".into(), None),
        0 => (CardState::New, format!("new #{}", c.due), None),
        1 | 4 => {
            let secs = c.due as i64 - now;
            let s = if secs <= 0 {
                "learning now".to_string()
            } else if secs < 3600 {
                format!("learning in {}m", secs / 60)
            } else {
                format!("learning in {}h", secs / 3600)
            };
            (CardState::Learning, s, Some(0))
        }
        _ => {
            let delta = c.due as i64 - today;
            let s = if delta < 0 {
                format!("{}d overdue", -delta)
            } else if delta == 0 {
                "today".to_string()
            } else if delta == 1 {
                "tomorrow".to_string()
            } else {
                let date = chrono::Local::now().date_naive() + chrono::Duration::days(delta);
                if delta <= 30 {
                    format!("in {delta}d")
                } else {
                    date.format("%Y-%m-%d").to_string()
                }
            };
            let state = if c.queue == 3 { CardState::Learning } else { CardState::Review };
            (state, s, Some(delta as i32))
        }
    }
}
