//! Headless commands. Every one of these has a TUI equivalent; the TUI is
//! just another client of `ankh-core`, not a privileged one.

mod output;

use std::io::{IsTerminal, Read, Write};

use ankh_core::engine::SyncOp;
use ankh_core::{AuthStore, Engine, Error, Paths, Result, SyncOptions, SyncOutcome};

use crate::{Command, Format};
pub use output::Out;

pub fn init_logging(paths: &Paths) {
    use tracing_subscriber::{fmt, EnvFilter};
    let _ = paths.ensure_dirs();
    let Ok(file) = std::fs::OpenOptions::new().create(true).append(true).open(paths.log_file()) else { return };
    let filter = EnvFilter::try_from_env("ANKH_LOG").unwrap_or_else(|_| EnvFilter::new("info"));
    let _ = fmt().with_env_filter(filter).with_writer(file).with_ansi(false).try_init();
}

pub fn run(cmd: Command, paths: Paths, format: Format) -> i32 {
    let out = Out::new(format);
    match dispatch(cmd, paths, &out) {
        Ok(()) => 0,
        Err(e) => {
            out.error(&e);
            e.exit_code()
        }
    }
}

fn dispatch(cmd: Command, paths: Paths, out: &Out) -> Result<()> {
    match cmd {
        Command::Tui => unreachable!(),
        Command::Login { username, password_stdin } => login(paths, username, password_stdin, out),
        Command::Logout => {
            AuthStore::new(&paths.profile).clear()?;
            out.ok("logged out", serde_json::json!({ "profile": paths.profile }));
            Ok(())
        }
        Command::Status => status(paths, out),
        Command::Sync { download, upload, no_media, media_only } => {
            let op = if media_only {
                SyncOp::MediaOnly
            } else if download {
                SyncOp::FullDownload
            } else if upload {
                SyncOp::FullUpload
            } else {
                SyncOp::Normal
            };
            sync(paths, op, SyncOptions { media: !no_media }, out)
        }
        Command::Decks => decks(paths, out),
        Command::Next { deck } => next(paths, deck, out),
        Command::Search { query, sort, reverse, limit } => search(paths, &query.join(" "), &sort, reverse, limit, out),
        Command::Note { note_id } => note(paths, note_id, out),
        Command::Edit { note_id } => edit(paths, note_id, out),
        Command::Add { fields, deck, notetype, tags, file } => add(paths, fields, deck, notetype, tags, file, out),
        Command::Export { query, out: path, apkg, with_scheduling, no_media } => {
            if apkg {
                let Some(p) = path else { return Err(anyhow::anyhow!("--apkg needs --out FILE.apkg").into()) };
                export_apkg(paths, &query.join(" "), &p, with_scheduling, !no_media, out)
            } else {
                export(paths, &query.join(" "), path, out)
            }
        }
        Command::Import { path, notetype, deck } => import(paths, &path, notetype, deck, out),
        Command::Deck { op } => deck_op(paths, op, out),
        Command::Options { deck, edit, fsrs } => options(paths, &deck, edit, fsrs, out),
        Command::Fsrs { op: crate::FsrsOp::Optimize { deck } } => fsrs_optimize(paths, &deck, out),
        Command::Stats { deck, days } => stats(paths, deck, days, out),
        Command::Notetypes => notetypes(paths, out),
        Command::Config { defaults } => {
            if defaults {
                print!("{}", crate::lua::DEFAULTS);
            } else {
                out.ok(
                    &format!("init.lua     {}\ncollection   {}\nlog          {}", paths.init_lua().display(), paths.collection().display(), paths.log_file().display()),
                    serde_json::json!({ "init_lua": paths.init_lua(), "collection": paths.collection(), "log": paths.log_file() }),
                );
            }
            Ok(())
        }
        Command::Card { card_id } => card(paths, card_id, out),
        Command::Bulk { query, suspend, unsuspend, bury, flag, tag, untag, move_to, forget, due, delete, yes } => {
            let op = if suspend {
                BulkOp::Suspend
            } else if unsuspend {
                BulkOp::Unsuspend
            } else if bury {
                BulkOp::Bury
            } else if let Some(n) = flag {
                BulkOp::Flag(n)
            } else if let Some(t) = tag {
                BulkOp::Tag(t)
            } else if let Some(t) = untag {
                BulkOp::Untag(t)
            } else if let Some(d) = move_to {
                BulkOp::Move(d)
            } else if forget {
                BulkOp::Forget
            } else if let Some(d) = due {
                BulkOp::Due(d)
            } else if delete {
                BulkOp::Delete { yes }
            } else {
                return Err(anyhow::anyhow!("bulk needs an operation flag (see --help)").into());
            };
            bulk(paths, &query.join(" "), op, out)
        }
        Command::Answer { card_id, rating, secs } => answer(paths, card_id, &rating, secs, out),
    }
}

