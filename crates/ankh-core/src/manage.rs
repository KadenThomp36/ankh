//! Deck management, deck options, FSRS, statistics, package import/export.

use std::collections::BTreeMap;

use anki::decks::DeckId as AnkiDeckId;
use anki::services::{DeckConfigService, ImportExportService, SchedulerService, StatsService};
use anki_proto::deck_config::deck_config::Config as ConfigProto;
use serde::{Deserialize, Serialize};

use crate::decks::DeckId;
use crate::engine::Engine;
use crate::error::Result;

// ---------------------------------------------------------------------------
// Decks
// ---------------------------------------------------------------------------

impl Engine {
    pub fn create_deck(&mut self, name: &str) -> Result<DeckId> {
        let d = self.col()?.get_or_create_normal_deck(name)?;
        Ok(DeckId(d.id.0))
    }

    pub fn rename_deck(&mut self, deck: DeckId, new_name: &str) -> Result<()> {
        self.col()?.rename_deck(AnkiDeckId(deck.0), new_name)?;
        Ok(())
    }

    /// Delete a deck, its subdecks and every card in them. Returns cards removed.
    pub fn delete_deck(&mut self, deck: DeckId) -> Result<usize> {
        Ok(self.col()?.remove_decks_and_child_decks(&[AnkiDeckId(deck.0)])?.output)
    }

