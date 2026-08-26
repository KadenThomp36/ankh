//! Studying: fetch the next card, answer it, undo, bury/suspend/flag.
//!
//! The scheduler is rslib's; this module only shapes its output for a UI
//! and keeps the "current card" bookkeeping honest (a card can only be
//! answered with the states it was shown with).

use anki::card::CardId;
use anki::card_rendering::extract_av_tags;
use anki::decks::DeckId as AnkiDeckId;
use anki::scheduler::answering::{CardAnswer, Rating as AnkiRating};
use anki::scheduler::states::SchedulingStates;
use anki::template::RenderedNode;
use anki::timestamp::TimestampMillis;
use anki_proto::card_rendering::av_tag::Value as AvValue;
use anki_proto::scheduler::bury_or_suspend_cards_request::Mode as BuryMode;
use serde::Serialize;

use crate::decks::DeckId;
use crate::engine::Engine;
use crate::error::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Rating {
    Again,
    Hard,
    Good,
    Easy,
}

impl Rating {
    pub fn from_button(n: u8) -> Option<Rating> {
        match n {
            1 => Some(Rating::Again),
            2 => Some(Rating::Hard),
            3 => Some(Rating::Good),
            4 => Some(Rating::Easy),
            _ => None,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Rating::Again => "again",
            Rating::Hard => "hard",
            Rating::Good => "good",
            Rating::Easy => "easy",
        }
    }
    fn to_anki(self) -> AnkiRating {
        match self {
            Rating::Again => AnkiRating::Again,
            Rating::Hard => AnkiRating::Hard,
            Rating::Good => AnkiRating::Good,
            Rating::Easy => AnkiRating::Easy,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum QueueKind {
    New,
    Learning,
    Review,
}

/// Something the card wants played: a media file or a TTS request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Av {
    File { name: String },
    Tts { text: String, lang: String },
}

/// Remaining counts for the current deck, as shown in the review header.
#[derive(Debug, Clone, Copy, Default, Serialize)]
pub struct Counts {
    pub new: u32,
    pub learn: u32,
    pub review: u32,
}

/// Everything the UI needs to show and answer one card.
#[derive(Debug, Clone, Serialize)]
pub struct ReviewCard {
    pub card_id: i64,
    pub note_id: i64,
    pub deck_id: DeckId,
    pub deck_name: String,
    pub notetype: String,
    pub kind: QueueKind,
    pub flag: u8,
    pub tags: Vec<String>,
    pub question_html: String,
    pub answer_html: String,
    pub css: String,
    pub question_av: Vec<Av>,
    pub answer_av: Vec<Av>,
    /// Interval strings for again/hard/good/easy, e.g. `["<10m", "2.7mo", "3.3mo", "4.4mo"]`.
    pub buttons: [String; 4],
    pub counts: Counts,
    #[serde(skip)]
    states: Option<SchedulingStates>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Congrats {
    pub learn_remaining: u32,
    pub secs_until_next_learn: u32,
    pub review_remaining: bool,
    pub new_remaining: bool,
    pub have_buried: bool,
    pub is_filtered: bool,
}

fn nodes_to_html(nodes: &[RenderedNode]) -> String {
    nodes
        .iter()
        .map(|n| match n {
            RenderedNode::Text { text } => text.as_str(),
            RenderedNode::Replacement { current_text, .. } => current_text.as_str(),
        })
        .collect()
}

/// `extract_av_tags` leaves `[anki:play:q:0]` markers for the desktop's JS
/// player. Show a small speaker glyph instead.
fn replace_play_markers(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(i) = rest.find("[anki:play:") {
        out.push_str(&rest[..i]);
        match rest[i..].find(']') {
            Some(j) => {
                out.push_str("<span style=\"opacity:0.4\">♪</span>");
                rest = &rest[i + j + 1..];
            }
            None => {
                out.push_str(&rest[i..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

fn convert_av(tags: Vec<anki_proto::card_rendering::AvTag>) -> Vec<Av> {
    tags.into_iter()
        .filter_map(|t| match t.value? {
            AvValue::SoundOrVideo(name) => Some(Av::File { name }),
            AvValue::Tts(tts) => Some(Av::Tts { text: tts.field_text, lang: tts.lang }),
        })
        .collect()
}

impl Engine {
    /// Make `deck` the current deck (Anki's notion) so the queue is built for it.
    pub fn select_deck(&mut self, deck: DeckId) -> Result<()> {
        self.col()?.set_current_deck(AnkiDeckId(deck.0))?;
        Ok(())
    }

    pub fn current_deck(&mut self) -> Result<(DeckId, String)> {
        let d = self.col()?.get_current_deck()?;
        Ok((DeckId(d.id.0), d.human_name()))
    }

    /// Next due card in the current deck, fully rendered, or `None` when the
    /// deck is finished for today.
    pub fn next_card(&mut self) -> Result<Option<ReviewCard>> {
        let col = self.col()?;
        let queued = col.get_queued_cards(1, false)?;
        let counts = Counts {
            new: queued.new_count as u32,
            learn: queued.learning_count as u32,
            review: queued.review_count as u32,
        };
        let Some(qc) = queued.cards.into_iter().next() else { return Ok(None) };
        let cid = qc.card.id();
        let render = col.render_existing_card(cid, false, false)?;
        let tr = col.tr().clone();
        let (question_html, q_av) = extract_av_tags(nodes_to_html(&render.qnodes), true, &tr);
        let (answer_html, a_av) = extract_av_tags(nodes_to_html(&render.anodes), false, &tr);
        let buttons = col.describe_next_states(&qc.states)?;
        let mut buttons_arr: [String; 4] = Default::default();
        for (i, b) in buttons.into_iter().take(4).enumerate() {
            buttons_arr[i] = b;
        }
        let deck = col.get_deck(qc.card.deck_id())?;
        let note = col.storage.get_note(qc.card.note_id())?;
        let notetype = note
            .as_ref()
            .and_then(|n| col.get_notetype(n.notetype_id).ok().flatten())
            .map(|nt| nt.name.clone())
            .unwrap_or_default();
        let proto: anki_proto::cards::Card = qc.card.clone().into();
        Ok(Some(ReviewCard {
            card_id: cid.0,
            note_id: proto.note_id,
            deck_id: DeckId(proto.deck_id),
            deck_name: deck.map(|d| d.human_name()).unwrap_or_default(),
            notetype,
            // rslib's queue module is private; the card's queue number says the same thing.
            kind: match proto.queue {
                0 => QueueKind::New,
                1 | 3 => QueueKind::Learning,
                _ => QueueKind::Review,
            },
            flag: proto.flags as u8,
            tags: note.map(|n| n.tags).unwrap_or_default(),
            question_html: replace_play_markers(&question_html),
            answer_html: replace_play_markers(&answer_html),
            css: render.css,
            question_av: convert_av(q_av),
            answer_av: convert_av(a_av),
            buttons: buttons_arr,
            counts,
            states: Some(qc.states),
        }))
    }

    /// Answer a card previously returned by [`Engine::next_card`].
    pub fn answer(&mut self, card: &ReviewCard, rating: Rating, millis_taken: u32) -> Result<()> {
        let states = card.states.clone().ok_or_else(|| anyhow::anyhow!("card has no scheduling states"))?;
        let new_state = match rating {
            Rating::Again => states.again,
            Rating::Hard => states.hard,
            Rating::Good => states.good,
            Rating::Easy => states.easy,
        };
        let mut ans = CardAnswer {
            card_id: CardId(card.card_id),
            current_state: states.current,
            new_state,
            rating: rating.to_anki(),
            answered_at: TimestampMillis::now(),
            milliseconds_taken: millis_taken,
            custom_data: None,
            from_queue: true,
        };
        self.col()?.answer_card(&mut ans)?;
        Ok(())
    }

    /// Undo the last undoable operation; returns its description.
    pub fn undo(&mut self) -> Result<Option<String>> {
        let col = self.col()?;
        if col.can_undo().is_none() {
            return Ok(None);
        }
        let out = col.undo()?;
        Ok(Some(out.output.undone_op.describe(col.tr())))
    }

    pub fn bury(&mut self, card_id: i64) -> Result<()> {
        self.col()?.bury_or_suspend_cards(&[CardId(card_id)], BuryMode::BuryUser)?;
        Ok(())
    }

    pub fn suspend(&mut self, card_id: i64) -> Result<()> {
        self.col()?.bury_or_suspend_cards(&[CardId(card_id)], BuryMode::Suspend)?;
        Ok(())
    }

    /// 0 clears; 1..=7 are Anki's colours (red, orange, green, blue, pink, turquoise, purple).
    pub fn set_flag(&mut self, card_id: i64, flag: u8) -> Result<()> {
        self.col()?.set_card_flag(&[CardId(card_id)], flag as u32)?;
        Ok(())
    }

    /// Toggle the `marked` tag on the card's note.
    pub fn toggle_marked(&mut self, note_id: i64) -> Result<bool> {
        let col = self.col()?;
        let nid = anki::notes::NoteId(note_id);
        let note = col.storage.get_note(nid)?.ok_or_else(|| anyhow::anyhow!("note not found"))?;
        let marked = note.tags.iter().any(|t| t.eq_ignore_ascii_case("marked"));
        if marked {
            col.remove_tags_from_notes(&[nid], "marked")?;
        } else {
            col.add_tags_to_notes(&[nid], "marked")?;
        }
        Ok(!marked)
    }

    pub fn congrats(&mut self) -> Result<Congrats> {
        let c = self.col()?.congrats_info()?;
        Ok(Congrats {
            learn_remaining: c.learn_remaining,
            secs_until_next_learn: c.secs_until_next_learn,
            review_remaining: c.review_remaining,
            new_remaining: c.new_remaining,
            have_buried: c.have_sched_buried || c.have_user_buried,
            is_filtered: c.is_filtered_deck,
        })
    }

    pub fn unbury_current_deck(&mut self) -> Result<()> {
        let col = self.col()?;
        let did = col.get_current_deck()?.id;
        col.unbury_deck(did, anki_proto::scheduler::unbury_deck_request::Mode::All)?;
        Ok(())
    }

    /// Absolute path of a media file referenced by a card.
    pub fn media_path(&self, name: &str) -> std::path::PathBuf {
        self.paths().media_folder().join(name)
    }
}