fn find_deck(eng: &mut Engine, name: &str) -> Result<ankh_core::DeckId> {
    let tree = eng.deck_tree()?;
    let want = name.to_lowercase();
    tree.all()
        .into_iter()
        .find(|d| d.full_name.to_lowercase() == want || d.name.to_lowercase() == want)
        .map(|d| d.id)
        .ok_or_else(|| anyhow::anyhow!("no deck named {name:?}").into())
}

fn next(paths: Paths, deck: Option<String>, out: &Out) -> Result<()> {
    let mut eng = Engine::open(paths)?;
    if let Some(name) = deck {
        let id = find_deck(&mut eng, &name)?;
        eng.select_deck(id)?;
    }
    let card = eng.next_card()?;
    eng.close()?;
    match card {
        None => out.ok("nothing due", serde_json::json!({ "card": null })),
        Some(c) => {
            let opts = ankh_render::Options {
                stylesheet: Some(ankh_render::Stylesheet::parse(&c.css)),
                reveal_hints: true,
                ..Default::default()
            };
            let q = ankh_render::render_html(&c.question_html, &opts).plain_text();
            let a = ankh_render::render_html(&c.answer_html, &opts).plain_text();
            let text = format!(
                "card {}  [{}]  {} · new {} · learn {} · review {}\n\n{q}\n\n--- answer ---\n{a}\n\n1 again {}   2 hard {}   3 good {}   4 easy {}",
                c.card_id,
                match c.kind { ankh_core::QueueKind::New => "new", ankh_core::QueueKind::Learning => "learning", ankh_core::QueueKind::Review => "review" },
                c.deck_name, c.counts.new, c.counts.learn, c.counts.review,
                c.buttons[0], c.buttons[1], c.buttons[2], c.buttons[3]
            );
            let mut json = serde_json::to_value(&c).unwrap();
            json["question_text"] = serde_json::Value::String(q);
            json["answer_text"] = serde_json::Value::String(a);
            out.ok(&text, serde_json::json!({ "card": json }));
        }
    }
    Ok(())
}

fn answer(paths: Paths, card_id: i64, rating: &str, secs: u32, out: &Out) -> Result<()> {
    let rating = match rating.to_lowercase().as_str() {
        "1" | "again" => ankh_core::Rating::Again,
        "2" | "hard" => ankh_core::Rating::Hard,
        "3" | "good" => ankh_core::Rating::Good,
        "4" | "easy" => ankh_core::Rating::Easy,
        other => return Err(anyhow::anyhow!("unknown rating {other:?} (again|hard|good|easy|1-4)").into()),
    };
    let mut eng = Engine::open(paths)?;
    // The scheduler needs the states the card was shown with; re-fetch and check it's still next.
    let card = eng.next_card()?.filter(|c| c.card_id == card_id).ok_or_else(|| {
        anyhow::anyhow!("card {card_id} is not the next card in the current deck; run `ankh next` first")
    })?;
    eng.answer(&card, rating, secs.saturating_mul(1000))?;
    eng.close()?;
    out.ok(
        &format!("answered {card_id}: {}", rating.label()),
        serde_json::json!({ "card_id": card_id, "rating": rating }),
    );
    Ok(())
}

fn prompt(label: &str) -> std::io::Result<String> {
    eprint!("{label}: ");
    std::io::stderr().flush()?;
    let mut s = String::new();
    std::io::stdin().read_line(&mut s)?;
    Ok(s.trim_end_matches(['\r', '\n']).to_string())
}

