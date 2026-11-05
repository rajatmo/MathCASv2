//! The math font of the notebook: the bundled STIX Two Math (SIL Open Font License 1.1, licence text in
//! [`FONT_LICENSE`]) and the parts of its OpenType MATH table that typesetting needs (design 08, sections 4.7 and
//! 4.10).
//!
//! - [`font`] gives the process-wide parsed font; [`MathFont::new`] parses another font with a MATH table.
//! - [`Constants`] are the MATH constants (fraction, script, radical, limit and stack parameters).
//! - Per glyph: [`MathFont::metrics`], [`MathFont::italic_correction`], [`MathFont::top_accent`],
//!   [`MathFont::ssty`] (script-size forms), [`MathFont::variants`] and [`MathFont::assembly`].
//! - [`MathFont::stretch`] grows a glyph (a delimiter, a radical sign, an arrow) to a target length from its size
//!   variants or its assembly; the result is a list of glyphs with offsets.
//! - [`MathFont::outline`] and [`rasterize`] give a glyph's shape, for painters that draw glyphs by id (size
//!   variants and assembly parts have no Unicode code point, so they cannot go through a text pipeline).
//!
//! Every length is in ems of the font (design units divided by units per em); y is up from the baseline.
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use ttf_parser::{math, Face, GlyphId};

mod raster;
mod stretch;

pub use raster::{rasterize, GlyphBitmap};
pub use stretch::{Stretch, StretchPiece};

/// The font file, embedded.
pub static FONT_DATA: &[u8] = include_bytes!("../fonts/STIXTwoMath-Regular.otf");
/// Its licence text (SIL Open Font License 1.1); ship it with any binary that embeds the font.
pub static FONT_LICENSE: &str = include_str!("../fonts/OFL.txt");
/// The font's family name, for painters that register [`FONT_DATA`] with their own text pipeline.
pub const FONT_FAMILY: &str = "STIX Two Math";

/// A glyph id in the math font.
pub type Glyph = u16;

/// Advance and ink extent of one glyph, in ems.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GlyphMetrics {
    /// horizontal advance
    pub advance: f32,
    /// ink height above the baseline (negative for glyphs entirely below it)
    pub height: f32,
    /// ink depth below the baseline (negative for glyphs entirely above it)
    pub depth: f32,
    /// ink extent left of the origin
    pub x_min: f32,
    /// ink extent right of the origin
    pub x_max: f32,
}

/// One piece of a glyph assembly (OpenType `GlyphPart`), lengths in ems.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Part {
    /// the piece's glyph
    pub glyph: Glyph,
    /// connector length at the start (bottom or left)
    pub start_connector: f32,
    /// connector length at the end (top or right)
    pub end_connector: f32,
    /// the piece's full advance in the stretch direction
    pub full_advance: f32,
    /// whether the piece may be repeated
    pub extender: bool,
}

/// The MATH constants the layout uses (a subset of OpenType `MathConstants`), in ems unless noted.
#[allow(missing_docs)]
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Constants {
    /// percent
    pub script_percent: f32,
    /// percent
    pub script_script_percent: f32,
    pub delimited_sub_formula_min_height: f32,
    pub display_operator_min_height: f32,
    pub axis_height: f32,
    pub accent_base_height: f32,
    pub subscript_shift_down: f32,
    pub subscript_top_max: f32,
    pub subscript_baseline_drop_min: f32,
    pub superscript_shift_up: f32,
    pub superscript_shift_up_cramped: f32,
    pub superscript_bottom_min: f32,
    pub superscript_baseline_drop_max: f32,
    pub sub_superscript_gap_min: f32,
    pub superscript_bottom_max_with_subscript: f32,
    pub space_after_script: f32,
    pub upper_limit_gap_min: f32,
    pub upper_limit_baseline_rise_min: f32,
    pub lower_limit_gap_min: f32,
    pub lower_limit_baseline_drop_min: f32,
    pub stack_top_shift_up: f32,
    pub stack_top_display_style_shift_up: f32,
    pub stack_bottom_shift_down: f32,
    pub stack_bottom_display_style_shift_down: f32,
    pub stack_gap_min: f32,
    pub stack_display_style_gap_min: f32,
    pub stretch_stack_gap_above_min: f32,
    pub stretch_stack_gap_below_min: f32,
    pub fraction_numerator_shift_up: f32,
    pub fraction_numerator_display_style_shift_up: f32,
    pub fraction_denominator_shift_down: f32,
    pub fraction_denominator_display_style_shift_down: f32,
    pub fraction_numerator_gap_min: f32,
    pub fraction_num_display_style_gap_min: f32,
    pub fraction_rule_thickness: f32,
    pub fraction_denominator_gap_min: f32,
    pub fraction_denom_display_style_gap_min: f32,
    pub overbar_vertical_gap: f32,
    pub overbar_rule_thickness: f32,
    pub overbar_extra_ascender: f32,
    pub underbar_vertical_gap: f32,
    pub underbar_rule_thickness: f32,
    pub underbar_extra_descender: f32,
    pub radical_vertical_gap: f32,
    pub radical_display_style_vertical_gap: f32,
    pub radical_rule_thickness: f32,
    pub radical_extra_ascender: f32,
    pub radical_kern_before_degree: f32,
    pub radical_kern_after_degree: f32,
    /// percent
    pub radical_degree_bottom_raise_percent: f32,
    pub min_connector_overlap: f32,
}

