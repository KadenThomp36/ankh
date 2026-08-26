//! Bridge from `ankh-render` lines to ratatui lines.

use ankh_render::{Align, Line as RLine, LineKind, Style as RStyle};
use ratatui::layout::Alignment;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use super::theme::Theme;

fn luma(c: ankh_render::Color) -> f32 {
    (0.2126 * c.0 as f32 + 0.7152 * c.1 as f32 + 0.0722 * c.2 as f32) / 255.0
}

pub fn style(s: &RStyle, theme: &Theme) -> Style {
    let mut st = Style::default();
    if let Some(c) = s.fg {
        // Card authors pick colours for a white (or black) page. Near-black text
        // on a dark theme (or near-white on a light one) would vanish; let the
        // theme's foreground stand in unless the card also sets a background.
        let l = luma(c);
        let invisible = s.bg.is_none() && ((theme.dark && l < 0.22) || (!theme.dark && l > 0.85));
        if !invisible {
            st = st.fg(Color::Rgb(c.0, c.1, c.2));
        }
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

/// Where an image goes in the rendered text: its first row and its cell size.
pub struct ImageSlot {
    pub row: usize,
    pub src: String,
    pub width: u16,
    pub height: u16,
}

/// Convert wrapped lines to ratatui lines. `width` is used to draw rules.
/// `image_size` decides how many rows an image gets (None → a text placeholder).
pub fn to_lines(
    lines: &[RLine],
    width: usize,
    theme: &Theme,
    mut image_size: impl FnMut(&str) -> Option<(u16, u16)>,
) -> (Vec<Line<'static>>, Vec<ImageSlot>) {
    let mut out = Vec::with_capacity(lines.len());
    let mut slots = Vec::new();
    for l in lines {
        match l.kind {
            LineKind::Blank => out.push(Line::default()),
            LineKind::Rule => {
                let w = (width * 2 / 3).max(8);
                out.push(Line::from(Span::styled("─".repeat(w), theme.border())).alignment(Alignment::Center));
            }
            LineKind::Image => {
                let src = l.spans.first().map(|s| s.text.clone()).unwrap_or_default();
                match image_size(&src) {
                    Some((w, h)) => {
                        slots.push(ImageSlot { row: out.len(), src, width: w, height: h });
                        for _ in 0..h {
                            out.push(Line::default());
                        }
                    }
                    None => out.push(
                        Line::from(vec![
                            Span::styled("  ", theme.muted()),
                            Span::styled(src, theme.muted().add_modifier(Modifier::ITALIC)),
                        ])
                        .alignment(Alignment::Center),
                    ),
                }
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
    (out, slots)
}
