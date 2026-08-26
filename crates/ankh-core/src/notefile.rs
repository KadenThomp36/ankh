//! The note file: Markdown + YAML-ish frontmatter, one or more notes.
//!
//! ```markdown
//! ---
//! note: 1633756077719        # absent for new notes
//! notetype: Korean Vocab
//! deck: Korean::Vocabulary
//! tags: leech TTMIK-1.24
//! ---
//!
//! ## Front
//!
//! 조사
//!
//! ## Back
//!
//! [grammar] particle
//! ```
//!
//! Field headings are `## <field name>`; a heading that isn't a field of the
//! notetype is ordinary content. Several notes in one file each start with
//! their own frontmatter; `notetype`, `deck` and `tags` are inherited from
//! the previous note when omitted, so batch files stay short.

use serde::{Deserialize, Serialize};

use crate::markdown::{html_to_md, md_to_html};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoteDoc {
    /// None for a note that doesn't exist yet.
    pub id: Option<i64>,
    pub notetype: String,
    pub deck: String,
    pub tags: Vec<String>,
    /// (field name, Markdown body)
    pub fields: Vec<(String, String)>,
}

impl NoteDoc {
    /// Field bodies as Anki HTML, in file order.
    pub fn fields_html(&self) -> Vec<(String, String)> {
        self.fields.iter().map(|(n, md)| (n.clone(), md_to_html(md))).collect()
    }

    pub fn from_html(
        id: Option<i64>,
        notetype: &str,
        deck: &str,
        tags: &[String],
        fields: &[(String, String)],
    ) -> Self {
        NoteDoc {
            id,
            notetype: notetype.to_string(),
            deck: deck.to_string(),
            tags: tags.to_vec(),
            fields: fields.iter().map(|(n, h)| (n.clone(), html_to_md(h))).collect(),
        }
    }
}

/// Serialise notes to a file body.
pub fn write(notes: &[NoteDoc]) -> String {
    let mut out = String::new();
    let mut prev: Option<&NoteDoc> = None;
    for n in notes {
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str("---\n");
        if let Some(id) = n.id {
            out.push_str(&format!("note: {id}\n"));
        }
        let inherit = |f: fn(&NoteDoc) -> &str| prev.map(|p| f(p) == f(n)).unwrap_or(false);
        if !inherit(|d| &d.notetype) {
            out.push_str(&format!("notetype: {}\n", n.notetype));
        }
        if !inherit(|d| &d.deck) {
            out.push_str(&format!("deck: {}\n", n.deck));
        }
        if prev.map(|p| p.tags != n.tags).unwrap_or(true) {
            out.push_str(&format!("tags: {}\n", n.tags.join(" ")));
        }
        out.push_str("---\n");
        for (name, body) in &n.fields {
            out.push_str(&format!("\n## {name}\n\n"));
            if !body.is_empty() {
                out.push_str(body);
                out.push('\n');
            }
        }
        prev = Some(n);
    }
    out
}

#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("line {line}: {msg}")]
    At { line: usize, msg: String },
}

