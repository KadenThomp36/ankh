use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("not logged in — run `ankh login`")]
    NotLoggedIn,

    #[error("a full sync is required (upload {}, download {})",
        if *.upload_ok { "possible" } else { "not possible" },
        if *.download_ok { "possible" } else { "not possible" })]
    FullSyncRequired { upload_ok: bool, download_ok: bool },

    #[error("the collection is busy (a sync is running)")]
    Busy,

    #[error("keyring: {0}")]
    Keyring(#[from] keyring::Error),

    #[error("anki: {0}")]
    Anki(#[from] anki::error::AnkiError),

    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    #[error("{0}")]
    Other(#[from] anyhow::Error),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

impl Error {
    /// Semantic process exit code for the CLI contract (see docs/cli.md).
    pub fn exit_code(&self) -> i32 {
        match self {
            Error::NotLoggedIn => 3,
            Error::FullSyncRequired { .. } => 4,
            Error::Busy => 5,
            Error::Anki(anki::error::AnkiError::SyncError { .. }) => 6,
            Error::Keyring(_) => 7,
            _ => 1,
        }
    }
}
