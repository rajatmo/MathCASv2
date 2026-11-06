//! The fonts of the PDF export: the notebook's bundled fonts (Noto Sans for prose, DejaVu Sans Mono for code,
//! the DejaVu symbol subset) and the math font, with glyph lookup through a fallback chain, advances, ink boxes and
//! outlines in ems; and the metrics `mc_boxes` lays out typeset content with.
use mc_boxes::layout::metrics::math_char;
use mc_boxes::layout::{CharMetrics, FontRole, Metrics};
use mc_mathfont::PathCmd;
use std::sync::OnceLock;
use ttf_parser::{Face, GlyphId, OutlineBuilder};

static PROSE: &[u8] = include_bytes!("../../../mc-ui/fonts/NotoSans-Regular.ttf");
static PROSE_BOLD: &[u8] = include_bytes!("../../../mc-ui/fonts/NotoSans-Bold.ttf");
static PROSE_ITALIC: &[u8] = include_bytes!("../../../mc-ui/fonts/NotoSans-Italic.ttf");
static MONO: &[u8] = include_bytes!("../../../mc-ui/fonts/DejaVuSansMono.ttf");
static MONO_BOLD: &[u8] = include_bytes!("../../../mc-ui/fonts/DejaVuSansMono-Bold.ttf");
static SYMBOLS: &[u8] = include_bytes!("../../../mc-ui/fonts/DejaVuSans-Symbols.ttf");

/// A font of the export.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum Font {
    Prose,
    ProseBold,
    ProseItalic,
    Mono,
    MonoBold,
    Symbols,
    Math,
}

impl Font {
    const ALL: [Font; 7] =
        [Font::Prose, Font::ProseBold, Font::ProseItalic, Font::Mono, Font::MonoBold, Font::Symbols, Font::Math];

    fn index(self) -> usize {
        Font::ALL.iter().position(|f| *f == self).unwrap_or(0)
    }

    /// The PostScript-like name written into the PDF.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Font::Prose => "NotoSans",
            Font::ProseBold => "NotoSans-Bold",
            Font::ProseItalic => "NotoSans-Italic",
            Font::Mono => "DejaVuSansMono",
            Font::MonoBold => "DejaVuSansMono-Bold",
            Font::Symbols => "DejaVuSans-Symbols",
            Font::Math => "STIXTwoMath",
        }
    }
}

/// The parsed faces.
pub(crate) struct Fonts {
    faces: Vec<Face<'static>>,
}

/// The process-wide fonts (parsed once).
pub(crate) fn fonts() -> &'static Fonts {
    static F: OnceLock<Fonts> = OnceLock::new();
    F.get_or_init(|| {
        let parse = |d: &'static [u8]| Face::parse(d, 0).expect("a bundled font parses");
        let mut faces: Vec<Face<'static>> =
            [PROSE, PROSE_BOLD, PROSE_ITALIC, MONO, MONO_BOLD, SYMBOLS].into_iter().map(parse).collect();
        faces.push(mc_mathfont::font().face().clone());
        Fonts { faces }
    })
}

/// Ink box of a glyph in ems: x min, y min, x max, y max (y up).
pub(crate) type Bbox = [f32; 4];

