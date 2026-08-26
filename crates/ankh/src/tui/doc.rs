//! Bridge from `ankh-render` lines to ratatui lines.

use ankh_render::{Align, Line as RLine, LineKind, Style as RStyle};
use ratatui::layout::Alignment;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use super::theme::Theme;

pub fn style(s: &RStyle, theme: &Theme) -> Style {
    let mut st = Style::default();
    if let Some(c) = s.fg {
        st = st.fg(Color::Rgb(c.0, c.1, c.2));
    }
    if let Some(c) = s.bg {
        st = st.bg(Color::Rgb(c.0, c.1, c.2));
    }
    if s.bold {
        st = st.add_modifier(Modifier::BOLD);
    }
    if s.italic {
        st = st.add_modifier(Modifier::ITALIC);
    }
    if s.underline {
        st = st.add_modifier(Modifier::UNDERLINED);
    }
    if s.strike {
        st = st.add_modifier(Modifier::CROSSED_OUT);
    }
    if s.dim {
        st = st.add_modifier(Modifier::DIM);
    }
    if s.code {
        st = st.bg(theme.bg_alt).fg(theme.accent);
    }
    st
}

fn alignment(a: Align) -> Alignment {
    match a {
        Align::Center => Alignment::Center,
        Align::Right => Alignment::Right,
        _ => Alignment::Left,
    }
}

/// Convert wrapped lines to ratatui lines. `width` is used to draw rules.
pub fn to_lines(lines: &[RLine], width: usize, theme: &Theme) -> Vec<Line<'static>> {
    let mut out = Vec::with_capacity(lines.len());
    for l in lines {
        match l.kind {
            LineKind::Blank => out.push(Line::default()),
            LineKind::Rule => {
                let w = (width * 2 / 3).max(8);
                out.push(Line::from(Span::styled("─".repeat(w), theme.border())).alignment(Alignment::Center));
            }
            LineKind::Image => {
                let src = l.spans.first().map(|s| s.text.clone()).unwrap_or_default();
                out.push(
                    Line::from(vec![
                        Span::styled("  ", theme.muted()),
                        Span::styled(src, theme.muted().add_modifier(Modifier::ITALIC)),
                    ])
                    .alignment(Alignment::Center),
                );
            }
            LineKind::Text => {
                let indent = " ".repeat(l.indent as usize);
                if let Some(ann) = &l.annotation {
                    let mut spans = vec![Span::raw(indent.clone())];
                    spans.extend(ann.iter().map(|s| Span::styled(s.text.clone(), style(&s.style, theme))));
                    out.push(Line::from(spans).alignment(alignment(l.align)));
                }
                let mut spans = vec![Span::raw(indent)];
                spans.extend(l.spans.iter().map(|s| Span::styled(s.text.clone(), style(&s.style, theme))));
                out.push(Line::from(spans).alignment(alignment(l.align)));
            }
        }
    }
    out
}
