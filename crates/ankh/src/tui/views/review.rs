//! The study screen. This is where the design budget goes.

use std::time::Instant;

use ankh_core::{Congrats, QueueKind, Rating, ReviewCard};
use ankh_render::{render_html, wrap_document, Align, Document, Options};
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::tui::banner;
use crate::tui::doc;
use crate::tui::theme::{CountKind, Theme};

pub enum Stage {
    Question,
    Answer,
    Done(Congrats),
    Empty,
}

pub struct ReviewView {
    pub deck_name: String,
    pub card: Option<ReviewCard>,
    pub stage: Stage,
    pub shown_at: Instant,
    pub scroll: u16,
    doc: Document,
    /// Cache of (width, wrapped lines) for the current doc.
    wrapped: Option<(usize, Vec<ankh_render::Line>)>,
    pub reviewed: u32,
    pub session_started: Instant,
}

impl ReviewView {
    pub fn new(deck_name: String) -> Self {
        ReviewView {
            deck_name,
            card: None,
            stage: Stage::Empty,
            shown_at: Instant::now(),
            scroll: 0,
            doc: Document::default(),
            wrapped: None,
            reviewed: 0,
            session_started: Instant::now(),
        }
    }

    fn set_doc(&mut self, html: &str) {
        self.doc = render_html(html, &Options { default_align: Align::Center });
        self.wrapped = None;
        self.scroll = 0;
    }

    pub fn show_card(&mut self, card: ReviewCard) {
        self.set_doc(&card.question_html);
        self.card = Some(card);
        self.stage = Stage::Question;
        self.shown_at = Instant::now();
    }

    pub fn show_answer(&mut self) {
        if let (Stage::Question, Some(c)) = (&self.stage, &self.card) {
            let html = c.answer_html.clone();
            self.set_doc(&html);
            self.stage = Stage::Answer;
        }
    }

    pub fn finish(&mut self, congrats: Congrats) {
        self.card = None;
        self.stage = Stage::Done(congrats);
        self.wrapped = None;
    }

    pub fn answer_shown(&self) -> bool {
        matches!(self.stage, Stage::Answer)
    }

    pub fn millis_taken(&self) -> u32 {
        self.shown_at.elapsed().as_millis().min(u32::MAX as u128) as u32
    }

    pub fn scroll_by(&mut self, d: i32) {
        self.scroll = (self.scroll as i32 + d).max(0) as u16;
    }

    fn lines(&mut self, width: usize) -> &Vec<ankh_render::Line> {
        if self.wrapped.as_ref().map(|(w, _)| *w != width).unwrap_or(true) {
            self.wrapped = Some((width, wrap_document(&self.doc, width)));
        }
        &self.wrapped.as_ref().unwrap().1
    }