/// A parsed math font with per-glyph caches. Shared freely between threads.
pub struct MathFont {
    face: Face<'static>,
    upem: f32,
    /// the MATH constants
    pub constants: Constants,
    metrics: Mutex<HashMap<Glyph, GlyphMetrics>>,
    ssty_cache: Mutex<HashMap<(Glyph, u8), Glyph>>,
}

/// The process-wide bundled font (parsed once).
pub fn font() -> &'static MathFont {
    static F: OnceLock<MathFont> = OnceLock::new();
    F.get_or_init(|| MathFont::new(FONT_DATA).expect("the bundled math font parses and has a MATH table"))
}

impl MathFont {
    /// Parses a font with an OpenType MATH table; `None` when it does not parse or has no MATH constants.
    pub fn new(data: &'static [u8]) -> Option<MathFont> {
        let face = Face::parse(data, 0).ok()?;
        let upem = face.units_per_em() as f32;
        let mut f = MathFont {
            face,
            upem,
            constants: Constants::default(),
            metrics: Mutex::new(HashMap::new()),
            ssty_cache: Mutex::new(HashMap::new()),
        };
        f.constants = f.read_constants()?;
        Some(f)
    }

    /// The underlying face, for painters that need more than this crate offers.
    pub fn face(&self) -> &Face<'static> {
        &self.face
    }

    /// Design units per em.
    pub fn units_per_em(&self) -> f32 {
        self.upem
    }

    fn em(&self, v: impl Into<f32>) -> f32 {
        v.into() / self.upem
    }

    fn table(&self) -> Option<&math::Table<'static>> {
        self.face.tables().math.as_ref()
    }

    fn read_constants(&self) -> Option<Constants> {
        let c = self.table()?.constants?;
        let v = |m: math::MathValue| m.value as f32 / self.upem;
        Some(Constants {
            script_percent: c.script_percent_scale_down() as f32,
            script_script_percent: c.script_script_percent_scale_down() as f32,
            delimited_sub_formula_min_height: self.em(c.delimited_sub_formula_min_height() as f32),
            display_operator_min_height: self.em(c.display_operator_min_height() as f32),
            axis_height: v(c.axis_height()),
            accent_base_height: v(c.accent_base_height()),
            subscript_shift_down: v(c.subscript_shift_down()),
            subscript_top_max: v(c.subscript_top_max()),
            subscript_baseline_drop_min: v(c.subscript_baseline_drop_min()),
            superscript_shift_up: v(c.superscript_shift_up()),
            superscript_shift_up_cramped: v(c.superscript_shift_up_cramped()),
            superscript_bottom_min: v(c.superscript_bottom_min()),
            superscript_baseline_drop_max: v(c.superscript_baseline_drop_max()),
            sub_superscript_gap_min: v(c.sub_superscript_gap_min()),
            superscript_bottom_max_with_subscript: v(c.superscript_bottom_max_with_subscript()),
            space_after_script: v(c.space_after_script()),
            upper_limit_gap_min: v(c.upper_limit_gap_min()),
            upper_limit_baseline_rise_min: v(c.upper_limit_baseline_rise_min()),
            lower_limit_gap_min: v(c.lower_limit_gap_min()),
            lower_limit_baseline_drop_min: v(c.lower_limit_baseline_drop_min()),
            stack_top_shift_up: v(c.stack_top_shift_up()),
            stack_top_display_style_shift_up: v(c.stack_top_display_style_shift_up()),
            stack_bottom_shift_down: v(c.stack_bottom_shift_down()),
            stack_bottom_display_style_shift_down: v(c.stack_bottom_display_style_shift_down()),
            stack_gap_min: v(c.stack_gap_min()),
            stack_display_style_gap_min: v(c.stack_display_style_gap_min()),
            stretch_stack_gap_above_min: v(c.stretch_stack_gap_above_min()),
            stretch_stack_gap_below_min: v(c.stretch_stack_gap_below_min()),
            fraction_numerator_shift_up: v(c.fraction_numerator_shift_up()),
            fraction_numerator_display_style_shift_up: v(c.fraction_numerator_display_style_shift_up()),
            fraction_denominator_shift_down: v(c.fraction_denominator_shift_down()),
            fraction_denominator_display_style_shift_down: v(c.fraction_denominator_display_style_shift_down()),
            fraction_numerator_gap_min: v(c.fraction_numerator_gap_min()),
            fraction_num_display_style_gap_min: v(c.fraction_num_display_style_gap_min()),
            fraction_rule_thickness: v(c.fraction_rule_thickness()),
            fraction_denominator_gap_min: v(c.fraction_denominator_gap_min()),
            fraction_denom_display_style_gap_min: v(c.fraction_denom_display_style_gap_min()),
            overbar_vertical_gap: v(c.overbar_vertical_gap()),
            overbar_rule_thickness: v(c.overbar_rule_thickness()),
            overbar_extra_ascender: v(c.overbar_extra_ascender()),
            underbar_vertical_gap: v(c.underbar_vertical_gap()),
            underbar_rule_thickness: v(c.underbar_rule_thickness()),
            underbar_extra_descender: v(c.underbar_extra_descender()),
            radical_vertical_gap: v(c.radical_vertical_gap()),
            radical_display_style_vertical_gap: v(c.radical_display_style_vertical_gap()),
            radical_rule_thickness: v(c.radical_rule_thickness()),
            radical_extra_ascender: v(c.radical_extra_ascender()),
            radical_kern_before_degree: v(c.radical_kern_before_degree()),
            radical_kern_after_degree: v(c.radical_kern_after_degree()),
            radical_degree_bottom_raise_percent: c.radical_degree_bottom_raise_percent() as f32,
            min_connector_overlap: self.table()?.variants.map_or(0.0, |v| self.em(v.min_connector_overlap as f32)),
        })
    }

    /// The glyph for a character, if the font has one.
    pub fn glyph(&self, c: char) -> Option<Glyph> {
        self.face.glyph_index(c).map(|g| g.0)
    }

    /// Advance and ink extent of a glyph (zero ink for a glyph without outline, such as a space).
    pub fn metrics(&self, g: Glyph) -> GlyphMetrics {
        if let Some(m) = lock(&self.metrics).get(&g) {
            return *m;
        }
        let gid = GlyphId(g);
        let advance = self.face.glyph_hor_advance(gid).map_or(0.0, |a| self.em(a as f32));
        let m = match self.face.glyph_bounding_box(gid) {
            Some(r) => GlyphMetrics {
                advance,
                height: self.em(r.y_max as f32),
                depth: -self.em(r.y_min as f32),
                x_min: self.em(r.x_min as f32),
                x_max: self.em(r.x_max as f32),
            },
            None => GlyphMetrics { advance, ..Default::default() },
        };
        lock(&self.metrics).insert(g, m);
        m
    }

    /// The italic correction of a glyph (0 when it has none).
    pub fn italic_correction(&self, g: Glyph) -> f32 {
        self.table()
            .and_then(|t| t.glyph_info)
            .and_then(|i| i.italic_corrections)
            .and_then(|v| v.get(GlyphId(g)))
            .map_or(0.0, |v| self.em(v.value as f32))
    }

    /// The horizontal position where a top accent attaches (half the advance when the font gives none).
    pub fn top_accent(&self, g: Glyph) -> f32 {
        self.top_accent_raw(g).unwrap_or_else(|| self.metrics(g).advance / 2.0)
    }

    /// The top accent attachment the font gives for the glyph, if any.
    pub fn top_accent_raw(&self, g: Glyph) -> Option<f32> {
        self.table()
            .and_then(|t| t.glyph_info)
            .and_then(|i| i.top_accent_attachments)
            .and_then(|v| v.get(GlyphId(g)))
            .map(|v| self.em(v.value as f32))
    }

    /// The script-size form of a glyph (OpenType feature `ssty`; level 1 for script, 2 for script-script size);
    /// the glyph itself at level 0 or when the font has none.
    pub fn ssty(&self, g: Glyph, level: u8) -> Glyph {
        if level == 0 {
            return g;
        }
        let key = (g, level);
        if let Some(v) = lock(&self.ssty_cache).get(&key) {
            return *v;
        }
        let found = self.find_ssty(g, level).unwrap_or(g);
        lock(&self.ssty_cache).insert(key, found);
        found
    }

    fn find_ssty(&self, g: Glyph, level: u8) -> Option<Glyph> {
        use ttf_parser::gsub::{SingleSubstitution, SubstitutionSubtable};
        let gsub = self.face.tables().gsub?;
        let feature = gsub.features.find(ttf_parser::Tag::from_bytes(b"ssty"))?;
        for li in feature.lookup_indices {
            let lookup = gsub.lookups.get(li)?;
            for k in 0..lookup.subtables.len() {
                match lookup.subtables.get::<SubstitutionSubtable>(k)? {
                    SubstitutionSubtable::Alternate(a) => {
                        if let Some(i) = a.coverage.get(GlyphId(g)) {
                            let set = a.alternate_sets.get(i)?;
                            let n = set.alternates.len();
                            if n == 0 {
                                return None;
                            }
                            return set.alternates.get((level as u16 - 1).min(n - 1)).map(|x| x.0);
                        }
                    }
                    SubstitutionSubtable::Single(s) => {
                        if let Some(i) = s.coverage().get(GlyphId(g)) {
                            return match s {
                                SingleSubstitution::Format1 { delta, .. } => Some((g as i32 + delta as i32) as u16),
                                SingleSubstitution::Format2 { substitutes, .. } => substitutes.get(i).map(|x| x.0),
                            };
                        }
                    }
                    _ => {}
                }
            }
        }
        None
    }

    /// The x-height in ems.
    pub fn x_height(&self) -> f32 {
        self.face.x_height().map_or(0.45, |h| self.em(h as f32))
    }

    /// The ascender and descender of the font (both positive), in ems.
    pub fn ascent_descent(&self) -> (f32, f32) {
        (self.em(self.face.ascender() as f32), -self.em(self.face.descender() as f32))
    }

    /// Whether the glyph is an "extended shape" (scripts attach to its ink, not to the em box).
    pub fn is_extended_shape(&self, g: Glyph) -> bool {
        self.table()
            .and_then(|t| t.glyph_info)
            .and_then(|i| i.extended_shapes)
            .is_some_and(|c| c.contains(GlyphId(g)))
    }

    fn construction(&self, g: Glyph, vertical: bool) -> Option<math::GlyphConstruction<'static>> {
        let v = self.table()?.variants?;
        if vertical {
            v.vertical_constructions.get(GlyphId(g))
        } else {
            v.horizontal_constructions.get(GlyphId(g))
        }
    }

    /// Size variants of a glyph in one direction, smallest first, with their advance in that direction (the glyph
    /// itself is usually the first).
    pub fn variants(&self, g: Glyph, vertical: bool) -> Vec<(Glyph, f32)> {
        match self.construction(g, vertical) {
            Some(c) => c.variants.into_iter().map(|v| (v.variant_glyph.0, self.em(v.advance_measurement as f32))).collect(),
            None => vec![],
        }
    }

    /// The parts of the glyph's assembly in one direction (bottom to top, or left to right) and the assembly's
    /// italic correction.
    pub fn assembly(&self, g: Glyph, vertical: bool) -> Option<(Vec<Part>, f32)> {
        let a = self.construction(g, vertical)?.assembly?;
        let parts = a
            .parts
            .into_iter()
            .map(|p| Part {
                glyph: p.glyph_id.0,
                start_connector: self.em(p.start_connector_length as f32),
                end_connector: self.em(p.end_connector_length as f32),
                full_advance: self.em(p.full_advance as f32),
                extender: p.part_flags.extender(),
            })
            .collect();
        Some((parts, self.em(a.italics_correction.value as f32)))
    }

    /// The outline of a glyph in ems, y up, as path commands (empty for a glyph without outline).
    pub fn outline(&self, g: Glyph) -> Vec<PathCmd> {
        struct B(Vec<PathCmd>, f32);
        impl ttf_parser::OutlineBuilder for B {
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
        let mut b = B(vec![], self.upem);
        self.face.outline_glyph(GlyphId(g), &mut b);
        b.0
    }
}

