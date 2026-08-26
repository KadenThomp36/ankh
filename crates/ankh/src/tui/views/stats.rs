//! Statistics: the calendar heatmap first, then forecast, counts, retention.

use ankh_core::Stats;
use chrono::Datelike;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::tui::theme::Theme;

pub struct StatsView {
    pub title: String,
    pub stats: Stats,
    pub scroll: u16,
}

impl StatsView {
    pub fn new(title: String, stats: Stats) -> Self {
        StatsView { title, stats, scroll: 0 }
    }

    pub fn draw(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(theme.border())
            .title(Line::from(Span::styled(format!(" stats · {} ", self.title), theme.title())));
        let inner = block.inner(area);
        f.render_widget(block, area);
        let inner = Rect { x: inner.x + 1, width: inner.width.saturating_sub(2), ..inner };

        let mut lines: Vec<Line> = Vec::new();
        let s = &self.stats;

        // --- today
        let t = &s.today;
        let pct = t.correct.saturating_mul(100).checked_div(t.answered).unwrap_or(0);
        lines.push(section("today", theme));
        lines.push(Line::from(vec![
            Span::styled(format!("  {} ", t.answered), theme.accent().add_modifier(Modifier::BOLD)),
            Span::styled("cards in ", theme.muted()),
            Span::styled(format!("{:.1} min", t.secs / 60.0), Style::default().fg(theme.fg)),
            Span::styled(format!("  ·  {pct}% correct  ·  "), theme.muted()),
            Span::styled(format!("{}", t.learn), Style::default().fg(theme.new)),
            Span::styled(" learn  ", theme.muted()),
            Span::styled(format!("{}", t.review), Style::default().fg(theme.review)),
            Span::styled(" review  ", theme.muted()),
            Span::styled(format!("{}", t.relearn), Style::default().fg(theme.learn)),
            Span::styled(" relearn", theme.muted()),
        ]));
        lines.push(Line::default());

        // --- calendar heatmap: up to 52 weeks, columns = weeks, rows = weekdays
        lines.push(section("reviews · last year", theme));
        let weeks = ((inner.width.saturating_sub(6)) / 2).clamp(8, 52) as i32;
        let today = chrono::Local::now().date_naive();
        let weekday_today = today.weekday().num_days_from_monday() as i32; // 0 = Mon
        let max = s.reviews_per_day.values().copied().max().unwrap_or(1).max(1);
        let level = |n: u32| -> Color {
            if n == 0 {
                theme.bg_alt
            } else {
                let r = n as f32 / max as f32;
                let (cr, cg, cb) = match theme.review {
                    Color::Rgb(r, g, b) => (r as f32, g as f32, b as f32),
                    _ => (158.0, 206.0, 106.0),
                };
                let k = 0.35 + 0.65 * r.sqrt();
                Color::Rgb((cr * k) as u8, (cg * k) as u8, (cb * k) as u8)
            }
        };
        let labels = ["mon", "tue", "wed", "thu", "fri", "sat", "sun"];
        for wd in 0..7 {
            let mut spans = vec![Span::styled(format!("  {} ", labels[wd as usize]), theme.muted())];
            for w in (0..weeks).rev() {
                // days ago for this cell
                let ago = w * 7 + (weekday_today - wd);
                if ago < 0 {
                    spans.push(Span::raw("  "));
                    continue;
                }
                let n = s.reviews_per_day.get(&ago).copied().unwrap_or(0);
                spans.push(Span::styled("■ ", Style::default().fg(level(n))));
            }
            lines.push(Line::from(spans));
        }
        let year_total: u32 = s.reviews_per_day.values().sum();
        let streak = (0..).take_while(|d| s.reviews_per_day.get(d).copied().unwrap_or(0) > 0).count();
        let active = s.reviews_per_day.len();
        lines.push(Line::from(vec![
            Span::styled(format!("  {year_total} reviews on {active} days · "), theme.muted()),
            Span::styled(format!("{streak}-day streak"), if streak > 0 { theme.accent() } else { theme.muted() }),
        ]));
        lines.push(Line::default());

        // --- forecast (next 30 days)
        lines.push(section("due · next 30 days", theme));
        let horizon = 30;
        let backlog: u32 = s.forecast.iter().filter(|(d, _)| **d < 0).map(|(_, n)| n).sum();
        let fmax = (0..=horizon).map(|d| s.forecast.get(&d).copied().unwrap_or(0)).max().unwrap_or(1).max(1);
        let bar_h = 5u32;
        for row in (1..=bar_h).rev() {
            let mut spans = vec![Span::styled(
                format!(
                    "{:>5} ",
                    if row == bar_h {
                        fmax.to_string()
                    } else if row == 1 {
                        "0".into()
                    } else {
                        String::new()
                    }
                ),
                theme.muted(),
            )];
            for d in 0..=horizon {
                let n = s.forecast.get(&d).copied().unwrap_or(0);
                let h = (n as f32 / fmax as f32 * bar_h as f32).ceil() as u32;
                let color = if d == 0 { theme.review } else { theme.accent };
                spans.push(Span::styled(if h >= row { "▇ " } else { "  " }, Style::default().fg(color)));
            }
            lines.push(Line::from(spans));
        }
        lines.push(Line::from(vec![
            Span::styled("      today", theme.muted()),
            Span::styled(" ".repeat(2 * 14), theme.muted()),
            Span::styled("+15d", theme.muted()),
            Span::styled(" ".repeat(2 * 13), theme.muted()),
            Span::styled("+30d", theme.muted()),
        ]));
        let week: u32 = (0..=7).map(|d| s.forecast.get(&d).copied().unwrap_or(0)).sum();
        lines.push(Line::from(vec![
            Span::styled(format!("  {week} due this week"), theme.muted()),
            Span::styled(
                if backlog > 0 { format!(" · {backlog} overdue") } else { String::new() },
                Style::default().fg(theme.warn),
            ),
        ]));
        lines.push(Line::default());

        // --- card counts
        lines.push(section("cards", theme));
        let c = &s.counts;
        let total = c.new + c.learning + c.relearning + c.young + c.mature + c.suspended + c.buried;
        let width = inner.width.saturating_sub(4) as u32;
        let parts = [
            (c.new, theme.new, "new"),
            (c.learning + c.relearning, theme.learn, "learning"),
            (c.young, theme.review, "young"),
            (c.mature, Color::Rgb(0x6a, 0x9e, 0x4a), "mature"),
            (c.suspended, theme.warn, "suspended"),
            (c.buried, theme.muted, "buried"),
        ];
        let mut bar = vec![Span::raw("  ")];
        if total > 0 {
            for (n, color, _) in parts {
                let w = (n as u64 * width as u64 / total as u64) as usize;
                bar.push(Span::styled("█".repeat(w), Style::default().fg(color)));
            }
        }
        lines.push(Line::from(bar));
        let mut legend = vec![Span::raw("  ")];
        for (n, color, name) in parts {
            legend.push(Span::styled("■ ", Style::default().fg(color)));
            legend.push(Span::styled(format!("{n} {name}   "), theme.muted()));
        }
        lines.push(Line::from(legend));
        lines.push(Line::from(Span::styled(format!("  {total} cards total"), theme.muted())));
        lines.push(Line::default());

        // --- retention / buttons
        lines.push(section("answers · last month", theme));
        let names = ["learning", "young", "mature"];
        for (i, row) in s.buttons.iter().enumerate() {
            let tot: u32 = row.iter().sum();
            let mut spans = vec![Span::styled(format!("  {:<9}", names[i]), theme.muted())];
            if tot == 0 {
                spans.push(Span::styled("—", theme.muted()));
            } else {
                let colors = [theme.learn, theme.warn, theme.review, theme.new];
                let bw = inner.width.saturating_sub(40).max(10) as u32;
                for (j, n) in row.iter().enumerate() {
                    let w = (*n as u64 * bw as u64 / tot as u64) as usize;
                    spans.push(Span::styled("█".repeat(w), Style::default().fg(colors[j])));
                }
                let correct = tot - row[0];
                spans.push(Span::styled(
                    format!("  {:.0}% correct ({tot})", correct as f32 * 100.0 / tot as f32),
                    theme.muted(),
                ));
            }
            lines.push(Line::from(spans));
        }
        lines.push(Line::from(vec![
            Span::styled("  average retrievability ", theme.muted()),
            Span::styled(format!("{:.0}%", s.average_retrievability), theme.accent()),
        ]));
        lines.push(Line::default());

        // --- hours
        lines.push(section("reviews by hour · last month", theme));
        let hmax = s.hours.iter().map(|(t, _)| *t).max().unwrap_or(1).max(1);
        let mut spans = vec![Span::raw("  ")];
        for (t, c) in &s.hours {
            let lvl = (*t as f32 / hmax as f32 * 7.0).round() as usize;
            let glyph = [" ", "▁", "▂", "▃", "▄", "▅", "▆", "▇"][lvl.min(7)];
            let ok = if *t > 0 { *c as f32 / *t as f32 } else { 1.0 };
            let color = if ok < 0.7 { theme.learn } else { theme.review };
            spans.push(Span::styled(glyph.to_string(), Style::default().fg(color)));
            spans.push(Span::raw(" "));
        }
        lines.push(Line::from(spans));
        lines.push(Line::from(Span::styled("  0h                    12h                    23h", theme.muted())));
        lines.push(Line::default());

        // --- intervals
        lines.push(section("intervals", theme));
        let buckets = [(1, 7, "≤1w"), (8, 30, "≤1mo"), (31, 90, "≤3mo"), (91, 365, "≤1y"), (366, u32::MAX, ">1y")];
        let itotal: u32 = s.intervals.values().sum();
        for (lo, hi, name) in buckets {
            let n: u32 = s.intervals.iter().filter(|(d, _)| **d >= lo && **d <= hi).map(|(_, n)| n).sum();
            let w = if itotal > 0 {
                (n as u64 * (inner.width.saturating_sub(30).max(10)) as u64 / itotal as u64) as usize
            } else {
                0
            };
            lines.push(Line::from(vec![
                Span::styled(format!("  {name:<5}"), theme.muted()),
                Span::styled("█".repeat(w), theme.accent()),
                Span::styled(format!(" {n}"), theme.muted()),
            ]));
        }

        let total_h = lines.len() as u16;
        let max_scroll = total_h.saturating_sub(inner.height);
        if self.scroll > max_scroll {
            self.scroll = max_scroll;
        }
        f.render_widget(Paragraph::new(lines).scroll((self.scroll, 0)), inner);
        if max_scroll > 0 {
            let s = format!("{}%", self.scroll as u32 * 100 / max_scroll as u32);
            f.render_widget(
                Paragraph::new(Span::styled(s.clone(), theme.muted())).alignment(Alignment::Right),
                Rect {
                    x: inner.right().saturating_sub(s.len() as u16),
                    y: inner.bottom().saturating_sub(1),
                    width: s.len() as u16,
                    height: 1,
                },
            );
        }
        let _ = Layout::vertical([Constraint::Min(0)]);
    }
}

fn section<'a>(title: &str, theme: &Theme) -> Line<'a> {
    Line::from(Span::styled(format!("─ {title} "), theme.muted().add_modifier(Modifier::BOLD)))
}
