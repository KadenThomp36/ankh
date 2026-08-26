//! HTML → [`Document`] → wrapped lines.
//!
//! Anki cards are HTML documents. Terminals are not browsers. This crate
//! extracts the *meaning* of a card — paragraphs, emphasis, colour, ruby,
//! images, rules — into a small, styled block model, and wraps it to a
//! width with correct Unicode widths (CJK, emoji, combining marks).
//!
//! What is honoured: block structure (`div`, `p`, `br`, `li`, `hN`, `hr`,
//! `table` rows, `blockquote`), inline emphasis (`b/strong`, `i/em`, `u`,
//! `s/del`, `code`, `sub/sup`), `<ruby>`/`<rt>`, `<img>`, and the inline CSS
//! subset `color`, `background(-color)`, `font-weight`, `font-style`,
//! `text-decoration`, `text-align`, `display:none`. Everything else is
//! ignored on purpose.

mod color;
mod css;
mod html;
mod wrap;

pub use color::Color;
pub use html::{render_html, Options};
pub use wrap::{wrap_document, Line, LineKind};

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
pub struct Style {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub dim: bool,
    pub code: bool,
    pub fg: Option<Color>,
    pub bg: Option<Color>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Span {
    pub text: String,
    pub style: Style,
    /// Ruby annotation (furigana) that belongs above this text.
    pub ruby: Option<String>,
}

impl Span {
    pub fn plain(text: impl Into<String>) -> Self {
        Span { text: text.into(), style: Style::default(), ruby: None }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Align {
    #[default]
    Inherit,
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Block {
    Para {
        spans: Vec<Span>,
        align: Align,
        indent: u16,
    },
    Rule,
    Image {
        src: String,
        alt: String,
    },
    /// Vertical whitespace between blocks (from empty `<div>`/`<p>`, `<br><br>`).
    Blank,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Document {
    pub blocks: Vec<Block>,
}

impl Document {
    /// Plain text with newlines between blocks; what `--format json` and
    /// searches want.
    pub fn plain_text(&self) -> String {
        let mut out = String::new();
        for b in &self.blocks {
            match b {
                Block::Para { spans, .. } => {
                    for s in spans {
                        out.push_str(&s.text);
                        if let Some(r) = &s.ruby {
                            out.push('(');
                            out.push_str(r);
                            out.push(')');
                        }
                    }
                    out.push('\n');
                }
                Block::Rule => out.push_str("---\n"),
                Block::Image { alt, src } => {
                    out.push_str(&format!("[image: {}]\n", if alt.is_empty() { src } else { alt }))
                }
                Block::Blank => out.push('\n'),
            }
        }
        out.trim_end().to_string()
    }

    pub fn images(&self) -> impl Iterator<Item = &str> {
        self.blocks.iter().filter_map(|b| match b {
            Block::Image { src, .. } => Some(src.as_str()),
            _ => None,
        })
    }
}
