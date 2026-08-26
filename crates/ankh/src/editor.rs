//! Hand a note file to `$EDITOR` and read it back.

use std::path::PathBuf;
use std::process::Command;

use ankh_core::{Engine, NoteDoc, Result};

pub fn editor_command() -> String {
    std::env::var("VISUAL")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| std::env::var("EDITOR").ok().filter(|s| !s.is_empty()))
        .unwrap_or_else(|| "vi".into())
}

fn temp_path(hint: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("ankh");
    let _ = std::fs::create_dir_all(&dir);
    dir.join(format!("{hint}-{}.ankh.md", std::process::id()))
}

/// Run the editor on `text`; returns the edited text, or None if unchanged.
pub fn edit_text(text: &str, hint: &str) -> std::io::Result<Option<String>> {
    let path = temp_path(hint);
    std::fs::write(&path, text)?;
    let cmd = editor_command();
    let mut parts = cmd.split_whitespace();
    let bin = parts.next().unwrap_or("vi");
    let status = Command::new(bin).args(parts).arg(&path).status()?;
    if !status.success() {
        let _ = std::fs::remove_file(&path);
        return Err(std::io::Error::other(format!("{cmd} exited with {status}")));
    }
    let edited = std::fs::read_to_string(&path)?;
    let _ = std::fs::remove_file(&path);
    Ok(if edited == text { None } else { Some(edited) })
}

pub struct SaveReport {
    pub added: usize,
    pub updated: usize,
    pub ids: Vec<i64>,
}

/// Parse an edited note file against the collection's notetypes and save
/// every note in it.
pub fn save_note_file(engine: &mut Engine, text: &str) -> Result<SaveReport> {
    let notetypes = engine.notetypes()?;
    let docs = ankh_core::notefile::parse(text, |nt| {
        notetypes.iter().find(|n| n.name.eq_ignore_ascii_case(nt)).map(|n| n.fields.clone())
    })
    .map_err(|e| anyhow::anyhow!("note file: {e}"))?;
    let mut report = SaveReport { added: 0, updated: 0, ids: vec![] };
    for doc in &docs {
        // Normalise the notetype name's case to what the collection uses.
        let mut doc = doc.clone();
        if let Some(n) = notetypes.iter().find(|n| n.name.eq_ignore_ascii_case(&doc.notetype)) {
            doc.notetype = n.name.clone();
        }
        let (id, created) = engine.save_note(&doc)?;
        if created {
            report.added += 1;
        } else {
            report.updated += 1;
        }
        report.ids.push(id);
    }
    Ok(report)
}

/// Text for a blank note in `deck` using `notetype`.
pub fn new_note_template(engine: &mut Engine, notetype: Option<&str>, deck: &str) -> Result<String> {
    let nt = match notetype {
        Some(n) => n.to_string(),
        None => engine.default_notetype()?,
    };
    let fields = engine.field_names(&nt)?.ok_or_else(|| anyhow::anyhow!("unknown notetype {nt:?}"))?;
    let doc = NoteDoc {
        id: None,
        notetype: nt,
        deck: deck.to_string(),
        tags: vec![],
        fields: fields.into_iter().map(|f| (f, String::new())).collect(),
    };
    let mut text = String::from(
        "# Fill in the fields below. Save and quit to add; leave empty to cancel.\n# Repeat the `---` block for more notes; notetype/deck/tags carry over.\n\n",
    );
    text.push_str(&ankh_core::notefile::write(&[doc]));
    Ok(text)
}

/// Strip the leading `# ` comment lines the template adds.
pub fn strip_leading_comments(text: &str) -> String {
    text.lines().skip_while(|l| l.starts_with("# ") || l.trim().is_empty()).collect::<Vec<_>>().join("\n")
}

/// True when the user left every field of every note empty.
pub fn is_blank(text: &str) -> bool {
    text.lines().filter(|l| !l.starts_with("---") && !l.starts_with("## ") && !l.starts_with("# ")).all(|l| {
        let t = l.trim();
        t.is_empty()
            || t.starts_with("notetype:")
            || t.starts_with("deck:")
            || t.starts_with("tags:")
            || t.starts_with("note:")
    })
}
