//! Key notation and multi-key sequence matching, neovim style.
//!
//! Sequences are written the way neovim writes them: `j`, `gg`, `<C-d>`,
//! `<S-Tab>`, `<CR>`, `<Space>`, `<leader>s`. A [`Keymap`] holds bindings
//! for one mode of one view, and answers "exact match / keep waiting /
//! nothing" for the keys pressed so far.

use std::collections::HashMap;
use std::fmt;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Key {
    pub code: KeyCode,
    pub mods: KeyModifiers,
}

impl Key {
    pub fn from_event(ev: KeyEvent) -> Self {
        let mut mods = ev.modifiers;
        // Shift is implied by an uppercase char; drop it so `J` == `<S-j>`.
        if let KeyCode::Char(c) = ev.code {
            if c.is_uppercase() || !c.is_alphabetic() {
                mods.remove(KeyModifiers::SHIFT);
            }
        }
        Key { code: ev.code, mods }
    }

    pub fn char(c: char) -> Self {
        Key { code: KeyCode::Char(c), mods: KeyModifiers::NONE }
    }
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self.code {
            KeyCode::Char(' ') => "Space".to_string(),
            KeyCode::Char('<') => "lt".to_string(),
            KeyCode::Char(c) => c.to_string(),
            KeyCode::Enter => "CR".into(),
            KeyCode::Esc => "Esc".into(),
            KeyCode::Tab => "Tab".into(),
            KeyCode::BackTab => "S-Tab".into(),
            KeyCode::Backspace => "BS".into(),
            KeyCode::Delete => "Del".into(),
            KeyCode::Up => "Up".into(),
            KeyCode::Down => "Down".into(),
            KeyCode::Left => "Left".into(),
            KeyCode::Right => "Right".into(),
            KeyCode::Home => "Home".into(),
            KeyCode::End => "End".into(),
            KeyCode::PageUp => "PageUp".into(),
            KeyCode::PageDown => "PageDown".into(),
            KeyCode::F(n) => format!("F{n}"),
            other => format!("{other:?}"),
        };
        let mut prefix = String::new();
        if self.mods.contains(KeyModifiers::CONTROL) {
            prefix.push_str("C-");
        }
        if self.mods.contains(KeyModifiers::ALT) {
            prefix.push_str("M-");
        }
        if self.mods.contains(KeyModifiers::SHIFT) && !matches!(self.code, KeyCode::Char(_)) {
            prefix.push_str("S-");
        }
        let plain_char = matches!(self.code, KeyCode::Char(c) if c != ' ' && c != '<');
        if prefix.is_empty() && plain_char {
            write!(f, "{name}")
        } else {
            write!(f, "<{prefix}{name}>")
        }
    }
}

pub const LEADER: Key = Key { code: KeyCode::Char(' '), mods: KeyModifiers::NONE };

/// Parse neovim key notation into a sequence.
pub fn parse_seq(s: &str) -> Result<Vec<Key>, String> {
    let mut out = Vec::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '<' {
            let mut name = String::new();
            let mut closed = false;
            for n in chars.by_ref() {
                if n == '>' {
                    closed = true;
                    break;
                }
                name.push(n);
            }
            if !closed {
                return Err(format!("unterminated <...> in {s:?}"));
            }
            out.push(parse_special(&name)?);
        } else {
            out.push(Key::char(c));
        }
    }
    Ok(out)
}

fn parse_special(name: &str) -> Result<Key, String> {
    let mut mods = KeyModifiers::NONE;
    let mut rest = name;
    loop {
        let lower = rest.to_ascii_lowercase();
        if lower.starts_with("c-") {
            mods |= KeyModifiers::CONTROL;
            rest = &rest[2..];
        } else if lower.starts_with("m-") || lower.starts_with("a-") {
            mods |= KeyModifiers::ALT;
            rest = &rest[2..];
        } else if lower.starts_with("s-") {
            mods |= KeyModifiers::SHIFT;
            rest = &rest[2..];
        } else {
            break;
        }
    }
    let code = match rest.to_ascii_lowercase().as_str() {
        "leader" => return Ok(LEADER),
        "cr" | "enter" | "return" => KeyCode::Enter,
        "esc" => KeyCode::Esc,
        "tab" => {
            if mods.contains(KeyModifiers::SHIFT) {
                mods.remove(KeyModifiers::SHIFT);
                KeyCode::BackTab
            } else {
                KeyCode::Tab
            }
        }
        "space" => KeyCode::Char(' '),
        "lt" => KeyCode::Char('<'),
        "bs" | "backspace" => KeyCode::Backspace,
        "del" | "delete" => KeyCode::Delete,
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "pageup" => KeyCode::PageUp,
        "pagedown" => KeyCode::PageDown,
        f if f.starts_with('f') && f[1..].parse::<u8>().is_ok() => KeyCode::F(f[1..].parse().unwrap()),
        other if other.chars().count() == 1 => KeyCode::Char(rest.chars().next().unwrap()),
        _ => return Err(format!("unknown key name <{name}>")),
    };
    Ok(Key { code, mods })
}