impl Fonts {
    fn face(&self, f: Font) -> &Face<'static> {
        &self.faces[f.index()]
    }

    fn upem(&self, f: Font) -> f32 {
        self.face(f).units_per_em() as f32
    }

    /// The glyph of `c` in `f` itself, if it has one.
    pub(crate) fn has(&self, f: Font, c: char) -> Option<u16> {
        self.face(f).glyph_index(c).map(|g| g.0).filter(|&g| g != 0)
    }

    /// The font and glyph that draw `c` when `f` is asked: `f`, then the math font, then the symbol font, else
    /// `f`'s question mark.
    pub(crate) fn glyph(&self, f: Font, c: char) -> (Font, u16) {
        for g in [f, Font::Math, Font::Symbols, Font::Mono] {
            if let Some(id) = self.has(g, c) {
                return (g, id);
            }
        }
        (f, self.has(f, '?').unwrap_or(0))
    }

    /// The advance of a glyph in ems.
    pub(crate) fn advance(&self, f: Font, g: u16) -> f32 {
        self.face(f).glyph_hor_advance(GlyphId(g)).map_or(0.0, |a| a as f32 / self.upem(f))
    }

    /// The width of `s` in `f` at 1 em (fallback glyphs measured in their own fonts).
    pub(crate) fn width(&self, f: Font, s: &str) -> f32 {
        s.chars()
            .map(|c| {
                let (g, id) = self.glyph(f, c);
                self.advance(g, id)
            })
            .sum()
    }

    /// Ascender and descender (both positive) in ems.
    pub(crate) fn ascent_descent(&self, f: Font) -> (f32, f32) {
        let face = self.face(f);
        (face.ascender() as f32 / self.upem(f), -(face.descender() as f32) / self.upem(f))
    }

    /// The ink box of a glyph in ems (zero for a glyph without ink).
    pub(crate) fn bbox(&self, f: Font, g: u16) -> Bbox {
        let u = self.upem(f);
        self.face(f)
            .glyph_bounding_box(GlyphId(g))
            .map_or([0.0; 4], |r| [r.x_min as f32 / u, r.y_min as f32 / u, r.x_max as f32 / u, r.y_max as f32 / u])
    }

    /// The outline of a glyph in ems, y up.
    pub(crate) fn outline(&self, f: Font, g: u16) -> Vec<PathCmd> {
        if f == Font::Math {
            return mc_mathfont::font().outline(g);
        }
        struct B(Vec<PathCmd>, f32);
        impl OutlineBuilder for B {
            fn move_to(&mut self, x: f32, y: f32) {
                self.0.push(PathCmd::Move(x / self.1, y / self.1));
            }
            fn line_to(&mut self, x: f32, y: f32) {
                self.0.push(PathCmd::Line(x / self.1, y / self.1));
            }
            fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
                self.0.push(PathCmd::Quad(x1 / self.1, y1 / self.1, x / self.1, y / self.1));
            }
            fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
                let u = self.1;
                self.0.push(PathCmd::Cubic(x1 / u, y1 / u, x2 / u, y2 / u, x / u, y / u));
            }
            fn close(&mut self) {
                self.0.push(PathCmd::Close);
            }
        }
        let mut b = B(Vec::new(), self.upem(f));
        let _ = self.face(f).outline_glyph(GlyphId(g), &mut b);
        b.0
    }
}

/// Metrics for typeset content in the code font (`bold` for inputs): every character the code font has is drawn in
/// it, the rest in the math font, as the notebook does (no math italics).
pub(crate) struct CodeMetrics {
    pub(crate) font: Font,
}

impl Metrics for CodeMetrics {
    fn char(&self, c: char, role: FontRole) -> CharMetrics {
        let f = fonts();
        match role {
            FontRole::Math => math_char(c),
            FontRole::Text => {
                let (font, g) = f.glyph(self.font, c);
                let b = f.bbox(font, g);
                CharMetrics { advance: f.advance(font, g), ascent: b[3].max(0.0), descent: (-b[1]).max(0.0) }
            }
        }
    }

    fn line(&self, _role: FontRole) -> (f32, f32) {
        fonts().ascent_descent(self.font)
    }

    fn role(&self, c: char) -> FontRole {
        if c == '\u{25a1}' || (fonts().has(self.font, c).is_none() && mc_mathfont::font().glyph(c).is_some()) {
            FontRole::Math
        } else {
            FontRole::Text
        }
    }

    fn line_pitch(&self) -> Option<f32> {
        Some(LINE_PITCH_CODE)
    }

    fn italic_letters(&self) -> bool {
        false
    }
}

/// The distance of code lines in ems.
pub(crate) const LINE_PITCH_CODE: f32 = 1.3;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fonts_have_their_glyphs_and_fall_back() {
        let f = fonts();
        assert!(f.has(Font::Prose, 'a').is_some() && f.has(Font::Mono, 'a').is_some());
        let (font, _) = f.glyph(Font::Prose, '\u{2a0c}');
        assert_ne!(font, Font::Prose);
        let w = f.width(Font::Mono, "abc");
        assert!((w - 3.0 * f.width(Font::Mono, "x")).abs() < 1e-4, "{w}");
        assert!(!f.outline(Font::Prose, f.has(Font::Prose, 'a').unwrap()).is_empty());
    }
}
