//! Walk an HTML fragment and emit blocks.

use ego_tree::NodeRef;
use scraper::{Html, Node};

use crate::color::Color;
use crate::css::{parse_inline, Decl};
use crate::{Align, Block, Document, Span, Style};

#[derive(Debug, Clone, Copy)]
pub struct Options {
    /// Alignment for blocks that don't set their own (Anki's `.card` is
    /// usually centered).
    pub default_align: Align,
}

impl Default for Options {
    fn default() -> Self {
        Options { default_align: Align::Left }
    }
}

pub fn render_html(html: &str, opts: &Options) -> Document {
    let frag = Html::parse_fragment(html);
    let mut b = Builder { blocks: vec![], spans: vec![], align: Align::Inherit, indent: 0, prefix: None, opts: *opts };
    b.walk_children(frag.tree.root(), Style::default(), Align::Inherit, 0);
    b.flush();
    b.trim_blanks();
    Document { blocks: b.blocks }
}

struct Builder {
    blocks: Vec<Block>,
    spans: Vec<Span>,
    align: Align,
    indent: u16,
    /// Text to put at the start of the next paragraph (list bullets, quote bars).
    prefix: Option<String>,
    opts: Options,
}

const BLOCK_TAGS: &[&str] = &[
    "div",
    "p",
    "li",
    "ul",
    "ol",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "blockquote",
    "table",
    "tr",
    "thead",
    "tbody",
    "tfoot",
    "section",
    "article",
    "header",
    "footer",
    "center",
    "pre",
    "dl",
    "dt",
    "dd",
    "figure",
    "figcaption",
    "details",
    "summary",
    "form",
    "fieldset",
    "address",
    "main",
    "nav",
    "aside",
];
const SKIP_TAGS: &[&str] = &[
    "script", "style", "head", "title", "template", "noscript", "rp", "svg", "math", "audio", "video", "source",
    "track",
];

fn merge(parent: Style, d: &Decl) -> Style {
    let mut s = parent;
    s.bold = (s.bold && !d.unbold) || d.style.bold;
    s.italic = (s.italic && !d.unitalic) || d.style.italic;
    s.underline |= d.style.underline;
    s.strike |= d.style.strike;
    s.dim |= d.style.dim;
    s.code |= d.style.code;
    if d.style.fg.is_some() {
        s.fg = d.style.fg;
    }
    if d.style.bg.is_some() {
        s.bg = d.style.bg;
    }
    s
}

impl Builder {
    fn last_is_space(&self) -> bool {
        match self.spans.last() {
            None => true,
            Some(s) => s.text.ends_with(' ') || s.text.is_empty(),
        }
    }

    fn push_text(&mut self, text: &str, style: Style, preformatted: bool) {
        if preformatted {
            let mut first = true;
            for line in text.split('\n') {
                if !first {
                    self.flush_para();
                }
                first = false;
                if !line.is_empty() {
                    self.spans.push(Span { text: line.to_string(), style, ruby: None });
                }
            }
            return;
        }
        // Collapse runs of whitespace; drop leading space at paragraph start.
        let mut out = String::with_capacity(text.len());
        let mut prev_space = self.last_is_space();
        for c in text.chars() {
            if c == '\u{a0}' {
                // &nbsp; is deliberate spacing in Anki cards: keep it, don't collapse it.
                out.push(' ');
                prev_space = false;
            } else if c.is_whitespace() {
                if !prev_space {
                    out.push(' ');
                }
                prev_space = true;
            } else {
                out.push(c);
                prev_space = false;
            }
        }
        if out.is_empty() {
            return;
        }
        if let Some(last) = self.spans.last_mut() {
            if last.style == style && last.ruby.is_none() {
                last.text.push_str(&out);
                return;
            }
        }
        if let Some(p) = self.prefix.take() {
            if self.spans.is_empty() {
                self.spans.push(Span { text: p, style: Style { dim: true, ..Style::default() }, ruby: None });
            }
        }
        self.spans.push(Span { text: out, style, ruby: None });
    }

    fn flush_para(&mut self) {
        // Trim trailing whitespace.
        while let Some(last) = self.spans.last_mut() {
            let t = last.text.trim_end().len();
            last.text.truncate(t);
            if last.text.is_empty() && last.ruby.is_none() {
                self.spans.pop();
            } else {
                break;
            }
        }
        if !self.spans.is_empty() {
            let spans = std::mem::take(&mut self.spans);
            let align = if self.align == Align::Inherit { self.opts.default_align } else { self.align };
            self.blocks.push(Block::Para { spans, align, indent: self.indent });
        }
    }

