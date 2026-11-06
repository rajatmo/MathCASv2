//! Colours: the colour terms (`rgb_color`, `hue`, `gray_level` and the named colours), reading them as [`Rgba`],
//! and conversion between the colour models.
use crate::num::number;
use mc_core::{Store, View, T};

/// A colour with straight (not premultiplied) alpha, every component in [0, 1].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rgba {
    /// red
    pub r: f64,
    /// green
    pub g: f64,
    /// blue
    pub b: f64,
    /// opacity
    pub a: f64,
}

impl Rgba {
    /// Black, the default colour of lines, points and text.
    pub const BLACK: Rgba = Rgba::rgb(0.0, 0.0, 0.0);
    /// White, the default background.
    pub const WHITE: Rgba = Rgba::rgb(1.0, 1.0, 1.0);

    /// An opaque colour.
    pub const fn rgb(r: f64, g: f64, b: f64) -> Rgba {
        Rgba { r, g, b, a: 1.0 }
    }

    /// The colour with every component clamped to [0, 1] (components outside, and NaN, are clipped).
    pub fn clamped(self) -> Rgba {
        let c = |v: f64| if v.is_nan() { 0.0 } else { v.clamp(0.0, 1.0) };
        Rgba { r: c(self.r), g: c(self.g), b: c(self.b), a: c(self.a) }
    }

    /// The components as bytes (red, green, blue, alpha), rounded.
    pub fn bytes(self) -> [u8; 4] {
        let c = self.clamped();
        let b = |v: f64| (v * 255.0).round() as u8;
        [b(c.r), b(c.g), b(c.b), b(c.a)]
    }

    /// `#rrggbb` (the opacity is not part of it).
    pub fn hex(self) -> String {
        let [r, g, b, _] = self.bytes();
        format!("#{r:02x}{g:02x}{b:02x}")
    }

    /// The colour `t` of the way from `self` to `other` (0: `self`, 1: `other`), opacity included.
    pub fn mix(self, other: Rgba, t: f64) -> Rgba {
        let m = |a: f64, b: f64| a + (b - a) * t;
        Rgba { r: m(self.r, other.r), g: m(self.g, other.g), b: m(self.b, other.b), a: m(self.a, other.a) }
    }

    /// The same colour with its opacity multiplied by `k`.
    pub fn faded(self, k: f64) -> Rgba {
        Rgba { a: self.a * k, ..self }
    }

    /// The luminance as a gray level (ITU-R BT.601 weights).
    pub fn gray(self) -> f64 {
        0.299 * self.r + 0.587 * self.g + 0.114 * self.b
    }

    /// Hue, saturation and brightness, each in [0, 1] (hue 0 is red, a gray has hue 0).
    pub fn hsb(self) -> (f64, f64, f64) {
        let c = self.clamped();
        let max = c.r.max(c.g).max(c.b);
        let min = c.r.min(c.g).min(c.b);
        let d = max - min;
        let s = if max > 0.0 { d / max } else { 0.0 };
        if d == 0.0 {
            return (0.0, s, max);
        }
        let h = if max == c.r {
            ((c.g - c.b) / d).rem_euclid(6.0)
        } else if max == c.g {
            (c.b - c.r) / d + 2.0
        } else {
            (c.r - c.g) / d + 4.0
        };
        (h / 6.0, s, max)
    }

    /// The colour of hue `h` (cyclic: 0 and 1 are red), saturation `s` and brightness `v`.
    pub fn from_hsb(h: f64, s: f64, v: f64) -> Rgba {
        let h = h.rem_euclid(1.0) * 6.0;
        let (s, v) = (s.clamp(0.0, 1.0), v.clamp(0.0, 1.0));
        let k = h.floor();
        let f = h - k;
        let (p, q, t) = (v * (1.0 - s), v * (1.0 - s * f), v * (1.0 - s * (1.0 - f)));
        let (r, g, b) = match k as i64 {
            0 => (v, t, p),
            1 => (q, v, p),
            2 => (p, v, t),
            3 => (p, q, v),
            4 => (t, p, v),
            _ => (v, p, q),
        };
        Rgba::rgb(r, g, b)
    }
}

/// The named colours: symbols that stand for a colour wherever one is expected.
pub const NAMED: &[(&str, Rgba)] = &[
    ("black", Rgba::rgb(0.0, 0.0, 0.0)),
    ("white", Rgba::rgb(1.0, 1.0, 1.0)),
    ("gray", Rgba::rgb(0.5, 0.5, 0.5)),
    ("light_gray", Rgba::rgb(0.85, 0.85, 0.85)),
    ("dark_gray", Rgba::rgb(0.25, 0.25, 0.25)),
    ("red", Rgba::rgb(1.0, 0.0, 0.0)),
    ("green", Rgba::rgb(0.0, 1.0, 0.0)),
    ("blue", Rgba::rgb(0.0, 0.0, 1.0)),
    ("cyan", Rgba::rgb(0.0, 1.0, 1.0)),
    ("magenta", Rgba::rgb(1.0, 0.0, 1.0)),
    ("yellow", Rgba::rgb(1.0, 1.0, 0.0)),
    ("orange", Rgba::rgb(1.0, 0.5, 0.0)),
    ("purple", Rgba::rgb(0.5, 0.0, 0.5)),
    ("brown", Rgba::rgb(0.6, 0.4, 0.2)),
    ("pink", Rgba::rgb(1.0, 0.5, 0.5)),
];