fn login(paths: Paths, username: Option<String>, password_stdin: bool, out: &Out) -> Result<()> {
    let username = match username {
        Some(u) => u,
        None => prompt("AnkiWeb username (email)")?,
    };
    let password = if password_stdin || !std::io::stdin().is_terminal() {
        let mut s = String::new();
        std::io::stdin().read_to_string(&mut s)?;
        s.trim_end_matches(['\r', '\n']).to_string()
    } else {
        rpassword::prompt_password("AnkiWeb password: ").map_err(Error::Io)?
    };
    let creds = Engine::login(&paths, &username, &password)?;
    drop(password);
    AuthStore::new(&paths.profile).save(&creds)?;
    out.ok(
        &format!("logged in as {} (sync key stored in keyring)", creds.username),
        serde_json::json!({ "profile": paths.profile, "username": creds.username }),
    );
    Ok(())
}

fn status(paths: Paths, out: &Out) -> Result<()> {
    let creds = AuthStore::new(&paths.profile).load()?;
    let collection = paths.collection();
    let exists = collection.exists();
    let (cards, notes, media) = if exists {
        let media = std::fs::read_dir(paths.media_folder()).map(|d| d.count()).unwrap_or(0);
        let mut eng = Engine::open(paths.clone())?;
        let tree = eng.deck_tree()?;
        let cards: u32 = tree.roots.iter().map(|d| d.total_with_children).sum();
        (cards, 0, media)
    } else {
        (0, 0, 0)
    };
    let json = serde_json::json!({
        "profile": paths.profile,
        "logged_in": creds.is_some(),
        "username": creds.as_ref().map(|c| c.username.clone()),
        "endpoint": creds.as_ref().and_then(|c| c.endpoint.clone()),
        "collection": collection,
        "collection_exists": exists,
        "cards": cards,
        "media_files": media,
    });
    let _ = notes;
    let text = format!(
        "profile      {}\nlogged in    {}\ncollection   {}{}\ncards        {}\nmedia files  {}",
        paths.profile,
        match &creds {
            Some(c) => format!("yes ({})", c.username),
            None => "no".into(),
        },
        collection.display(),
        if exists { "" } else { "  (not created yet — run `ankh sync`)" },
        cards,
        media,
    );
    out.ok(&text, json);
    Ok(())
}

fn sync(paths: Paths, op: SyncOp, opts: SyncOptions, out: &Out) -> Result<()> {
    let store = AuthStore::new(&paths.profile);
    let mut creds = store.require()?;
    let mut eng = Engine::open(paths)?;
    let mut report = match op {
        SyncOp::Normal => eng.sync(&mut creds, opts)?,
        SyncOp::FullDownload => eng.full_download(&creds, opts)?,
        SyncOp::FullUpload => eng.full_upload(&creds, opts)?,
        SyncOp::MediaOnly => {
            eng.sync_media(&creds)?;
            ankh_core::SyncReport { outcome: SyncOutcome::NoChanges, server_message: String::new(), media_synced: true }
        }
    };
    if let SyncOutcome::FullSyncRequired { download_ok: true, .. } = report.outcome {
        if eng.is_pristine()? {
            eprintln!("empty local collection — downloading from AnkiWeb");
            report = eng.full_download(&creds, opts)?;
        }
    }
    // Persist a shard redirect so later syncs go straight there.
    store.save(&creds)?;
    eng.close()?;
    let text = match &report.outcome {
        SyncOutcome::NoChanges => "already in sync".to_string(),
        SyncOutcome::Synced => "synced".to_string(),
        SyncOutcome::FullDownloaded => "downloaded collection from AnkiWeb".to_string(),
        SyncOutcome::FullUploaded => "uploaded collection to AnkiWeb".to_string(),
        SyncOutcome::FullSyncRequired { upload_ok, download_ok } => {
            eprintln!("resolve with `ankh sync --download` or `ankh sync --upload`");
            return Err(Error::FullSyncRequired { upload_ok: *upload_ok, download_ok: *download_ok });
        }
    };
    let text = if report.media_synced { format!("{text} (media synced)") } else { text };
    let text = if report.server_message.is_empty() {
        text
    } else {
        format!("{text}\nAnkiWeb says: {}", report.server_message)
    };
    out.ok(&text, serde_json::to_value(&report).unwrap());
    Ok(())
}

