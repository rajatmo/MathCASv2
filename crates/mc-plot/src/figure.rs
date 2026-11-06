//! A plot as a `graphics` term: the series as lines or points in axis coordinates (logarithms on log axes), a
//! frame with ticks (explicit ones with `10^k` labels on logarithmic axes), grid lines, axis labels and a legend.
//! `mc-graphics` renders it.
use mc_core::{Heap, Store, T};
use mc_graphics::color::Rgba;
use mc_graphics::gradient::Gradient;
use mc_graphics::rgb_term;

/// Points of one curve or data set, already on the axes ([`Axis::to`]); `None` breaks the line.
pub(crate) struct Series {
    pub(crate) points: Vec<(f64, Option<f64>)>,
    /// what the data sidecar calls it when the legend does not: the plotted expression, or `series k`
    pub(crate) name: String,
}

/// The page size in points (pixels of the PNG).
pub(crate) const W: f64 = 640.0;
pub(crate) const H: f64 = 400.0;
/// Text size of tick labels, axis labels and legend entries.
const LABEL_SIZE: f64 = 13.0;
/// Line width of curves.
const CURVE: f64 = 2.0;
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
/// Grid lines at major ticks, and the fainter ones between powers of ten.
const GRID: f64 = 230.0 / 255.0;
/// The opacity of filled regions under curves.
const FILL_OPACITY: f64 = 0.22;
const FAINT: f64 = 242.0 / 255.0;

pub(crate) type Rgb = (u8, u8, u8);

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
            Axis::SignedLog(c) => Some(v.signum() * (v.abs() / c).ln_1p() / std::f64::consts::LN_10),
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

/// A corner of the plot area (where the legend goes).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) enum Corner {
    #[default]
    TopRight,
    TopLeft,
    BottomLeft,
    BottomRight,
}

impl Corner {
    fn name(self) -> &'static str {
        match self {
            Corner::TopRight => "top_right",
            Corner::TopLeft => "top_left",
            Corner::BottomLeft => "bottom_left",
            Corner::BottomRight => "bottom_right",
        }
    }
}

/// How a curve is stroked.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) enum Dash {
    #[default]
    Solid,
    Dashed,
    Dotted,
    DotDashed,
}

impl Dash {
    /// The dash pattern (on, off, ...) in points for a line of width 2; empty for a solid line.
    fn pattern(self) -> &'static [f64] {
        match self {
            Dash::Solid => &[],
            Dash::Dashed => &[8.0, 5.0],
            Dash::Dotted => &[0.5, 4.5],
            Dash::DotDashed => &[8.0, 4.0, 0.5, 4.0],
        }
    }
}

/// Everything about a plot's appearance besides its data: axis scales, lines or points, axis labels, the legend
/// (one entry per series, in order; series beyond the entries have none), and per series a colour and a dash
/// pattern (series beyond the lists take the default colour cycle and solid lines).
#[derive(Clone, Debug, Default)]
pub(crate) struct Look {
    pub(crate) scale: Scale,
    /// lines (true) or points
    pub(crate) joined: bool,
    pub(crate) x_label: Option<String>,
    pub(crate) y_label: Option<String>,
    pub(crate) legend: Vec<String>,
    pub(crate) legend_at: Corner,
    pub(crate) colors: Vec<Rgb>,
    pub(crate) dashes: Vec<Dash>,
    /// the plot range's ends given by `plot_range:`, in data coordinates (`None`: automatic)
    pub(crate) range: [[Option<f64>; 2]; 2],
    /// what is filled under (or between) curves
    pub(crate) filling: Option<Filling>,
    /// `color_function:` of density and contour plots (`None`: the default gradient)
    pub(crate) gradient: Option<Gradient>,
    /// `color_bar: false`
    pub(crate) hide_color_bar: bool,
    /// the values at the ends of the colour bar, for plots coloured by value
    pub(crate) bar_range: Option<(f64, f64)>,
    /// legend entries drawn as filled squares of these colours (bar charts, histograms) rather than curve samples
    pub(crate) legend_swatches: Vec<Rgb>,
    /// the ranges as computed, without padding (rasters fill the plot area)
    pub(crate) exact: bool,
    /// equal units on both axes
    pub(crate) equal: bool,
    /// axes through the origin instead of a frame
    pub(crate) origin_axes: bool,
    /// the page size in points (`None`: 640 x 400)
    pub(crate) size: Option<(f64, f64)>,
    /// labelled x ticks (bar charts), with no x grid lines
    pub(crate) x_ticks: Option<Vec<(f64, String)>>,
    /// `plot_points:` of density and contour plots
    pub(crate) plot_points: Option<usize>,
    /// `contours:` of contour plots
    pub(crate) contours: Option<Contours>,
    /// `data_range:` of list density plots
    pub(crate) data_range: Option<[[f64; 2]; 2]>,
    /// `bins:` and `normalized:` of histograms
    pub(crate) bins: Bins,
    pub(crate) normalized: bool,
    /// `labels:` of bar charts
    pub(crate) labels: Vec<String>,
}

