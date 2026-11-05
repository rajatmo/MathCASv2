//! Rasterising a plot: a frame with nice-number ticks and labels, curves or points, to PNG bytes.
use ab_glyph::{Font, FontRef, PxScale, ScaleFont};
use std::sync::OnceLock;
use tiny_skia::{Color, FillRule, Paint, PathBuilder, Pixmap, Stroke, Transform};

/// Points of one curve or data set; `None` breaks the line.
pub(crate) struct Series {
    pub(crate) points: Vec<(f64, Option<f64>)>,
}

const W: u32 = 640;
const H: u32 = 400;
/// Plot area: left, top, right, bottom margins in pixels.
const ML: f32 = 64.0;
const MT: f32 = 16.0;
const MR: f32 = 16.0;
const MB: f32 = 40.0;
/// The default colour cycle: modern earth tones with strong contrast on white, ordered so neighbouring curves
/// differ in both hue and lightness: deep teal, terracotta, ochre, aubergine, olive, slate indigo, espresso.
const COLORS: [(u8, u8, u8); 7] = [
    (0x1B, 0x5E, 0x63),
    (0xC1, 0x50, 0x2E),
    (0xC9, 0x9A, 0x2E),
    (0x5B, 0x2C, 0x4F),
    (0x6B, 0x7A, 0x2F),
    (0x34, 0x50, 0x6E),
    (0x3B, 0x2A, 0x22),
];

/// Which axes are logarithmic. On a log axis the series' coordinates are already log10 of the values.
#[derive(Clone, Copy, Default)]
pub(crate) struct Scale {
    pub(crate) xlog: bool,
    pub(crate) ylog: bool,
}

/// A tick label: plain text, or 10 to an integer power (drawn with a superscript).
enum Label {
    Text(String),
    Pow10(i64),
}

/// Major ticks (position, label) and minor grid positions of the axis [lo, hi].
fn axis(lo: f64, hi: f64, log: bool) -> (Vec<(f64, Label)>, Vec<f64>) {
    if !log {
        return (ticks(lo, hi).into_iter().map(|t| (t, Label::Text(label(t, lo, hi)))).collect(), vec![]);
    }
    let decades = hi - lo;
    if decades < 1.2 {
        // under about a decade: nice ticks on the values themselves, placed at their logarithms
        let (a, b) = (10f64.powf(lo), 10f64.powf(hi));
        let t = ticks(a, b).into_iter().filter(|v| *v > 0.0).map(|v| (v.log10(), Label::Text(label(v, a, b))));
        return (t.collect(), vec![]);
    }
    // powers of ten, every `every` decades so that at most about 8 are labelled
    let every = (decades / 8.0).ceil().max(1.0) as i64;
    let (k0, k1) = (lo.ceil() as i64, hi.floor() as i64);
    let major = (k0..=k1).filter(|k| k.rem_euclid(every) == 0).map(|k| (k as f64, Label::Pow10(k))).collect();
    // 2..9 times each power as minor grid lines when the decades are wide enough to show them
    let mut minor = vec![];
    if every == 1 && decades <= 8.0 {
        for k in (lo.floor() as i64)..=(hi.ceil() as i64) {
            for m in 2..10 {
                let u = k as f64 + (m as f64).log10();
                if u > lo && u < hi {
                    minor.push(u);
                }
            }
        }
    }
    (major, minor)
}