    pub fn draw(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        let title = Line::from(vec![Span::styled(format!(" {} ", self.deck_name), theme.title())]);
        let counts = self.card.as_ref().map(|c| c.counts).unwrap_or_default();
        let kind = self.card.as_ref().map(|c| c.kind);
        let count_span = |n: u32, k: CountKind, active: bool| {
            let mut s = theme.count(k, n);
            if active {
                s = s.add_modifier(Modifier::UNDERLINED);
            }
            Span::styled(format!("{n}"), s)
        };
        let right = Line::from(vec![
            Span::raw(" "),
            count_span(counts.new, CountKind::New, kind == Some(QueueKind::New)),
            Span::styled(" · ", theme.muted()),
            count_span(counts.learn, CountKind::Learn, kind == Some(QueueKind::Learning)),
            Span::styled(" · ", theme.muted()),
            count_span(counts.review, CountKind::Review, kind == Some(QueueKind::Review)),
            Span::raw(" "),
        ]);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(theme.border())
            .title(title)
            .title(right.alignment(Alignment::Right));
        let inner = block.inner(area);
        f.render_widget(block, area);

        let [body, footer] = Layout::vertical([Constraint::Min(1), Constraint::Length(3)]).areas(inner);

        match &self.stage {
            Stage::Done(c) => {
                let c = c.clone();
                self.draw_done(f, body, theme, &c);
            }
            Stage::Empty => {
                f.render_widget(
                    Paragraph::new(Line::from(Span::styled("nothing to study", theme.muted())))
                        .alignment(Alignment::Center),
                    body,
                );
            }
            Stage::Question | Stage::Answer => {
                let margin = if body.width > 90 { (body.width - 90) / 2 } else { 2.min(body.width / 8) };
                let text_area = Rect {
                    x: body.x + margin,
                    y: body.y + 1,
                    width: body.width.saturating_sub(margin * 2),
                    height: body.height.saturating_sub(1),
                };
                let width = text_area.width as usize;
                let lines = self.lines(width).clone();
                let rendered = doc::to_lines(&lines, width, theme);
                let total = rendered.len() as u16;
                let max_scroll = total.saturating_sub(text_area.height);
                if self.scroll > max_scroll {
                    self.scroll = max_scroll;
                }
                // Vertically centre short cards.
                let pad = if total < text_area.height { (text_area.height - total) / 3 } else { 0 };
                let area = Rect { y: text_area.y + pad, height: text_area.height - pad, ..text_area };
                f.render_widget(Paragraph::new(rendered).scroll((self.scroll, 0)), area);
                if max_scroll > 0 {
                    let pct = (self.scroll as f32 / max_scroll as f32 * 100.0) as u16;
                    let s = format!("{pct}%");
                    let r = Rect {
                        x: body.right().saturating_sub(s.len() as u16 + 1),
                        y: body.bottom().saturating_sub(1),
                        width: s.len() as u16,
                        height: 1,
                    };
                    f.render_widget(Paragraph::new(Span::styled(s, theme.muted())), r);
                }
            }
        }
        self.draw_footer(f, footer, theme);
    }

