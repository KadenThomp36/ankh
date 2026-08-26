//! The [`Engine`] owns one open collection plus the machinery to talk to
//! AnkiWeb. It is deliberately synchronous from the caller's point of view:
//! the CLI blocks, and the TUI moves the engine onto a worker thread for
//! long operations (see [`Engine::sync_in_background`]).

use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use anki::collection::{Collection, CollectionBuilder};
use anki::sync::login::{sync_login, SyncAuth};
use anki::sync::media::progress::MediaSyncProgress;
use anki::timestamp::TimestampSecs;
use tokio::runtime::Runtime;

use crate::auth::Credentials;
use crate::decks::DeckTree;
use crate::error::{Error, Result};
use crate::paths::Paths;
use crate::sync::{SyncOptions, SyncOutcome, SyncProgress, SyncReport};

/// Shared progress state, type-erased.
///
/// rslib's `ProgressState` lives in a private module, so it can't be named
/// outside the crate — but it *can* be captured by closures whose type is
/// inferred from `CollectionBuilder::set_shared_progress_state`. These three
/// closures are the only handle we keep.
#[derive(Clone)]
pub struct ProgressLink {
    attach: Arc<dyn Fn(&mut CollectionBuilder) + Send + Sync>,
    snapshot: Arc<dyn Fn() -> String + Send + Sync>,
    reset: Arc<dyn Fn() + Send + Sync>,
}

impl ProgressLink {
    fn new() -> Self {
        let state = Arc::new(Mutex::new(Default::default()));
        let s1 = state.clone();
        let attach: Arc<dyn Fn(&mut CollectionBuilder) + Send + Sync> = Arc::new(move |b: &mut CollectionBuilder| {
            b.set_shared_progress_state(s1.clone());
        });
        let s2 = state.clone();
        let snapshot: Arc<dyn Fn() -> String + Send + Sync> = Arc::new(move || match s2.lock() {
            Ok(g) => format!("{:?}", g.last_progress),
            Err(_) => "None".to_string(),
        });
        let reset: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
            if let Ok(mut g) = state.lock() {
                g.reset();
            }
        });
        ProgressLink { attach, snapshot, reset }
    }

    pub fn snapshot(&self) -> SyncProgress {
        SyncProgress::parse_debug(&(self.snapshot)())
    }
}

pub struct Engine {
    paths: Paths,
    col: Option<Collection>,
    rt: Runtime,
    client: reqwest::Client,
    progress: ProgressLink,
}

impl Engine {
    /// Open (creating if necessary) the profile's collection.
    pub fn open(paths: Paths) -> Result<Self> {
        paths.ensure_dirs()?;
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .worker_threads(2)
            .thread_name("ankh-io")
            .build()?;
        let progress = ProgressLink::new();
        let mut eng = Engine { col: None, rt, client: reqwest::Client::new(), progress, paths };
        eng.reopen()?;
        Ok(eng)
    }

    /// Log in without a collection. Used by `ankh login`, which must work
    /// before any data exists.
    pub fn login(paths: &Paths, username: &str, password: &str) -> Result<Credentials> {
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
        let _ = paths;
        let auth = rt.block_on(sync_login(username, password, None, reqwest::Client::new()))?;
        Ok(Credentials { username: username.to_string(), hkey: auth.hkey, endpoint: None })
    }

    pub fn paths(&self) -> &Paths {
        &self.paths
    }

    pub fn progress(&self) -> ProgressLink {
        self.progress.clone()
    }

    pub fn sync_progress(&self) -> SyncProgress {
        self.progress.snapshot()
    }

    fn builder(&self) -> CollectionBuilder {
        let mut b = CollectionBuilder::new(self.paths.collection());
        b.set_media_paths(self.paths.media_folder(), self.paths.media_db());
        (self.progress.attach)(&mut b);
        b
    }

    fn reopen(&mut self) -> Result<()> {
        if self.col.is_none() {
            self.col = Some(self.builder().build()?);
        }
        Ok(())
    }

    /// Direct access for the domain modules in this crate. Errors if a
    /// background sync currently owns the collection.
    pub(crate) fn col(&mut self) -> Result<&mut Collection> {
        self.col.as_mut().ok_or(Error::Busy)
    }

    pub fn is_busy(&self) -> bool {
        self.col.is_none()
    }

