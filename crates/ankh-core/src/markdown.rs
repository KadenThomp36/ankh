//! Field HTML ⇄ Markdown.
//!
//! Anki fields are HTML fragments written by a WYSIWYG editor: `<br>` for
//! line breaks, `<div>` wrappers, `<b>`/`<i>`, images, `[sound:]` tags,
//! `{{c1::cloze}}` markers. For editing in `$EDITOR` we want Markdown, but
//! never at the cost of corrupting a note. So:
//!
//! - HTML → Markdown is attempted, then converted back and compared. If the
//!   round trip isn't faithful the field is emitted as raw HTML, marked with
//!   [`RAW_MARKER`], and saved back verbatim.
//! - Markdown → HTML produces Anki-style fragments: no `<p>` wrappers,
//!   paragraphs joined by `<br><br>`, newlines as `<br>`.

use pulldown_cmark::{html, Event, Options, Parser, Tag, TagEnd};

/// Put at the top of a field body to say "this is HTML, keep it verbatim".
pub const RAW_MARKER: &str = "<!-- html -->";

/// Markdown → Anki field HTML.
pub fn md_to_html(md: &str) -> String {
    let md = md.trim();
    if let Some(raw) = md.strip_prefix(RAW_MARKER) {
        return raw.trim().to_string();
    }
    if md.is_empty() {
        return String::new();
    }
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    let parser = Parser::new_ext(md, opts).map(|ev| match ev {
        // A newline in the editor is a line break on the card.
        Event::SoftBreak => Event::HardBreak,
        other => other,
    });
    // Track paragraph boundaries so we can emit Anki-style fragments.
    let mut out = String::new();
    let mut events: Vec<Event> = parser.collect();
    // Drop the outer <p> of every top-level paragraph.
    let mut depth = 0usize;
    let mut keep: Vec<Event> = Vec::with_capacity(events.len());
    for ev in events.drain(..) {
        match &ev {
            Event::Start(Tag::Paragraph) if depth == 0 => {
                depth += 1;
                keep.push(Event::Html("\u{0}P\u{0}".into()));
                continue;
            }
            Event::End(TagEnd::Paragraph) if depth == 1 => {
                depth -= 1;
                keep.push(Event::Html("\u{0}/P\u{0}".into()));
                continue;
            }
            Event::Start(t) if is_block(t) => depth += 1,
            Event::End(t) if is_block_end(t) => depth = depth.saturating_sub(1),
            _ => {}
        }
        keep.push(ev);
    }
    html::push_html(&mut out, keep.into_iter());
    // Replace our paragraph sentinels with Anki's blank-line idiom.
    let out = out
        .replace("\u{0}/P\u{0}\n\u{0}P\u{0}", "<br><br>")
        .replace("\u{0}/P\u{0}\u{0}P\u{0}", "<br><br>")
        .replace("\u{0}P\u{0}", "")
        .replace("\u{0}/P\u{0}", "");
    out.replace("<br />\n", "<br>")
        .replace("<br />", "<br>")
        .replace("<br>\n", "<br>")
        .replace("\" />", "\">")
        .trim()
        .to_string()
}

fn is_block(t: &Tag) -> bool {
    matches!(t, Tag::List(_) | Tag::BlockQuote(_) | Tag::CodeBlock(_) | Tag::Table(_) | Tag::Heading { .. } | Tag::Item)
}

fn is_block_end(t: &TagEnd) -> bool {
    matches!(
        t,
        TagEnd::List(_) | TagEnd::BlockQuote(_) | TagEnd::CodeBlock | TagEnd::Table | TagEnd::Heading(_) | TagEnd::Item
    )
}

/// Anki field HTML → Markdown, or raw HTML (with [`RAW_MARKER`]) when the
/// conversion wouldn't survive a round trip.
pub fn html_to_md(html: &str) -> String {
    let html = html.trim();
    if html.is_empty() {
        return String::new();
    }
    if !looks_like_html(html) {
        // Plain text: still escape nothing; Markdown is a superset for our purposes.
        return html.to_string();
    }
    let md = match htmd::convert(html) {
        // htmd writes `<br>` as a two-space hard break; newlines are already
        // hard breaks for us, so drop the trailing spaces.
        // Anki content is full of literal brackets (`[sound:x.mp3]`, `[grammar]`);
        // unescape them — the round-trip check below catches the rare case
        // where that would turn text into a link.
        Ok(m) => {
            m.replace("  \n", "\n").replace("\n\n\n", "\n\n").replace("\\[", "[").replace("\\]", "]").trim().to_string()
        }
        Err(_) => return format!("{RAW_MARKER}\n{html}"),
    };
    if normalize(&md_to_html(&md)) == normalize(html) {
        md
    } else {
        format!("{RAW_MARKER}\n{html}")
    }
}

fn looks_like_html(s: &str) -> bool {
    s.contains('<') && s.contains('>') || s.contains("&nbsp;") || s.contains("&amp;") || s.contains("&lt;")
}

/// Normalise HTML enough that cosmetic differences don't count as lossy.
fn normalize(html: &str) -> String {
    let mut s = html.to_ascii_lowercase();
    for (from, to) in [
        ("<br />", "<br>"),
        ("<br/>", "<br>"),
        ("<strong>", "<b>"),
        ("</strong>", "</b>"),
        ("<em>", "<i>"),
        ("</em>", "</i>"),
        ("&nbsp;", " "),
        ("&#39;", "'"),
        ("&quot;", "\""),
        ("\n", ""),
        ("\r", ""),
    ] {
        s = s.replace(from, to);
    }
    // <div>x</div> and x<br> are the same thing to Anki's editor.
    s = s.replace("<div>", "").replace("</div>", "<br>");
    while s.ends_with("<br>") {
        s.truncate(s.len() - 4);
    }
    s = s.replace(" alt=\"\"", "");
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_becomes_anki_fragments() {
        assert_eq!(md_to_html("hello **world**"), "hello <strong>world</strong>");
        assert_eq!(md_to_html("line one\nline two"), "line one<br>line two");
        assert_eq!(md_to_html("para one\n\npara two"), "para one<br><br>para two");
        assert_eq!(md_to_html("- a\n- b"), "<ul>\n<li>a</li>\n<li>b</li>\n</ul>");
        assert_eq!(md_to_html("{{c1::Seoul}} is the capital"), "{{c1::Seoul}} is the capital");
        assert_eq!(md_to_html("[sound:x.mp3]"), "[sound:x.mp3]");
        assert_eq!(md_to_html("![](pic.png)"), "<img src=\"pic.png\" alt=\"\">");
        assert_eq!(
            md_to_html("<!-- html -->\n<span style=\"color:red\">x</span>"),
            "<span style=\"color:red\">x</span>"
        );
    }

    #[test]
    fn html_round_trips_or_stays_raw() {
        assert_eq!(html_to_md("plain text"), "plain text");
        assert_eq!(html_to_md("a<br>b"), "a\nb");
        assert_eq!(html_to_md("<b>bold</b> and <i>it</i>"), "**bold** and *it*");
        // Inline styles cannot be expressed in Markdown: keep the HTML.
        let styled = r#"<span style="color:rgb(173,122,190)">飮料水</span>"#;
        let md = html_to_md(styled);
        assert!(md.starts_with(RAW_MARKER), "{md}");
        assert_eq!(md_to_html(&md), styled);
        // Evita-style field with <br><br> spacing.
        let f = "beverage, drink<br><br>飮料水";
        assert_eq!(html_to_md(f), "beverage, drink\n\n飮料水");
        assert_eq!(md_to_html(&html_to_md(f)), f);
    }
}