fn decks(paths: Paths, out: &Out) -> Result<()> {
    let mut eng = Engine::open(paths)?;
    let tree = eng.deck_tree()?;
    eng.close()?;
    let rows: Vec<Vec<String>> = tree
        .all()
        .into_iter()
        .map(|d| {
            vec![
                format!("{}{}", "  ".repeat(d.level.saturating_sub(1) as usize), d.name),
                d.new.to_string(),
                d.learn.to_string(),
                d.review.to_string(),
                d.total_with_children.to_string(),
            ]
        })
        .collect();
    out.table(
        &["deck", "new", "learn", "due", "cards"],
        rows,
        || serde_json::to_value(&tree.roots).unwrap(),
        || tree.all().into_iter().map(|d| serde_json::to_value(d).unwrap()).collect(),
    );
    Ok(())
}

enum BulkOp {
    Suspend,
    Unsuspend,
    Bury,
    Flag(u8),
    Tag(String),
    Untag(String),
    Move(String),
    Forget,
    Due(String),
    Delete { yes: bool },
}

fn search(paths: Paths, query: &str, sort: &str, reverse: bool, limit: usize, out: &Out) -> Result<()> {
    let sort = ankh_core::SortBy::parse(sort).ok_or_else(|| anyhow::anyhow!("unknown sort column {sort:?}"))?;
    let mut eng = Engine::open(paths)?;
    let ids = eng.search(query, sort, reverse)?;
    let total = ids.len();
    let page: Vec<i64> = ids.into_iter().take(limit).collect();
    let rows = eng.browser_rows(&page)?;
    eng.close()?;
    let table: Vec<Vec<String>> = rows
        .iter()
        .map(|r| {
            vec![
                r.card_id.to_string(),
                r.sort_field.chars().take(48).collect(),
                r.deck.rsplit("::").next().unwrap_or("").to_string(),
                format!("{:?}", r.state).to_lowercase(),
                r.due.clone(),
                r.interval_days.to_string(),
                r.tags.join(" "),
            ]
        })
        .collect();
    if total > limit && matches!(out.format(), Format::Table) {
        eprintln!("showing {limit} of {total} (use --limit)");
    }
    out.table(
        &["id", "field", "deck", "state", "due", "ivl", "tags"],
        table,
        || serde_json::json!({ "total": total, "query": query, "cards": rows }),
        || rows.iter().map(|r| serde_json::to_value(r).unwrap()).collect(),
    );
    Ok(())
}

fn card(paths: Paths, card_id: i64, out: &Out) -> Result<()> {
    let mut eng = Engine::open(paths)?;
    let info = eng.card_info(card_id)?;
    let (q, a, css) = eng.render_card(card_id)?;
    eng.close()?;
    let opts = ankh_render::Options {
        stylesheet: Some(ankh_render::Stylesheet::parse(&css)),
        reveal_hints: true,
        ..Default::default()
    };
    let qt = ankh_render::render_html(&q, &opts).plain_text();
    let at = ankh_render::render_html(&a, &opts).plain_text();
    let date = |t: i64| {
        chrono::DateTime::from_timestamp(t, 0)
            .map(|d| d.with_timezone(&chrono::Local).format("%Y-%m-%d").to_string())
            .unwrap_or_default()
    };
    let mut text = format!(
        "card {}  note {}\ndeck       {}\nnotetype   {} · {}\npreset     {}\nadded      {}\ndue        {}\ninterval   {}d\nreviews    {} ({} lapses)\n",
        info.card_id, info.note_id, info.deck, info.notetype, info.template, info.preset, date(info.added),
        info.due_date.map(date).unwrap_or_else(|| "—".into()), info.interval_days, info.reviews, info.lapses
    );
    if let (Some(s), Some(d)) = (info.stability, info.difficulty) {
        text.push_str(&format!("fsrs       stability {s:.1}d · difficulty {:.0}%\n", d * 10.0));
    }
    text.push_str(&format!("\n{qt}\n\n--- answer ---\n{at}\n"));
    if !info.revlog.is_empty() {
        text.push_str("\nrecent reviews\n");
        for r in info.revlog.iter().take(10) {
            let btn = ["", "again", "hard", "good", "easy"].get(r.button as usize).copied().unwrap_or("?");
            text.push_str(&format!("  {}  {:<6} {:<9} {:.0}s\n", date(r.time), btn, r.kind, r.taken_secs));
        }
    }
    let mut json = serde_json::to_value(&info).unwrap();
    json["question_text"] = serde_json::Value::String(qt);
    json["answer_text"] = serde_json::Value::String(at);
    out.ok(text.trim_end(), json);
    Ok(())
}

