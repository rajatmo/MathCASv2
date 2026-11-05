//! Drawing a plot: a frame with nice-number ticks and labels, curves or points. One drawing routine ([`draw`])
//! paints onto a [`Canvas`]; two canvases turn it into PNG bytes (rasterised with tiny-skia) and into a one-page
//! vector PDF (lines, markers and Helvetica text; no fonts embedded).
use ab_glyph::{Font, FontRef, PxScale, ScaleFont};
use std::fmt::Write as _;
use std::sync::OnceLock;
use tiny_skia::{Color, FillRule, Paint, PathBuilder, Pixmap, Stroke, Transform};

/// Points of one curve or data set, already on the axes ([`Axis::to`]); `None` breaks the line.
pub(crate) struct Series {
    pub(crate) points: Vec<(f64, Option<f64>)>,
}

const W: u32 = 640;
const H: u32 = 400;
/// Plot area: left, top, right, bottom margins in pixels (PDF points).
const ML: f32 = 64.0;
const MT: f32 = 16.0;
const MR: f32 = 16.0;
const MB: f32 = 40.0;
/// The default colour cycle: modern earth tones with strong contrast on white, ordered so neighbouring curves
/// differ in both hue and lightness: deep teal, terracotta, ochre, aubergine, olive, slate indigo, espresso.
const COLORS: [Rgb; 7] = [
    (0x1B, 0x5E, 0x63),
    (0xC1, 0x50, 0x2E),
    (0xC9, 0x9A, 0x2E),
    (0x5B, 0x2C, 0x4F),
    (0x6B, 0x7A, 0x2F),
    (0x34, 0x50, 0x6E),
    (0x3B, 0x2A, 0x22),
];
const GRID: Rgb = (230, 230, 230);
const FAINT: Rgb = (242, 242, 242);
const FRAME: Rgb = (60, 60, 60);
/// Label text: 85% black, as the raster text blends it.
const TEXT: Rgb = (38, 38, 38);

type Rgb = (u8, u8, u8);

/// The scale of one axis. Coordinates on the axis are the transformed values ([`Axis::to`]).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) enum Axis {
    #[default]
    Linear,
    /// log10 of the value; values that are not positive have no place.
    Log,
    /// sign(v) log10(1 + |v|/c): logarithmic for |v| much larger than c, linear near 0, both signs (c > 0).
    SignedLog(f64),
}

impl Axis {
    /// The value `v` on the axis (`None` where it has no place).
    pub(crate) fn to(self, v: f64) -> Option<f64> {
        match self {
            Axis::Linear => Some(v),
            Axis::Log => (v > 0.0).then(|| v.log10()),
            Axis::SignedLog(c) => {
                Some(v.signum() * (v.abs() / c).ln_1p() / std::f64::consts::LN_10)
            }
        }
    }

    /// The value at axis coordinate `u` (the inverse of [`Axis::to`]).
    pub(crate) fn from(self, u: f64) -> f64 {
        match self {
            Axis::Linear => u,
            Axis::Log => 10f64.powf(u),
            Axis::SignedLog(c) => u.signum() * c * (u.abs() * std::f64::consts::LN_10).exp_m1(),
        }
    }
}

/// The scales of both axes.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Scale {
    pub(crate) x: Axis,
    pub(crate) y: Axis,
}

/// A tick label: plain text, or ±10 to an integer power (drawn with a superscript).
#[derive(Debug, PartialEq)]
enum Label {
    Text(String),
    Pow10 { neg: bool, k: i64 },
}

/// Major ticks (position, label) and minor grid positions of the axis [lo, hi] (axis coordinates).
fn axis(lo: f64, hi: f64, ax: Axis) -> (Vec<(f64, Label)>, Vec<f64>) {
    match ax {
        Axis::Linear => (
            ticks(lo, hi)
                .into_iter()
                .map(|t| (t, Label::Text(label(t, lo, hi))))
                .collect(),
            vec![],
        ),
        Axis::Log => log_axis(lo, hi),
        Axis::SignedLog(c) => signed_log_axis(lo, hi, c),
    }
}

