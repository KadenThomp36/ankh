//! The card browser: an Anki search, a table, a preview, bulk operations.

use ankh_core::{BrowserRow, CardInfo, CardState, Engine, Result, SortBy};
use ankh_render::{render_html, wrap_document, Align, Document, Options, Stylesheet};
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;
use unicode_width::UnicodeWidthStr;

use crate::tui::doc;
use crate::tui::theme::Theme;
use crate::tui::views::review::flag_color;

pub struct BrowserView {
    /// The query that produced `ids`.
    pub query: String,
    /// The query being typed (insert mode).
    pub input: String,
    pub cursor: usize,
    pub ids: Vec<i64>,
    rows: Vec<Option<BrowserRow>>,
    pub selected: usize,
    pub scroll: usize,
    pub sort: SortBy,
    pub reverse: bool,
    /// Visual-mode anchor; the selection is anchor..=selected.
    pub anchor: Option<usize>,
    pub preview: bool,
    preview_doc: Option<(i64, bool, Document)>,
    pub preview_answer: bool,
    pub info: Option<CardInfo>,
    pub error: Option<String>,
}

impl BrowserView {
    pub fn new(query: String) -> Self {
        BrowserView {
            input: query.clone(),
            cursor: query.chars().count(),
            query,
            ids: vec![],
            rows: vec![],
            selected: 0,
            scroll: 0,
            sort: SortBy::SortField,
            reverse: false,
            anchor: None,
            preview: true,
            preview_doc: None,
            preview_answer: false,
            info: None,
            error: None,
        }
    }