/// The PNG bytes of the plot of `series` (lines when `joined`, else points), with log axes per `scale`.
pub(crate) fn render(series: &[Series], joined: bool, scale: Scale) -> Option<Vec<u8>> {
    let (x0, x1) = padded(range(series.iter().flat_map(|s| s.points.iter().filter(|p| p.1.is_some()).map(|p| p.0))));
    let (y0, y1) = padded(robust(series.iter().flat_map(|s| s.points.iter().filter_map(|p| p.1)).collect()));
    let mut pm = Pixmap::new(W, H)?;
    pm.fill(Color::WHITE);
    let (pw, ph) = (W as f32 - ML - MR, H as f32 - MT - MB);
    let sx = |x: f64| ML + ((x - x0) / (x1 - x0)) as f32 * pw;
    let sy = |y: f64| MT + ((y1 - y) / (y1 - y0)) as f32 * ph;
    // grid, ticks and labels
    let mut grid = Paint::default();
    grid.set_color_rgba8(230, 230, 230, 255);
    let thin = Stroke { width: 1.0, ..Stroke::default() };
    let mut faint = Paint::default();
    faint.set_color_rgba8(242, 242, 242, 255);
    let (xmajor, xminor) = axis(x0, x1, scale.xlog);
    let (ymajor, yminor) = axis(y0, y1, scale.ylog);
    for u in xminor {
        line(&mut pm, &faint, &thin, (sx(u), MT), (sx(u), MT + ph));
    }
    for u in yminor {
        line(&mut pm, &faint, &thin, (ML, sy(u)), (ML + pw, sy(u)));
    }
    for (t, l) in &xmajor {
        line(&mut pm, &grid, &thin, (sx(*t), MT), (sx(*t), MT + ph));
        draw_label(&mut pm, l, sx(*t), MT + ph + 6.0, 0.5);
    }
    for (t, l) in &ymajor {
        line(&mut pm, &grid, &thin, (ML, sy(*t)), (ML + pw, sy(*t)));
        draw_label(&mut pm, l, ML - 6.0, sy(*t) - 7.0, 1.0);
    }
    // the frame
    let mut black = Paint::default();
    black.set_color_rgba8(60, 60, 60, 255);
    let rect = PathBuilder::from_rect(tiny_skia::Rect::from_xywh(ML, MT, pw, ph)?);
    pm.stroke_path(&rect, &black, &thin, Transform::identity(), None);
    // the data, clipped to the plot area
    let mut clip = tiny_skia::Mask::new(W, H)?;
    clip.fill_path(&rect, FillRule::Winding, false, Transform::identity());
    for (k, s) in series.iter().enumerate() {
        let (r, g, b) = COLORS[k % COLORS.len()];
        let mut paint = Paint::default();
        paint.set_color_rgba8(r, g, b, 255);
        paint.anti_alias = true;
        if joined {
            let span = y1 - y0;
            let mut pb = PathBuilder::new();
            let mut prev: Option<f64> = None;
            for &(x, y) in &s.points {
                match (y, prev) {
                    // a jump larger than the visible range is a pole: break the line there
                    (Some(y), Some(p)) if (y - p).abs() <= span => pb.line_to(sx(x), sy(y)),
                    (Some(y), _) => pb.move_to(sx(x), sy(y)),
                    (None, _) => {}
                }
                prev = y;
            }
            if let Some(path) = pb.finish() {
                let st = Stroke { width: 2.0, ..Stroke::default() };
                pm.stroke_path(&path, &paint, &st, Transform::identity(), Some(&clip));
            }
        } else {
            for &(x, y) in &s.points {
                if let Some(c) = y.and_then(|y| PathBuilder::from_circle(sx(x), sy(y), 3.0)) {
                    pm.fill_path(&c, &paint, FillRule::Winding, Transform::identity(), Some(&clip));
                }
            }
        }
    }
    pm.encode_png().ok()
}

fn line(pm: &mut Pixmap, paint: &Paint, st: &Stroke, a: (f32, f32), b: (f32, f32)) {
    let mut pb = PathBuilder::new();
    pb.move_to(a.0, a.1);
    pb.line_to(b.0, b.1);
    if let Some(p) = pb.finish() {
        pm.stroke_path(&p, paint, st, Transform::identity(), None);
    }
}

/// The smallest and largest value (0 and 1 for none).
fn range(v: impl Iterator<Item = f64>) -> (f64, f64) {
    let (lo, hi) = v.fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), x| (a.min(x), b.max(x)));
    if lo.is_finite() {
        (lo, hi)
    } else {
        (0.0, 1.0)
    }
}

/// The 1%..99% quantile range, or the full range when no value lies more than half that range outside it.
pub(crate) fn robust(mut v: Vec<f64>) -> (f64, f64) {
    if v.len() < 10 {
        return range(v.into_iter());
    }
    v.sort_by(f64::total_cmp);
    let q = |p: f64| v[((v.len() - 1) as f64 * p) as usize];
    let (lo, hi) = (q(0.01), q(0.99));
    let w = (hi - lo).max(f64::MIN_POSITIVE);
    let (min, max) = (v[0], v[v.len() - 1]);
    (if min >= lo - w / 2.0 { min } else { lo - w / 4.0 }, if max <= hi + w / 2.0 { max } else { hi + w / 4.0 })
}

/// A range widened by 4% (and to a unit around a single value).
fn padded((lo, hi): (f64, f64)) -> (f64, f64) {
    if hi - lo <= f64::EPSILON * lo.abs().max(1.0) {
        return (lo - 1.0, hi + 1.0);
    }
    let p = (hi - lo) * 0.04;
    (lo - p, hi + p)
}

/// Nice tick step for [lo, hi]: 1, 2 or 5 times a power of ten, about 6 ticks.
fn step(lo: f64, hi: f64) -> f64 {
    let raw = (hi - lo) / 6.0;
    let mag = 10f64.powf(raw.log10().floor());
    let n = raw / mag;
    mag * if n < 1.5 { 1.0 } else if n < 3.5 { 2.0 } else if n < 7.5 { 5.0 } else { 10.0 }
}

fn ticks(lo: f64, hi: f64) -> Vec<f64> {
    let s = step(lo, hi);
    let mut t = (lo / s).ceil() * s;
    let mut v = vec![];
    while t <= hi && v.len() < 20 {
        v.push(t);
        t += s;
    }
    v
}