fn bulk(paths: Paths, query: &str, op: BulkOp, out: &Out) -> Result<()> {
    let mut eng = Engine::open(paths)?;
    let cids = eng.search(query, ankh_core::SortBy::SortField, false)?;
    if cids.is_empty() {
        out.ok("no cards match", serde_json::json!({ "matched": 0 }));
        return Ok(());
    }
    let n = cids.len();
    let msg = match op {
        BulkOp::Suspend => format!("suspended {} cards", eng.suspend_cards(&cids)?),
        BulkOp::Unsuspend => {
            eng.unsuspend_cards(&cids)?;
            format!("unsuspended {n} cards")
        }
        BulkOp::Bury => format!("buried {} cards", eng.bury_cards(&cids)?),
        BulkOp::Flag(f) => {
            if f > 7 {
                return Err(anyhow::anyhow!("flag must be 0-7").into());
            }
            format!("flagged {} cards", eng.flag_cards(&cids, f)?)
        }
        BulkOp::Tag(t) => {
            let nids = eng.note_ids_for_cards(&cids)?;
            format!("tagged {} notes", eng.add_tags(&nids, &t)?)
        }
        BulkOp::Untag(t) => {
            let nids = eng.note_ids_for_cards(&cids)?;
            format!("untagged {} notes", eng.remove_tags(&nids, &t)?)
        }
        BulkOp::Move(d) => {
            let id = eng.deck_id_by_name(&d, true)?.expect("created");
            format!("moved {} cards to {d}", eng.move_cards(&cids, id)?)
        }
        BulkOp::Forget => {
            eng.forget_cards(&cids)?;
            format!("reset {n} cards to new")
        }
        BulkOp::Due(d) => {
            eng.set_due(&cids, &d)?;
            format!("set due date on {n} cards")
        }
        BulkOp::Delete { yes } => {
            let nids = eng.note_ids_for_cards(&cids)?;
            if !yes {
                if !std::io::stdin().is_terminal() {
                    return Err(anyhow::anyhow!("refusing to delete {} notes without --yes", nids.len()).into());
                }
                let ans = prompt(&format!("delete {} notes and all their cards? [y/N]", nids.len()))?;
                if !ans.eq_ignore_ascii_case("y") {
                    out.ok("cancelled", serde_json::json!({ "matched": n, "deleted": 0 }));
                    return Ok(());
                }
            }
            format!("deleted {} notes", eng.delete_notes(&nids)?)
        }
    };
    eng.close()?;
    out.ok(&msg, serde_json::json!({ "matched": n, "result": msg }));
    Ok(())
}

fn note(paths: Paths, note_id: i64, out: &Out) -> Result<()> {
    let mut eng = Engine::open(paths)?;
    let doc = eng.note_doc(note_id)?;
    let data = eng.note(note_id)?;
    eng.close()?;
    out.ok(&ankh_core::notefile::write(&[doc]), serde_json::to_value(&data).unwrap());
    Ok(())
}