pub fn format_seq(seq: &[Key]) -> String {
    seq.iter().map(|k| k.to_string()).collect()
}

#[derive(Debug, Clone)]
pub struct Binding<A> {
    pub seq: Vec<Key>,
    pub action: A,
    pub desc: String,
}

/// Bindings for one (view, mode).
#[derive(Debug, Clone)]
pub struct Keymap<A: Clone> {
    bindings: HashMap<Vec<Key>, Binding<A>>,
}

#[derive(Debug)]
pub enum Match<'a, A> {
    Exact(&'a Binding<A>),
    /// The keys so far are a prefix of one or more bindings; the list is what
    /// could come next (for which-key).
    Prefix(Vec<(String, &'a str)>),
    None,
}

impl<A: Clone> Default for Keymap<A> {
    fn default() -> Self {
        Self { bindings: HashMap::new() }
    }
}

impl<A: Clone> Keymap<A> {
    pub fn bind(&mut self, seq: &str, action: A, desc: impl Into<String>) -> &mut Self {
        let seq = parse_seq(seq).expect("valid default key sequence");
        self.bindings.insert(seq.clone(), Binding { seq, action, desc: desc.into() });
        self
    }

    /// Used by the Lua layer (milestone 6) to remove defaults.
    #[allow(dead_code)]
    pub fn unbind(&mut self, seq: &[Key]) {
        self.bindings.remove(seq);
    }

    pub fn lookup(&self, pending: &[Key]) -> Match<'_, A> {
        if let Some(b) = self.bindings.get(pending) {
            // An exact binding that is also a prefix of longer ones wins immediately
            // (like neovim with timeoutlen=0 for ambiguity we don't want).
            return Match::Exact(b);
        }
        let mut next: Vec<(String, &str)> = Vec::new();
        for (seq, b) in &self.bindings {
            if seq.len() > pending.len() && seq.starts_with(pending) {
                let label = format_seq(&seq[pending.len()..]);
                next.push((label, b.desc.as_str()));
            }
        }
        if next.is_empty() {
            Match::None
        } else {
            next.sort();
            Match::Prefix(next)
        }
    }

    /// All bindings, sorted by their notation for stable display.
    pub fn bindings(&self) -> Vec<&Binding<A>> {
        let mut v: Vec<&Binding<A>> = self.bindings.values().collect();
        v.sort_by_key(|b| format_seq(&b.seq));
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_notation() {
        assert_eq!(parse_seq("gg").unwrap(), vec![Key::char('g'), Key::char('g')]);
        assert_eq!(parse_seq("<C-d>").unwrap(), vec![Key { code: KeyCode::Char('d'), mods: KeyModifiers::CONTROL }]);
        assert_eq!(parse_seq("<leader>s").unwrap(), vec![LEADER, Key::char('s')]);
        assert_eq!(parse_seq("<S-Tab>").unwrap(), vec![Key { code: KeyCode::BackTab, mods: KeyModifiers::NONE }]);
        assert!(parse_seq("<C-").is_err());
    }

    #[test]
    fn round_trips_display() {
        for s in ["gg", "<C-d>", "<Space>x", "<CR>", "<M-j>", "<F5>", "J"] {
            let seq = parse_seq(s).unwrap();
            assert_eq!(format_seq(&seq), s);
        }
    }

    #[test]
    fn prefix_matching() {
        let mut km: Keymap<u8> = Keymap::default();
        km.bind("gg", 1, "top").bind("G", 2, "bottom").bind("<leader>sd", 3, "sync down");
        assert!(matches!(km.lookup(&parse_seq("g").unwrap()), Match::Prefix(_)));
        assert!(matches!(km.lookup(&parse_seq("gg").unwrap()), Match::Exact(b) if b.action == 1));
        assert!(matches!(km.lookup(&parse_seq("x").unwrap()), Match::None));
        if let Match::Prefix(next) = km.lookup(&parse_seq("<leader>").unwrap()) {
            assert_eq!(next, vec![("sd".to_string(), "sync down")]);
        } else {
            panic!()
        }
    }
}