    pub fn close(mut self) -> Result<()> {
        if let Some(col) = self.col.take() {
            col.close(None)?;
        }
        Ok(())
    }

    // ----- sync ----------------------------------------------------------

    fn auth_for(creds: &Credentials) -> Result<SyncAuth> {
        let endpoint = match &creds.endpoint {
            Some(e) => Some(reqwest::Url::parse(e).map_err(|e| anyhow::anyhow!("bad endpoint: {e}"))?),
            None => None,
        };
        Ok(SyncAuth { hkey: creds.hkey.clone(), endpoint, io_timeout_secs: None })
    }

    /// Normal (incremental) sync. Never performs a full sync on its own; if
    /// one is required the report says so and nothing has been changed.
    /// Updates `creds.endpoint` if AnkiWeb redirected us to a shard.
    pub fn sync(&mut self, creds: &mut Credentials, opts: SyncOptions) -> Result<SyncReport> {
        let auth = Self::auth_for(creds)?;
        let client = self.client.clone();
        let out = {
            let Engine { col, rt, .. } = self;
            let col = col.as_mut().ok_or(Error::Busy)?;
            rt.block_on(col.normal_sync(auth.clone(), client))?
        };
        if let Some(ep) = &out.new_endpoint {
            creds.endpoint = Some(ep.clone());
        }
        let outcome: SyncOutcome = out.required.into();
        let mut media_synced = false;
        if opts.media && !matches!(outcome, SyncOutcome::FullSyncRequired { .. }) {
            self.sync_media(creds)?;
            media_synced = true;
        }
        Ok(SyncReport { outcome, server_message: out.server_message, media_synced })
    }

    /// Replace the local collection with AnkiWeb's copy.
    pub fn full_download(&mut self, creds: &Credentials, opts: SyncOptions) -> Result<SyncReport> {
        let auth = Self::auth_for(creds)?;
        let col = self.col.take().ok_or(Error::Busy)?;
        let res = self.rt.block_on(col.full_download(auth, self.client.clone()));
        self.reopen()?;
        res?;
        let media_synced = opts.media && {
            self.sync_media(creds)?;
            true
        };
        Ok(SyncReport { outcome: SyncOutcome::FullDownloaded, server_message: String::new(), media_synced })
    }

    /// Replace AnkiWeb's collection with the local one.
    pub fn full_upload(&mut self, creds: &Credentials, opts: SyncOptions) -> Result<SyncReport> {
        let auth = Self::auth_for(creds)?;
        let col = self.col.take().ok_or(Error::Busy)?;
        let res = self.rt.block_on(col.full_upload(auth, self.client.clone()));
        self.reopen()?;
        res?;
        let media_synced = opts.media && {
            self.sync_media(creds)?;
            true
        };
        Ok(SyncReport { outcome: SyncOutcome::FullUploaded, server_message: String::new(), media_synced })
    }

    pub fn sync_media(&mut self, creds: &Credentials) -> Result<()> {
        let auth = Self::auth_for(creds)?;
        let client = self.client.clone();
        let Engine { col, rt, .. } = self;
        let col = col.as_mut().ok_or(Error::Busy)?;
        let progress = col.new_progress_handler::<MediaSyncProgress>();
        let mgr = col.media()?;
        rt.block_on(mgr.sync_media(progress, auth, client, None))?;
        Ok(())
    }

    /// Run `op` on a worker thread while the caller keeps its UI responsive.
    /// The collection is moved into the worker and handed back by
    /// [`SyncHandle::finish`]; until then the engine reports [`Error::Busy`].
    pub fn sync_in_background(&mut self, creds: Credentials, op: SyncOp, opts: SyncOptions) -> Result<SyncHandle> {
        let col = self.col.take().ok_or(Error::Busy)?;
        let paths = self.paths.clone();
        let client = self.client.clone();
        let progress = self.progress.clone();
        let handle = self.rt.handle().clone();
        (progress.reset)();
        let join = std::thread::Builder::new().name("ankh-sync".into()).spawn(move || {
            let mut worker = Worker { col: Some(col), paths, client, progress, handle };
            let mut creds = creds;
            let report = worker.run(&mut creds, op, opts);
            let col = worker.take_col();
            (report, creds, col)
        })?;
        Ok(SyncHandle { join: Some(join) })
    }

