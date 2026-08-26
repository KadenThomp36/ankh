//! Reading, editing and adding notes.

use anki::decks::DeckId as AnkiDeckId;
use anki::notes::NoteId;
use serde::Serialize;

use crate::engine::Engine;
use crate::error::Result;
use crate::notefile::NoteDoc;

#[derive(Debug, Clone, Serialize)]
pub struct NotetypeInfo {
    pub id: i64,
    pub name: String,
    pub fields: Vec<String>,
    pub cloze: bool,
    pub notes: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct NoteData {
    pub id: i64,
    pub notetype: String,
    pub deck: String,
    pub tags: Vec<String>,
    pub fields: Vec<(String, String)>,
    pub card_ids: Vec<i64>,
}

impl Engine {
    pub fn notetypes(&mut self) -> Result<Vec<NotetypeInfo>> {
        let col = self.col()?;
        let mut out = Vec::new();
        for nt in col.get_all_notetypes()? {
            let notes =
                col.search_notes_unordered(format!("mid:{}", nt.id.0).as_str()).map(|v| v.len() as u32).unwrap_or(0);
            out.push(NotetypeInfo {
                id: nt.id.0,
                name: nt.name.clone(),
                fields: nt.fields.iter().map(|f| f.name.clone()).collect(),
                cloze: nt.config.kind == 1,
                notes,
            });
        }
        out.sort_by(|a, b| b.notes.cmp(&a.notes).then(a.name.cmp(&b.name)));
        Ok(out)
    }

    pub fn field_names(&mut self, notetype: &str) -> Result<Option<Vec<String>>> {
        let col = self.col()?;
        Ok(col.get_notetype_by_name(notetype)?.map(|nt| nt.fields.iter().map(|f| f.name.clone()).collect()))
    }

    pub fn note(&mut self, note_id: i64) -> Result<NoteData> {
        let col = self.col()?;
        let nid = NoteId(note_id);
        let note = col.storage.get_note(nid)?.ok_or_else(|| anyhow::anyhow!("note {note_id} not found"))?;
        let nt = col.get_notetype(note.notetype_id)?.ok_or_else(|| anyhow::anyhow!("notetype missing"))?;
        let cards = col.storage.all_cards_of_note(nid)?;
        let deck = match cards.first() {
            Some(c) => col.get_deck(c.deck_id())?.map(|d| d.human_name()).unwrap_or_default(),
            None => String::new(),
        };
        Ok(NoteData {
            id: note_id,
            notetype: nt.name.clone(),
            deck,
            tags: note.tags.clone(),
            fields: nt.fields.iter().zip(note.fields()).map(|(f, v)| (f.name.clone(), v.clone())).collect(),
            card_ids: cards.iter().map(|c| c.id().0).collect(),
        })
    }

    pub fn note_doc(&mut self, note_id: i64) -> Result<NoteDoc> {
        let n = self.note(note_id)?;
        Ok(NoteDoc::from_html(Some(n.id), &n.notetype, &n.deck, &n.tags, &n.fields))
    }

    /// Write a parsed note file entry back. Returns the note id (new or
    /// existing) and whether it was created.
    pub fn save_note(&mut self, doc: &NoteDoc) -> Result<(i64, bool)> {
        let fields = doc.fields_html();
        let fields = self.import_local_media(fields)?;
        let col = self.col()?;
        let nt = col
            .get_notetype_by_name(&doc.notetype)?
            .ok_or_else(|| anyhow::anyhow!("unknown notetype {:?}", doc.notetype))?;
        let deck = col.get_or_create_normal_deck(&doc.deck)?;
        let mut tags: Vec<String> = doc.tags.iter().filter(|t| !t.is_empty()).cloned().collect();
        tags.dedup();
        match doc.id {
            Some(id) => {
                let mut note =
                    col.storage.get_note(NoteId(id))?.ok_or_else(|| anyhow::anyhow!("note {id} not found"))?;
                if note.notetype_id != nt.id {
                    return Err(anyhow::anyhow!(
                        "changing a note's notetype isn't supported here; use the desktop app"
                    )
                    .into());
                }
                for (i, f) in nt.fields.iter().enumerate() {
                    if let Some((_, html)) = fields.iter().find(|(n, _)| n == &f.name) {
                        note.set_field(i, html.clone())?;
                    }
                }
                note.tags = tags;
                col.update_note(&mut note)?;
                // Move cards if the deck changed.
                let cards = col.storage.all_cards_of_note(NoteId(id))?;
                let ids: Vec<_> = cards.iter().filter(|c| c.deck_id() != deck.id).map(|c| c.id()).collect();
                if !ids.is_empty() {
                    col.set_deck(&ids, deck.id)?;
                }
                Ok((id, false))
            }
            None => {
                let mut note = nt.new_note();
                for (i, f) in nt.fields.iter().enumerate() {
                    if let Some((_, html)) = fields.iter().find(|(n, _)| n == &f.name) {
                        note.set_field(i, html.clone())?;
                    }
                }
                note.tags = tags;
                col.add_note(&mut note, AnkiDeckId(deck.id.0))?;
                Ok((note.id.0, true))
            }
        }
    }

    /// `<img src="/abs/or/relative/path.png">` pointing at a real file
    /// outside the media folder is copied in and the reference rewritten.
    fn import_local_media(&mut self, fields: Vec<(String, String)>) -> Result<Vec<(String, String)>> {
        let media_folder = self.paths().media_folder();
        let mut out = Vec::with_capacity(fields.len());
        for (name, html) in fields {
            let mut html = html;
            let mut search = 0;
            while let Some(i) = html[search..].find("src=\"") {
                let start = search + i + 5;
                let Some(len) = html[start..].find('"') else { break };
                let src = html[start..start + len].to_string();
                search = start + len;
                if src.contains("://") || src.starts_with("data:") {
                    continue;
                }
                let path = std::path::Path::new(&src);
                let candidate =
                    if path.is_absolute() { path.to_path_buf() } else { std::env::current_dir()?.join(path) };
                if candidate.is_file() && !candidate.starts_with(&media_folder) {
                    let data = std::fs::read(&candidate)?;
                    let fname = candidate.file_name().and_then(|f| f.to_str()).unwrap_or("image").to_string();
                    let stored = self.col()?.media()?.add_file(&fname, &data)?.into_owned();
                    html.replace_range(start..start + len, &stored);
                    search = start + stored.len();
                }
            }
            out.push((name, html));
        }
        Ok(out)
    }

    /// A sensible notetype for a new note: the most-used non-cloze one.
    pub fn default_notetype(&mut self) -> Result<String> {
        let nts = self.notetypes()?;
        Ok(nts.iter().find(|n| !n.cloze).or(nts.first()).map(|n| n.name.clone()).unwrap_or_else(|| "Basic".into()))
    }
}
