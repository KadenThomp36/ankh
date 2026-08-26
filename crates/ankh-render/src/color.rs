use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub struct Color(pub u8, pub u8, pub u8);

impl Color {
    /// Parse a CSS colour: `#rgb`, `#rrggbb`, `rgb(r, g, b)`, `rgba(...)`,
    /// or a named colour from the CSS list.
    pub fn parse(s: &str) -> Option<Color> {
        let s = s.trim().to_ascii_lowercase();
        if let Some(hex) = s.strip_prefix('#') {
            return match hex.len() {
                3 | 4 => {
                    let d = |i: usize| u8::from_str_radix(&hex[i..i + 1], 16).ok().map(|v| v * 17);
                    Some(Color(d(0)?, d(1)?, d(2)?))
                }
                6 | 8 => {
                    let d = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
                    Some(Color(d(0)?, d(2)?, d(4)?))
                }
                _ => None,
            };
        }
        if let Some(inner) = s.strip_prefix("rgb(").or_else(|| s.strip_prefix("rgba(")) {
            let inner = inner.trim_end_matches(')');
            let parts: Vec<&str> = inner.split([',', ' ', '/']).filter(|p| !p.is_empty()).collect();
            if parts.len() >= 3 {
                let n = |p: &str| -> Option<u8> {
                    if let Some(pct) = p.strip_suffix('%') {
                        pct.parse::<f32>().ok().map(|v| (v * 2.55).round().clamp(0.0, 255.0) as u8)
                    } else {
                        p.parse::<f32>().ok().map(|v| v.round().clamp(0.0, 255.0) as u8)
                    }
                };
                return Some(Color(n(parts[0])?, n(parts[1])?, n(parts[2])?));
            }
            return None;
        }
        named(&s)
    }
}

fn named(s: &str) -> Option<Color> {
    let c = match s {
        "black" => (0, 0, 0),
        "white" => (255, 255, 255),
        "red" => (255, 0, 0),
        "green" => (0, 128, 0),
        "lime" => (0, 255, 0),
        "blue" => (0, 0, 255),
        "yellow" => (255, 255, 0),
        "orange" => (255, 165, 0),
        "purple" => (128, 0, 128),
        "violet" => (238, 130, 238),
        "pink" => (255, 192, 203),
        "hotpink" => (255, 105, 180),
        "magenta" | "fuchsia" => (255, 0, 255),
        "cyan" | "aqua" => (0, 255, 255),
        "teal" => (0, 128, 128),
        "navy" => (0, 0, 128),
        "maroon" => (128, 0, 0),
        "olive" => (128, 128, 0),
        "gray" | "grey" => (128, 128, 128),
        "silver" => (192, 192, 192),
        "lightgray" | "lightgrey" => (211, 211, 211),
        "darkgray" | "darkgrey" => (169, 169, 169),
        "dimgray" | "dimgrey" => (105, 105, 105),
        "brown" => (165, 42, 42),
        "gold" => (255, 215, 0),
        "coral" => (255, 127, 80),
        "salmon" => (250, 128, 114),
        "tomato" => (255, 99, 71),
        "crimson" => (220, 20, 60),
        "firebrick" => (178, 34, 34),
        "darkred" => (139, 0, 0),
        "indianred" => (205, 92, 92),
        "orangered" => (255, 69, 0),
        "darkorange" => (255, 140, 0),
        "khaki" => (240, 230, 140),
        "lightgreen" => (144, 238, 144),
        "darkgreen" => (0, 100, 0),
        "forestgreen" => (34, 139, 34),
        "seagreen" => (46, 139, 87),
        "limegreen" => (50, 205, 50),
        "springgreen" => (0, 255, 127),
        "yellowgreen" => (154, 205, 50),
        "greenyellow" => (173, 255, 47),
        "turquoise" => (64, 224, 208),
        "aquamarine" => (127, 255, 212),
        "skyblue" => (135, 206, 235),
        "lightblue" => (173, 216, 230),
        "deepskyblue" => (0, 191, 255),
        "dodgerblue" => (30, 144, 255),
        "royalblue" => (65, 105, 225),
        "steelblue" => (70, 130, 180),
        "cornflowerblue" => (100, 149, 237),
        "slateblue" => (106, 90, 205),
        "mediumblue" => (0, 0, 205),
        "darkblue" => (0, 0, 139),
        "midnightblue" => (25, 25, 112),
        "indigo" => (75, 0, 130),
        "darkviolet" => (148, 0, 211),
        "blueviolet" => (138, 43, 226),
        "mediumpurple" => (147, 112, 219),
        "orchid" => (218, 112, 214),
        "plum" => (221, 160, 221),
        "lavender" => (230, 230, 250),
        "beige" => (245, 245, 220),
        "tan" => (210, 180, 140),
        "wheat" => (245, 222, 179),
        "chocolate" => (210, 105, 30),
        "sienna" => (160, 82, 45),
        "peru" => (205, 133, 63),
        "ivory" => (255, 255, 240),
        "transparent" | "inherit" | "initial" | "currentcolor" => return None,
        _ => return None,
    };
    Some(Color(c.0, c.1, c.2))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_css_colours() {
        assert_eq!(Color::parse("#fff"), Some(Color(255, 255, 255)));
        assert_eq!(Color::parse("#ad7abe"), Some(Color(0xad, 0x7a, 0xbe)));
        assert_eq!(Color::parse("rgb(173,122,190)"), Some(Color(173, 122, 190)));
        assert_eq!(Color::parse("rgb(173, 122, 190)"), Some(Color(173, 122, 190)));
        assert_eq!(Color::parse("rgba(0 0 0 / 50%)"), Some(Color(0, 0, 0)));
        assert_eq!(Color::parse("Red"), Some(Color(255, 0, 0)));
        assert_eq!(Color::parse("inherit"), None);
        assert_eq!(Color::parse("#12"), None);
    }
}
