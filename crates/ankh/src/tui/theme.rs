//! Colour is information here, not decoration: new/learn/review always use
//! the same three hues everywhere in the app.

use ratatui::style::{Color, Modifier, Style};

#[derive(Debug, Clone)]
#[allow(dead_code)] // fields used by later views
pub struct Theme {
    pub name: &'static str,
    pub bg: Color,
    pub bg_alt: Color,
    pub fg: Color,
    pub muted: Color,
    pub border: Color,
    pub accent: Color,
    pub selection: Color,
    pub new: Color,
    pub learn: Color,
    pub review: Color,
    pub warn: Color,
    pub error: Color,
    pub ok: Color,
    /// Gradient used for banners/ASCII art, top to bottom.
    pub banner: [Color; 4],
}

impl Theme {
    pub fn tokyonight() -> Self {
        Theme {
            name: "tokyonight",
            bg: Color::Rgb(0x1a, 0x1b, 0x26),
            bg_alt: Color::Rgb(0x24, 0x28, 0x3b),
            fg: Color::Rgb(0xc0, 0xca, 0xf5),
            muted: Color::Rgb(0x56, 0x5f, 0x89),
            border: Color::Rgb(0x3b, 0x42, 0x61),
            accent: Color::Rgb(0x7a, 0xa2, 0xf7),
            selection: Color::Rgb(0x2f, 0x33, 0x4d),
            new: Color::Rgb(0x7a, 0xa2, 0xf7),
            learn: Color::Rgb(0xf7, 0x76, 0x8e),
            review: Color::Rgb(0x9e, 0xce, 0x6a),
            warn: Color::Rgb(0xe0, 0xaf, 0x68),
            error: Color::Rgb(0xf7, 0x76, 0x8e),
            ok: Color::Rgb(0x9e, 0xce, 0x6a),
            banner: [
                Color::Rgb(0xbb, 0x9a, 0xf7),
                Color::Rgb(0x9d, 0x9e, 0xf7),
                Color::Rgb(0x7a, 0xa2, 0xf7),
                Color::Rgb(0x7d, 0xcf, 0xff),
            ],
        }
    }

    pub fn base(&self) -> Style {
        Style::default().fg(self.fg).bg(self.bg)
    }
    pub fn muted(&self) -> Style {
        Style::default().fg(self.muted)
    }
    pub fn accent(&self) -> Style {
        Style::default().fg(self.accent)
    }
    pub fn border(&self) -> Style {
        Style::default().fg(self.border)
    }
    pub fn title(&self) -> Style {
        Style::default().fg(self.accent).add_modifier(Modifier::BOLD)
    }
    pub fn selected(&self) -> Style {
        Style::default().bg(self.selection).add_modifier(Modifier::BOLD)
    }
    pub fn count(&self, kind: CountKind, n: u32) -> Style {
        if n == 0 {
            return self.muted();
        }
        let c = match kind {
            CountKind::New => self.new,
            CountKind::Learn => self.learn,
            CountKind::Review => self.review,
        };
        Style::default().fg(c).add_modifier(Modifier::BOLD)
    }
    pub fn mode_pill(&self, mode: &str) -> Style {
        let bg = match mode {
            "NORMAL" => self.accent,
            "INSERT" => self.review,
            "COMMAND" => self.warn,
            "VISUAL" => Color::Rgb(0xbb, 0x9a, 0xf7),
            _ => self.muted,
        };
        Style::default().fg(self.bg).bg(bg).add_modifier(Modifier::BOLD)
    }
}

#[derive(Debug, Clone, Copy)]
pub enum CountKind {
    New,
    Learn,
    Review,
}
