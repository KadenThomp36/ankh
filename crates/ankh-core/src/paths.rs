use std::path::{Path, PathBuf};

use directories::ProjectDirs;

/// Where a profile keeps its files. Everything derives from three roots so the
/// whole app can be relocated with `--collection`, `$XDG_*`, or env overrides.
#[derive(Debug, Clone)]
pub struct Paths {
    pub profile: String,
    /// Directory holding `collection.anki2`, `collection.media/`, `collection.mdb`.
    pub data_dir: PathBuf,
    /// Directory holding `init.lua`.
    pub config_dir: PathBuf,
    /// Logs, caches.
    pub state_dir: PathBuf,
}

pub const DEFAULT_PROFILE: &str = "default";

impl Paths {
    /// Resolve paths for a profile. `collection_override` points the data dir
    /// at the parent of an arbitrary `collection.anki2` (e.g. a desktop Anki
    /// profile) without touching config/state locations.
    pub fn resolve(profile: Option<&str>, collection_override: Option<&Path>) -> Self {
        let profile = profile.unwrap_or(DEFAULT_PROFILE).to_string();
        let dirs = ProjectDirs::from("", "", "ankh").expect("a home directory");
        let data_dir = match collection_override {
            Some(p) => p.parent().map(Path::to_path_buf).unwrap_or_else(|| p.to_path_buf()),
            None => dirs.data_dir().join(&profile),
        };
        Paths {
            data_dir,
            config_dir: dirs.config_dir().to_path_buf(),
            state_dir: dirs.state_dir().unwrap_or(dirs.data_dir()).join(&profile),
            profile,
        }
    }

    pub fn collection(&self) -> PathBuf {
        self.data_dir.join("collection.anki2")
    }
    pub fn media_folder(&self) -> PathBuf {
        self.data_dir.join("collection.media")
    }
    pub fn media_db(&self) -> PathBuf {
        self.data_dir.join("collection.mdb")
    }
    pub fn init_lua(&self) -> PathBuf {
        self.config_dir.join("init.lua")
    }
    pub fn log_file(&self) -> PathBuf {
        self.state_dir.join("ankh.log")
    }

    pub fn ensure_dirs(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.data_dir)?;
        std::fs::create_dir_all(&self.state_dir)?;
        Ok(())
    }
}