    pub fn run_search(&mut self, engine: &mut Engine) {
        self.query = self.input.trim().to_string();
        match engine.search(&self.query, self.sort, self.reverse) {
            Ok(ids) => {
                self.ids = ids;
                self.rows = vec![None; self.ids.len()];
                self.selected = self.selected.min(self.ids.len().saturating_sub(1));
                self.anchor = None;
                self.preview_doc = None;
                self.error = None;
            }
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    /// Re-run the search after a mutation, keeping the cursor put.
    pub fn refresh(&mut self, engine: &mut Engine) {
        let sel = self.selected;
        self.run_search(engine);
        self.selected = sel.min(self.ids.len().saturating_sub(1));
        self.rows.iter_mut().for_each(|r| *r = None);
        self.preview_doc = None;
    }

    fn ensure_rows(&mut self, engine: &mut Engine, from: usize, to: usize) {
        let to = to.min(self.ids.len());
        let missing: Vec<i64> = (from..to).filter(|i| self.rows[*i].is_none()).map(|i| self.ids[i]).collect();
        if missing.is_empty() {
            return;
        }
        if let Ok(rows) = engine.browser_rows(&missing) {
            for r in rows {
                if let Some(i) = (from..to).find(|i| self.ids[*i] == r.card_id) {
                    self.rows[i] = Some(r);
                }
            }
        }
    }

    pub fn current(&self) -> Option<&BrowserRow> {
        self.rows.get(self.selected).and_then(|r| r.as_ref())
    }

    pub fn current_id(&self) -> Option<i64> {
        self.ids.get(self.selected).copied()
    }

    /// Card ids an operation applies to: the visual range, else the current row.
    pub fn targets(&self) -> Vec<i64> {
        match self.anchor {
            Some(a) => {
                let (lo, hi) = if a <= self.selected { (a, self.selected) } else { (self.selected, a) };
                self.ids[lo..=hi.min(self.ids.len().saturating_sub(1))].to_vec()
            }
            None => self.current_id().into_iter().collect(),
        }
    }

    pub fn move_by(&mut self, d: isize) {
        let n = self.ids.len() as isize;
        if n == 0 {
            return;
        }
        self.selected = (self.selected as isize + d).clamp(0, n - 1) as usize;
    }

    pub fn cycle_sort(&mut self, engine: &mut Engine) {
        let i = SortBy::ALL.iter().position(|s| *s == self.sort).unwrap_or(0);
        self.sort = SortBy::ALL[(i + 1) % SortBy::ALL.len()];
        self.refresh(engine);
    }

    pub fn toggle_reverse(&mut self, engine: &mut Engine) {
        self.reverse = !self.reverse;
        self.refresh(engine);
    }

    // ----- insert-mode editing -------------------------------------------

    pub fn insert_char(&mut self, c: char) {
        let idx = self.byte_idx();
        self.input.insert(idx, c);
        self.cursor += 1;
    }

    pub fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        self.cursor -= 1;
        let idx = self.byte_idx();
        self.input.remove(idx);
    }

    pub fn delete_word(&mut self) {
        while self.cursor > 0 && self.input.chars().nth(self.cursor - 1) == Some(' ') {
            self.backspace();
        }
        while self.cursor > 0 && self.input.chars().nth(self.cursor - 1) != Some(' ') {
            self.backspace();
        }
    }

    pub fn clear_input(&mut self) {
        self.input.clear();
        self.cursor = 0;
    }

    pub fn cursor_left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub fn cursor_right(&mut self) {
        self.cursor = (self.cursor + 1).min(self.input.chars().count());
    }

    fn byte_idx(&self) -> usize {
        self.input.char_indices().nth(self.cursor).map(|(i, _)| i).unwrap_or(self.input.len())
    }

    // ----- drawing --------------------------------------------------------

    fn preview_document(&mut self, engine: &mut Engine) -> Result<()> {
        let Some(id) = self.current_id() else { return Ok(()) };
        if let Some((pid, ans, _)) = &self.preview_doc {
            if *pid == id && *ans == self.preview_answer {
                return Ok(());
            }
        }
        let (q, a, css) = engine.render_card(id)?;
        let opts = Options {
            default_align: Align::Left,
            stylesheet: if css.trim().is_empty() { None } else { Some(Stylesheet::parse(&css)) },
            reveal_hints: true,
        };
        let doc = render_html(if self.preview_answer { &a } else { &q }, &opts);
        self.preview_doc = Some((id, self.preview_answer, doc));
        Ok(())
    }

    pub fn draw(&mut self, f: &mut Frame, area: Rect, theme: &Theme, engine: &mut Engine, insert_mode: bool) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(theme.border())
            .title(Line::from(Span::styled(" browser ", theme.title())));
        let inner = block.inner(area);
        f.render_widget(block, area);

        let preview_h = if self.preview && inner.height > 14 { (inner.height / 3).max(6) } else { 0 };
        let [search, header, table, preview] = Layout::vertical([
            Constraint::Length(2),
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(preview_h),
        ])
        .areas(inner);

        // Search line
        let prompt_style = if insert_mode { theme.accent() } else { theme.muted() };
        let count =
            format!("{} cards · sort {}{}", self.ids.len(), self.sort.label(), if self.reverse { " ↑" } else { " ↓" });
        let count_w = count.width() as u16 + 2;
        let q_area = Rect { width: search.width.saturating_sub(count_w), height: 1, ..search };
        let mut spans = vec![Span::styled(" / ", prompt_style)];
        if self.input.is_empty() && !insert_mode {
            spans.push(Span::styled("type / to search — e.g. deck:Korean is:due tag:leech", theme.muted()));
        } else {
            spans.push(Span::styled(self.input.clone(), Style::default().fg(theme.fg)));
        }
        f.render_widget(Paragraph::new(Line::from(spans)), q_area);
        if insert_mode {
            let prefix: String = self.input.chars().take(self.cursor).collect();
            f.set_cursor_position((q_area.x + 3 + prefix.width() as u16, q_area.y));
        }
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(count, theme.muted())).alignment(Alignment::Right)),
            Rect { x: search.x + search.width - count_w, width: count_w, height: 1, ..search },
        );
        if let Some(e) = &self.error {
            f.render_widget(
                Paragraph::new(Span::styled(format!("   {e}"), Style::default().fg(theme.error))),
                Rect { y: search.y + 1, height: 1, ..search },
            );
        }

        // Table
        let h = table.height as usize;
        if h > 0 {
            if self.selected < self.scroll {
                self.scroll = self.selected;
            } else if self.selected >= self.scroll + h {
                self.scroll = self.selected + 1 - h;
            }
            self.ensure_rows(engine, self.scroll, self.scroll + h);
        }
        let w = table.width as usize;
        let due_w = 14;
        let ivl_w = 6;
        let deck_w = (w / 4).clamp(8, 28);
        let field_w = w.saturating_sub(due_w + ivl_w + deck_w + 6);
        let hdr = Line::from(vec![
            Span::styled(format!("  {:<fw$}", "FIELD", fw = field_w), theme.muted().add_modifier(Modifier::BOLD)),
            Span::styled(format!(" {:<dw$}", "DECK", dw = deck_w), theme.muted().add_modifier(Modifier::BOLD)),
            Span::styled(format!(" {:>iw$}", "IVL", iw = ivl_w), theme.muted().add_modifier(Modifier::BOLD)),
            Span::styled(format!(" {:<duw$}", "DUE", duw = due_w), theme.muted().add_modifier(Modifier::BOLD)),
        ]);
        f.render_widget(Paragraph::new(hdr), header);

        let sel_range = self.anchor.map(|a| if a <= self.selected { a..=self.selected } else { self.selected..=a });
        let mut lines = Vec::with_capacity(h);
        for i in self.scroll..(self.scroll + h).min(self.ids.len()) {
            let is_cur = i == self.selected;
            let in_visual = sel_range.as_ref().map(|r| r.contains(&i)).unwrap_or(false);
            let line = match &self.rows[i] {
                None => Line::from(Span::styled("  …", theme.muted())),
                Some(r) => {
                    let marker = if r.marked {
                        "★"
                    } else if r.flag != 0 {
                        "⚑"
                    } else {
                        " "
                    };
                    let marker_style = if r.flag != 0 {
                        Style::default().fg(flag_color(r.flag))
                    } else {
                        Style::default().fg(theme.warn)
                    };
                    let field_style = match r.state {
                        CardState::Suspended => Style::default().fg(theme.warn).add_modifier(Modifier::DIM),
                        CardState::Buried => theme.muted(),
                        _ => Style::default().fg(theme.fg),
                    };
                    let due_style = match r.state {
                        CardState::New => Style::default().fg(theme.new),
                        CardState::Learning => Style::default().fg(theme.learn),
                        CardState::Review if r.due_days.map(|d| d <= 0).unwrap_or(false) => {
                            Style::default().fg(theme.review)
                        }
                        CardState::Review => theme.muted(),
                        _ => theme.muted(),
                    };
                    let ivl = if r.interval_days == 0 {
                        String::new()
                    } else if r.interval_days >= 365 {
                        format!("{:.1}y", r.interval_days as f32 / 365.0)
                    } else if r.interval_days >= 30 {
                        format!("{:.1}mo", r.interval_days as f32 / 30.0)
                    } else {
                        format!("{}d", r.interval_days)
                    };
                    Line::from(vec![
                        Span::styled(if is_cur { "▌" } else { " " }, theme.accent()),
                        Span::styled(marker, marker_style),
                        Span::styled(fit(&r.sort_field, field_w), field_style),
                        Span::styled(format!(" {}", fit(&short_deck(&r.deck), deck_w)), theme.muted()),
                        Span::styled(format!(" {ivl:>iw$}", iw = ivl_w), theme.muted()),
                        Span::styled(format!(" {}", fit(&r.due, due_w)), due_style),
                    ])
                }
            };
            let line = if is_cur || in_visual {
                let mut l = line;
                for s in &mut l.spans {
                    s.style = s.style.patch(if in_visual && !is_cur {
                        Style::default().bg(theme.bg_alt)
                    } else {
                        theme.selected()
                    });
                }
                l
            } else {
                line
            };
            lines.push(line);
        }
        if self.ids.is_empty() {
            lines.push(Line::from(Span::styled(
                if self.query.is_empty() { "  press / and type a search, Enter to run it" } else { "  no cards match" },
                theme.muted(),
            )));
        }
        f.render_widget(Paragraph::new(lines), table);

        // Preview
        if preview_h > 0 {
            let _ = self.preview_document(engine);
            let title = if self.preview_answer { " answer " } else { " question " };
            let pb = Block::default()
                .borders(Borders::TOP)
                .border_style(theme.border())
                .title(Span::styled(title, theme.muted()))
                .title(Line::from(Span::styled(" Tab flips ", theme.muted())).alignment(Alignment::Right));
            let pinner = pb.inner(preview);
            f.render_widget(pb, preview);
            if let Some((_, _, d)) = &self.preview_doc {
                let width = pinner.width.saturating_sub(2) as usize;
                let wrapped = wrap_document(d, width);
                let (lines, _) = doc::to_lines(&wrapped, width, theme, |_| None);
                f.render_widget(
                    Paragraph::new(lines),
                    Rect { x: pinner.x + 1, width: pinner.width.saturating_sub(2), ..pinner },
                );
            }
        }

        if let Some(info) = &self.info {
            draw_info(f, area, theme, info);
        }
    }
}