    fn draw_footer(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let Some(card) = &self.card else {
            if matches!(self.stage, Stage::Done(_)) {
                let l = Line::from(vec![
                    Span::styled(" q ", theme.mode_pill("NORMAL")),
                    Span::styled(" back to decks   ", theme.muted()),
                    Span::styled(" U ", theme.mode_pill("NORMAL")),
                    Span::styled(" unbury ", theme.muted()),
                ])
                .alignment(Alignment::Center);
                f.render_widget(Paragraph::new(l), Rect { y: area.y + 1, height: 1, ..area });
            }
            return;
        };
        let [meta, buttons] = Layout::vertical([Constraint::Length(1), Constraint::Length(2)]).areas(area);
        // Meta line: kind pill, flag, marked, notetype, timer.
        let (kind_label, kind_color) = match card.kind {
            QueueKind::New => ("NEW", theme.new),
            QueueKind::Learning => ("LEARN", theme.learn),
            QueueKind::Review => ("REVIEW", theme.review),
        };
        let mut spans = vec![
            Span::styled(
                format!(" {kind_label} "),
                Style::default().fg(theme.bg).bg(kind_color).add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
        ];
        if card.flag != 0 {
            spans.push(Span::styled("⚑ ", Style::default().fg(flag_color(card.flag))));
        }
        if card.tags.iter().any(|t| t.eq_ignore_ascii_case("marked")) {
            spans.push(Span::styled("★ ", Style::default().fg(theme.warn)));
        }
        spans.push(Span::styled(card.notetype.clone(), theme.muted()));
        if !card.tags.is_empty() {
            let tags: Vec<&str> =
                card.tags.iter().filter(|t| !t.eq_ignore_ascii_case("marked")).map(String::as_str).take(4).collect();
            if !tags.is_empty() {
                spans.push(Span::styled(format!("  #{}", tags.join(" #")), theme.muted()));
            }
        }
        let secs = self.shown_at.elapsed().as_secs();
        let timer = Line::from(vec![
            Span::styled(format!("{} done · ", self.reviewed), theme.muted()),
            Span::styled(
                format!("{}:{:02} ", secs / 60, secs % 60),
                if secs > 60 { Style::default().fg(theme.warn) } else { theme.muted() },
            ),
        ])
        .alignment(Alignment::Right);
        f.render_widget(Paragraph::new(Line::from(spans)), meta);
        f.render_widget(Paragraph::new(timer), meta);

        // Buttons
        let line = if self.answer_shown() {
            let mut spans = Vec::new();
            let colors = [theme.learn, theme.warn, theme.review, theme.new];
            for (i, (label, ivl)) in [Rating::Again, Rating::Hard, Rating::Good, Rating::Easy]
                .iter()
                .map(|r| r.label())
                .zip(card.buttons.iter())
                .enumerate()
            {
                let c = colors[i];
                spans.push(Span::styled(
                    format!(" {} ", i + 1),
                    Style::default().fg(theme.bg).bg(c).add_modifier(Modifier::BOLD),
                ));
                spans.push(Span::styled(format!(" {label} "), Style::default().fg(c).add_modifier(Modifier::BOLD)));
                spans.push(Span::styled(ivl.to_string(), theme.muted()));
                spans.push(Span::raw("    "));
            }
            Line::from(spans).alignment(Alignment::Center)
        } else {
            Line::from(vec![
                Span::styled(" Space ", theme.mode_pill("NORMAL")),
                Span::styled(" show answer", theme.muted()),
            ])
            .alignment(Alignment::Center)
        };
        f.render_widget(Paragraph::new(line), Rect { y: buttons.y + 1, height: 1, ..buttons });
    }

    fn draw_done(&self, f: &mut Frame, area: Rect, theme: &Theme, c: &Congrats) {
        let mut lines = banner::lines("done", theme);
        for l in &mut lines {
            *l = l.clone().alignment(Alignment::Center);
        }
        lines.push(Line::default());
        let mins = self.session_started.elapsed().as_secs() / 60;
        lines.push(
            Line::from(vec![
                Span::styled(format!("{} cards", self.reviewed), theme.accent().add_modifier(Modifier::BOLD)),
                Span::styled(format!(" in {} min · {}", mins.max(1), self.deck_name), theme.muted()),
            ])
            .alignment(Alignment::Center),
        );
        lines.push(Line::default());
        let mut notes = Vec::new();
        if c.learn_remaining > 0 {
            let m = c.secs_until_next_learn / 60;
            notes.push(format!(
                "{} learning card{} due in {} min",
                c.learn_remaining,
                if c.learn_remaining == 1 { "" } else { "s" },
                m.max(1)
            ));
        }
        if c.review_remaining {
            notes.push("more reviews are waiting behind today's limit".into());
        }
        if c.new_remaining {
            notes.push("more new cards are waiting behind today's limit".into());
        }
        if c.have_buried {
            notes.push("some cards are buried — U to unbury".into());
        }
        if notes.is_empty() {
            notes.push("this deck is finished for today".into());
        }
        for n in notes {
            lines.push(Line::from(Span::styled(n, theme.muted())).alignment(Alignment::Center));
        }
        let h = lines.len() as u16;
        let y = area.y + area.height.saturating_sub(h) / 3;
        f.render_widget(Paragraph::new(lines), Rect { y, height: h.min(area.height), ..area });
    }
}

pub fn flag_color(flag: u8) -> Color {
    match flag {
        1 => Color::Rgb(0xf7, 0x76, 0x8e),
        2 => Color::Rgb(0xff, 0x9e, 0x64),
        3 => Color::Rgb(0x9e, 0xce, 0x6a),
        4 => Color::Rgb(0x7a, 0xa2, 0xf7),
        5 => Color::Rgb(0xff, 0x75, 0xa0),
        6 => Color::Rgb(0x73, 0xda, 0xca),
        7 => Color::Rgb(0xbb, 0x9a, 0xf7),
        _ => Color::Reset,
    }
}