/// Parse a file body. `field_names(notetype)` tells the parser which `##`
/// headings are fields; unknown notetypes fail.
pub fn parse(text: &str, field_names: impl Fn(&str) -> Option<Vec<String>>) -> Result<Vec<NoteDoc>, ParseError> {
    let lines: Vec<&str> = text.lines().collect();
    let mut i = 0;
    let mut notes: Vec<NoteDoc> = Vec::new();
    // skip leading blank lines
    while i < lines.len() && lines[i].trim().is_empty() {
        i += 1;
    }
    while i < lines.len() {
        if lines[i].trim_end() != "---" {
            return Err(ParseError::At { line: i + 1, msg: "expected `---` to start a note's frontmatter".into() });
        }
        let fm_start = i + 1;
        let mut j = fm_start;
        while j < lines.len() && lines[j].trim_end() != "---" {
            j += 1;
        }
        if j >= lines.len() {
            return Err(ParseError::At { line: fm_start, msg: "unterminated frontmatter".into() });
        }
        let prev = notes.last().cloned();
        let mut id = None;
        let mut notetype = prev.as_ref().map(|p| p.notetype.clone());
        let mut deck = prev.as_ref().map(|p| p.deck.clone());
        let mut tags = prev.as_ref().map(|p| p.tags.clone());
        for (k, l) in lines[fm_start..j].iter().enumerate() {
            let l = l.split('#').next().unwrap_or("").trim();
            if l.is_empty() {
                continue;
            }
            let Some((key, val)) = l.split_once(':') else {
                return Err(ParseError::At {
                    line: fm_start + k + 1,
                    msg: format!("expected `key: value`, got {l:?}"),
                });
            };
            let val = val.trim().trim_matches('"');
            match key.trim() {
                "note" | "id" => {
                    id = Some(val.parse().map_err(|_| ParseError::At {
                        line: fm_start + k + 1,
                        msg: "note id must be a number".into(),
                    })?)
                }
                "notetype" | "model" => notetype = Some(val.to_string()),
                "deck" => deck = Some(val.to_string()),
                "tags" => {
                    tags = Some(
                        val.trim_start_matches('[')
                            .trim_end_matches(']')
                            .split([' ', ','])
                            .filter(|t| !t.is_empty())
                            .map(String::from)
                            .collect(),
                    )
                }
                "ankh" => {}
                other => return Err(ParseError::At { line: fm_start + k + 1, msg: format!("unknown key {other:?}") }),
            }
        }
        let notetype = notetype.ok_or(ParseError::At { line: fm_start, msg: "missing `notetype:`".into() })?;
        let deck = deck.ok_or(ParseError::At { line: fm_start, msg: "missing `deck:`".into() })?;
        let names = field_names(&notetype)
            .ok_or(ParseError::At { line: fm_start, msg: format!("unknown notetype {notetype:?}") })?;

        // Body: until the next frontmatter start (a `---` line followed by a `key:` line).
        i = j + 1;
        let body_start = i;
        while i < lines.len() {
            if lines[i].trim_end() == "---" {
                let next = lines.get(i + 1).map(|l| l.trim()).unwrap_or("");
                let is_key = next
                    .split_once(':')
                    .map(|(k, _)| !k.is_empty() && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'))
                    .unwrap_or(false);
                if is_key || next == "---" || (next.is_empty() && i + 1 >= lines.len()) {
                    break;
                }
            }
            i += 1;
        }
        let body = &lines[body_start..i];
        let mut fields: Vec<(String, String)> = Vec::new();
        let mut current: Option<(String, Vec<&str>)> = None;
        for l in body {
            if let Some(h) = l.strip_prefix("## ") {
                let h = h.trim();
                if names.iter().any(|n| n == h) {
                    if let Some((name, buf)) = current.take() {
                        fields.push((name, buf.join("\n").trim().to_string()));
                    }
                    current = Some((h.to_string(), Vec::new()));
                    continue;
                }
            }
            match current.as_mut() {
                Some((_, buf)) => buf.push(l),
                None => {
                    if !l.trim().is_empty() {
                        // Content before any field heading goes to the first field.
                        current = Some((names[0].clone(), vec![l]));
                    }
                }
            }
        }
        if let Some((name, buf)) = current.take() {
            fields.push((name, buf.join("\n").trim().to_string()));
        }
        // Fields not mentioned stay empty (new note) — order by notetype.
        let mut ordered = Vec::with_capacity(names.len());
        for n in &names {
            let body = fields.iter().find(|(fname, _)| fname == n).map(|(_, b)| b.clone()).unwrap_or_default();
            ordered.push((n.clone(), body));
        }
        notes.push(NoteDoc { id, notetype, deck, tags: tags.unwrap_or_default(), fields: ordered });
        while i < lines.len() && lines[i].trim().is_empty() {
            i += 1;
        }
    }
    Ok(notes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(nt: &str) -> Option<Vec<String>> {
        match nt {
            "Basic" => Some(vec!["Front".into(), "Back".into()]),
            "Korean Vocab" => Some(vec!["Korean".into(), "English".into(), "Notes".into()]),
            _ => None,
        }
    }

    #[test]
    fn round_trips_a_note() {
        let doc = NoteDoc::from_html(
            Some(42),
            "Korean Vocab",
            "Korean::Vocab",
            &["leech".into()],
            &[
                ("Korean".into(), "조사".into()),
                ("English".into(), "[grammar] particle<br>helper".into()),
                ("Notes".into(), String::new()),
            ],
        );
        let text = write(std::slice::from_ref(&doc));
        assert!(text.contains("note: 42\n"));
        assert!(text.contains("## English\n\n[grammar] particle\nhelper\n"));
        let parsed = parse(&text, names).unwrap();
        assert_eq!(parsed, vec![doc]);
        assert_eq!(parsed[0].fields_html()[1].1, "[grammar] particle<br>helper");
    }

    #[test]
    fn batch_file_inherits_frontmatter() {
        let text = "---\nnotetype: Basic\ndeck: Inbox\ntags: new\n---\n## Front\nhello\n## Back\nworld\n\n---\n---\n## Front\nsecond\n\n---\ndeck: Other\n---\nthird front only\n";
        let notes = parse(text, names).unwrap();
        assert_eq!(notes.len(), 3);
        assert_eq!(notes[1].deck, "Inbox");
        assert_eq!(notes[1].tags, vec!["new"]);
        assert_eq!(notes[1].fields[1].1, "");
        assert_eq!(notes[2].deck, "Other");
        assert_eq!(notes[2].fields[0].1, "third front only");
    }

    #[test]
    fn errors_are_located() {
        let e = parse("---\nnotetype: Nope\ndeck: x\n---\n", names).unwrap_err();
        assert!(e.to_string().contains("unknown notetype"));
        let e = parse("---\nbogus: 1\n---\n", names).unwrap_err();
        assert!(e.to_string().starts_with("line 2"));
    }

    #[test]
    fn non_field_headings_are_content() {
        let text = "---\nnotetype: Basic\ndeck: d\n---\n## Front\n## Not a field\nx\n## Back\ny\n";
        let n = parse(text, names).unwrap();
        assert_eq!(n[0].fields[0].1, "## Not a field\nx");
    }
}