fn edit(paths: Paths, note_id: i64, out: &Out) -> Result<()> {
    let mut eng = Engine::open(paths)?;
    let doc = eng.note_doc(note_id)?;
    let text = ankh_core::notefile::write(&[doc]);
    let Some(edited) = crate::editor::edit_text(&text, &format!("note-{note_id}"))? else {
        out.ok("no changes", serde_json::json!({ "note_id": note_id, "changed": false }));
        return Ok(());
    };
    let r = crate::editor::save_note_file(&mut eng, &edited)?;
    eng.close()?;
    out.ok(
        &format!("saved note {note_id}"),
        serde_json::json!({ "note_id": note_id, "changed": true, "updated": r.updated, "added": r.added }),
    );
    Ok(())
}

fn add(
    paths: Paths,
    fields: Vec<String>,
    deck: Option<String>,
    notetype: Option<String>,
    tags: Option<String>,
    file: Option<String>,
    out: &Out,
) -> Result<()> {
    let mut eng = Engine::open(paths)?;
    let deck_name = match deck {
        Some(d) => d,
        None => eng.current_deck()?.1,
    };
    let report = if let Some(f) = file {
        let text = if f == "-" {
            let mut s = String::new();
            std::io::stdin().read_to_string(&mut s)?;
            s
        } else {
            std::fs::read_to_string(&f)?
        };
        crate::editor::save_note_file(&mut eng, &text)?
    } else if fields.is_empty() {
        if !std::io::stdin().is_terminal() {
            return Err(anyhow::anyhow!(
                "no fields given and stdin is not a terminal; use --file - to read a note file"
            )
            .into());
        }
        let text = crate::editor::new_note_template(&mut eng, notetype.as_deref(), &deck_name)?;
        let Some(edited) = crate::editor::edit_text(&text, "new")? else {
            out.ok("cancelled", serde_json::json!({ "added": 0 }));
            return Ok(());
        };
        let body = crate::editor::strip_leading_comments(&edited);
        if crate::editor::is_blank(&body) {
            out.ok("cancelled (empty note)", serde_json::json!({ "added": 0 }));
            return Ok(());
        }
        crate::editor::save_note_file(&mut eng, &body)?
    } else {
        let nt = match notetype {
            Some(n) => n,
            None => eng.default_notetype()?,
        };
        let names = eng.field_names(&nt)?.ok_or_else(|| anyhow::anyhow!("unknown notetype {nt:?}"))?;
        if fields.len() > names.len() {
            return Err(anyhow::anyhow!(
                "{nt} has {} fields ({}), got {}",
                names.len(),
                names.join(", "),
                fields.len()
            )
            .into());
        }
        let doc = ankh_core::NoteDoc {
            id: None,
            notetype: nt,
            deck: deck_name,
            tags: tags.map(|t| t.split_whitespace().map(String::from).collect()).unwrap_or_default(),
            fields: names
                .iter()
                .enumerate()
                .map(|(i, n)| (n.clone(), fields.get(i).cloned().unwrap_or_default()))
                .collect(),
        };
        let (id, _) = eng.save_note(&doc)?;
        crate::editor::SaveReport { added: 1, updated: 0, ids: vec![id] }
    };
    eng.close()?;
    out.ok(
        &format!(
            "added {} note{}{}",
            report.added,
            if report.added == 1 { "" } else { "s" },
            if report.updated > 0 { format!(", updated {}", report.updated) } else { String::new() }
        ),
        serde_json::json!({ "added": report.added, "updated": report.updated, "note_ids": report.ids }),
    );
    Ok(())
}

fn export(paths: Paths, query: &str, path: Option<std::path::PathBuf>, out: &Out) -> Result<()> {
    let mut eng = Engine::open(paths)?;
    let cids = eng.search(query, ankh_core::SortBy::Created, false)?;
    let nids = eng.note_ids_for_cards(&cids)?;
    let mut docs = Vec::with_capacity(nids.len());
    for nid in &nids {
        docs.push(eng.note_doc(*nid)?);
    }
    eng.close()?;
    let text = ankh_core::notefile::write(&docs);
    match path {
        Some(p) => {
            std::fs::write(&p, &text)?;
            out.ok(
                &format!("exported {} notes to {}", docs.len(), p.display()),
                serde_json::json!({ "notes": docs.len(), "path": p }),
            );
        }
        None => match out.format() {
            Format::Table => print!("{text}"),
            _ => out.ok("", serde_json::json!({ "notes": docs })),
        },
    }
    Ok(())
}

