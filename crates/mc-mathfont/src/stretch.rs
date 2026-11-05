//! Growing a glyph to a target length: the smallest size variant that is long enough, else the glyph assembly
//! (OpenType `MathGlyphConstruction`), else the largest variant.
use crate::{Glyph, MathFont, Part};

/// A glyph grown to a length, at size 1 em: the pieces to draw and the extent of the whole.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Stretch {
    /// glyphs with their origins, in ems, y up from the baseline of the whole
    pub pieces: Vec<StretchPiece>,
    /// horizontal advance of the whole
    pub advance: f32,
    /// extent above the baseline
    pub height: f32,
    /// extent below the baseline
    pub depth: f32,
}

/// One glyph of a [`Stretch`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StretchPiece {
    /// the glyph
    pub glyph: Glyph,
    /// its origin, in ems from the origin of the whole
    pub x: f32,
    /// its baseline, in ems above the baseline of the whole
    pub y: f32,
}

/// Most repetitions of an extender tried before giving up (a delimiter around 200 em of content).
const MAX_REPEATS: usize = 200;

impl MathFont {
    /// The glyph `g` grown to at least `target` ems, vertically (delimiters, radical signs, integrals) or
    /// horizontally (arrows, braces). A vertical assembly sits on the baseline (depth 0); callers centre it.
    pub fn stretch(&self, g: Glyph, target: f32, vertical: bool) -> Stretch {
        let vs = self.variants(g, vertical);
        if let Some((v, _)) = vs.iter().find(|(_, adv)| *adv >= target) {
            return self.single(*v);
        }
        if let Some((parts, _)) = self.assembly(g, vertical) {
            if !parts.is_empty() {
                return self.assemble(&parts, target, vertical);
            }
        }
        self.single(vs.last().map_or(g, |v| v.0))
    }

    /// One glyph as a stretch.
    pub fn single(&self, g: Glyph) -> Stretch {
        let m = self.metrics(g);
        Stretch { pieces: vec![StretchPiece { glyph: g, x: 0.0, y: 0.0 }], advance: m.advance, height: m.height, depth: m.depth }
    }

    fn assemble(&self, parts: &[Part], target: f32, vertical: bool) -> Stretch {
        let o = self.constants.min_connector_overlap;
        let has_ext = parts.iter().any(|p| p.extender);
        // the fewest repetitions of the extenders that reach the target with minimal overlaps
        let mut seq: Vec<Part> = vec![];
        for r in 0..MAX_REPEATS {
            seq.clear();
            for p in parts {
                let n = if p.extender { r } else { 1 };
                seq.extend(std::iter::repeat_n(*p, n));
            }
            if seq.is_empty() {
                continue;
            }
            let full: f32 = seq.iter().map(|p| p.full_advance).sum();
            if full - o * (seq.len() as f32 - 1.0) >= target || !has_ext {
                break;
            }
        }
        // then spread the overlap evenly over the joints, within what the connectors allow
        let joints = seq.len().saturating_sub(1);
        let full: f32 = seq.iter().map(|p| p.full_advance).sum();
        let max_ov = seq.windows(2).map(|w| w[0].end_connector.min(w[1].start_connector)).fold(f32::INFINITY, f32::min);
        let ov = if joints > 0 { ((full - target) / joints as f32).min(max_ov).max(o) } else { 0.0 };
        let mut out = Stretch::default();
        let mut pos = 0.0;
        for p in &seq {
            let m = self.metrics(p.glyph);
            if vertical {
                out.pieces.push(StretchPiece { glyph: p.glyph, x: 0.0, y: pos + m.depth });
                out.advance = out.advance.max(m.advance);
            } else {
                out.pieces.push(StretchPiece { glyph: p.glyph, x: pos, y: 0.0 });
                out.height = out.height.max(m.height);
                out.depth = out.depth.max(m.depth);
            }
            pos += p.full_advance - ov;
        }
        let total = pos + ov;
        if vertical {
            out.height = total;
        } else {
            out.advance = total;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use crate::font;

    #[test]
    fn small_target_gives_the_glyph_and_big_target_an_assembly() {
        let f = font();
        let p = f.glyph('(').unwrap();
        let s = f.stretch(p, 0.5, true);
        assert_eq!(s.pieces.len(), 1);
        assert_eq!(s.pieces[0].glyph, p);
        let big = f.stretch(p, 12.0, true);
        assert!(big.pieces.len() >= 3, "{big:?}");
        assert!(big.height + big.depth >= 12.0 - 1e-3, "{big:?}");
        // a medium target picks a size variant that is long enough
        let mid = f.stretch(p, 2.0, true);
        assert_eq!(mid.pieces.len(), 1);
        assert!(mid.height + mid.depth >= 1.9, "{mid:?}");
    }

    #[test]
    fn horizontal_arrow_grows() {
        let f = font();
        let a = f.glyph('→').unwrap();
        let s = f.stretch(a, 6.0, false);
        assert!(s.advance >= 6.0 - 1e-3, "{s:?}");
    }
}
