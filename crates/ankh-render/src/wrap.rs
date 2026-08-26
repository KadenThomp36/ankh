//! Width-aware wrapping of a [`Document`] into terminal lines.
//!
//! Break opportunities: after whitespace, after hyphens, and between any
//! two CJK graphemes. A ruby span is atomic and reserves the wider of its
//! base and annotation. Widths come from `unicode-width`, so combining
//! marks, CJK and emoji occupy the right number of cells.

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::{Align, Block, Document, Span, Style};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub spans: Vec<Span>,
    pub align: Align,
    pub indent: u16,
    /// A parallel line of ruby annotations to draw directly above `spans`,
    /// already padded so columns line up.
    pub annotation: Option<Vec<Span>>,
    pub kind: LineKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
    Text,
    Rule,
    Image,
    Blank,
}

impl Line {
    pub fn width(&self) -> usize {
        self.spans.iter().map(|s| s.text.width()).sum::<usize>() + self.indent as usize
    }
}

/// One unbreakable unit of text.
struct Unit {
    text: String,
    style: Style,
    ruby: Option<String>,
    width: usize,
    /// May a line break follow this unit?
    breakable_after: bool,
    ends_with_space: bool,
}

fn is_cjk(g: &str) -> bool {
    g.chars()
        .next()
        .map(|c| {
            matches!(c as u32,
            0x1100..=0x11FF | 0x2E80..=0x303F | 0x3040..=0x30FF | 0x3100..=0x31FF | 0x3400..=0x4DBF |
            0x4E00..=0x9FFF | 0xA960..=0xA97F | 0xAC00..=0xD7FF | 0xF900..=0xFAFF | 0xFE30..=0xFE4F |
            0xFF00..=0xFFEF | 0x20000..=0x3134F)
        })
        .unwrap_or(false)
}

fn units(spans: &[Span]) -> Vec<Unit> {
    let mut out = Vec::new();
    for s in spans {
        if let Some(r) = &s.ruby {
            let w = s.text.width().max(r.width());
            out.push(Unit {
                text: s.text.clone(),
                style: s.style,
                ruby: Some(r.clone()),
                width: w,
                breakable_after: true,
                ends_with_space: false,
            });
            continue;
        }
        let mut cur = String::new();
        let flush = |cur: &mut String, out: &mut Vec<Unit>, breakable: bool| {
            if !cur.is_empty() {
                let w = cur.width();
                let sp = cur.ends_with(' ');
                out.push(Unit {
                    text: std::mem::take(cur),
                    style: s.style,
                    ruby: None,
                    width: w,
                    breakable_after: breakable,
                    ends_with_space: sp,
                });
            }
        };
        for g in s.text.graphemes(true) {
            if g.chars().all(char::is_whitespace) {
                cur.push_str(g);
                flush(&mut cur, &mut out, true);
            } else if is_cjk(g) {
                flush(&mut cur, &mut out, true);
                cur.push_str(g);
                flush(&mut cur, &mut out, true);
            } else if g == "-" || g == "‐" || g == "–" {
                cur.push_str(g);
                flush(&mut cur, &mut out, true);
            } else {
                cur.push_str(g);
            }
        }
        flush(&mut cur, &mut out, false);
    }
    // The last unit of a span followed by another span's first unit: allow a break
    // only if the boundary is a space/CJK boundary, which the loop already encoded.
    out
}

fn split_hard(u: &Unit, width: usize) -> Vec<Unit> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut w = 0;
    for g in u.text.graphemes(true) {
        let gw = g.width();
        if w + gw > width && !cur.is_empty() {
            out.push(Unit {
                text: std::mem::take(&mut cur),
                style: u.style,
                ruby: None,
                width: w,
                breakable_after: true,
                ends_with_space: false,
            });
            w = 0;
        }
        cur.push_str(g);
        w += gw;
    }
    if !cur.is_empty() {
        out.push(Unit {
            text: cur,
            style: u.style,
            ruby: None,
            width: w,
            breakable_after: u.breakable_after,
            ends_with_space: u.ends_with_space,
        });
    }
    out
}

