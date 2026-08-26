//! Walk an HTML fragment and emit blocks.

use ego_tree::NodeRef;
use scraper::{Html, Node};

use crate::color::Color;
use crate::css::{parse_inline, Decl, Stylesheet};
use crate::{Align, Block, Document, Span, Style};

#[derive(Debug, Clone, Default)]
pub struct Options {
    /// Alignment for blocks that don't set their own. The stylesheet's
    /// `.card { text-align }` overrides this when present.
    pub default_align: Align,
    /// The notetype's CSS.
    pub stylesheet: Option<Stylesheet>,
    /// Show the contents of `{{hint:Field}}` blocks instead of the link.
    pub reveal_hints: bool,
}

pub fn render_html(html: &str, opts: &Options) -> Document {
    let frag = Html::parse_fragment(html);
    let mut root_align = opts.default_align;
    let mut root_style = Style::default();
    if let Some(sheet) = &opts.stylesheet {
        for d in sheet.card() {
            if d.align != Align::Inherit {
                root_align = d.align;
            }
            // `.card` colours are designed for Anki's white/black page, not the
            // user's terminal theme, so only emphasis is inherited from it.
            let mut d2 = *d;
            d2.style.fg = None;
            d2.style.bg = None;
            root_style = merge(root_style, &d2);
        }
    }
    let mut b = Builder {
        blocks: vec![],
        spans: vec![],
        align: Align::Inherit,
        indent: 0,
        prefix: None,
        default_align: root_align,
        sheet: opts.stylesheet.clone().unwrap_or_default(),
        reveal_hints: opts.reveal_hints,
    };
    b.walk_children(frag.tree.root(), root_style, Align::Inherit, 0);
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
    default_align: Align,
    sheet: Stylesheet,
    reveal_hints: bool,
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

    /// Text with a couple of Anki-isms: `[[type:Field]]` placeholders and
    /// MathJax `\( … \)` / `\[ … \]` delimiters.
    fn push_text_special(&mut self, text: &str, style: Style) {
        if let Some(i) = text.find("[[type:") {
            if let Some(j) = text[i..].find("]]") {
                self.push_text_special(&text[..i], style);
                self.push_text("[type the answer]", Style { dim: true, italic: true, ..style }, false);
                self.push_text_special(&text[i + j + 2..], style);
                return;
            }
        }
        for (open, close) in [("\\(", "\\)"), ("\\[", "\\]")] {
            if let Some(i) = text.find(open) {
                if let Some(j) = text[i + 2..].find(close) {
                    self.push_text_special(&text[..i], style);
                    let math = text[i + 2..i + 2 + j].trim();
                    self.push_text(math, Style { code: true, ..style }, false);
                    self.push_text_special(&text[i + 2 + j + 2..], style);
                    return;
                }
            }
        }
        self.push_text(text, style, false);
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
            let align = if self.align == Align::Inherit { self.default_align } else { self.align };
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
            Node::Text(t) => self.push_text_special(t, style),
            Node::Element(el) => {
                let tag = el.name().to_ascii_lowercase();
                if SKIP_TAGS.contains(&tag.as_str()) {
                    return;
                }
                let classes: Vec<String> = el.classes().map(|c| c.to_ascii_lowercase()).collect();
                let is_hint = classes.iter().any(|c| c == "hint");
                // {{hint:Field}} renders a link plus a hidden div; show one or the other.
                if is_hint && tag == "a" && self.reveal_hints {
                    return;
                }
                let mut style = style;
                let mut align = align;
                let mut hidden = false;
                for d in self.sheet.matching(&tag, &classes, el.attr("id")) {
                    style = merge(style, d);
                    if d.align != Align::Inherit {
                        align = d.align;
                    }
                    hidden |= d.hidden;
                }
                let decl = el.attr("style").map(parse_inline).unwrap_or_default();
                hidden |= decl.hidden;
                if is_hint && tag == "div" && self.reveal_hints {
                    hidden = false;
                }
                if hidden || classes.iter().any(|c| c == "hidden") {
                    return;
                }
                let mut style = merge(style, &decl);
                if decl.align != Align::Inherit {
                    align = decl.align;
                }
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
    use crate::css::Stylesheet;

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
    fn stylesheet_classes_apply_and_card_colours_are_ignored() {
        let css = ".card { text-align: center; color: black; } .eng.title { color: #999999; font-size: 80%; } .center { text-align:center }";
        let opts = Options { stylesheet: Some(Stylesheet::parse(css)), ..Default::default() };
        let doc = render_html(r#"<span class="eng title">English:</span><div style="text-align:left">x</div>"#, &opts);
        let p = para(&doc, 0);
        assert_eq!(p[0].style.fg, Some(Color(0x99, 0x99, 0x99)));
        assert!(p[0].style.dim);
        assert!(matches!(doc.blocks[0], Block::Para { align: Align::Center, .. }));
        assert!(matches!(doc.blocks[1], Block::Para { align: Align::Left, .. }));
        // `.card { color: black }` must not paint text black on a dark terminal.
        let doc = render_html("plain", &opts);
        assert_eq!(para(&doc, 0)[0].style.fg, None);
    }

    #[test]
    fn hints_type_answer_and_mathjax() {
        let html = r##"<a class=hint href="#">Phonetics</a><div id="hint1" class=hint style="display: none">jal</div>[[type:Back]] and \(x^2\)"##;
        let doc = render_html(html, &Options::default());
        let t = doc.plain_text();
        assert!(t.contains("Phonetics") && !t.contains("jal"), "{t}");
        assert!(t.contains("[type the answer]"));
        assert!(t.contains("x^2") && !t.contains("\\("));
        let doc = render_html(html, &Options { reveal_hints: true, ..Default::default() });
        let t = doc.plain_text();
        assert!(!t.contains("Phonetics") && t.contains("jal"), "{t}");
    }

    #[test]
    fn whitespace_collapses_and_entities_decode() {
        let doc = render_html("  a &amp;\n\n  b&nbsp;&nbsp;c   ", &Options::default());
        assert_eq!(doc.plain_text(), "a & b  c");
    }
}
