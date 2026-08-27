use std::sync::LazyLock;

use anki::error::AnkiError;
use anki::prelude::I18n;
use thiserror::Error;

/// rslib renders its sync/network messages through Fluent; we only ever want
/// the English template.
static TR: LazyLock<I18n> = LazyLock::new(I18n::template_only);

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

    /// AnkiWeb refused the sync. Rendered at the `From<AnkiError>` boundary
    /// because `AnkiError`'s own `Display` prints nothing but the variant name
    /// — a wrong password, an outdated client and a server outage all arrive
    /// as the bare string `SyncError`. The `kind` and message live one level
    /// down, in the source.
    #[error("sync: {0}")]
    AnkiSync(String),

    /// Same treatment for the network: `NetworkError` has no `Display` of its
    /// own either.
    #[error("network: {0}")]
    AnkiNetwork(String),

    #[error("anki: {0}")]
    Anki(AnkiError),

    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    #[error("{0}")]
    Other(#[from] anyhow::Error),
}

impl From<AnkiError> for Error {
    fn from(e: AnkiError) -> Self {
        match e {
            AnkiError::SyncError { source } => Error::AnkiSync(format!("{:?}: {}", source.kind, source.message(&TR))),
            AnkiError::NetworkError { source } => {
                Error::AnkiNetwork(format!("{:?}: {}", source.kind, source.message(&TR)))
            }
            other => Error::Anki(other),
        }
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

impl Error {
    /// Semantic process exit code for the CLI contract (see docs/cli.md).
    pub fn exit_code(&self) -> i32 {
        match self {
            Error::NotLoggedIn => 3,
            Error::FullSyncRequired { .. } => 4,
            Error::Busy => 5,
            Error::AnkiSync(_) | Error::AnkiNetwork(_) => 6,
            Error::Keyring(_) => 7,
            _ => 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use anki::error::{NetworkError, NetworkErrorKind, SyncError, SyncErrorKind};

    use super::*;

    /// The bug this guards against: `ankh login` with a bad password printed
    /// `error: anki: SyncError` and nothing else.
    #[test]
    fn sync_errors_keep_their_kind_and_message() {
        let e: Error =
            AnkiError::SyncError { source: SyncError { info: String::new(), kind: SyncErrorKind::AuthFailed } }.into();
        let msg = e.to_string();
        assert!(msg.contains("AuthFailed"), "{msg}");
        assert!(msg.contains("password"), "{msg}");
        assert_eq!(e.exit_code(), 6);
    }

    /// `info` carries the detail for the kinds that have no canned message.
    #[test]
    fn server_messages_survive() {
        let e: Error = AnkiError::SyncError {
            source: SyncError { info: "backup in progress".into(), kind: SyncErrorKind::ServerMessage },
        }
        .into();
        assert!(e.to_string().contains("backup in progress"), "{e}");
    }

    /// docs/cli.md promises 6 for "sync/network error"; network used to fall
    /// through to the generic 1.
    #[test]
    fn network_errors_are_legible_and_share_the_sync_exit_code() {
        let e: Error =
            AnkiError::NetworkError { source: NetworkError { info: String::new(), kind: NetworkErrorKind::Offline } }
                .into();
        assert!(e.to_string().contains("Offline"), "{e}");
        assert_eq!(e.exit_code(), 6);
    }

    /// Everything else still goes through the catch-all.
    #[test]
    fn other_anki_errors_are_unchanged() {
        let e: Error = AnkiError::CollectionNotOpen.into();
        assert_eq!(e.exit_code(), 1);
        assert!(e.to_string().starts_with("anki: "), "{e}");
    }
}