fn emit(line_units: &mut Vec<Unit>, align: Align, indent: u16, out: &mut Vec<Line>) {
    if line_units.is_empty() {
        return;
    }
    // Drop trailing whitespace on the wrapped line.
    if let Some(last) = line_units.last_mut() {
        let t = last.text.trim_end().to_string();
        last.width = t.width();
        last.text = t;
    }
    let has_ruby = line_units.iter().any(|u| u.ruby.is_some());
    let mut spans = Vec::new();
    let mut ann: Vec<Span> = Vec::new();
    for u in line_units.drain(..) {
        if u.text.is_empty() && u.ruby.is_none() {
            continue;
        }
        let base_w = u.text.width();
        let (base_pad_l, base_pad_r) = pad_to(base_w, u.width);
        let text = format!("{}{}{}", " ".repeat(base_pad_l), u.text, " ".repeat(base_pad_r));
        if has_ruby {
            let r = u.ruby.clone().unwrap_or_default();
            let (l, rr) = pad_to(r.width(), u.width);
            let ann_text = format!("{}{}{}", " ".repeat(l), r, " ".repeat(rr));
            let mut st = Style { dim: true, ..Style::default() };
            st.fg = u.style.fg;
            ann.push(Span { text: ann_text, style: st, ruby: None });
        }
        spans.push(Span { text, style: u.style, ruby: u.ruby });
    }
    // Merge adjacent same-style spans for cheaper drawing.
    let mut merged: Vec<Span> = Vec::with_capacity(spans.len());
    for s in spans {
        match merged.last_mut() {
            Some(l) if l.style == s.style && l.ruby.is_none() && s.ruby.is_none() => l.text.push_str(&s.text),
            _ => merged.push(s),
        }
    }
    out.push(Line {
        spans: merged,
        align,
        indent,
        annotation: if has_ruby { Some(ann) } else { None },
        kind: LineKind::Text,
    });
}

fn pad_to(w: usize, target: usize) -> (usize, usize) {
    let extra = target.saturating_sub(w);
    (extra / 2, extra - extra / 2)
}

pub fn wrap_document(doc: &Document, width: usize) -> Vec<Line> {
    let width = width.max(4);
    let mut out = Vec::new();
    for b in &doc.blocks {
        match b {
            Block::Rule => out.push(Line {
                spans: vec![],
                align: Align::Center,
                indent: 0,
                annotation: None,
                kind: LineKind::Rule,
            }),
            Block::Blank => {
                out.push(Line { spans: vec![], align: Align::Left, indent: 0, annotation: None, kind: LineKind::Blank })
            }
            Block::Image { src, alt } => out.push(Line {
                spans: vec![Span::plain(src.clone()), Span::plain(alt.clone())],
                align: Align::Center,
                indent: 0,
                annotation: None,
                kind: LineKind::Image,
            }),
            Block::Para { spans, align, indent } => {
                let avail = width.saturating_sub(*indent as usize).max(2);
                let mut line: Vec<Unit> = Vec::new();
                let mut w = 0;
                for u in units(spans) {
                    let pieces = if u.width > avail { split_hard(&u, avail) } else { vec![u] };
                    for u in pieces {
                        let uw_visible = if u.ends_with_space { u.width.saturating_sub(1) } else { u.width };
                        if w + uw_visible > avail && !line.is_empty() {
                            emit(&mut line, *align, *indent, &mut out);
                            w = 0;
                            // Don't start a line with pure whitespace.
                            if u.text.trim().is_empty() && u.ruby.is_none() {
                                continue;
                            }
                        }
                        w += u.width;
                        line.push(u);
                    }
                }
                emit(&mut line, *align, *indent, &mut out);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{render_html, Options};

    fn text(l: &Line) -> String {
        l.spans.iter().map(|s| s.text.as_str()).collect()
    }

    #[test]
    fn wraps_latin_on_spaces() {
        let doc = render_html("the quick brown fox jumps over the lazy dog", &Options::default());
        let lines = wrap_document(&doc, 16);
        let t: Vec<String> = lines.iter().map(text).collect();
        assert_eq!(t, vec!["the quick brown", "fox jumps over", "the lazy dog"]);
        assert!(lines.iter().all(|l| l.width() <= 16));
    }

    #[test]
    fn wraps_cjk_anywhere_with_correct_widths() {
        let doc = render_html("죄송합니다. 저는 한국어를 공부합니다.", &Options::default());
        let lines = wrap_document(&doc, 12);
        for l in &lines {
            assert!(l.width() <= 12, "{:?} is {}", text(l), l.width());
        }
        assert_eq!(text(&lines[0]), "죄송합니다.");
    }

    #[test]
    fn ruby_is_atomic_and_aligned() {
        let doc = render_html("<ruby>漢<rt>かん</rt></ruby><ruby>字<rt>じ</rt></ruby>を書く", &Options::default());
        let lines = wrap_document(&doc, 40);
        assert_eq!(lines.len(), 1);
        let ann: String = lines[0].annotation.as_ref().unwrap().iter().map(|s| s.text.as_str()).collect();
        let base = text(&lines[0]);
        assert_eq!(ann.width(), base.width());
        // 漢 (2 cells) sits centred under かん (4 cells).
        assert_eq!(base, " 漢 字を書く");
        assert_eq!(ann.trim_end(), "かんじ");
    }

    #[test]
    fn hard_breaks_overlong_words() {
        let doc = render_html("supercalifragilisticexpialidocious", &Options::default());
        let lines = wrap_document(&doc, 10);
        assert_eq!(lines.len(), 4);
        assert!(lines.iter().all(|l| l.width() <= 10));
    }
}
