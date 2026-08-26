//! Colour is information here, not decoration: new/learn/review always use
//! the same three hues everywhere in the app.

use ratatui::style::{Color, Modifier, Style};

#[derive(Debug, Clone)]
#[allow(dead_code)] // fields used by later views
pub struct Theme {
    pub name: &'static str,
    pub dark: bool,
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
            dark: true,
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

    pub fn by_name(name: &str) -> Option<Theme> {
        Some(match name.to_ascii_lowercase().replace('_', "-").as_str() {
            "tokyonight" | "tokyo-night" => Theme::tokyonight(),
            "catppuccin" | "catppuccin-mocha" | "mocha" => Theme::catppuccin(),
            "gruvbox" | "gruvbox-dark" => Theme::gruvbox(),
            "rose-pine" | "rosepine" => Theme::rose_pine(),
            "nord" => Theme::nord(),
            "dracula" => Theme::dracula(),
            _ => return None,
        })
    }

    pub const NAMES: [&'static str; 6] = ["tokyonight", "catppuccin", "gruvbox", "rose-pine", "nord", "dracula"];

    fn rgb(hex: u32) -> Color {
        Color::Rgb((hex >> 16) as u8, (hex >> 8 & 0xff) as u8, (hex & 0xff) as u8)
    }

    #[allow(clippy::too_many_arguments)]
    fn build(
        name: &'static str,
        bg: u32,
        bg_alt: u32,
        fg: u32,
        muted: u32,
        border: u32,
        accent: u32,
        selection: u32,
        new: u32,
        learn: u32,
        review: u32,
        warn: u32,
        banner: [u32; 4],
    ) -> Theme {
        let c = Theme::rgb;
        Theme {
            name,
            dark: true,
            bg: c(bg),
            bg_alt: c(bg_alt),
            fg: c(fg),
            muted: c(muted),
            border: c(border),
            accent: c(accent),
            selection: c(selection),
            new: c(new),
            learn: c(learn),
            review: c(review),
            warn: c(warn),
            error: c(learn),
            ok: c(review),
            banner: [c(banner[0]), c(banner[1]), c(banner[2]), c(banner[3])],
        }
    }

    pub fn catppuccin() -> Self {
        Theme::build(
            "catppuccin",
            0x1e1e2e,
            0x313244,
            0xcdd6f4,
            0x6c7086,
            0x45475a,
            0x89b4fa,
            0x45475a,
            0x89b4fa,
            0xf38ba8,
            0xa6e3a1,
            0xf9e2af,
            [0xcba6f7, 0xb4befe, 0x89b4fa, 0x89dceb],
        )
    }
    pub fn gruvbox() -> Self {
        Theme::build(
            "gruvbox",
            0x282828,
            0x3c3836,
            0xebdbb2,
            0x928374,
            0x504945,
            0x83a598,
            0x504945,
            0x83a598,
            0xfb4934,
            0xb8bb26,
            0xfabd2f,
            [0xd3869b, 0xb16286, 0x83a598, 0x8ec07c],
        )
    }
    pub fn rose_pine() -> Self {
        Theme::build(
            "rose-pine",
            0x191724,
            0x1f1d2e,
            0xe0def4,
            0x6e6a86,
            0x403d52,
            0xc4a7e7,
            0x403d52,
            0x9ccfd8,
            0xeb6f92,
            0x31748f,
            0xf6c177,
            [0xebbcba, 0xc4a7e7, 0x9ccfd8, 0x31748f],
        )
    }
    pub fn nord() -> Self {
        Theme::build(
            "nord",
            0x2e3440,
            0x3b4252,
            0xeceff4,
            0x4c566a,
            0x434c5e,
            0x88c0d0,
            0x434c5e,
            0x81a1c1,
            0xbf616a,
            0xa3be8c,
            0xebcb8b,
            [0xb48ead, 0x81a1c1, 0x88c0d0, 0x8fbcbb],
        )
    }
    pub fn dracula() -> Self {
        Theme::build(
            "dracula",
            0x282a36,
            0x44475a,
            0xf8f8f2,
            0x6272a4,
            0x44475a,
            0xbd93f9,
            0x44475a,
            0x8be9fd,
            0xff5555,
            0x50fa7b,
            0xf1fa8c,
            [0xff79c6, 0xbd93f9, 0x8be9fd, 0x50fa7b],
        )
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

/// Best-effort guess without querying the terminal: `COLORFGBG` (set by
/// rxvt, konsole, some others) is "fg;bg" with bg ≤ 6 meaning dark.
pub fn terminal_is_dark() -> bool {
    match std::env::var("COLORFGBG") {
        Ok(v) => v.rsplit(';').next().and_then(|bg| bg.parse::<u8>().ok()).map(|bg| bg <= 6 || bg == 8).unwrap_or(true),
        Err(_) => true,
    }
}
