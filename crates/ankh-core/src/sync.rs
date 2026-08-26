//! Sync policy lives here; the wire protocol lives in `rslib`.

use anki::sync::collection::normal::SyncActionRequired;
use serde::Serialize;

#[derive(Debug, Clone, Copy, Default)]
pub struct SyncOptions {
    /// Also sync media after a successful collection sync.
    pub media: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum SyncOutcome {
    NoChanges,
    Synced,
    /// The schemas diverged; the caller must pick a direction.
    FullSyncRequired {
        upload_ok: bool,
        download_ok: bool,
    },
    FullDownloaded,
    FullUploaded,
}

#[derive(Debug, Clone, Serialize)]
pub struct SyncReport {
    pub outcome: SyncOutcome,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub server_message: String,
    pub media_synced: bool,
}

/// A UI-friendly snapshot of what a running sync is doing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncProgress {
    Idle,
    Connecting,
    Collection { stage: &'static str, added: usize, removed: usize },
    Full { transferred: usize, total: usize },
    Media { checked: usize, downloaded: usize, uploaded: usize },
}

impl SyncProgress {
    /// Build from the `Debug` rendering of rslib's `Option<Progress>`.
    ///
    /// rslib keeps its `progress` module private, so we cannot name the type
    /// and pattern-match on it; the shape is stable enough (`Variant(Struct {
    /// field: value, .. })`) to parse. See docs/adr/0001-rslib.md.
    pub fn parse_debug(s: &str) -> Self {
        let s = s.trim();
        let Some(inner) = s.strip_prefix("Some(") else { return SyncProgress::Idle };
        let variant = inner.split('(').next().unwrap_or("").trim();
        let field = |name: &str| -> usize {
            let key = format!("{name}: ");
            inner
                .find(&key)
                .and_then(|i| inner[i + key.len()..].split(|c: char| !c.is_ascii_digit()).next())
                .and_then(|v| v.parse().ok())
                .unwrap_or(0)
        };
        let word = |name: &str| -> String {
            let key = format!("{name}: ");
            inner
                .find(&key)
                .map(|i| inner[i + key.len()..].chars().take_while(|c| c.is_alphanumeric()).collect())
                .unwrap_or_default()
        };
        match variant {
            "NormalSync" => SyncProgress::Collection {
                stage: match word("stage").as_str() {
                    "Connecting" => "connecting",
                    "Finalizing" => "finalizing",
                    _ => "syncing",
                },
                added: field("local_update") + field("remote_update"),
                removed: field("local_remove") + field("remote_remove"),
            },
            "FullSync" => SyncProgress::Full { transferred: field("transferred_bytes"), total: field("total_bytes") },
            "MediaSync" => SyncProgress::Media {
                checked: field("checked"),
                downloaded: field("downloaded_files"),
                uploaded: field("uploaded_files"),
            },
            _ => SyncProgress::Connecting,
        }
    }
}

impl From<SyncActionRequired> for SyncOutcome {
    fn from(r: SyncActionRequired) -> Self {
        match r {
            SyncActionRequired::NoChanges => SyncOutcome::NoChanges,
            SyncActionRequired::NormalSyncRequired => SyncOutcome::Synced,
            SyncActionRequired::FullSyncRequired { upload_ok, download_ok } => {
                SyncOutcome::FullSyncRequired { upload_ok, download_ok }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_rslib_debug_output() {
        assert_eq!(SyncProgress::parse_debug("None"), SyncProgress::Idle);
        assert_eq!(
            SyncProgress::parse_debug("Some(MediaSync(MediaSyncProgress { checked: 12, downloaded_files: 3, downloaded_deletions: 0, uploaded_files: 1, uploaded_deletions: 0 }))"),
            SyncProgress::Media { checked: 12, downloaded: 3, uploaded: 1 }
        );
        assert_eq!(
            SyncProgress::parse_debug("Some(NormalSync(NormalSyncProgress { stage: Finalizing, local_update: 2, local_remove: 0, remote_update: 5, remote_remove: 1 }))"),
            SyncProgress::Collection { stage: "finalizing", added: 7, removed: 1 }
        );
        assert_eq!(
            SyncProgress::parse_debug("Some(FullSync(FullSyncProgress { transferred_bytes: 10, total_bytes: 100 }))"),
            SyncProgress::Full { transferred: 10, total: 100 }
        );
    }
}