fn log_axis(lo: f64, hi: f64) -> (Vec<(f64, Label)>, Vec<f64>) {
    let decades = hi - lo;
    if decades < 1.2 {
        // under about a decade: nice ticks on the values themselves, placed at their logarithms
        let (a, b) = (10f64.powf(lo), 10f64.powf(hi));
        let t = ticks(a, b)
            .into_iter()
            .filter(|v| *v > 0.0)
            .map(|v| (v.log10(), Label::Text(label(v, a, b))));
        return (t.collect(), vec![]);
    }
    // powers of ten, every `every` decades so that at most about 8 are labelled
    let every = (decades / 8.0).ceil().max(1.0) as i64;
    let (k0, k1) = (lo.ceil() as i64, hi.floor() as i64);
    let major = (k0..=k1)
        .filter(|k| k.rem_euclid(every) == 0)
        .map(|k| (k as f64, Label::Pow10 { neg: false, k }))
        .collect();
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

/// Ticks of a signed-log axis: 0 and ±10^k for powers at least c (smaller ones crowd into the linear part around 0),
/// every `every` decades so that at most about 9 are labelled.
fn signed_log_axis(lo: f64, hi: f64, c: f64) -> (Vec<(f64, Label)>, Vec<f64>) {
    let ax = Axis::SignedLog(c);
    let top = ax.from(lo).abs().max(ax.from(hi).abs());
    let (k0, k1) = (c.log10().ceil() as i64, top.log10().floor() as i64);
    let mut cand: Vec<(f64, bool, i64)> = vec![];
    for k in k0..=k1.max(k0) {
        for neg in [false, true] {
            let v = if neg {
                -(10f64.powi(k as i32))
            } else {
                10f64.powi(k as i32)
            };
            if let Some(u) = ax.to(v).filter(|u| (lo..=hi).contains(u)) {
                cand.push((u, neg, k));
            }
        }
    }
    let every = (cand.len() as f64 / 8.0).ceil().max(1.0) as i64;
    let mut major: Vec<(f64, Label)> = cand
        .into_iter()
        .filter(|&(_, _, k)| (k - k1).rem_euclid(every) == 0)
        .map(|(u, neg, k)| (u, Label::Pow10 { neg, k }))
        .collect();
    if (lo..=hi).contains(&0.0) {
        major.push((0.0, Label::Text("0".into())));
    }
    major.sort_by(|a, b| a.0.total_cmp(&b.0));
    (major, vec![])
}

/// Where a plot is painted: the drawing operations [`draw`] needs, in pixel coordinates (y down).
trait Canvas {
    /// A straight line.
    fn line(&mut self, a: (f32, f32), b: (f32, f32), color: Rgb, width: f32);
    /// A rectangle's outline.
    fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: Rgb, width: f32);
    /// Polylines (each a run of points), clipped to the rectangle `clip` (x, y, w, h).
    fn polylines(
        &mut self,
        runs: &[Vec<(f32, f32)>],
        color: Rgb,
        width: f32,
        clip: (f32, f32, f32, f32),
    );
    /// Filled discs of radius `r`, clipped to `clip`.
    fn dots(&mut self, centres: &[(f32, f32)], r: f32, color: Rgb, clip: (f32, f32, f32, f32));
    /// Text at `size` pixels with its top at `y`, aligned at `x` (`ax`: 0 left, 0.5 centre, 1 right).
    fn text(&mut self, s: &str, x: f32, y: f32, ax: f32, size: f32);
}

