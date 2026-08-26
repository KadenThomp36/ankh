//! Sync credentials. We never persist the AnkiWeb password: `login`
//! exchanges it for a host key (`hkey`) and only that key is stored, in the
//! OS keyring under service `ankh` / user `<profile>`.
//!
//! `ANKH_SYNC_KEY` (+ optional `ANKH_SYNC_ENDPOINT`, `ANKH_SYNC_USER`) override
//! the keyring for CI and headless use.

use serde::{Deserialize, Serialize};

use crate::{Error, Result};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Credentials {
    pub username: String,
    pub hkey: String,
    /// AnkiWeb shards users onto `syncN.ankiweb.net`; the first sync tells us
    /// which, and we must keep using it.
    #[serde(default)]
    pub endpoint: Option<String>,
}

const SERVICE: &str = "ankh";

/// Storage for [`Credentials`], one entry per profile.
pub struct AuthStore {
    profile: String,
}

impl AuthStore {
    pub fn new(profile: impl Into<String>) -> Self {
        Self { profile: profile.into() }
    }

    fn entry(&self) -> Result<keyring::Entry> {
        Ok(keyring::Entry::new(SERVICE, &self.profile)?)
    }

    pub fn load(&self) -> Result<Option<Credentials>> {
        if let Ok(hkey) = std::env::var("ANKH_SYNC_KEY") {
            return Ok(Some(Credentials {
                username: std::env::var("ANKH_SYNC_USER").unwrap_or_default(),
                hkey,
                endpoint: std::env::var("ANKH_SYNC_ENDPOINT").ok(),
            }));
        }
        match self.entry()?.get_password() {
            Ok(json) => Ok(serde_json::from_str(&json).ok()),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn require(&self) -> Result<Credentials> {
        self.load()?.ok_or(Error::NotLoggedIn)
    }

    pub fn save(&self, creds: &Credentials) -> Result<()> {
        let json = serde_json::to_string(creds).expect("credentials serialize");
        self.entry()?.set_password(&json)?;
        Ok(())
    }

    pub fn clear(&self) -> Result<()> {
        match self.entry()?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
}
