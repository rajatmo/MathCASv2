//! The drawing interface every renderer implements ([`Surface`]): filled and stroked paths, dots and typeset
//! text, with rectangular clipping. Coordinates are page points (1/72 inch), x to the right and y down from the
//! top-left corner of the page.
use crate::color::Rgba;
use crate::label::Glyph;

/// A path segment.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Seg {
    /// start a subpath
    Move(f32, f32),
    /// a straight segment
    Line(f32, f32),
    /// a cubic Bézier segment: two control points and the end
    Cubic(f32, f32, f32, f32, f32, f32),
    /// close the subpath
    Close,
}

/// A path: subpaths of straight and cubic segments.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Path {
    /// the segments
    pub segs: Vec<Seg>,
}

/// The control-point distance of a quarter circle drawn as one cubic, as a fraction of the radius.
const KAPPA: f32 = 0.552_284_8;

impl Path {
    /// Start a subpath.
    pub fn move_to(&mut self, x: f32, y: f32) {
        self.segs.push(Seg::Move(x, y));
    }

    /// A straight segment.
    pub fn line_to(&mut self, x: f32, y: f32) {
        self.segs.push(Seg::Line(x, y));
    }

    /// A cubic segment.
    pub fn cubic_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.segs.push(Seg::Cubic(x1, y1, x2, y2, x, y));
    }

    /// Close the subpath.
    pub fn close(&mut self) {
        self.segs.push(Seg::Close);
    }

    /// Whether there is nothing to draw.
    pub fn is_empty(&self) -> bool {
        self.segs.is_empty()
    }

    /// A rectangle.
    pub fn rect(x: f32, y: f32, w: f32, h: f32) -> Path {
        let mut p = Path::default();
        p.move_to(x, y);
        p.line_to(x + w, y);
        p.line_to(x + w, y + h);
        p.line_to(x, y + h);
        p.close();
        p
    }

    /// An open polyline (or a closed polygon).
    pub fn polyline(pts: &[(f32, f32)], closed: bool) -> Path {
        let mut p = Path::default();
        p.add_polyline(pts, closed);
        p
    }

    /// Add a polyline as a subpath.
    pub fn add_polyline(&mut self, pts: &[(f32, f32)], closed: bool) {
        let Some((&(x, y), rest)) = pts.split_first() else { return };
        self.move_to(x, y);
        for &(x, y) in rest {
            self.line_to(x, y);
        }
        if closed {
            self.close();
        }
    }

    /// An elliptic arc from angle `a0` to `a1` (radians, anticlockwise as seen on the page, which is y down: the
    /// angles are those of the plot's y-up coordinates), centre `(cx, cy)`, radii `rx`, `ry`, as cubics of at
    /// most a quarter turn each. Starts a subpath unless `join` (then a line joins the current point to its start).
    pub fn arc(&mut self, (cx, cy): (f32, f32), (rx, ry): (f32, f32), a0: f64, a1: f64, join: bool) {
        let at = |a: f64| (cx + rx * a.cos() as f32, cy - ry * a.sin() as f32);
        let (x0, y0) = at(a0);
        if join {
            self.line_to(x0, y0);
        } else {
            self.move_to(x0, y0);
        }
        let n = ((a1 - a0).abs() / std::f64::consts::FRAC_PI_2).ceil().clamp(1.0, 64.0) as usize;
        let step = (a1 - a0) / n as f64;
        // the control-point factor of an arc of this angle
        let k = (4.0 / 3.0 * (step / 4.0).tan()) as f32;
        for i in 0..n {
            let (s, e) = (a0 + step * i as f64, a0 + step * (i + 1) as f64);
            let (sx, sy) = at(s);
            let (ex, ey) = at(e);
            // tangents: d/da (cos a, -sin a) scaled by the radii
            let (ts, te) = ((-(s.sin()) as f32, -(s.cos()) as f32), (-(e.sin()) as f32, -(e.cos()) as f32));
            self.cubic_to(
                sx + k * rx * ts.0,
                sy + k * ry * ts.1,
                ex - k * rx * te.0,
                ey - k * ry * te.1,
                ex,
                ey,
            );
        }
    }

    /// A full ellipse as four cubics.
    pub fn ellipse(cx: f32, cy: f32, rx: f32, ry: f32) -> Path {
        let (kx, ky) = (KAPPA * rx, KAPPA * ry);
        let mut p = Path::default();
        p.move_to(cx + rx, cy);
        p.cubic_to(cx + rx, cy + ky, cx + kx, cy + ry, cx, cy + ry);
        p.cubic_to(cx - kx, cy + ry, cx - rx, cy + ky, cx - rx, cy);
        p.cubic_to(cx - rx, cy - ky, cx - kx, cy - ry, cx, cy - ry);
        p.cubic_to(cx + kx, cy - ry, cx + rx, cy - ky, cx + rx, cy);
        p.close();
        p
    }
}

