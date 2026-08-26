//! Inline images: Kitty → Sixel → iTerm2 → half-block fallback, via
//! `ratatui-image`. Decoding and protocol encoding are cached per
//! (path, cell size), so redraws are cheap.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use image::DynamicImage;
use ratatui::layout::{Rect, Size};
use ratatui_image::picker::{Picker, ProtocolType};
use ratatui_image::protocol::Protocol;
use ratatui_image::{FontSize, Image, Resize};

pub struct Images {
    picker: Option<Picker>,
    decoded: HashMap<PathBuf, Option<DynamicImage>>,
    encoded: HashMap<(PathBuf, u16, u16), Option<Protocol>>,
}

impl Images {
    /// Pick a protocol from the environment. We deliberately do *not* query
    /// the terminal with escape sequences by default: a terminal that never
    /// answers leaves a reader on stdin that eats every later keystroke.
    /// `ANKH_QUERY_TERMINAL=1` opts into the query for unusual setups.
    pub fn detect() -> Self {
        if std::env::var_os("ANKH_NO_IMAGES").is_some() {
            return Images { picker: None, decoded: HashMap::new(), encoded: HashMap::new() };
        }
        let picker =
            if std::env::var_os("ANKH_QUERY_TERMINAL").is_some() { Picker::from_query_stdio().ok() } else { None }
                .unwrap_or_else(|| {
                    #[allow(deprecated)] // the query alternative can hang; see above
                    let mut p =
                        Picker::from_fontsize(font_size_from_ioctl().unwrap_or(FontSize { width: 8, height: 16 }));
                    p.set_protocol_type(protocol_from_env());
                    p
                });
        Images { picker: Some(picker), decoded: HashMap::new(), encoded: HashMap::new() }
    }

    pub fn protocol_name(&self) -> &'static str {
        match self.picker.as_ref().map(|p| p.protocol_type()) {
            Some(ProtocolType::Kitty) => "kitty",
            Some(ProtocolType::Sixel) => "sixel",
            Some(ProtocolType::Iterm2) => "iterm2",
            Some(ProtocolType::Halfblocks) => "halfblocks",
            None => "off",
        }
    }

    fn decode(&mut self, path: &Path) -> Option<&DynamicImage> {
        if !self.decoded.contains_key(path) {
            let img = image::ImageReader::open(path)
                .ok()
                .and_then(|r| r.with_guessed_format().ok())
                .and_then(|r| r.decode().ok());
            self.decoded.insert(path.to_path_buf(), img);
        }
        self.decoded.get(path).and_then(|o| o.as_ref())
    }

    /// Cell size an image will occupy when fitted into `max`, keeping aspect.
    pub fn size_for(&mut self, path: &Path, max: Size) -> Option<Size> {
        let picker = self.picker.as_ref()?;
        let font = picker.font_size();
        let img = self.decode(path)?;
        let natural =
            Size::new(img.width().div_ceil(font.width as u32) as u16, img.height().div_ceil(font.height as u32) as u16);
        let mut w = natural.width.max(1);
        let mut h = natural.height.max(1);
        if w > max.width {
            h = ((h as f32) * (max.width as f32 / w as f32)).ceil() as u16;
            w = max.width;
        }
        if h > max.height {
            w = ((w as f32) * (max.height as f32 / h as f32)).ceil() as u16;
            h = max.height;
        }
        Some(Size::new(w.max(1), h.max(1)))
    }

    pub fn draw(&mut self, f: &mut ratatui::Frame, path: &Path, area: Rect) {
        let Some(picker) = self.picker.as_ref() else { return };
        let key = (path.to_path_buf(), area.width, area.height);
        if !self.encoded.contains_key(&key) {
            let proto = {
                let picker = picker.clone();
                self.decode(path).cloned().and_then(|img| {
                    picker.new_protocol(img, Size::new(area.width, area.height), Resize::Fit(None)).ok()
                })
            };
            self.encoded.insert(key.clone(), proto);
        }
        if let Some(Some(p)) = self.encoded.get(&key) {
            f.render_widget(Image::new(p), area);
        }
    }
}

fn env(k: &str) -> String {
    std::env::var(k).unwrap_or_default()
}

fn protocol_from_env() -> ProtocolType {
    let term = env("TERM");
    let program = env("TERM_PROGRAM");
    let in_tmux = term.starts_with("tmux") || term.starts_with("screen") || program == "tmux";
    if in_tmux {
        // Passthrough is unreliable; half-blocks always work.
        return ProtocolType::Halfblocks;
    }
    if !env("KITTY_WINDOW_ID").is_empty() || term.contains("kitty") || term.contains("ghostty") || program == "ghostty"
    {
        return ProtocolType::Kitty;
    }
    if !env("WEZTERM_EXECUTABLE").is_empty() || program == "WezTerm" {
        return ProtocolType::Kitty;
    }
    if !env("ITERM_SESSION_ID").is_empty()
        || program == "iTerm.app"
        || !env("LC_TERMINAL").is_empty() && env("LC_TERMINAL") == "iTerm2"
    {
        return ProtocolType::Iterm2;
    }
    if !env("KONSOLE_VERSION").is_empty() || term.contains("foot") || term.contains("mlterm") || program == "Konsole" {
        return ProtocolType::Sixel;
    }
    ProtocolType::Halfblocks
}

/// Cell size in pixels from the tty, when the terminal reports it.
fn font_size_from_ioctl() -> Option<FontSize> {
    let mut ws: libc::winsize = unsafe { std::mem::zeroed() };
    // SAFETY: TIOCGWINSZ fills a winsize struct; stdout is a valid fd for the lifetime of the call.
    let rc = unsafe { libc::ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ, &mut ws) };
    if rc != 0 || ws.ws_col == 0 || ws.ws_row == 0 || ws.ws_xpixel == 0 || ws.ws_ypixel == 0 {
        return None;
    }
    Some(FontSize { width: ws.ws_xpixel / ws.ws_col, height: ws.ws_ypixel / ws.ws_row })
}