    /// Persist the fold state so it survives restarts and syncs.
    pub fn set_deck_collapsed(&mut self, deck: DeckId, collapsed: bool) -> Result<()> {
        self.col()?.set_deck_collapsed(
            AnkiDeckId(deck.0),
            collapsed,
            anki_proto::decks::set_deck_collapsed_request::Scope::Reviewer,
        )?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Deck options (a preset, edited as TOML)
// ---------------------------------------------------------------------------

/// The options a preset exposes for editing. Field names match Anki's
/// deck-options screen; comments in the TOML explain units.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DeckOptions {
    pub preset: String,
    pub new_per_day: u32,
    pub reviews_per_day: u32,
    /// Minutes. `[1, 10]`
    pub learn_steps: Vec<f32>,
    /// Minutes.
    pub relearn_steps: Vec<f32>,
    pub graduating_interval_good: u32,
    pub graduating_interval_easy: u32,
    pub maximum_review_interval: u32,
    pub leech_threshold: u32,
    /// "suspend" or "tag"
    pub leech_action: String,
    pub bury_new: bool,
    pub bury_reviews: bool,
    pub bury_interday_learning: bool,
    /// "due" or "random"
    pub new_card_insert_order: String,
    pub desired_retention: f32,
    pub fsrs_params: Vec<f32>,
    // SM-2 only
    pub initial_ease: f32,
    pub easy_multiplier: f32,
    pub hard_multiplier: f32,
    pub interval_multiplier: f32,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeckOptionsInfo {
    pub deck: String,
    pub preset_id: i64,
    pub fsrs_enabled: bool,
    pub decks_using_preset: u32,
    pub days_since_optimize: u32,
    pub presets: Vec<String>,
}

fn options_from_proto(name: &str, c: &ConfigProto) -> DeckOptions {
    DeckOptions {
        preset: name.to_string(),
        new_per_day: c.new_per_day,
        reviews_per_day: c.reviews_per_day,
        learn_steps: c.learn_steps.clone(),
        relearn_steps: c.relearn_steps.clone(),
        graduating_interval_good: c.graduating_interval_good,
        graduating_interval_easy: c.graduating_interval_easy,
        maximum_review_interval: c.maximum_review_interval,
        leech_threshold: c.leech_threshold,
        leech_action: if c.leech_action == 1 { "tag".into() } else { "suspend".into() },
        bury_new: c.bury_new,
        bury_reviews: c.bury_reviews,
        bury_interday_learning: c.bury_interday_learning,
        new_card_insert_order: if c.new_card_insert_order == 1 { "random".into() } else { "due".into() },
        desired_retention: c.desired_retention,
        fsrs_params: c.fsrs_params_6.clone(),
        initial_ease: c.initial_ease,
        easy_multiplier: c.easy_multiplier,
        hard_multiplier: c.hard_multiplier,
        interval_multiplier: c.interval_multiplier,
    }
}

fn apply_options(c: &mut ConfigProto, o: &DeckOptions) {
    c.new_per_day = o.new_per_day;
    c.reviews_per_day = o.reviews_per_day;
    c.learn_steps = o.learn_steps.clone();
    c.relearn_steps = o.relearn_steps.clone();
    c.graduating_interval_good = o.graduating_interval_good;
    c.graduating_interval_easy = o.graduating_interval_easy;
    c.maximum_review_interval = o.maximum_review_interval;
    c.leech_threshold = o.leech_threshold;
    c.leech_action = if o.leech_action.eq_ignore_ascii_case("tag") { 1 } else { 0 };
    c.bury_new = o.bury_new;
    c.bury_reviews = o.bury_reviews;
    c.bury_interday_learning = o.bury_interday_learning;
    c.new_card_insert_order = if o.new_card_insert_order.eq_ignore_ascii_case("random") { 1 } else { 0 };
    c.desired_retention = o.desired_retention;
    c.fsrs_params_6 = o.fsrs_params.clone();
    c.initial_ease = o.initial_ease;
    c.easy_multiplier = o.easy_multiplier;
    c.hard_multiplier = o.hard_multiplier;
    c.interval_multiplier = o.interval_multiplier;
}

impl DeckOptions {
    /// TOML with explanatory comments, for `$EDITOR`.
    pub fn to_toml(&self, info: &DeckOptionsInfo) -> String {
        let body = toml::to_string_pretty(self).unwrap_or_default();
        format!(
            "# Deck options for {deck}\n# Preset shared by {n} deck(s). Rename `preset` to fork it into a new preset.\n# Other presets: {presets}\n# FSRS is {fsrs}; `fsrs_params` are the 21 FSRS-6 weights (run `:fsrs optimize`).\n# Steps are minutes; intervals are days; desired_retention is 0.70–0.99.\n\n{body}",
            deck = info.deck,
            n = info.decks_using_preset,
            presets = if info.presets.is_empty() { "—".to_string() } else { info.presets.join(", ") },
            fsrs = if info.fsrs_enabled { "on" } else { "off" },
        )
    }

    pub fn from_toml(text: &str) -> Result<DeckOptions> {
        toml::from_str(text).map_err(|e| anyhow::anyhow!("options: {e}").into())
    }
}

impl Engine {
    pub fn deck_options(&mut self, deck: DeckId) -> Result<(DeckOptions, DeckOptionsInfo)> {
        let col = self.col()?;
        let upd = DeckConfigService::get_deck_configs_for_update(&mut *col, anki_proto::decks::DeckId { did: deck.0 })?;
        let cur = upd.current_deck.clone().unwrap_or_default();
        let entry = upd
            .all_config
            .iter()
            .find(|c| c.config.as_ref().map(|c| c.id) == Some(cur.config_id))
            .or(upd.all_config.first())
            .ok_or_else(|| anyhow::anyhow!("deck has no options preset"))?;
        let cfg = entry.config.clone().unwrap_or_default();
        let inner = cfg.config.clone().unwrap_or_default();
        let info = DeckOptionsInfo {
            deck: cur.name.clone(),
            preset_id: cfg.id,
            fsrs_enabled: upd.fsrs,
            decks_using_preset: entry.use_count,
            days_since_optimize: upd.days_since_last_fsrs_optimize,
            presets: upd
                .all_config
                .iter()
                .filter_map(|c| c.config.as_ref().map(|c| c.name.clone()))
                .filter(|n| n != &cfg.name)
                .collect(),
        };
        Ok((options_from_proto(&cfg.name, &inner), info))
    }

    /// Save options. A changed `preset` name creates a new preset and
    /// assigns the deck to it; otherwise the current preset is updated in
    /// place (affecting every deck that shares it).
    pub fn save_deck_options(&mut self, deck: DeckId, opts: &DeckOptions) -> Result<()> {
        let col = self.col()?;
        let upd = DeckConfigService::get_deck_configs_for_update(&mut *col, anki_proto::decks::DeckId { did: deck.0 })?;
        let cur = upd.current_deck.clone().unwrap_or_default();
        let mut cfg = upd
            .all_config
            .iter()
            .find(|c| c.config.as_ref().map(|c| c.id) == Some(cur.config_id))
            .and_then(|c| c.config.clone())
            .unwrap_or_default();
        let existing = upd.all_config.iter().filter_map(|c| c.config.as_ref()).find(|c| c.name == opts.preset).cloned();
        match existing {
            Some(other) if other.id != cfg.id => cfg = other,
            None if opts.preset != cfg.name => {
                cfg.id = 0; // new preset
                cfg.name = opts.preset.clone();
            }
            _ => {}
        }
        let mut inner = cfg.config.clone().unwrap_or_default();
        apply_options(&mut inner, opts);
        cfg.config = Some(inner);
        let _ = DeckConfigService::update_deck_configs(
            &mut *col,
            anki_proto::deck_config::UpdateDeckConfigsRequest {
                target_deck_id: deck.0,
                configs: vec![cfg],
                removed_config_ids: vec![],
                mode: 0,
                card_state_customizer: upd.card_state_customizer,
                limits: cur.limits,
                new_cards_ignore_review_limit: upd.new_cards_ignore_review_limit,
                fsrs: upd.fsrs,
                apply_all_parent_limits: upd.apply_all_parent_limits,
                fsrs_reschedule: false,
                fsrs_health_check: upd.fsrs_health_check,
            },
        )?;
        Ok(())
    }

    pub fn set_fsrs_enabled(&mut self, deck: DeckId, enabled: bool) -> Result<()> {
        let col = self.col()?;
        let upd = DeckConfigService::get_deck_configs_for_update(&mut *col, anki_proto::decks::DeckId { did: deck.0 })?;
        let cur = upd.current_deck.clone().unwrap_or_default();
        let _ = DeckConfigService::update_deck_configs(
            &mut *col,
            anki_proto::deck_config::UpdateDeckConfigsRequest {
                target_deck_id: deck.0,
                configs: vec![],
                removed_config_ids: vec![],
                mode: 0,
                card_state_customizer: upd.card_state_customizer,
                limits: cur.limits,
                new_cards_ignore_review_limit: upd.new_cards_ignore_review_limit,
                fsrs: enabled,
                apply_all_parent_limits: upd.apply_all_parent_limits,
                fsrs_reschedule: false,
                fsrs_health_check: upd.fsrs_health_check,
            },
        )?;
        Ok(())
    }

    /// Optimise FSRS parameters from this deck's review history and store
    /// them in its preset. Returns (params, review count used).
    pub fn fsrs_optimize(&mut self, deck: DeckId) -> Result<(Vec<f32>, u32)> {
        let (opts, info) = self.deck_options(deck)?;
        let search = format!("preset:\"{}\"", opts.preset);
        let resp = {
            let col = self.col()?;
            col.compute_fsrs_params(anki_proto::scheduler::ComputeFsrsParamsRequest {
                search,
                current_params: opts.fsrs_params.clone(),
                ignore_revlogs_before_ms: 0,
                num_of_relearning_steps: opts.relearn_steps.len() as u32,
                health_check: false,
            })?
        };
        if resp.params.is_empty() {
            return Err(anyhow::anyhow!("not enough reviews to optimise ({} usable)", resp.fsrs_items).into());
        }
        let mut new_opts = opts;
        new_opts.fsrs_params = resp.params.clone();
        let _ = info;
        self.save_deck_options(deck, &new_opts)?;
        Ok((resp.params, resp.fsrs_items))
    }
}

// ---------------------------------------------------------------------------
// Statistics
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize)]
pub struct TodayStats {
    pub answered: u32,
    pub secs: f32,
    pub correct: u32,
    pub mature_correct: u32,
    pub mature_count: u32,
    pub learn: u32,
    pub review: u32,
    pub relearn: u32,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct CardCounts {
    pub new: u32,
    pub learning: u32,
    pub relearning: u32,
    pub young: u32,
    pub mature: u32,
    pub suspended: u32,
    pub buried: u32,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Stats {
    pub search: String,
    pub today: TodayStats,
    /// Reviews per day; key = days ago (0 = today), value = count. Up to 365 days.
    pub reviews_per_day: BTreeMap<i32, u32>,
    /// Cards due per day ahead; key = days from today (negative = overdue backlog).
    pub forecast: BTreeMap<i32, u32>,
    pub counts: CardCounts,
    /// Interval histogram: days → cards.
    pub intervals: BTreeMap<u32, u32>,
    /// Reviews per hour of day (0..24) over the last month: (total, correct).
    pub hours: Vec<(u32, u32)>,
    /// Answer buttons over the last month: [again, hard, good, easy] for learning/young/mature.
    pub buttons: [[u32; 4]; 3],
    /// Percent (0–100).
    pub average_retrievability: f32,
    /// True retention over the last month (correct / answered, mature cards).
    pub mature_retention: Option<f32>,
    pub cards_added: BTreeMap<i32, u32>,
}

impl Engine {
    pub fn stats(&mut self, search: &str, days: u32) -> Result<Stats> {
        let col = self.col()?;
        let g = col.graphs(anki_proto::stats::GraphsRequest { search: search.to_string(), days })?;
        let mut s = Stats { search: search.to_string(), ..Default::default() };
        if let Some(t) = g.today {
            s.today = TodayStats {
                answered: t.answer_count,
                secs: t.answer_millis as f32 / 1000.0,
                correct: t.correct_count,
                mature_correct: t.mature_correct,
                mature_count: t.mature_count,
                learn: t.learn_count,
                review: t.review_count,
                relearn: t.relearn_count,
            };
        }
        if let Some(r) = g.reviews {
            for (day, c) in r.count {
                let n = c.learn + c.relearn + c.young + c.mature + c.filtered;
                if n > 0 {
                    s.reviews_per_day.insert(-day, n);
                }
            }
        }
        if let Some(f) = g.future_due {
            for (day, n) in f.future_due {
                s.forecast.insert(day, n);
            }
        }
        if let Some(cc) = g.card_counts.and_then(|c| c.excluding_inactive) {
            s.counts = CardCounts {
                new: cc.new_cards,
                learning: cc.learn,
                relearning: cc.relearn,
                young: cc.young,
                mature: cc.mature,
                suspended: cc.suspended,
                buried: cc.buried,
            };
        }
        if let Some(i) = g.intervals {
            for (d, n) in i.intervals {
                s.intervals.insert(d, n);
            }
        }
        if let Some(h) = g.hours {
            s.hours = h.one_month.iter().map(|x| (x.total, x.correct)).collect();
        }
        if let Some(b) = g.buttons.and_then(|b| b.one_month) {
            let fill = |v: &Vec<u32>| {
                [
                    v.first().copied().unwrap_or(0),
                    v.get(1).copied().unwrap_or(0),
                    v.get(2).copied().unwrap_or(0),
                    v.get(3).copied().unwrap_or(0),
                ]
            };
            s.buttons = [fill(&b.learning), fill(&b.young), fill(&b.mature)];
            let m = s.buttons[2];
            let total: u32 = m.iter().sum();
            if total > 0 {
                s.mature_retention = Some((total - m[0]) as f32 / total as f32);
            }
        }
        if let Some(r) = g.retrievability {
            s.average_retrievability = r.average;
        }
        if let Some(a) = g.added {
            for (day, n) in a.added {
                s.cards_added.insert(-day, n);
            }
        }
        Ok(s)
    }
}

// ---------------------------------------------------------------------------
// Import / export
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize)]
pub struct ImportSummary {
    pub added: usize,
    pub updated: usize,
    pub duplicates: usize,
    pub conflicting: usize,
}

