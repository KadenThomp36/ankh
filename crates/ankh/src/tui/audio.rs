//! Card audio via an external player. `mpv` first, then `ffplay`; nothing
//! blocks the UI and a new card stops whatever was playing.

use std::path::PathBuf;
use std::process::{Child, Command, Stdio};

pub struct Player {
    child: Option<Child>,
    backend: Option<Backend>,
    pub enabled: bool,
}

#[derive(Clone, Copy)]
enum Backend {
    Mpv,
    Ffplay,
}

impl Player {
    pub fn new() -> Self {
        let backend = if which("mpv") {
            Some(Backend::Mpv)
        } else if which("ffplay") {
            Some(Backend::Ffplay)
        } else {
            None
        };
        Player { child: None, backend, enabled: true }
    }

    pub fn available(&self) -> bool {
        self.backend.is_some()
    }

    pub fn stop(&mut self) {
        if let Some(mut c) = self.child.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }

    /// Play files back to back. Missing files are skipped silently.
    pub fn play(&mut self, files: &[PathBuf]) {
        self.stop();
        if !self.enabled {
            return;
        }
        let files: Vec<&PathBuf> = files.iter().filter(|f| f.exists()).collect();
        let Some(backend) = self.backend else { return };
        if files.is_empty() {
            return;
        }
        let mut cmd = match backend {
            Backend::Mpv => {
                let mut c = Command::new("mpv");
                c.args(["--no-video", "--really-quiet", "--no-terminal", "--keep-open=no"]);
                c.args(files.iter().map(|f| f.as_os_str()));
                c
            }
            Backend::Ffplay => {
                // ffplay takes one file; play the first only.
                let mut c = Command::new("ffplay");
                c.args(["-nodisp", "-autoexit", "-loglevel", "quiet"]).arg(files[0]);
                c
            }
        };
        cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
        self.child = cmd.spawn().ok();
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        self.stop();
    }
}

fn which(bin: &str) -> bool {
    std::env::var_os("PATH").map(|p| std::env::split_paths(&p).any(|d| d.join(bin).is_file())).unwrap_or(false)
}
