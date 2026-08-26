//! Just enough CSS: inline `style=""` declarations we care about.

use crate::color::Color;
use crate::{Align, Style};

#[derive(Debug, Default, Clone, Copy)]
pub struct Decl {
    pub hidden: bool,
    pub align: Align,
    pub style: Style,
    /// Whether the declaration explicitly reset bold/italic (`font-weight: normal`).
    pub unbold: bool,
    pub unitalic: bool,
}

pub fn parse_inline(style_attr: &str) -> Decl {
    let mut d = Decl::default();
    for decl in style_attr.split(';') {
        let Some((k, v)) = decl.split_once(':') else { continue };
        let k = k.trim().to_ascii_lowercase();
        let v = v.trim().trim_end_matches("!important").trim();
        match k.as_str() {
            "color" => d.style.fg = Color::parse(v),
            "background" | "background-color" => d.style.bg = Color::parse(v.split_whitespace().next().unwrap_or("")),
            "font-weight" => match v.to_ascii_lowercase().as_str() {
                "bold" | "bolder" => d.style.bold = true,
                "normal" | "lighter" => d.unbold = true,
                n => {
                    if let Ok(w) = n.parse::<u16>() {
                        if w >= 600 {
                            d.style.bold = true
                        } else {
                            d.unbold = true
                        }
                    }
                }
            },
            "font-style" => match v.to_ascii_lowercase().as_str() {
                "italic" | "oblique" => d.style.italic = true,
                "normal" => d.unitalic = true,
                _ => {}
            },
            "text-decoration" | "text-decoration-line" => {
                let v = v.to_ascii_lowercase();
                if v.contains("underline") {
                    d.style.underline = true;
                }
                if v.contains("line-through") {
                    d.style.strike = true;
                }
            }
            "text-align" => {
                d.align = match v.to_ascii_lowercase().as_str() {
                    "center" => Align::Center,
                    "right" | "end" => Align::Right,
                    "left" | "start" => Align::Left,
                    _ => Align::Inherit,
                }
            }
            "display" => {
                if v.eq_ignore_ascii_case("none") {
                    d.hidden = true;
                }
            }
            "visibility" => {
                if v.eq_ignore_ascii_case("hidden") {
                    d.hidden = true;
                }
            }
            "opacity" if v.parse::<f32>().map(|o| o < 0.5).unwrap_or(false) => d.style.dim = true,
            // Terminals have one font size; map big → bold, small → dim.
            "font-size" => {
                let v = v.to_ascii_lowercase();
                let num: Option<f32> = v.trim_end_matches(|c: char| c.is_alphabetic() || c == '%').parse().ok();
                match (num, &v) {
                    (Some(n), v) if v.ends_with("px") => {
                        if n >= 24.0 {
                            d.style.bold = true
                        } else if n <= 14.0 {
                            d.style.dim = true
                        }
                    }
                    (Some(n), v) if v.ends_with('%') => {
                        if n >= 130.0 {
                            d.style.bold = true
                        } else if n <= 85.0 {
                            d.style.dim = true
                        }
                    }
                    (Some(n), v) if v.ends_with("em") || v.ends_with("rem") => {
                        if n >= 1.3 {
                            d.style.bold = true
                        } else if n <= 0.85 {
                            d.style.dim = true
                        }
                    }
                    (_, v) if v.contains("large") => d.style.bold = true,
                    (_, v) if v.contains("small") => d.style.dim = true,
                    _ => {}
                }
            }
            _ => {}
        }
    }
    d
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_inline_subset() {
        let d = parse_inline("font-family: Batang; color:rgb(173,122,190); font-size:26px; font-weight: bold");
        assert_eq!(d.style.fg, Some(Color(173, 122, 190)));
        assert!(d.style.bold, "26px counts as big");
        assert!(!d.hidden);
        let d = parse_inline("display: none");
        assert!(d.hidden);
        let d = parse_inline("text-align:center;text-decoration: underline line-through");
        assert_eq!(d.align, Align::Center);
        assert!(d.style.underline && d.style.strike);
    }
}

// ---------------------------------------------------------------------------
// Stylesheets (the notetype's CSS)
// ---------------------------------------------------------------------------

/// One compound selector: `div.card.night#id` → tag + classes + id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selector {
    pub tag: Option<String>,
    pub classes: Vec<String>,
    pub id: Option<String>,
}

impl Selector {
    /// Parse a selector list entry. Descendant/child combinators are
    /// approximated by matching only the *last* compound selector, which is
    /// what matters for the styling Anki cards actually use. Pseudo-classes
    /// and attribute selectors make the rule unmatchable (returns `None`).
    fn parse(s: &str) -> Option<Selector> {
        let s = s.trim();
        if s.is_empty() || s.contains(':') || s.contains('[') || s.contains('@') {
            return None;
        }
        let last = s.split([' ', '>', '+', '~']).rfind(|p| !p.is_empty())?;
        let mut sel = Selector { tag: None, classes: vec![], id: None };
        let mut cur = String::new();
        let mut kind = 't';
        let flush = |sel: &mut Selector, kind: char, cur: &mut String| {
            if cur.is_empty() {
                return;
            }
            match kind {
                '.' => sel.classes.push(cur.to_ascii_lowercase()),
                '#' => sel.id = Some(std::mem::take(cur)),
                _ => {
                    if cur != "*" {
                        sel.tag = Some(cur.to_ascii_lowercase())
                    }
                }
            }
            cur.clear();
        };
        for c in last.chars() {
            match c {
                '.' | '#' => {
                    flush(&mut sel, kind, &mut cur);
                    kind = c;
                }
                _ => cur.push(c),
            }
        }
        flush(&mut sel, kind, &mut cur);
        Some(sel)
    }