impl ImportSummary {
    fn from_log(log: Option<anki_proto::import_export::import_response::Log>) -> Self {
        let Some(l) = log else { return Self::default() };
        ImportSummary {
            added: l.new.len(),
            updated: l.updated.len(),
            duplicates: l.duplicate.len(),
            conflicting: l.conflicting.len(),
        }
    }
}

impl Engine {
    /// Import an `.apkg` (shared deck or export).
    pub fn import_apkg(&mut self, path: &str) -> Result<ImportSummary> {
        let col = self.col()?;
        let resp = col.import_anki_package(anki_proto::import_export::ImportAnkiPackageRequest {
            package_path: path.to_string(),
            options: Some(anki_proto::import_export::ImportAnkiPackageOptions {
                merge_notetypes: false,
                update_notes: 1, // if newer
                update_notetypes: 1,
                with_scheduling: true,
                with_deck_configs: false,
            }),
        })?;
        Ok(ImportSummary::from_log(resp.log))
    }

    /// Export matching cards to an `.apkg`. Returns the number of notes exported.
    pub fn export_apkg(&mut self, path: &str, search: &str, with_scheduling: bool, with_media: bool) -> Result<u32> {
        let col = self.col()?;
        let limit = if search.trim().is_empty() {
            anki_proto::import_export::ExportLimit {
                limit: Some(anki_proto::import_export::export_limit::Limit::WholeCollection(
                    anki_proto::generic::Empty {},
                )),
            }
        } else {
            let ids = col.search_cards(search, anki::search::SortMode::NoOrder)?;
            anki_proto::import_export::ExportLimit {
                limit: Some(anki_proto::import_export::export_limit::Limit::CardIds(anki_proto::cards::CardIds {
                    cids: ids.into_iter().map(|c| c.0).collect(),
                })),
            }
        };
        let n = col.export_anki_package(anki_proto::import_export::ExportAnkiPackageRequest {
            out_path: path.to_string(),
            limit: Some(limit),
            options: Some(anki_proto::import_export::ExportAnkiPackageOptions {
                with_scheduling,
                with_deck_configs: false,
                with_media,
                legacy: true,
            }),
        })?;
        Ok(n.val)
    }

    /// Import a CSV/TSV whose columns map to the notetype's fields in order.
    pub fn import_csv(&mut self, path: &str, notetype: &str, deck: &str) -> Result<ImportSummary> {
        let nt_id = {
            let col = self.col()?;
            col.get_notetype_by_name(notetype)?.ok_or_else(|| anyhow::anyhow!("unknown notetype {notetype:?}"))?.id.0
        };
        let deck_id = self.create_deck(deck)?.0;
        let col = self.col()?;
        let meta = ImportExportService::get_csv_metadata(
            &mut *col,
            anki_proto::import_export::CsvMetadataRequest {
                path: path.to_string(),
                delimiter: None,
                notetype_id: Some(nt_id),
                deck_id: Some(deck_id),
                is_html: None,
            },
        )?;
        let resp = ImportExportService::import_csv(
            &mut *col,
            anki_proto::import_export::ImportCsvRequest { path: path.to_string(), metadata: Some(meta) },
        )?;
        Ok(ImportSummary::from_log(resp.log))
    }
}