/// A tick label with as many decimals as the step needs (no float noise, no `-0`).
fn label(t: f64, lo: f64, hi: f64) -> String {
    let s = step(lo, hi);
    let d = (-s.log10().floor()).max(0.0) as usize;
    let r = format!("{:.*}", d, t);
    if r.trim_start_matches('-').chars().all(|c| c == '0' || c == '.') {
        format!("{:.*}", d, 0.0)
    } else {
        r
    }
}

/// A system font for labels (none: labels are left out).
fn font() -> Option<&'static FontRef<'static>> {
    static FONT: OnceLock<Option<FontRef<'static>>> = OnceLock::new();
    FONT.get_or_init(|| {
        let dirs = [
            std::env::var("WINDIR").map(|w| format!("{w}/Fonts")).unwrap_or_default(),
            "/usr/share/fonts/truetype/dejavu".into(),
            "/System/Library/Fonts/Supplemental".into(),
        ];
        for d in dirs {
            for f in ["arial.ttf", "segoeui.ttf", "DejaVuSans.ttf", "Arial.ttf"] {
                if let Ok(bytes) = std::fs::read(format!("{d}/{f}")) {
                    let bytes: &'static [u8] = Box::leak(bytes.into_boxed_slice());
                    if let Ok(f) = FontRef::try_from_slice(bytes) {
                        return Some(f);
                    }
                }
            }
        }
        None
    })
    .as_ref()
}

/// The width of `s` at `size` pixels.
fn text_width(s: &str, size: f32) -> f32 {
    let Some(font) = font() else { return 0.0 };
    let sf = font.as_scaled(PxScale::from(size));
    s.chars().map(|c| sf.h_advance(font.glyph_id(c))).sum()
}

/// A tick label with its top at `y`, aligned at `x`; powers of ten as "10" with a raised, smaller exponent.
fn draw_label(pm: &mut Pixmap, l: &Label, x: f32, y: f32, ax: f32) {
    match l {
        Label::Text(s) => text(pm, s, x, y, ax, 13.0),
        Label::Pow10(k) => {
            let e = if *k < 0 { format!("\u{2212}{}", -k) } else { k.to_string() };
            let (wb, we) = (text_width("10", 13.0), text_width(&e, 10.0));
            let left = x - ax * (wb + we);
            text(pm, "10", left, y + 2.0, 0.0, 13.0);
            text(pm, &e, left + wb, y - 3.0, 0.0, 10.0);
        }
    }
}

/// Draw `s` at `size` pixels with its top at `y`, aligned at `x` (`ax`: 0 left, 0.5 centre, 1 right).
fn text(pm: &mut Pixmap, s: &str, x: f32, y: f32, ax: f32, size: f32) {
    let Some(font) = font() else { return };
    let scale = PxScale::from(size);
    let sf = font.as_scaled(scale);
    let width: f32 = s.chars().map(|c| sf.h_advance(font.glyph_id(c))).sum();
    let mut pen = x - ax * width;
    let base = y + sf.ascent();
    let (w, h) = (pm.width() as i32, pm.height() as i32);
    for c in s.chars() {
        let id = font.glyph_id(c);
        let g = id.with_scale_and_position(scale, ab_glyph::point(pen, base));
        pen += sf.h_advance(id);
        let Some(og) = font.outline_glyph(g) else { continue };
        let bb = og.px_bounds();
        let pixels = pm.pixels_mut();
        og.draw(|gx, gy, cov| {
            let (px, py) = (bb.min.x as i32 + gx as i32, bb.min.y as i32 + gy as i32);
            if px < 0 || py < 0 || px >= w || py >= h {
                return;
            }
            let p = &mut pixels[(py * w + px) as usize];
            let k = 1.0 - cov.clamp(0.0, 1.0) * 0.85;
            let f = |v: u8| (v as f32 * k) as u8;
            *p = tiny_skia::PremultipliedColorU8::from_rgba(f(p.red()), f(p.green()), f(p.blue()), 255).unwrap_or(*p);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_have_no_float_noise() {
        assert_eq!(label(0.30000000000000004, 0.0, 1.0), "0.3");
        assert_eq!(label(-0.0000000001, -1.0, 1.0), "0.0");
        assert_eq!(label(200.0, 0.0, 1000.0), "200");
        assert_eq!(ticks(0.0, 1.0), vec![0.0, 0.2, 0.4, 0.6000000000000001, 0.8, 1.0]);
    }

    #[test]
    fn the_robust_range_clips_poles() {
        let mut v: Vec<f64> = (1..=1000).map(|k| k as f64 / 1000.0).collect();
        v.push(1e12);
        let (lo, hi) = robust(v);
        assert!(lo < 0.02 && hi < 2.0, "{lo} {hi}");
    }
}
