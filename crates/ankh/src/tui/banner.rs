//! ASCII art. Rendered from a bundled figlet font so themes can recolour it
//! and the same code draws the splash, deck headers and the "done" screen.

use figlet_rs::FIGfont;
use ratatui::style::Style;
use ratatui::text::{Line, Span};

use super::theme::Theme;

pub fn lines(text: &str, theme: &Theme) -> Vec<Line<'static>> {
    let font = FIGfont::standard().expect("bundled figlet font");
    let Some(fig) = font.convert(text) else { return vec![] };
    let rendered = fig.to_string();
    let raw: Vec<&str> = rendered.lines().collect();
    // Trim blank leading/trailing rows figlet likes to add.
    let first = raw.iter().position(|l| !l.trim().is_empty()).unwrap_or(0);
    let last = raw.iter().rposition(|l| !l.trim().is_empty()).unwrap_or(0);
    let rows = &raw[first..=last];
    let n = rows.len().max(1);
    rows.iter()
        .enumerate()
        .map(|(i, l)| {
            let idx = (i * theme.banner.len()) / n;
            Line::from(Span::styled(
                l.trim_end().to_string(),
                Style::default().fg(theme.banner[idx.min(theme.banner.len() - 1)]),
            ))
        })
        .collect()
}

pub fn width(lines: &[Line<'_>]) -> u16 {
    lines.iter().map(|l| l.width() as u16).max().unwrap_or(0)
}