    /// Reattach a collection returned by a finished background sync.
    pub fn finish_background(&mut self, handle: SyncHandle) -> Result<(SyncReport, Credentials)> {
        let (report, creds, col) = handle.join()?;
        match col {
            Some(c) => self.col = Some(c),
            None => self.reopen()?,
        }
        Ok((report?, creds))
    }

    /// True for a collection that has never held a card — the common case
    /// right after `ankh login`, where the only sensible full-sync answer is
    /// "download".
    pub fn is_pristine(&mut self) -> Result<bool> {
        let tree = self.deck_tree()?;
        Ok(tree.roots.iter().all(|d| d.total_with_children == 0))
    }

    // ----- decks ---------------------------------------------------------

    pub fn deck_tree(&mut self) -> Result<DeckTree> {
        let col = self.col()?;
        let tree = col.deck_tree(Some(TimestampSecs::now()))?;
        Ok(DeckTree::from_proto(tree))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncOp {
    Normal,
    FullDownload,
    FullUpload,
    MediaOnly,
}

/// The half of an [`Engine`] that a background sync needs.
struct Worker {
    col: Option<Collection>,
    paths: Paths,
    client: reqwest::Client,
    progress: ProgressLink,
    handle: tokio::runtime::Handle,
}

impl Worker {
    fn reopen(&mut self) -> Result<()> {
        if self.col.is_none() {
            let mut b = CollectionBuilder::new(self.paths.collection());
            b.set_media_paths(self.paths.media_folder(), self.paths.media_db());
            (self.progress.attach)(&mut b);
            self.col = Some(b.build()?);
        }
        Ok(())
    }

    fn take_col(&mut self) -> Option<Collection> {
        self.col.take()
    }

    fn media(&mut self, creds: &Credentials) -> Result<()> {
        let auth = Engine::auth_for(creds)?;
        let col = self.col.as_mut().ok_or(Error::Busy)?;
        let progress = col.new_progress_handler::<MediaSyncProgress>();
        let mgr = col.media()?;
        self.handle.block_on(mgr.sync_media(progress, auth, self.client.clone(), None))?;
        Ok(())
    }

    fn run(&mut self, creds: &mut Credentials, op: SyncOp, opts: SyncOptions) -> Result<SyncReport> {
        match op {
            SyncOp::Normal => {
                let auth = Engine::auth_for(creds)?;
                let col = self.col.as_mut().ok_or(Error::Busy)?;
                let out = self.handle.block_on(col.normal_sync(auth, self.client.clone()))?;
                if let Some(ep) = &out.new_endpoint {
                    creds.endpoint = Some(ep.clone());
                }
                let outcome: SyncOutcome = out.required.into();
                let mut media_synced = false;
                if opts.media && !matches!(outcome, SyncOutcome::FullSyncRequired { .. }) {
                    self.media(creds)?;
                    media_synced = true;
                }
                Ok(SyncReport { outcome, server_message: out.server_message, media_synced })
            }
            SyncOp::FullDownload | SyncOp::FullUpload => {
                let auth = Engine::auth_for(creds)?;
                let col = self.col.take().ok_or(Error::Busy)?;
                let res = if op == SyncOp::FullDownload {
                    self.handle.block_on(col.full_download(auth, self.client.clone()))
                } else {
                    self.handle.block_on(col.full_upload(auth, self.client.clone()))
                };
                self.reopen()?;
                res?;
                let media_synced = opts.media && {
                    self.media(creds)?;
                    true
                };
                let outcome =
                    if op == SyncOp::FullDownload { SyncOutcome::FullDownloaded } else { SyncOutcome::FullUploaded };
                Ok(SyncReport { outcome, server_message: String::new(), media_synced })
            }
            SyncOp::MediaOnly => {
                self.media(creds)?;
                Ok(SyncReport { outcome: SyncOutcome::NoChanges, server_message: String::new(), media_synced: true })
            }
        }
    }
}

type SyncResult = (Result<SyncReport>, Credentials, Option<Collection>);

pub struct SyncHandle {
    join: Option<JoinHandle<SyncResult>>,
}

impl SyncHandle {
    pub fn is_finished(&self) -> bool {
        self.join.as_ref().map(|j| j.is_finished()).unwrap_or(true)
    }

    fn join(mut self) -> Result<SyncResult> {
        let j = self.join.take().ok_or_else(|| anyhow::anyhow!("sync handle already joined"))?;
        j.join().map_err(|_| anyhow::anyhow!("sync thread panicked").into())
    }
}