/// Paint the plot of `series` (lines when `joined`, else points) with the axes of `scale`.
fn draw(c: &mut dyn Canvas, series: &[Series], joined: bool, scale: Scale) {
    let (x0, x1) =
        padded(range(series.iter().flat_map(|s| {
            s.points.iter().filter(|p| p.1.is_some()).map(|p| p.0)
        })));
    let (y0, y1) = padded(robust(
        series
            .iter()
            .flat_map(|s| s.points.iter().filter_map(|p| p.1))
            .collect(),
    ));
    let (pw, ph) = (W as f32 - ML - MR, H as f32 - MT - MB);
    let sx = |x: f64| ML + ((x - x0) / (x1 - x0)) as f32 * pw;
    let sy = |y: f64| MT + ((y1 - y) / (y1 - y0)) as f32 * ph;
    // grid, ticks and labels
    let (xmajor, xminor) = axis(x0, x1, scale.x);
    let (ymajor, yminor) = axis(y0, y1, scale.y);
    for u in xminor {
        c.line((sx(u), MT), (sx(u), MT + ph), FAINT, 1.0);
    }
    for u in yminor {
        c.line((ML, sy(u)), (ML + pw, sy(u)), FAINT, 1.0);
    }
    for (t, l) in &xmajor {
        c.line((sx(*t), MT), (sx(*t), MT + ph), GRID, 1.0);
        draw_label(c, l, sx(*t), MT + ph + 6.0, 0.5);
    }
    for (t, l) in &ymajor {
        c.line((ML, sy(*t)), (ML + pw, sy(*t)), GRID, 1.0);
        draw_label(c, l, ML - 6.0, sy(*t) - 7.0, 1.0);
    }
    c.rect(ML, MT, pw, ph, FRAME, 1.0);
    // the data, clipped to the plot area
    let clip = (ML, MT, pw, ph);
    for (k, s) in series.iter().enumerate() {
        let color = COLORS[k % COLORS.len()];
        if joined {
            let span = y1 - y0;
            let mut runs: Vec<Vec<(f32, f32)>> = vec![];
            let mut prev: Option<f64> = None;
            for &(x, y) in &s.points {
                match (y, prev) {
                    // a jump larger than the visible range is a pole: break the line there
                    (Some(y), Some(p)) if (y - p).abs() <= span => {
                        if let Some(r) = runs.last_mut() {
                            r.push((sx(x), sy(y)));
                        }
                    }
                    (Some(y), _) => runs.push(vec![(sx(x), sy(y))]),
                    (None, _) => {}
                }
                prev = y;
            }
            runs.retain(|r| r.len() > 1);
            c.polylines(&runs, color, 2.0, clip);
        } else {
            let pts: Vec<(f32, f32)> = s
                .points
                .iter()
                .filter_map(|&(x, y)| y.map(|y| (sx(x), sy(y))))
                .collect();
            c.dots(&pts, 3.0, color, clip);
        }
    }
}

/// A tick label with its top at `y`, aligned at `x`; powers of ten as "10" with a raised, smaller exponent.
fn draw_label(c: &mut dyn Canvas, l: &Label, x: f32, y: f32, ax: f32) {
    match l {
        Label::Text(s) => c.text(s, x, y, ax, 13.0),
        Label::Pow10 { neg, k } => {
            let base = if *neg { "\u{2212}10" } else { "10" };
            let e = if *k < 0 {
                format!("\u{2212}{}", -k)
            } else {
                k.to_string()
            };
            let (wb, we) = (text_width(base, 13.0), text_width(&e, 10.0));
            let left = x - ax * (wb + we);
            c.text(base, left, y + 2.0, 0.0, 13.0);
            c.text(&e, left + wb, y - 3.0, 0.0, 10.0);
        }
    }
}

/// The PNG bytes of the plot of `series` (lines when `joined`, else points), with the axes of `scale`.
pub(crate) fn render(series: &[Series], joined: bool, scale: Scale) -> Option<Vec<u8>> {
    let mut pm = Pixmap::new(W, H)?;
    pm.fill(Color::WHITE);
    draw(&mut Raster(&mut pm), series, joined, scale);
    pm.encode_png().ok()
}

/// The bytes of a one-page vector PDF of the same plot (page size W x H points).
pub(crate) fn render_pdf(series: &[Series], joined: bool, scale: Scale) -> Vec<u8> {
    let mut v = Vector(String::new());
    v.0.push_str("1 j 1 J\n");
    draw(&mut v, series, joined, scale);
    pdf_document(&v.0)
}

fn paint(color: Rgb) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color_rgba8(color.0, color.1, color.2, 255);
    p.anti_alias = true;
    p
}

/// The raster canvas.
struct Raster<'a>(&'a mut Pixmap);

impl Raster<'_> {
    fn mask(&self, clip: (f32, f32, f32, f32)) -> Option<tiny_skia::Mask> {
        let rect =
            PathBuilder::from_rect(tiny_skia::Rect::from_xywh(clip.0, clip.1, clip.2, clip.3)?);
        let mut m = tiny_skia::Mask::new(self.0.width(), self.0.height())?;
        m.fill_path(&rect, FillRule::Winding, false, Transform::identity());
        Some(m)
    }
}