fn import(
    paths: Paths,
    path: &std::path::Path,
    notetype: Option<String>,
    deck: Option<String>,
    out: &Out,
) -> Result<()> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    let mut eng = Engine::open(paths)?;
    match ext.as_str() {
        "apkg" => {
            let s = eng.import_apkg(&path.to_string_lossy())?;
            eng.close()?;
            out.ok(
                &format!("imported: {} added, {} updated, {} duplicates, {} conflicting", s.added, s.updated, s.duplicates, s.conflicting),
                serde_json::to_value(&s).unwrap(),
            );
        }
        "csv" | "tsv" | "txt" => {
            let nt = match notetype {
                Some(n) => n,
                None => eng.default_notetype()?,
            };
            let deck = match deck {
                Some(d) => d,
                None => eng.current_deck()?.1,
            };
            let s = eng.import_csv(&path.to_string_lossy(), &nt, &deck)?;
            eng.close()?;
            out.ok(
                &format!("imported into {deck} as {nt}: {} added, {} updated, {} duplicates", s.added, s.updated, s.duplicates),
                serde_json::to_value(&s).unwrap(),
            );
        }
        "colpkg" => {
            return Err(anyhow::anyhow!(
                "importing a .colpkg replaces the whole collection; restore it in Anki desktop, then `ankh sync --download`"
            )
            .into())
        }
        _ => {
            let text = std::fs::read_to_string(path)?;
            let r = crate::editor::save_note_file(&mut eng, &text)?;
            eng.close()?;
            out.ok(
                &format!("added {}, updated {}", r.added, r.updated),
                serde_json::json!({ "added": r.added, "updated": r.updated, "note_ids": r.ids }),
            );
        }
    }
    Ok(())
}

fn export_apkg(
    paths: Paths,
    query: &str,
    path: &std::path::Path,
    with_scheduling: bool,
    with_media: bool,
    out: &Out,
) -> Result<()> {
    let mut eng = Engine::open(paths)?;
    let n = eng.export_apkg(&path.to_string_lossy(), query, with_scheduling, with_media)?;
    eng.close()?;
    out.ok(&format!("exported {n} notes to {}", path.display()), serde_json::json!({ "notes": n, "path": path }));
    Ok(())
}

fn deck_op(paths: Paths, op: crate::DeckOp, out: &Out) -> Result<()> {
    let mut eng = Engine::open(paths)?;
    match op {
        crate::DeckOp::Create { name } => {
            let id = eng.create_deck(&name)?;
            out.ok(&format!("created {name}"), serde_json::json!({ "deck_id": id, "name": name }));
        }
        crate::DeckOp::Rename { name, new_name } => {
            let id = find_deck(&mut eng, &name)?;
            eng.rename_deck(id, &new_name)?;
            out.ok(&format!("renamed {name} → {new_name}"), serde_json::json!({ "deck_id": id, "name": new_name }));
        }
        crate::DeckOp::Delete { name, yes } => {
            let id = find_deck(&mut eng, &name)?;
            if !yes {
                if !std::io::stdin().is_terminal() {
                    return Err(anyhow::anyhow!("refusing to delete {name} without --yes").into());
                }
                let ans = prompt(&format!("delete {name}, its subdecks and all their cards? [y/N]"))?;
                if !ans.eq_ignore_ascii_case("y") {
                    out.ok("cancelled", serde_json::json!({ "deleted": 0 }));
                    return Ok(());
                }
            }
            let n = eng.delete_deck(id)?;
            out.ok(&format!("deleted {name} ({n} cards)"), serde_json::json!({ "deck_id": id, "cards": n }));
        }
    }
    eng.close()?;
    Ok(())
}

