//! What the layout needs to know about fonts it does not own: the cell's text font is the painter's (egui
//! measures it), the math font is bundled. Lengths are in ems of a 1 em font, y up.
use mc_mathfont::font;

/// The font a character is drawn in (design 08, section 4.10).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FontRole {
    /// the cell's text font (Consolas for code cells): letters, digits, ASCII operators
    Text,
    /// the bundled math font (STIX Two Math): Greek, math symbols, placeholders, big operators
    Math,
}

/// The extent of one character at 1 em.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CharMetrics {
    /// horizontal advance
    pub advance: f32,
    /// height above the baseline used for layout (ink or font ascent, as the painter measures)
    pub ascent: f32,
    /// depth below the baseline used for layout
    pub descent: f32,
}

/// The font measurements the layout uses. The painter implements it over its own text measurement; tests use
/// [`MonoMetrics`]; [`MathFontMetrics`] measures everything with the bundled math font.
pub trait Metrics {
    /// Metrics of `c` drawn in `role` at 1 em.
    fn char(&self, c: char, role: FontRole) -> CharMetrics;

    /// Height above the baseline and depth below it of an ordinary line of `role` text at 1 em (the caret's
    /// extent on an empty line, the strut of a row).
    fn line(&self, role: FontRole) -> (f32, f32) {
        match role {
            FontRole::Text => (0.8, 0.2),
            FontRole::Math => (0.75, 0.25),
        }
    }

    /// Size of a graphic in ems: (width, height above the baseline). Default: 8 x 5.
    fn graphic(&self, path: &str) -> (f32, f32) {
        let _ = path;
        (8.0, 5.0)
    }

    /// The font `c` is drawn in. Default: [`role_of`] (the math font for everything it has beyond Latin-1); a
    /// painter that draws in its text font gives [`FontRole::Text`] for every character that font has.
    fn role(&self, c: char) -> FontRole {
        role_of(c)
    }

    /// The height of the math axis (the middle of a minus sign; fraction bars and centred delimiters sit on it)
    /// in ems, when the text font says (`None`: the math font's MATH constant).
    fn axis_height(&self) -> Option<f32> {
        None
    }

    /// The distance between the baselines of two plain text lines in ems, when the painter's text lines have one
    /// (`None`: the layout's own spacing). Lines no taller than a text line are then exactly this far apart, as in
    /// the painter's plain text.
    fn line_pitch(&self) -> Option<f32> {
        None
    }

    /// Traditional form draws single-letter names in math italics (the math font's letters). Default true; a
    /// painter that keeps every letter in its text font says false.
    fn italic_letters(&self) -> bool {
        true
    }
}

/// The font role of a character: the math font for what the text font lacks (Greek, math symbols, letter-like
/// symbols, arrows), the text font for ASCII and Latin-1.
pub fn role_of(c: char) -> FontRole {
    if (c as u32) < 0x250 || c == '\u{2009}' || c == '\u{a0}' {
        FontRole::Text
    } else if font().glyph(c).is_some() {
        FontRole::Math
    } else {
        FontRole::Text
    }
}

/// Math-font metrics of a character (its ink extent, and the advance), `?` for characters the font lacks.
pub fn math_char(c: char) -> CharMetrics {
    let f = font();
    let g = f.glyph(c).or_else(|| f.glyph('?')).unwrap_or(0);
    let m = f.metrics(g);
    CharMetrics { advance: m.advance, ascent: m.height.max(0.0), descent: m.depth.max(0.0) }
}

/// Fixed metrics for tests and headless use: the text font is monospaced (advance 0.6 em, ascent 0.75 em,
/// descent 0.25 em, spaces too); the math font is the bundled one.
#[derive(Clone, Copy, Debug, Default)]
pub struct MonoMetrics;

impl Metrics for MonoMetrics {
    fn char(&self, c: char, role: FontRole) -> CharMetrics {
        match role {
            FontRole::Text => CharMetrics { advance: 0.6, ascent: 0.75, descent: 0.25 },
            FontRole::Math => math_char(c),
        }
    }

    fn line(&self, role: FontRole) -> (f32, f32) {
        match role {
            FontRole::Text => (0.75, 0.25),
            FontRole::Math => (0.75, 0.25),
        }
    }
}

/// Everything measured with the bundled math font (its Latin letters and digits included).
#[derive(Clone, Copy, Debug, Default)]
pub struct MathFontMetrics;

impl Metrics for MathFontMetrics {
    fn char(&self, c: char, _role: FontRole) -> CharMetrics {
        math_char(c)
    }

    fn line(&self, _role: FontRole) -> (f32, f32) {
        font().ascent_descent()
    }
}