    pub fn matches(&self, tag: &str, classes: &[String], id: Option<&str>) -> bool {
        if let Some(t) = &self.tag {
            if t != tag {
                return false;
            }
        }
        if let Some(i) = &self.id {
            if id != Some(i.as_str()) {
                return false;
            }
        }
        self.classes.iter().all(|c| classes.iter().any(|x| x == c))
    }

    /// CSS specificity, enough to order `.card` below `.card.blue`.
    fn specificity(&self) -> u32 {
        (self.id.is_some() as u32) * 100 + self.classes.len() as u32 * 10 + self.tag.is_some() as u32
    }
}

#[derive(Debug, Clone)]
pub struct Rule {
    pub selector: Selector,
    pub decl: Decl,
    order: usize,
}

/// A parsed notetype stylesheet, reduced to the properties we honour.
#[derive(Debug, Clone, Default)]
pub struct Stylesheet {
    rules: Vec<Rule>,
}

impl Stylesheet {
    pub fn parse(css: &str) -> Self {
        let css = strip_comments(css);
        let mut rules = Vec::new();
        let mut rest = css.as_str();
        let mut order = 0;
        while let Some(open) = rest.find('{') {
            let selectors = &rest[..open];
            let Some(close) = rest[open..].find('}') else { break };
            let body = &rest[open + 1..open + close];
            rest = &rest[open + close + 1..];
            // Skip @media / @font-face blocks (their bodies may contain nested braces).
            if selectors.trim_start().starts_with('@') {
                // consume until the matching close of the nested block
                if let Some(extra) = rest.find('}') {
                    if body.contains('{') {
                        rest = &rest[extra + 1..];
                    }
                }
                continue;
            }
            let decl = parse_inline(body);
            for s in selectors.split(',') {
                if let Some(selector) = Selector::parse(s) {
                    rules.push(Rule { selector, decl, order });
                    order += 1;
                }
            }
        }
        Stylesheet { rules }
    }

    /// Declarations that apply to an element, in cascade order (least to
    /// most specific, then source order).
    pub fn matching(&self, tag: &str, classes: &[String], id: Option<&str>) -> Vec<&Decl> {
        let mut hits: Vec<&Rule> = self.rules.iter().filter(|r| r.selector.matches(tag, classes, id)).collect();
        hits.sort_by_key(|r| (r.selector.specificity(), r.order));
        hits.into_iter().map(|r| &r.decl).collect()
    }

    /// The `.card` rule's declarations (root defaults).
    pub fn card(&self) -> Vec<&Decl> {
        self.matching("div", &["card".to_string()], None)
    }

    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }
}

fn strip_comments(css: &str) -> String {
    let mut out = String::with_capacity(css.len());
    let mut rest = css;
    while let Some(i) = rest.find("/*") {
        out.push_str(&rest[..i]);
        match rest[i + 2..].find("*/") {
            Some(j) => rest = &rest[i + 2 + j + 2..],
            None => {
                rest = "";
            }
        }
    }
    out.push_str(rest);
    // Anki users write `//` comments that browsers ignore as broken declarations.
    out.lines().map(|l| l.split("//").next().unwrap_or("")).collect::<Vec<_>>().join("\n")
}

#[cfg(test)]
mod sheet_tests {
    use super::*;

    #[test]
    fn parses_anki_notetype_css() {
        let css = r#"
        .card { font-family: arial; //font-family: Gulim;
          font-size: 20px; text-align: center; color: black; background-color: white; }
        .eng.title { color: #999999; font-size:80%; }
        .notes { color: #449933; font-size: 80% }
        a { color: #6eb9ff; display: block; }
        details[open] summary { border-bottom: 1px solid #aaa; }
        .center { text-align: center; }
        "#;
        let sheet = Stylesheet::parse(css);
        let card = sheet.card();
        assert_eq!(card.len(), 1);
        assert_eq!(card[0].align, Align::Center);
        let m = sheet.matching("span", &["eng".into(), "title".into()], None);
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].style.fg, Some(Color(0x99, 0x99, 0x99)));
        assert!(m[0].style.dim, "80% font-size is dim");
        assert!(sheet.matching("span", &["eng".into()], None).is_empty());
        assert_eq!(sheet.matching("a", &[], None).len(), 1);
        assert_eq!(sheet.matching("div", &["center".into()], None)[0].align, Align::Center);
    }

    #[test]
    fn specificity_orders_cascade() {
        let sheet = Stylesheet::parse(".x { color: red } div.x.y { color: blue } div { color: green }");
        let m = sheet.matching("div", &["x".into(), "y".into()], None);
        let last = m.last().unwrap();
        assert_eq!(last.style.fg, Some(Color(0, 0, 255)));
        assert_eq!(m[0].style.fg, Some(Color(0, 128, 0)));
    }
}