impl Canvas for Raster<'_> {
    fn line(&mut self, a: (f32, f32), b: (f32, f32), color: Rgb, width: f32) {
        let mut pb = PathBuilder::new();
        pb.move_to(a.0, a.1);
        pb.line_to(b.0, b.1);
        if let Some(p) = pb.finish() {
            let mut pt = paint(color);
            pt.anti_alias = false;
            self.0.stroke_path(
                &p,
                &pt,
                &Stroke {
                    width,
                    ..Stroke::default()
                },
                Transform::identity(),
                None,
            );
        }
    }

    fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: Rgb, width: f32) {
        if let Some(r) = tiny_skia::Rect::from_xywh(x, y, w, h) {
            let p = PathBuilder::from_rect(r);
            let mut pt = paint(color);
            pt.anti_alias = false;
            self.0.stroke_path(
                &p,
                &pt,
                &Stroke {
                    width,
                    ..Stroke::default()
                },
                Transform::identity(),
                None,
            );
        }
    }

    fn polylines(
        &mut self,
        runs: &[Vec<(f32, f32)>],
        color: Rgb,
        width: f32,
        clip: (f32, f32, f32, f32),
    ) {
        let mask = self.mask(clip);
        let mut pb = PathBuilder::new();
        for r in runs {
            pb.move_to(r[0].0, r[0].1);
            for p in &r[1..] {
                pb.line_to(p.0, p.1);
            }
        }
        if let Some(path) = pb.finish() {
            let st = Stroke {
                width,
                ..Stroke::default()
            };
            self.0.stroke_path(
                &path,
                &paint(color),
                &st,
                Transform::identity(),
                mask.as_ref(),
            );
        }
    }

    fn dots(&mut self, centres: &[(f32, f32)], r: f32, color: Rgb, clip: (f32, f32, f32, f32)) {
        let mask = self.mask(clip);
        let pt = paint(color);
        for &(x, y) in centres {
            if let Some(c) = PathBuilder::from_circle(x, y, r) {
                self.0.fill_path(
                    &c,
                    &pt,
                    FillRule::Winding,
                    Transform::identity(),
                    mask.as_ref(),
                );
            }
        }
    }

    fn text(&mut self, s: &str, x: f32, y: f32, ax: f32, size: f32) {
        raster_text(self.0, s, x, y, ax, size);
    }
}

/// The vector canvas: a PDF content stream (y flipped: PDF's origin is the bottom left).
struct Vector(String);

impl Vector {
    fn color(&mut self, c: Rgb, op: &str) {
        let f = |v: u8| v as f32 / 255.0;
        let _ = writeln!(self.0, "{:.3} {:.3} {:.3} {op}", f(c.0), f(c.1), f(c.2));
    }

    fn clip(&mut self, clip: (f32, f32, f32, f32)) {
        let _ = writeln!(
            self.0,
            "{:.2} {:.2} {:.2} {:.2} re W n",
            clip.0,
            fy(clip.1 + clip.3),
            clip.2,
            clip.3
        );
    }
}

/// A y coordinate in PDF space.
fn fy(y: f32) -> f32 {
    H as f32 - y
}

impl Canvas for Vector {
    fn line(&mut self, a: (f32, f32), b: (f32, f32), color: Rgb, width: f32) {
        self.color(color, "RG");
        let _ = writeln!(
            self.0,
            "{width:.2} w {:.2} {:.2} m {:.2} {:.2} l S",
            a.0,
            fy(a.1),
            b.0,
            fy(b.1)
        );
    }

    fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: Rgb, width: f32) {
        self.color(color, "RG");
        let _ = writeln!(
            self.0,
            "{width:.2} w {x:.2} {:.2} {w:.2} {h:.2} re S",
            fy(y + h)
        );
    }

    fn polylines(
        &mut self,
        runs: &[Vec<(f32, f32)>],
        color: Rgb,
        width: f32,
        clip: (f32, f32, f32, f32),
    ) {
        self.0.push_str("q\n");
        self.clip(clip);
        self.color(color, "RG");
        let _ = writeln!(self.0, "{width:.2} w");
        for r in runs {
            let _ = writeln!(self.0, "{:.2} {:.2} m", r[0].0, fy(r[0].1));
            for p in &r[1..] {
                let _ = writeln!(self.0, "{:.2} {:.2} l", p.0, fy(p.1));
            }
            self.0.push_str("S\n");
        }
        self.0.push_str("Q\n");
    }

    fn dots(&mut self, centres: &[(f32, f32)], r: f32, color: Rgb, clip: (f32, f32, f32, f32)) {
        self.0.push_str("q\n");
        self.clip(clip);
        self.color(color, "rg");
        // a circle from four Bezier quarters
        let k = 0.552_284_8 * r;
        for &(x, y) in centres {
            let y = fy(y);
            let _ = writeln!(
                self.0,
                "{:.2} {:.2} m {:.2} {:.2} {:.2} {:.2} {:.2} {:.2} c {:.2} {:.2} {:.2} {:.2} {:.2} {:.2} c \
                 {:.2} {:.2} {:.2} {:.2} {:.2} {:.2} c {:.2} {:.2} {:.2} {:.2} {:.2} {:.2} c f",
                x + r, y,
                x + r, y + k, x + k, y + r, x, y + r,
                x - k, y + r, x - r, y + k, x - r, y,
                x - r, y - k, x - k, y - r, x, y - r,
                x + k, y - r, x + r, y - k, x + r, y
            );
        }
        self.0.push_str("Q\n");
    }

    fn text(&mut self, s: &str, x: f32, y: f32, ax: f32, size: f32) {
        // Helvetica's WinAnsi encoding has no minus sign: a hyphen stands in
        let s = s.replace('\u{2212}', "-");
        let left = x - ax * text_width(&s, size);
        let base = y + ascent(size);
        let esc: String = s
            .chars()
            .flat_map(|c| match c {
                '(' | ')' | '\\' => vec!['\\', c],
                c if c.is_ascii() => vec![c],
                _ => vec!['?'],
            })
            .collect();
        self.color(TEXT, "rg");
        let _ = writeln!(
            self.0,
            "BT /F1 {size:.1} Tf {left:.2} {:.2} Td ({esc}) Tj ET",
            fy(base)
        );
    }
}

/// A one-page PDF around `content`, with Helvetica as font `F1`.
fn pdf_document(content: &str) -> Vec<u8> {
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {W} {H}] /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>"
        ),
        format!("<< /Length {} >>\nstream\n{content}endstream", content.len()),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>".to_string(),
    ];
    let mut out = String::from("%PDF-1.4\n");
    let mut offsets = vec![];
    for (i, o) in objects.iter().enumerate() {
        offsets.push(out.len());
        let _ = write!(out, "{} 0 obj\n{o}\nendobj\n", i + 1);
    }
    let xref = out.len();
    let _ = write!(out, "xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1);
    for o in offsets {
        let _ = writeln!(out, "{o:010} 00000 n ");
    }
    let _ = write!(
        out,
        "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
        objects.len() + 1
    );
    out.into_bytes()
}

/// The smallest and largest value (0 and 1 for none).
fn range(v: impl Iterator<Item = f64>) -> (f64, f64) {
    let (lo, hi) = v.fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), x| {
        (a.min(x), b.max(x))
    });
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
    (
        if min >= lo - w / 2.0 {
            min
        } else {
            lo - w / 4.0
        },
        if max <= hi + w / 2.0 {
            max
        } else {
            hi + w / 4.0
        },
    )
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
    mag * if n < 1.5 {
        1.0
    } else if n < 3.5 {
        2.0
    } else if n < 7.5 {
        5.0
    } else {
        10.0
    }
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
    if r.trim_start_matches('-')
        .chars()
        .all(|c| c == '0' || c == '.')
    {
        format!("{:.*}", d, 0.0)
    } else {
        r
    }
}