    fn flush(&mut self) {
        self.flush_para();
    }

    fn blank(&mut self) {
        if matches!(self.blocks.last(), Some(Block::Blank)) || self.blocks.is_empty() {
            return;
        }
        self.blocks.push(Block::Blank);
    }

    fn trim_blanks(&mut self) {
        while matches!(self.blocks.last(), Some(Block::Blank)) {
            self.blocks.pop();
        }
    }

    fn walk_children(&mut self, node: NodeRef<'_, Node>, style: Style, align: Align, indent: u16) {
        for child in node.children() {
            self.walk(child, style, align, indent);
        }
    }

    fn walk(&mut self, node: NodeRef<'_, Node>, style: Style, align: Align, indent: u16) {
        match node.value() {
            Node::Text(t) => self.push_text(t, style, false),
            Node::Element(el) => {
                let tag = el.name().to_ascii_lowercase();
                if SKIP_TAGS.contains(&tag.as_str()) {
                    return;
                }
                let decl = el.attr("style").map(parse_inline).unwrap_or_default();
                if decl.hidden || el.has_class("hidden", scraper::CaseSensitivity::AsciiCaseInsensitive) {
                    return;
                }
                let mut style = merge(style, &decl);
                let mut align = if decl.align != Align::Inherit { decl.align } else { align };
                match tag.as_str() {
                    "br" => {
                        if self.spans.is_empty() {
                            self.blank();
                        } else {
                            self.flush_para();
                        }
                        return;
                    }
                    "hr" => {
                        self.flush_para();
                        self.blocks.push(Block::Rule);
                        return;
                    }
                    "img" => {
                        let src = el.attr("src").unwrap_or("").to_string();
                        if src.is_empty() {
                            return;
                        }
                        self.flush_para();
                        self.blocks.push(Block::Image { src, alt: el.attr("alt").unwrap_or("").to_string() });
                        return;
                    }
                    "ruby" => {
                        self.ruby(node, style);
                        return;
                    }
                    "b" | "strong" => style.bold = true,
                    "i" | "em" | "cite" | "var" | "dfn" => style.italic = true,
                    "u" | "ins" => style.underline = true,
                    "s" | "del" | "strike" => style.strike = true,
                    "code" | "kbd" | "tt" | "samp" => style.code = true,
                    "small" => style.dim = true,
                    "mark" => style.bg = Some(Color(255, 255, 0)),
                    "a" => style.underline = true,
                    "font" => {
                        if let Some(c) = el.attr("color").and_then(Color::parse) {
                            style.fg = Some(c);
                        }
                    }
                    "sup" => {
                        self.push_text("^", style, false);
                    }
                    "sub" => {
                        self.push_text("_", style, false);
                    }
                    "td" | "th" => {
                        if !self.spans.is_empty() && !self.last_is_space() {
                            self.push_text("  ", style, false);
                            if let Some(l) = self.spans.last_mut() {
                                l.text.push_str("  ");
                            }
                        }
                        if tag == "th" {
                            style.bold = true;
                        }
                    }
                    "center" => align = Align::Center,
                    "h1" | "h2" | "h3" => style.bold = true,
                    "h4" | "h5" | "h6" => {
                        style.bold = true;
                        style.dim = true;
                    }
                    _ => {}
                }
                if let Some(a) = el.attr("align") {
                    align = match a.to_ascii_lowercase().as_str() {
                        "center" => Align::Center,
                        "right" => Align::Right,
                        "left" => Align::Left,
                        _ => align,
                    };
                }
                let is_block = BLOCK_TAGS.contains(&tag.as_str());
                if is_block {
                    self.flush_para();
                    let saved = (self.align, self.indent);
                    self.align = align;
                    let mut indent = indent;
                    match tag.as_str() {
                        "li" => {
                            self.prefix = Some("• ".into());
                            indent += 2;
                        }
                        "blockquote" => {
                            self.prefix = Some("│ ".into());
                            indent += 2;
                        }
                        "dd" => indent += 2,
                        _ => {}
                    }
                    self.indent = indent;
                    let before = self.blocks.len();
                    self.walk_children(node, style, align, indent);
                    self.flush_para();
                    self.prefix = None;
                    if tag == "pre" {
                        // handled as text nodes above; nothing special
                    }
                    // Empty <div></div> / <p></p> mean vertical space in Anki cards.
                    if self.blocks.len() == before && matches!(tag.as_str(), "div" | "p") && !node.has_children() {
                        self.blank();
                    }
                    self.align = saved.0;
                    self.indent = saved.1;
                } else if tag == "pre" {
                    self.flush_para();
                    self.walk_pre(node, style);
                    self.flush_para();
                } else {
                    self.walk_children(node, style, align, indent);
                }
            }
            _ => self.walk_children(node, style, align, indent),
        }
    }

