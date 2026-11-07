//! Chrome colours (round nb-svg): a front end shows a graphic's chrome (background, frame, axes, ticks, labels,
//! legend box, grid lines, and primitives drawn without a colour) in its own theme while the data keeps its
//! colours. The renderers mark chrome by [`Role`]: the SVG surface writes a class on every chrome element
//! (`mcf-<role>` when its fill is chrome, `mcs-<role>` when its stroke is), keeping the light colours as its
//! attributes, so every other reader sees the light look; a front end rasterising the SVG injects [`Theme::css`]
//! (CSS overrides presentation attributes). The raster surface recolours chrome itself when it is given a theme
//! (3D pictures turned in the notebook). Files written for export (PNG, SVG, PDF) keep [`Theme::LIGHT`].
use crate::color::Rgba;
use crate::surface::Role;
use std::fmt::Write as _;

/// The colours chrome is drawn in. Colours should be opaque: a chrome element keeps its own opacity (a faint
/// rule stays faint), multiplied with the theme colour's.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Theme {
    /// the page's background (`None`: transparent)
    pub background: Option<Rgba>,
    /// labels and primitives drawn without a colour
    pub text: Rgba,
    /// frame, axes and ticks
    pub frame: Rgba,
    /// grid lines
    pub grid: Rgba,
    /// the legend's box
    pub panel: Rgba,
}

impl Theme {
    /// What every file is written with: a white page, near-black text, a dark gray frame, light gray grid lines.
    pub const LIGHT: Theme = Theme {
        background: Some(Rgba::WHITE),
        text: crate::page::TEXT,
        frame: crate::page::FRAME,
        grid: crate::page::GRID,
        panel: Rgba::WHITE,
    };

    /// The colour an element of `role` drawn as `c` is shown in (`None`: not drawn, a transparent background).
    pub fn color(&self, role: Role, c: Rgba) -> Option<Rgba> {
        let base = match role {
            Role::Data => return Some(c),
            Role::Background => self.background?,
            Role::Frame => self.frame,
            Role::Text | Role::Ink => self.text,
            Role::Grid => self.grid,
            Role::Panel => self.panel,
        };
        Some(base.faded(c.a))
    }

    /// The style sheet that shows an SVG written by this crate in this theme (one rule per role and property).
    pub fn css(&self) -> String {
        let mut out = String::new();
        for role in Role::CHROME {
            let value = match self.color(role, Rgba::WHITE) {
                Some(c) => c.hex(),
                None => "none".to_string(),
            };
            let w = role.word();
            let _ = write!(out, ".mcf-{w}{{fill:{value}}}.mcs-{w}{{stroke:{value}}}");
        }
        out
    }

    /// A number that differs between themes of different colours (part of a front end's cache key).
    pub fn key(&self) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        let mut eat = |c: Option<Rgba>| {
            let bytes = c.map_or([0, 0, 0, 0], |c| c.bytes());
            for b in bytes.iter().chain(&[u8::from(c.is_some())]) {
                h ^= u64::from(*b);
                h = h.wrapping_mul(0x0100_0000_01b3);
            }
        };
        eat(self.background);
        eat(Some(self.text));
        eat(Some(self.frame));
        eat(Some(self.grid));
        eat(Some(self.panel));
        h
    }

    /// The contrast ratio (WCAG 2) of two opaque colours: 1 for equal colours, 21 for black on white.
    pub fn contrast(a: Rgba, b: Rgba) -> f64 {
        let lum = |c: Rgba| {
            let lin = |v: f64| if v <= 0.040_45 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) };
            let c = c.clamped();
            0.2126 * lin(c.r) + 0.7152 * lin(c.g) + 0.0722 * lin(c.b)
        };
        let (x, y) = (lum(a), lum(b));
        (x.max(y) + 0.05) / (x.min(y) + 0.05)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chrome_takes_the_theme_and_data_keeps_its_colour() {
        let dark = Theme {
            background: None,
            text: Rgba::rgb(0.8, 0.8, 0.8),
            frame: Rgba::rgb(0.6, 0.6, 0.6),
            grid: Rgba::rgb(0.2, 0.2, 0.2),
            panel: Rgba::rgb(0.1, 0.1, 0.1),
        };
        let red = Rgba::rgb(1.0, 0.0, 0.0);
        assert_eq!(dark.color(Role::Data, red), Some(red));
        assert_eq!(dark.color(Role::Background, Rgba::WHITE), None);
        assert_eq!(dark.color(Role::Text, Rgba::BLACK), Some(dark.text));
        assert_eq!(dark.color(Role::Ink, Rgba::BLACK), Some(dark.text));
        // a faint rule stays faint
        assert_eq!(dark.color(Role::Frame, Rgba { a: 0.35, ..Rgba::BLACK }), Some(Rgba { a: 0.35, ..dark.frame }));
        let css = dark.css();
        assert!(css.contains(".mcf-bg{fill:none}"), "{css}");
        assert!(css.contains(".mcf-text{fill:#cccccc}") && css.contains(".mcs-frame{stroke:#999999}"), "{css}");
        assert!(!css.contains("data"));
        assert_ne!(dark.key(), Theme::LIGHT.key());
        assert_eq!(Theme::LIGHT.key(), Theme::LIGHT.key());
        assert!((Theme::contrast(Rgba::BLACK, Rgba::WHITE) - 21.0).abs() < 1e-9);
    }
}