/// A system font for labels (none: raster labels are left out; PDF labels use Helvetica with estimated widths).
fn font() -> Option<&'static FontRef<'static>> {
    static FONT: OnceLock<Option<FontRef<'static>>> = OnceLock::new();
    FONT.get_or_init(|| {
        let dirs = [
            std::env::var("WINDIR")
                .map(|w| format!("{w}/Fonts"))
                .unwrap_or_default(),
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

/// The width of `s` at `size` pixels (the system font's advances; Arial's equal Helvetica's).
fn text_width(s: &str, size: f32) -> f32 {
    let Some(font) = font() else {
        return 0.55 * size * s.chars().count() as f32;
    };
    let sf = font.as_scaled(PxScale::from(size));
    s.chars().map(|c| sf.h_advance(font.glyph_id(c))).sum()
}

/// The ascent at `size` pixels.
fn ascent(size: f32) -> f32 {
    match font() {
        Some(font) => font.as_scaled(PxScale::from(size)).ascent(),
        None => 0.9 * size,
    }
}

/// Draw `s` at `size` pixels with its top at `y`, aligned at `x` (`ax`: 0 left, 0.5 centre, 1 right).
fn raster_text(pm: &mut Pixmap, s: &str, x: f32, y: f32, ax: f32, size: f32) {
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
        let Some(og) = font.outline_glyph(g) else {
            continue;
        };
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
            *p = tiny_skia::PremultipliedColorU8::from_rgba(
                f(p.red()),
                f(p.green()),
                f(p.blue()),
                255,
            )
            .unwrap_or(*p);
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
        assert_eq!(
            ticks(0.0, 1.0),
            vec![0.0, 0.2, 0.4, 0.6000000000000001, 0.8, 1.0]
        );
    }

    #[test]
    fn the_robust_range_clips_poles() {
        let mut v: Vec<f64> = (1..=1000).map(|k| k as f64 / 1000.0).collect();
        v.push(1e12);
        let (lo, hi) = robust(v);
        assert!(lo < 0.02 && hi < 2.0, "{lo} {hi}");
    }

    #[test]
    fn signed_log_round_trips_and_is_odd() {
        let ax = Axis::SignedLog(1e-8);
        for v in [-1e10, -3.5, -1e-9, 0.0, 2e-8, 7.0, 1e10] {
            let u = ax.to(v).unwrap();
            assert!(
                (ax.from(u) - v).abs() <= 1e-12 * v.abs().max(1e-8),
                "{v} -> {u} -> {}",
                ax.from(u)
            );
            assert_eq!(ax.to(-v).unwrap(), -u);
        }
        assert_eq!(Axis::Log.to(-1.0), None);
    }

    #[test]
    fn signed_log_ticks_cover_both_signs_and_zero() {
        let ax = Axis::SignedLog(1e-8);
        let (lo, hi) = (ax.to(-1e10).unwrap(), ax.to(1e10).unwrap());
        let (major, _) = axis(lo, hi, ax);
        assert!(major
            .iter()
            .any(|(u, l)| *u == 0.0 && *l == Label::Text("0".into())));
        assert!(major
            .iter()
            .any(|(_, l)| *l == Label::Pow10 { neg: true, k: 10 }));
        assert!(major
            .iter()
            .any(|(_, l)| *l == Label::Pow10 { neg: false, k: 10 }));
        assert!(major.len() <= 11, "{}", major.len());
        assert!(major.windows(2).all(|w| w[0].0 < w[1].0));
    }

    #[test]
    fn the_pdf_is_well_formed() {
        let s = Series {
            points: (0..50)
                .map(|k| (k as f64, Some((k as f64).sin())))
                .collect(),
        };
        let pdf = render_pdf(&[s], true, Scale::default());
        let text = String::from_utf8(pdf).unwrap();
        assert!(text.starts_with("%PDF-1.4") && text.ends_with("%%EOF\n"));
        // the xref offsets point at the objects
        let xref = text.find("\nxref\n").unwrap() + 1;
        let start: usize = text
            .split("startxref\n")
            .nth(1)
            .unwrap()
            .lines()
            .next()
            .unwrap()
            .parse()
            .unwrap();
        assert_eq!(start, xref);
        for (i, line) in text[xref..].lines().skip(3).take(5).enumerate() {
            let off: usize = line[..10].parse().unwrap();
            assert!(
                text[off..].starts_with(&format!("{} 0 obj", i + 1)),
                "object {}",
                i + 1
            );
        }
        // the stream length is right
        let len: usize = text
            .split("/Length ")
            .nth(1)
            .unwrap()
            .split(' ')
            .next()
            .unwrap()
            .parse()
            .unwrap();
        let open = ">>\nstream\n";
        let body = &text[text.find(open).unwrap() + open.len()..];
        assert_eq!(body.find("endstream").unwrap(), len);
        assert!(text.contains(" l\n") && text.contains("Tj ET"));
    }
}