    fn walk_pre(&mut self, node: NodeRef<'_, Node>, style: Style) {
        for c in node.children() {
            match c.value() {
                Node::Text(t) => self.push_text(t, Style { code: true, ..style }, true),
                _ => self.walk_pre(c, style),
            }
        }
    }

    fn ruby(&mut self, node: NodeRef<'_, Node>, style: Style) {
        // Base text is everything that isn't <rt>/<rp>; the annotation is the <rt> text.
        let mut base = String::new();
        let mut rt = String::new();
        fn collect(n: NodeRef<'_, Node>, base: &mut String, rt: &mut String, in_rt: bool) {
            match n.value() {
                Node::Text(t) => {
                    if in_rt {
                        rt.push_str(t)
                    } else {
                        base.push_str(t)
                    }
                }
                Node::Element(e) => {
                    let name = e.name().to_ascii_lowercase();
                    if name == "rp" {
                        return;
                    }
                    let in_rt = in_rt || name == "rt";
                    for c in n.children() {
                        collect(c, base, rt, in_rt);
                    }
                }
                _ => {}
            }
        }
        collect(node, &mut base, &mut rt, false);
        let base = base.split_whitespace().collect::<Vec<_>>().join(" ");
        let rt = rt.split_whitespace().collect::<Vec<_>>().join(" ");
        if base.is_empty() {
            return;
        }
        if let Some(p) = self.prefix.take() {
            if self.spans.is_empty() {
                self.spans.push(Span { text: p, style: Style { dim: true, ..Style::default() }, ruby: None });
            }
        }
        self.spans.push(Span { text: base, style, ruby: if rt.is_empty() { None } else { Some(rt) } });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn para(doc: &Document, i: usize) -> &Vec<Span> {
        match &doc.blocks[i] {
            Block::Para { spans, .. } => spans,
            other => panic!("block {i} is {other:?}"),
        }
    }

    #[test]
    fn evita_vocab_card() {
        let html = r#"<span style="">beverage, drink (esp. a soft/cold drink)</span><br><br>
<span style="font-family: Batang; color:rgb(173,122,190); font-size:26px; ">飮料水</span>"#;
        let doc = render_html(html, &Options::default());
        assert_eq!(doc.blocks.len(), 3);
        assert_eq!(para(&doc, 0)[0].text, "beverage, drink (esp. a soft/cold drink)");
        assert_eq!(doc.blocks[1], Block::Blank);
        let hanja = &para(&doc, 2)[0];
        assert_eq!(hanja.text, "飮料水");
        assert_eq!(hanja.style.fg, Some(Color(173, 122, 190)));
    }

    #[test]
    fn inline_styles_and_blocks() {
        let doc = render_html(
            "<div>one <b>two</b> three</div><div style='text-align:center'>four</div><hr><ul><li>a</li><li>b</li></ul>",
            &Options::default(),
        );
        let p0 = para(&doc, 0);
        assert_eq!(p0.iter().map(|s| s.text.as_str()).collect::<Vec<_>>(), vec!["one ", "two", " three"]);
        assert!(p0[1].style.bold);
        assert!(matches!(doc.blocks[1], Block::Para { align: Align::Center, .. }));
        assert_eq!(doc.blocks[2], Block::Rule);
        assert_eq!(para(&doc, 3)[0].text, "• ");
        assert_eq!(para(&doc, 3)[1].text, "a");
    }

    #[test]
    fn ruby_and_images_and_hidden() {
        let doc = render_html(
            r#"<ruby>漢字<rp>(</rp><rt>かんじ</rt><rp>)</rp></ruby>です<img src="x.png"><span style="display:none">secret</span>"#,
            &Options::default(),
        );
        let p = para(&doc, 0);
        assert_eq!(p[0].text, "漢字");
        assert_eq!(p[0].ruby.as_deref(), Some("かんじ"));
        assert_eq!(p[1].text, "です");
        assert_eq!(doc.blocks[1], Block::Image { src: "x.png".into(), alt: String::new() });
        assert!(!doc.plain_text().contains("secret"));
    }

    #[test]
    fn whitespace_collapses_and_entities_decode() {
        let doc = render_html("  a &amp;\n\n  b&nbsp;&nbsp;c   ", &Options::default());
        assert_eq!(doc.plain_text(), "a & b  c");
    }
}