/// The levels of a contour plot: so many evenly between the least and greatest value, or these.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Contours {
    Count(usize),
    Values(Vec<f64>),
}

/// The bins of a histogram: automatic, about so many, of this width, or from lo to hi of width w.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) enum Bins {
    #[default]
    Auto,
    Count(usize),
    Width(f64),
    Range(f64, f64, f64),
}

/// What `filling:` fills: from each curve to the x axis (y = 0), the bottom or top of the plot, a value, or
/// between two curves (0-based).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Filling {
    Axis,
    Bottom,
    Top,
    Value(f64),
    Between(usize, usize),
}

impl Look {
    /// The colour of series `k`.
    pub(crate) fn color(&self, k: usize) -> Rgb {
        self.colors.get(k).copied().unwrap_or(COLORS[k % COLORS.len()])
    }

    /// The dash pattern of series `k`.
    fn dash(&self, k: usize) -> Dash {
        self.dashes.get(k).copied().unwrap_or_default()
    }
}

/// Major ticks with labels and minor grid positions of an axis.
type AxisTicks = (Vec<(f64, Label)>, Vec<f64>);

/// A tick label: plain text, or ±10 to an integer power (drawn with a superscript).
#[derive(Debug, PartialEq)]
enum Label {
    Text(String),
    Pow10 { neg: bool, k: i64 },
}

/// Major ticks (position, label) and minor grid positions of a logarithmic or signed-logarithmic axis [lo, hi]
/// (axis coordinates); `None` for a linear axis (its ticks are automatic).
fn axis(lo: f64, hi: f64, ax: Axis) -> Option<AxisTicks> {
    match ax {
        Axis::Linear => None,
        Axis::Log => Some(log_axis(lo, hi)),
        Axis::SignedLog(c) => Some(signed_log_axis(lo, hi, c)),
    }
}

