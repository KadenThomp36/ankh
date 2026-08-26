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
    }
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
