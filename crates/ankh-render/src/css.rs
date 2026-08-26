//! Just enough CSS: inline `style=""` declarations we care about.

use crate::color::Color;
use crate::{Align, Style};

#[derive(Debug, Default, Clone, Copy)]
pub struct Decl {
    pub hidden: bool,
    pub align: Align,
    pub style: Style,
    /// Whether the declaration explicitly reset bold/italic (`font-weight: normal`).
    pub unbold: bool,
    pub unitalic: bool,
}

pub fn parse_inline(style_attr: &str) -> Decl {
    let mut d = Decl::default();
    for decl in style_attr.split(';') {
        let Some((k, v)) = decl.split_once(':') else { continue };
        let k = k.trim().to_ascii_lowercase();
        let v = v.trim().trim_end_matches("!important").trim();
        match k.as_str() {
            "color" => d.style.fg = Color::parse(v),
            "background" | "background-color" => d.style.bg = Color::parse(v.split_whitespace().next().unwrap_or("")),
            "font-weight" => match v.to_ascii_lowercase().as_str() {
                "bold" | "bolder" => d.style.bold = true,
                "normal" | "lighter" => d.unbold = true,
                n => {
                    if let Ok(w) = n.parse::<u16>() {
                        if w >= 600 {
                            d.style.bold = true
                        } else {
                            d.unbold = true
                        }
                    }
                }
            },
            "font-style" => match v.to_ascii_lowercase().as_str() {
                "italic" | "oblique" => d.style.italic = true,
                "normal" => d.unitalic = true,
                _ => {}
            },
            "text-decoration" | "text-decoration-line" => {
                let v = v.to_ascii_lowercase();
                if v.contains("underline") {
                    d.style.underline = true;
                }
                if v.contains("line-through") {
                    d.style.strike = true;
                }
            }
            "text-align" => {
                d.align = match v.to_ascii_lowercase().as_str() {
                    "center" => Align::Center,
                    "right" | "end" => Align::Right,
                    "left" | "start" => Align::Left,
                    _ => Align::Inherit,
                }
            }
            "display" => {
                if v.eq_ignore_ascii_case("none") {
                    d.hidden = true;
                }
            }
            "visibility" => {
                if v.eq_ignore_ascii_case("hidden") {
                    d.hidden = true;
                }
            }
            "opacity" if v.parse::<f32>().map(|o| o < 0.5).unwrap_or(false) => d.style.dim = true,
            _ => {}
        }
    }
    d
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_inline_subset() {
        let d = parse_inline("font-family: Batang; color:rgb(173,122,190); font-size:26px; font-weight: bold");
        assert_eq!(d.style.fg, Some(Color(173, 122, 190)));
        assert!(d.style.bold);
        assert!(!d.hidden);
        let d = parse_inline("display: none");
        assert!(d.hidden);
        let d = parse_inline("text-align:center;text-decoration: underline line-through");
        assert_eq!(d.align, Align::Center);
        assert!(d.style.underline && d.style.strike);
    }
}