fn log_axis(lo: f64, hi: f64) -> AxisTicks {
    let decades = hi - lo;
    if decades < 1.2 {
        // under about a decade: nice ticks on the values themselves, placed at their logarithms
        let (a, b) = (10f64.powf(lo), 10f64.powf(hi));
        let s = mc_graphics::ticks::step(a, b, 6);
        let t = mc_graphics::ticks::positions(a, b, 6)
            .into_iter()
            .filter(|v| *v > 0.0)
            .map(|v| (v.log10(), Label::Text(mc_graphics::ticks::decimal(v, s))));
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
fn signed_log_axis(lo: f64, hi: f64, c: f64) -> AxisTicks {
    let ax = Axis::SignedLog(c);
    let top = ax.from(lo).abs().max(ax.from(hi).abs());
    let (k0, k1) = (c.log10().ceil() as i64, top.log10().floor() as i64);
    let mut cand: Vec<(f64, bool, i64)> = vec![];
    for k in k0..=k1.max(k0) {
        for neg in [false, true] {
            let v = if neg { -(10f64.powi(k as i32)) } else { 10f64.powi(k as i32) };
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

/// The radius of the points of a point plot with `n` points in all: 3 px up to 100 points, then shrinking as
/// 1/sqrt(n) to 1 px from 900 points, so that dense clouds (a Markov chain's 10^4 samples) keep their structure.
pub(crate) fn dot_radius(n: usize) -> f64 {
    (3.0 * (100.0 / n.max(1) as f64).sqrt()).clamp(1.0, 3.0)
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
    (
        if min >= lo - w / 2.0 { min } else { lo - w / 4.0 },
        if max <= hi + w / 2.0 { max } else { hi + w / 4.0 },
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

/// Builds terms on one heap.
struct B<'h> {
    h: &'h Heap,
}

impl<'h> B<'h> {
    fn call(&self, f: &str, args: &[T<'h>]) -> T<'h> {
        self.h.call(self.h.sym(f), args)
    }
    fn sym(&self, s: &str) -> T<'h> {
        self.h.sym_term(self.h.sym(s))
    }
    fn list(&self, items: &[T<'h>]) -> T<'h> {
        self.call(mc_api::names::LIST, items)
    }
    fn rule(&self, k: &str, v: T<'h>) -> T<'h> {
        self.call(mc_api::names::RULE, &[self.sym(k), v])
    }
    fn reals(&self, v: &[f64]) -> T<'h> {
        self.list(&v.iter().map(|&x| self.h.real(x)).collect::<Vec<_>>())
    }
    /// The points as an `n x 2` array of reals (packed when long enough).
    fn points(&self, pts: &[(f64, f64)]) -> T<'h> {
        let data: Vec<f64> = pts.iter().flat_map(|&(x, y)| [x, y]).collect();
        self.h.real_array(&[pts.len(), 2], &data)
    }
    fn color(&self, c: Rgb) -> T<'h> {
        let f = |v: u8| v as f64 / 255.0;
        rgb_term(self.h, Rgba::rgb(f(c.0), f(c.1), f(c.2)))
    }
    fn gray(&self, g: f64) -> T<'h> {
        self.call("gray_level", &[self.h.real(g)])
    }
    fn label(&self, l: &Label) -> T<'h> {
        match l {
            Label::Text(s) => self.h.string(&s.replace('-', "\u{2212}")),
            Label::Pow10 { neg, k } => {
                let base = if *neg { "\u{2212}10" } else { "10" };
                let e = k.to_string().replace('-', "\u{2212}");
                self.call("superscript", &[self.h.string(base), self.h.string(&e)])
            }
        }
    }
    /// The style of series `k`: colour, and for lines the width and dash pattern.
    fn style(&self, look: &Look, k: usize, point_d: f64) -> Vec<T<'h>> {
        let mut s = vec![self.color(look.color(k))];
        if look.joined {
            s.push(self.call("absolute_thickness", &[self.h.real(CURVE)]));
            let p = look.dash(k).pattern();
            if !p.is_empty() {
                s.push(self.call("absolute_dashing", &[self.reals(p)]));
            }
        } else {
            s.push(self.call("absolute_point_size", &[self.h.real(point_d)]));
        }
        s
    }
}

/// The runs of finite values of a line: a jump larger than `span` (the visible y range) is a pole, and the line
/// breaks there as it does where there is no value.
pub(crate) fn runs(points: &[(f64, Option<f64>)], span: f64) -> Vec<Vec<(f64, f64)>> {
    let mut runs: Vec<Vec<(f64, f64)>> = vec![];
    let mut prev: Option<f64> = None;
    for &(x, y) in points {
        match (y, prev) {
            (Some(y), Some(p)) if (y - p).abs() <= span => {
                if let Some(r) = runs.last_mut() {
                    r.push((x, y));
                }
            }
            (Some(y), _) => runs.push(vec![(x, y)]),
            (None, _) => {}
        }
        prev = y;
    }
    runs
}

/// What a plot draws besides its series: content under them (bars, rasters, filled regions, contour lines), the
/// extent of that content in axis coordinates, and the data sidecar's entries for it.
#[derive(Default)]
pub(crate) struct Extra<'h> {
    pub(crate) under: Vec<T<'h>>,
    pub(crate) bounds: Option<[(f64, f64); 2]>,
    pub(crate) entries: Vec<crate::sidecar::Entry>,
}

/// The x and y range of the plot (axis coordinates): the data's (the y range robust), widened to `extra`'s
/// bounds, padded unless `look.exact`, then the ends `look.range` gives.
pub(crate) fn ranges(series: &[Series], look: &Look, bounds: Option<[(f64, f64); 2]>) -> [(f64, f64); 2] {
    let xs: Vec<f64> = series.iter().flat_map(|s| s.points.iter().filter(|p| p.1.is_some()).map(|p| p.0)).collect();
    let ys: Vec<f64> = series.iter().flat_map(|s| s.points.iter().filter_map(|p| p.1)).collect();
    let union = |a: Option<(f64, f64)>, b: Option<(f64, f64)>| match (a, b) {
        (Some(a), Some(b)) => Some((a.0.min(b.0), a.1.max(b.1))),
        (a, b) => a.or(b),
    };
    let x = union((!xs.is_empty()).then(|| range(xs.into_iter())), bounds.map(|b| b[0])).unwrap_or((0.0, 1.0));
    let y = union((!ys.is_empty()).then(|| robust(ys)), bounds.map(|b| b[1])).unwrap_or((0.0, 1.0));
    let (mut x, mut y) = if look.exact && x.1 > x.0 && y.1 > y.0 { (x, y) } else { (padded(x), padded(y)) };
    for (r, given, ax) in [(&mut x, look.range[0], look.scale.x), (&mut y, look.range[1], look.scale.y)] {
        let lo = given[0].and_then(|v| ax.to(v)).unwrap_or(r.0);
        let hi = given[1].and_then(|v| ax.to(v)).unwrap_or(r.1);
        if hi > lo {
            *r = (lo, hi);
        }
    }
    [x, y]
}

/// The filled regions of `look.filling`, each in its series' colour, faint.
fn fillings<'h>(b: &B<'h>, series: &[Series], look: &Look, [_, (y0, y1)]: [(f64, f64); 2]) -> Vec<T<'h>> {
    let Some(fill) = look.filling else { return vec![] };
    let span = y1 - y0;
    let mut out = vec![];
    let style = |k: usize| vec![b.color(look.color(k)), b.call("opacity", &[b.h.real(FILL_OPACITY)])];
    match fill {
        Filling::Between(i, j) => {
            let (Some(a), Some(c)) = (series.get(i), series.get(j)) else { return vec![] };
            let polys: Vec<T<'h>> = between(&a.points, &c.points, span).iter().map(|p| b.points(p)).collect();
            if !polys.is_empty() {
                let mut part = style(i);
                part.push(b.call("polygon", &[b.list(&polys)]));
                out.push(b.list(&part));
            }
        }
        _ => {
            let base = match fill {
                Filling::Bottom => y0,
                Filling::Top => y1,
                Filling::Axis => look.scale.y.to(0.0).unwrap_or(y0),
                Filling::Value(v) => look.scale.y.to(v).unwrap_or(y0),
                Filling::Between(..) => y0,
            }
            .clamp(y0, y1);
            for (k, s) in series.iter().enumerate() {
                let polys: Vec<T<'h>> = runs(&s.points, span)
                    .into_iter()
                    .filter(|r| r.len() > 1)
                    .map(|mut r| {
                        let (first, last) = (r[0].0, r[r.len() - 1].0);
                        r.push((last, base));
                        r.push((first, base));
                        b.points(&r)
                    })
                    .collect();
                if !polys.is_empty() {
                    let mut part = style(k);
                    part.push(b.call("polygon", &[b.list(&polys)]));
                    out.push(b.list(&part));
                }
            }
        }
    }
    out
}

/// The regions between two sampled curves: both read at every x either has (linear between samples), cut where
/// either has no value or breaks; each region the first curve forwards, then the second backwards.
pub(crate) fn between(a: &[(f64, Option<f64>)], c: &[(f64, Option<f64>)], span: f64) -> Vec<Vec<(f64, f64)>> {
    let ra = runs(a, span);
    let rc = runs(c, span);
    // the value of a curve's runs at x, linear between its samples (runs are sorted in x for plots)
    let at = |rs: &[Vec<(f64, f64)>], x: f64| -> Option<f64> {
        rs.iter().find(|r| r.len() > 1 && r[0].0 <= x && x <= r[r.len() - 1].0).map(|r| {
            let k = r.partition_point(|p| p.0 < x).clamp(1, r.len() - 1);
            let (p, q) = (r[k - 1], r[k]);
            if q.0 == p.0 {
                p.1
            } else {
                p.1 + (q.1 - p.1) * (x - p.0) / (q.0 - p.0)
            }
        })
    };
    let mut xs: Vec<f64> = ra.iter().chain(&rc).flatten().map(|p| p.0).collect();
    xs.sort_by(f64::total_cmp);
    xs.dedup();
    let mut out = vec![];
    let mut cur: Vec<(f64, f64, f64)> = vec![];
    let mut flush = |cur: &mut Vec<(f64, f64, f64)>| {
        if cur.len() > 1 {
            let mut poly: Vec<(f64, f64)> = cur.iter().map(|&(x, y, _)| (x, y)).collect();
            poly.extend(cur.iter().rev().map(|&(x, _, z)| (x, z)));
            out.push(poly);
        }
        cur.clear();
    };
    for x in xs {
        match (at(&ra, x), at(&rc, x)) {
            (Some(y), Some(z)) => {
                // a jump between runs of one curve starts a new region
                if cur.last().is_some_and(|l| (l.1 - y).abs() > span || (l.2 - z).abs() > span) {
                    flush(&mut cur);
                }
                cur.push((x, y, z));
            }
            _ => flush(&mut cur),
        }
    }
    flush(&mut cur);
    out
}

/// The `graphics` term of the plot of `series` (and `extra`) as `look` says.
pub(crate) fn figure<'h>(h: &'h Heap, series: &[Series], look: &Look, extra: &Extra<'h>) -> T<'h> {
    let b = B { h };
    let [(x0, x1), (y0, y1)] = ranges(series, look, extra.bounds);
    let n: usize = series.iter().map(|s| s.points.len()).sum();
    let point_d = 2.0 * dot_radius(n);
    let mut content = extra.under.clone();
    content.extend(fillings(&b, series, look, [(x0, x1), (y0, y1)]));
    for (k, s) in series.iter().enumerate() {
        let mut part = b.style(look, k, point_d);
        if look.joined {
            let mut runs = runs(&s.points, y1 - y0);
            runs.retain(|r| r.len() > 1);
            let runs: Vec<T<'h>> = runs.iter().map(|r| b.points(r)).collect();
            part.push(b.call("line", &[b.list(&runs)]));
        } else {
            let pts: Vec<(f64, f64)> = s.points.iter().filter_map(|&(x, y)| y.map(|y| (x, y))).collect();
            part.push(b.call("point", &[b.points(&pts)]));
        }
        content.push(b.list(&part));
    }
    // ticks and grid lines: automatic on linear axes, powers of ten on logarithmic ones, or the plot's own
    let mut ticks = vec![];
    let mut grid = vec![];
    for (k, (lo, hi, ax)) in [(x0, x1, look.scale.x), (y0, y1, look.scale.y)].into_iter().enumerate() {
        if let (0, Some(t)) = (k, &look.x_ticks) {
            ticks.push(b.list(&t.iter().map(|(u, l)| b.list(&[h.real(*u), h.string(l)])).collect::<Vec<_>>()));
            grid.push(b.sym("none"));
            continue;
        }
        match axis(lo, hi, ax) {
            None => {
                ticks.push(b.sym("automatic"));
                grid.push(b.sym(if look.origin_axes { "none" } else { "automatic" }));
            }
            Some((major, minor)) => {
                ticks.push(b.list(&major.iter().map(|(u, l)| b.list(&[h.real(*u), b.label(l)])).collect::<Vec<_>>()));
                let lines: Vec<T<'h>> = minor
                    .iter()
                    .map(|&u| (u, FAINT))
                    .chain(major.iter().map(|(u, _)| (*u, GRID)))
                    .map(|(u, g)| b.list(&[h.real(u), b.gray(g)]))
                    .collect();
                grid.push(b.list(&lines));
            }
        }
    }
    let none = b.sym("none");
    let text = |s: &Option<String>| s.as_deref().map_or(none, |s| h.string(s));
    let (w, ht) = look.size.unwrap_or((W, H));
    let mut opts = vec![
        b.rule("image_size", b.reals(&[w, ht])),
        b.rule("aspect_ratio", b.sym(if look.equal { "automatic" } else { "full" })),
        b.rule("plot_range", b.list(&[b.reals(&[x0, x1]), b.reals(&[y0, y1])])),
        b.rule("plot_range_padding", b.sym("none")),
        b.rule("font_size", h.real(LABEL_SIZE)),
        b.rule("ticks", b.list(&ticks)),
        b.rule("grid_lines", b.list(&grid)),
    ];
    if look.origin_axes {
        opts.push(b.rule("axes", b.sym("true")));
        opts.push(b.rule("axes_label", b.list(&[text(&look.x_label), text(&look.y_label)])));
    } else {
        opts.push(b.rule("frame", b.sym("true")));
        opts.push(b.rule("frame_label", b.list(&[text(&look.x_label), text(&look.y_label)])));
    }
    if let (Some((lo, hi)), false) = (look.bar_range, look.hide_color_bar) {
        let g = look.gradient.clone().unwrap_or_else(Gradient::default_gradient);
        let stops: Vec<T<'h>> = g.stops().iter().map(|&c| rgb_term(h, c)).collect();
        opts.push(b.rule("color_bar", b.list(&[b.list(&stops), b.reals(&[lo, hi])])));
    }
    let markers = look.joined || series.is_empty();
    let entries: Vec<T<'h>> = look
        .legend
        .iter()
        .take(series.len().max(look.legend_swatches.len()))
        .enumerate()
        .map(|(k, l)| {
            if let Some(&c) = look.legend_swatches.get(k) {
                return b.list(&[b.list(&[b.color(c)]), h.string(l), b.sym("swatch")]);
            }
            let marker = b.sym(if markers { "line" } else { "point" });
            let style = b.style(look, k, 6.0);
            b.list(&[b.list(&style), h.string(l), marker])
        })
        .collect();
    if !entries.is_empty() {
        opts.push(b.rule("legend", b.list(&entries)));
        opts.push(b.rule("legend_position", b.sym(look.legend_at.name())));
    }
    b.call("graphics", &[b.list(&content), b.call("options", &opts)])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dense_point_clouds_get_smaller_points() {
        assert_eq!(dot_radius(1), 3.0);
        assert_eq!(dot_radius(100), 3.0);
        assert!((dot_radius(400) - 1.5).abs() < 1e-6);
        assert_eq!(dot_radius(10_000), 1.0);
        assert_eq!(dot_radius(0), 3.0);
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
            assert!((ax.from(u) - v).abs() <= 1e-12 * v.abs().max(1e-8), "{v} -> {u} -> {}", ax.from(u));
            assert_eq!(ax.to(-v).unwrap(), -u);
        }
        assert_eq!(Axis::Log.to(-1.0), None);
    }

    #[test]
    fn signed_log_ticks_cover_both_signs_and_zero() {
        let ax = Axis::SignedLog(1e-8);
        let (lo, hi) = (ax.to(-1e10).unwrap(), ax.to(1e10).unwrap());
        let (major, _) = axis(lo, hi, ax).unwrap();
        assert!(major.iter().any(|(u, l)| *u == 0.0 && *l == Label::Text("0".into())));
        assert!(major.iter().any(|(_, l)| *l == Label::Pow10 { neg: true, k: 10 }));
        assert!(major.iter().any(|(_, l)| *l == Label::Pow10 { neg: false, k: 10 }));
        assert!(major.len() <= 11, "{}", major.len());
        assert!(major.windows(2).all(|w| w[0].0 < w[1].0));
    }

    #[test]
    fn log_ticks_are_powers_of_ten_with_minor_lines() {
        let (major, minor) = axis(-2.0, 3.0, Axis::Log).unwrap();
        assert_eq!(major.len(), 6);
        assert_eq!(major[0], (-2.0, Label::Pow10 { neg: false, k: -2 }));
        assert_eq!(minor.len(), 8 * 5);
        assert!(axis(0.0, 1.0, Axis::Linear).is_none());
    }
}