fn options(paths: Paths, deck: &str, edit: bool, fsrs: Option<String>, out: &Out) -> Result<()> {
    let mut eng = Engine::open(paths)?;
    let id = find_deck(&mut eng, deck)?;
    if let Some(f) = fsrs {
        let on = matches!(f.to_ascii_lowercase().as_str(), "on" | "true" | "1" | "yes");
        eng.set_fsrs_enabled(id, on)?;
        eng.close()?;
        out.ok(&format!("FSRS {}", if on { "enabled" } else { "disabled" }), serde_json::json!({ "fsrs": on }));
        return Ok(());
    }
    let (opts, info) = eng.deck_options(id)?;
    let text = opts.to_toml(&info);
    if edit {
        let Some(edited) = crate::editor::edit_text(&text, "options")? else {
            eng.close()?;
            out.ok("no changes", serde_json::json!({ "changed": false }));
            return Ok(());
        };
        let new = ankh_core::DeckOptions::from_toml(&edited)?;
        eng.save_deck_options(id, &new)?;
        eng.close()?;
        out.ok(&format!("saved preset {:?}", new.preset), serde_json::json!({ "changed": true, "preset": new.preset }));
    } else {
        eng.close()?;
        out.ok(&text, serde_json::json!({ "options": opts, "info": info }));
    }
    Ok(())
}

fn fsrs_optimize(paths: Paths, deck: &str, out: &Out) -> Result<()> {
    let mut eng = Engine::open(paths)?;
    let id = find_deck(&mut eng, deck)?;
    eprintln!("optimising FSRS parameters for {deck} — this can take a minute…");
    let (params, items) = eng.fsrs_optimize(id)?;
    eng.close()?;
    out.ok(
        &format!(
            "optimised from {items} reviews: [{}]",
            params.iter().map(|p| format!("{p:.4}")).collect::<Vec<_>>().join(", ")
        ),
        serde_json::json!({ "params": params, "reviews": items }),
    );
    Ok(())
}

fn stats(paths: Paths, deck: Option<String>, days: u32, out: &Out) -> Result<()> {
    let mut eng = Engine::open(paths)?;
    let search = match &deck {
        Some(d) => format!("deck:\"{d}\""),
        None => String::new(),
    };
    let s = eng.stats(&search, days)?;
    eng.close()?;
    let t = &s.today;
    let c = &s.counts;
    let due_week: u32 = s.forecast.iter().filter(|(d, _)| **d <= 7).map(|(_, n)| n).sum();
    let reviews_30: u32 = s.reviews_per_day.iter().filter(|(d, _)| **d < 30).map(|(_, n)| n).sum();
    let backlog = s
        .forecast
        .keys()
        .next()
        .filter(|d| **d < 0)
        .map(|d| format!(" (backlog from {}d ago)", -d))
        .unwrap_or_default();
    let text = format!(
        "{}\n\ntoday      {} cards in {:.1} min, {}% correct\ncards      {} new · {} learning · {} young · {} mature · {} suspended · {} buried\nnext 7d    {} due{}\nlast 30d   {} reviews\nretention  {}\nmemory     {:.0}% average retrievability",
        deck.clone().unwrap_or_else(|| "whole collection".into()),
        t.answered,
        t.secs / 60.0,
        t.correct.saturating_mul(100).checked_div(t.answered).unwrap_or(0),
        c.new,
        c.learning + c.relearning,
        c.young,
        c.mature,
        c.suspended,
        c.buried,
        due_week,
        backlog,
        reviews_30,
        s.mature_retention.map(|r| format!("{:.1}% (mature, last month)", r * 100.0)).unwrap_or_else(|| "—".into()),
        s.average_retrievability,
    );
    out.ok(&text, serde_json::to_value(&s).unwrap());
    Ok(())
}

fn notetypes(paths: Paths, out: &Out) -> Result<()> {
    let mut eng = Engine::open(paths)?;
    let nts = eng.notetypes()?;
    eng.close()?;
    let rows = nts
        .iter()
        .map(|n| {
            vec![
                n.name.clone(),
                n.notes.to_string(),
                if n.cloze { "cloze".into() } else { String::new() },
                n.fields.join(", "),
            ]
        })
        .collect();
    out.table(
        &["notetype", "notes", "kind", "fields"],
        rows,
        || serde_json::to_value(&nts).unwrap(),
        || nts.iter().map(|n| serde_json::to_value(n).unwrap()).collect(),
    );
    Ok(())
}