/// How the ends of stroked lines look.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Cap {
    /// cut square at the end point
    Butt,
    /// a half disc beyond the end point
    Round,
}

/// How a path is stroked.
#[derive(Clone, Debug, PartialEq)]
pub struct Pen {
    /// colour and opacity
    pub color: Rgba,
    /// line width in points
    pub width: f32,
    /// dash pattern in points (on, off, ...); empty for a solid line
    pub dash: Vec<f32>,
    /// line ends (joins are always round)
    pub cap: Cap,
}

/// Typeset text placed on the page.
#[derive(Clone, Copy, Debug)]
pub struct Text<'a> {
    /// the glyphs, positioned from the label's top-left corner (y down)
    pub glyphs: &'a [Glyph],
    /// filled rectangles (fraction bars), from the same corner: x, y, w, h
    pub rules: &'a [[f32; 4]],
    /// the label's characters in order, spaces included (for text extraction and accessibility)
    pub text: &'a str,
    /// where the label's top-left corner goes
    pub origin: (f32, f32),
    /// turned a quarter turn anticlockwise about the origin (read upwards)
    pub up: bool,
    /// colour
    pub color: Rgba,
}

impl Text<'_> {
    /// A point of the label (from its top-left corner, y down) on the page.
    pub fn place(&self, x: f32, y: f32) -> (f32, f32) {
        if self.up {
            (self.origin.0 + y, self.origin.1 - x)
        } else {
            (self.origin.0 + x, self.origin.1 + y)
        }
    }
}

/// Where a graphic is painted.
pub trait Surface {
    /// Fill a path (non-zero winding).
    fn fill(&mut self, path: &Path, color: Rgba);
    /// Stroke a path.
    fn stroke(&mut self, path: &Path, pen: &Pen);
    /// Filled discs of diameter `d` at `centres`.
    fn dots(&mut self, centres: &[(f32, f32)], d: f32, color: Rgba);
    /// Typeset text.
    fn text(&mut self, text: &Text<'_>);
    /// Clip what follows to a rectangle (x, y, w, h) until [`Surface::unclip`].
    fn clip(&mut self, rect: [f32; 4]);
    /// End the last [`Surface::clip`].
    fn unclip(&mut self);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arcs_end_where_they_should() {
        let mut p = Path::default();
        p.arc((10.0, 10.0), (5.0, 5.0), 0.0, std::f64::consts::PI, false);
        assert_eq!(p.segs.len(), 3);
        let Seg::Cubic(.., x, y) = p.segs[2] else { panic!() };
        assert!((x - 5.0).abs() < 1e-5 && (y - 10.0).abs() < 1e-5);
        // the quarter point is above the centre on the page (y down)
        let Seg::Cubic(.., x, y) = p.segs[1] else { panic!() };
        assert!((x - 10.0).abs() < 1e-5 && (y - 5.0).abs() < 1e-5);
    }
}