/// The colour named `name`.
pub fn named(name: &str) -> Option<Rgba> {
    NAMED.iter().find(|(n, _)| *n == name).map(|(_, c)| *c)
}

/// What a term is as a colour.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Colour {
    /// a colour with numeric components
    Value(Rgba),
    /// a colour term with a component that is not a number yet (`rgb_color(a, 0, 0)`), or a symbol that is not a
    /// colour name: it may become a colour
    Symbolic,
    /// not a colour: a colour head with the wrong arguments, a string, a number
    Bad,
}

/// The colour heads.
pub const HEADS: [&str; 3] = ["rgb_color", "hue", "gray_level"];

/// Read `t` as a colour: a named colour, `rgb_color(r, g, b)` or `rgb_color(r, g, b, a)`, `hue(h)`,
/// `hue(h, s, b)` or `hue(h, s, b, a)`, `gray_level(g)` or `gray_level(g, a)`.
pub fn colour<S: Store + ?Sized>(h: &S, t: T<'_>) -> Colour {
    match h.view(t) {
        View::Sym(s) => match named(&h.sym_name(s)) {
            Some(c) => Colour::Value(c),
            None => Colour::Symbolic,
        },
        // "#rrggbb", as plot options write colours
        View::Str(s) => match hex_color(s) {
            Some(c) => Colour::Value(c),
            None => Colour::Bad,
        },
        View::App { head, args } => {
            let View::Sym(f) = h.view(head) else { return Colour::Bad };
            let name = h.sym_name(f);
            if !HEADS.contains(&name.as_str()) {
                return Colour::Bad;
            }
            let mut v = [0.0; 4];
            let mut symbolic = false;
            for (k, &a) in args.iter().take(4).enumerate() {
                match number(h, a) {
                    Some(x) => v[k] = x,
                    None if is_symbolic(h, a) => symbolic = true,
                    None => return Colour::Bad,
                }
            }
            let shape_ok = match name.as_str() {
                "rgb_color" => matches!(args.len(), 3 | 4),
                "hue" => matches!(args.len(), 1..=4),
                _ => matches!(args.len(), 1 | 2),
            };
            if !shape_ok {
                return Colour::Bad;
            }
            if symbolic {
                return Colour::Symbolic;
            }
            let c = match (name.as_str(), args.len()) {
                ("rgb_color", n) => Rgba { r: v[0], g: v[1], b: v[2], a: if n == 4 { v[3] } else { 1.0 } },
                ("hue", 1) => Rgba::from_hsb(v[0], 1.0, 1.0),
                ("hue", 2) => Rgba::from_hsb(v[0], v[1], 1.0),
                ("hue", n) => Rgba { a: if n == 4 { v[3] } else { 1.0 }, ..Rgba::from_hsb(v[0], v[1], v[2]) },
                (_, n) => Rgba { a: if n == 2 { v[1] } else { 1.0 }, ..Rgba::rgb(v[0], v[0], v[0]) },
            };
            Colour::Value(c)
        }
        _ => Colour::Bad,
    }
}

/// Whether `t` may still become a number: a symbol or a compound term that is not a list (`a`, `x + 1`).
pub fn is_symbolic<S: Store + ?Sized>(h: &S, t: T<'_>) -> bool {
    match h.view(t) {
        View::Sym(_) => true,
        View::App { head, .. } => !matches!(h.view(head), View::Sym(s) if h.sym_name(s) == mc_api::names::LIST),
        _ => false,
    }
}

/// A colour written `#rrggbb` (hexadecimal red, green, blue).
pub fn hex_color(s: &str) -> Option<Rgba> {
    let d = s.strip_prefix('#').filter(|d| d.len() == 6 && d.is_ascii())?;
    let c = |i: usize| u8::from_str_radix(&d[i..i + 2], 16).ok().map(|v| v as f64 / 255.0);
    Some(Rgba::rgb(c(0)?, c(2)?, c(4)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hsb_round_trips() {
        for c in [Rgba::rgb(1.0, 0.0, 0.0), Rgba::rgb(0.2, 0.6, 0.4), Rgba::rgb(0.5, 0.5, 0.5), Rgba::rgb(0.1, 0.2, 0.9)] {
            let (h, s, v) = c.hsb();
            let back = Rgba::from_hsb(h, s, v);
            for (a, b) in [(c.r, back.r), (c.g, back.g), (c.b, back.b)] {
                assert!((a - b).abs() < 1e-12, "{c:?} -> {back:?}");
            }
        }
        assert_eq!(Rgba::from_hsb(1.0 / 3.0, 1.0, 1.0).bytes(), [0, 255, 0, 255]);
        assert_eq!(Rgba::from_hsb(-1.0 / 3.0, 1.0, 1.0).bytes(), [0, 0, 255, 255]);
    }

    #[test]
    fn hex_colors() {
        assert_eq!(hex_color("#560591").map(|c| c.bytes()), Some([86, 5, 145, 255]));
        assert_eq!(hex_color("#FFffff"), Some(Rgba::WHITE));
        for bad in ["560591", "#56059", "#5605911", "#56059g", "#\u{e9}0591", ""] {
            assert_eq!(hex_color(bad), None, "{bad}");
        }
        assert_eq!(Rgba::rgb(1.0, 0.5, 0.0).hex(), "#ff8000");
    }
}