/// Locks a cache; a poisoned lock (a panic while holding it) only means a cache entry may be missing.
fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// A glyph outline command, coordinates in ems with y up.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PathCmd {
    /// start a contour
    Move(f32, f32),
    /// straight segment
    Line(f32, f32),
    /// quadratic Bézier: control point, end point
    Quad(f32, f32, f32, f32),
    /// cubic Bézier: two control points, end point
    Cubic(f32, f32, f32, f32, f32, f32),
    /// close the contour
    Close,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_font_has_math_constants() {
        let f = font();
        let c = &f.constants;
        assert_eq!(c.script_percent, 70.0);
        assert_eq!(c.script_script_percent, 55.0);
        assert!(c.axis_height > 0.2 && c.axis_height < 0.3, "{}", c.axis_height);
        assert!(c.fraction_rule_thickness > 0.0 && c.fraction_rule_thickness < 0.1);
        assert!(c.radical_rule_thickness > 0.0);
        assert!(c.min_connector_overlap >= 0.0);
        assert!(FONT_LICENSE.contains("SIL OPEN FONT LICENSE Version 1.1"));
    }

    #[test]
    fn glyph_metrics_and_italics() {
        let f = font();
        let x = f.glyph('x').unwrap();
        let m = f.metrics(x);
        assert!(m.advance > 0.3 && m.advance < 0.7, "{m:?}");
        assert!(m.height > 0.3 && m.depth.abs() < 0.05, "{m:?}");
        // the math italic f has an italic correction, upright x does not
        let it_f = f.glyph('\u{1D453}').unwrap();
        assert!(f.italic_correction(it_f) > 0.0);
        assert_eq!(f.italic_correction(x), 0.0);
        assert!(f.top_accent(x) > 0.0);
        assert!(f.x_height() > 0.4 && f.x_height() < 0.55);
        let (a, d) = f.ascent_descent();
        assert!(a > 0.5 && d > 0.1);
    }

    #[test]
    fn script_forms_and_variants() {
        let f = font();
        // the prime and some letters have script-size forms; a glyph without one maps to itself
        let a = f.glyph('\u{1D44E}').unwrap();
        let _ = f.ssty(a, 1);
        assert_eq!(f.ssty(a, 0), a);
        let paren = f.glyph('(').unwrap();
        let vs = f.variants(paren, true);
        assert!(vs.len() > 3, "{vs:?}");
        assert!(vs.windows(2).all(|w| w[0].1 <= w[1].1), "variants grow");
        let (parts, _) = f.assembly(paren, true).expect("paren assembly");
        assert!(parts.iter().any(|p| p.extender));
        let sum = f.glyph('∑').unwrap();
        assert!(f.variants(sum, true).len() >= 2);
        let arrow = f.glyph('→').unwrap();
        assert!(f.assembly(arrow, false).is_some() || !f.variants(arrow, false).is_empty());
        assert!(!f.outline(sum).is_empty());
        assert!(f.outline(f.glyph(' ').unwrap()).is_empty());
    }
}