fn short_deck(d: &str) -> String {
    d.rsplit("::").next().unwrap_or(d).to_string()
}

fn fit(s: &str, w: usize) -> String {
    let sw = s.width();
    if sw <= w {
        return format!("{s}{}", " ".repeat(w - sw));
    }
    let mut acc = 0;
    let mut out = String::new();
    for c in s.chars() {
        let cw = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
        if acc + cw > w.saturating_sub(1) {
            break;
        }
        acc += cw;
        out.push(c);
    }
    out.push('…');
    let ow = out.width();
    format!("{out}{}", " ".repeat(w.saturating_sub(ow)))
}

fn draw_info(f: &mut Frame, area: Rect, theme: &Theme, c: &CardInfo) {
    let date = |t: i64| {
        chrono::DateTime::from_timestamp(t, 0)
            .map(|d| d.with_timezone(&chrono::Local).format("%Y-%m-%d").to_string())
            .unwrap_or_default()
    };
    let kv = |k: &str, v: String| Line::from(vec![Span::styled(format!(" {k:<14}"), theme.muted()), Span::raw(v)]);
    let mut lines = vec![
        kv("deck", c.deck.clone()),
        kv("notetype", format!("{} · {}", c.notetype, c.template)),
        kv("preset", c.preset.clone()),
        kv("added", date(c.added)),
        kv("first review", c.first_review.map(date).unwrap_or_else(|| "—".into())),
        kv("latest review", c.latest_review.map(date).unwrap_or_else(|| "—".into())),
        kv("due", c.due_date.map(date).unwrap_or_else(|| "—".into())),
        kv("interval", format!("{}d", c.interval_days)),
        kv("reviews", format!("{} ({} lapses)", c.reviews, c.lapses)),
        kv("time", format!("{:.0}s avg · {:.0}s total", c.average_secs, c.total_secs)),
    ];
    if let (Some(s), Some(d)) = (c.stability, c.difficulty) {
        lines.push(kv(
            "fsrs",
            format!(
                "stability {s:.1}d · difficulty {:.0}%{}",
                d * 10.0,
                c.retrievability.map(|r| format!(" · retrievability {:.0}%", r * 100.0)).unwrap_or_default()
            ),
        ));
    } else {
        lines.push(kv("ease", format!("{:.0}%", c.ease * 100.0)));
    }
    lines.push(kv("ids", format!("card {} · note {}", c.card_id, c.note_id)));
    if !c.revlog.is_empty() {
        lines.push(Line::default());
        lines.push(Line::from(Span::styled(" recent reviews", theme.muted().add_modifier(Modifier::BOLD))));
        for r in c.revlog.iter().take(8) {
            let btn = ["", "again", "hard", "good", "easy"].get(r.button as usize).copied().unwrap_or("?");
            let btn_color = match r.button {
                1 => theme.learn,
                2 => theme.warn,
                3 => theme.review,
                4 => theme.new,
                _ => theme.muted,
            };
            lines.push(Line::from(vec![
                Span::styled(format!(" {}  ", date(r.time)), theme.muted()),
                Span::styled(format!("{btn:<6}"), Style::default().fg(btn_color)),
                Span::styled(format!("{:<9}", r.kind), theme.muted()),
                Span::raw(format!("{:.0}s", r.taken_secs)),
            ]));
        }
    }
    let h = lines.len() as u16 + 2;
    let w = 62.min(area.width);
    let rect = Rect {
        x: area.x + (area.width - w) / 2,
        y: area.y + area.height.saturating_sub(h) / 2,
        width: w,
        height: h.min(area.height),
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.accent())
        .title(Span::styled(" card info ", theme.title()))
        .style(Style::default().bg(theme.bg_alt));
    f.render_widget(Clear, rect);
    f.render_widget(Paragraph::new(lines).block(block).wrap(Wrap { trim: false }), rect);
}
