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
    /// error bars: (x, y) with the ends of a horizontal and of a vertical bar, all on the axes
    pub(crate) bars: Vec<Bar>,
}

/// An error bar (or interval) at a point, on the axes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Bar {
    pub(crate) at: (f64, f64),
    pub(crate) x: Option<(f64, f64)>,
    pub(crate) y: Option<(f64, f64)>,
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
    /// The signed logarithm with threshold c > 0: sign(v) log10(a + k |v|) with k = 10^n, n = max(0, ceil(-log10 c))
    /// and a = max(1, 4 n) ([`Axis::signed_log_constants`]); 0 at 0. Logarithmic for |v| much larger than c, and
    /// values far below c stay just inside ±log10(a), next to ±c, rather than falling to the axis (as the common
    /// plotting convention for this scale does).
    SignedLog(f64),
}

impl Axis {
    /// The value `v` on the axis (`None` where it has no place).
    pub(crate) fn to(self, v: f64) -> Option<f64> {
        match self {
            Axis::Linear => Some(v),
            Axis::Log => (v > 0.0).then(|| v.log10()),
            Axis::SignedLog(c) => {
                let (a, k) = Axis::signed_log_constants(c);
                Some(if v == 0.0 { 0.0 } else { v.signum() * (a + k * v.abs()).log10() })
            }
        }
    }

    /// The value at axis coordinate `u` (the inverse of [`Axis::to`]).
    pub(crate) fn from(self, u: f64) -> f64 {
        match self {
            Axis::Linear => u,
            Axis::Log => 10f64.powf(u),
            Axis::SignedLog(c) => {
                // between -log10(a) and log10(a) only 0 has a place
                let (a, k) = Axis::signed_log_constants(c);
                u.signum() * ((10f64.powf(u.abs()) - a) / k).max(0.0)
            }
        }
    }

    /// The constants `a` and `k` of a signed-log axis with threshold `c` (see [`Axis::SignedLog`]).
    pub(crate) fn signed_log_constants(c: f64) -> (f64, f64) {
        // exact powers of ten are not pushed to the next one by rounding (-log10(1e-8) is 8.000000000000002)
        let n = (-c.log10() - 1e-9).ceil().clamp(0.0, 300.0);
        ((4.0 * n).max(1.0), 10f64.powf(n))
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
pub(crate) struct Look<'h> {
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
    /// the colours of the legend's samples when the legend gives its own (`line_legend([colours], [labels])`), by
    /// entry; the curves keep theirs
    pub(crate) legend_colors: Vec<Rgb>,
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
    /// options of the graphic given to the plot (`plot_label`, `ticks`, `epilog`, ...), applied over the plot's own
    pub(crate) graphics: Vec<(String, T<'h>)>,
    /// `plot_style:` directives of each series (cycled), drawn after the plot's own style
    pub(crate) styles: Vec<T<'h>>,
    /// `filling:` per series (0-based), as rules give it
    pub(crate) fill_each: Vec<(usize, Filling)>,
    /// `plot_labels:` a label at the end of each series
    pub(crate) curve_labels: Vec<T<'h>>,
    /// legend entries that are expressions rather than strings (`plot_legends: [x^2, ...]`), by series
    pub(crate) legend_terms: Vec<T<'h>>,
    /// `axes: false` (and no frame): no frame, no axes
    pub(crate) bare: bool,
    /// `line_thickness:` the width of each curve in points (cycled; default 2)
    pub(crate) thickness: Vec<f64>,
    /// `joined: [true, false, ...]`: per data set (the rest as `joined`)
    pub(crate) joined_each: Vec<bool>,
    /// data rather than a sampled function: lines are never broken at jumps
    pub(crate) data: bool,
    /// `plot_range: all`: the whole range of the values (not the robust one)
    pub(crate) full: bool,
    /// the plot's name (the tag of its messages)
    pub(crate) who: &'static str,
    /// options given and not used (named in a message)
    pub(crate) ignored: Vec<String>,
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

impl Look<'_> {
    /// The colour of series `k`.
    pub(crate) fn color(&self, k: usize) -> Rgb {
        self.colors.get(k).copied().unwrap_or(COLORS[k % COLORS.len()])
    }

    /// Whether series `k` is drawn as a line.
    pub(crate) fn is_joined(&self, k: usize) -> bool {
        self.joined_each.get(k).copied().unwrap_or(self.joined)
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
        // labels too near 0 would overlap its label (and each other across 0): keep those at least 1/12 of the
        // axis away
        let gap = (hi - lo) / 12.0;
        major.retain(|(u, _)| u.abs() >= gap);
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

/// The y range of curves sampled in x order (every series' x values non-decreasing): the 1%..99% quantile range
/// of the y values weighted by the stretch of x each point stands for, so that the samples a refinement crowds
/// near a spike do not widen it, or the full range when no value lies more than half that range outside it; and
/// where a curve has no value at an end (log(0) at a start at 0), its value next to that end is in the range.
/// `None` for data not in x order.
fn curve_range(series: &[Series]) -> Option<(f64, f64)> {
    let mut v: Vec<(f64, f64)> = vec![];
    let mut keep: Vec<f64> = vec![];
    for s in series {
        let p = &s.points;
        if p.windows(2).any(|w| w[1].0 < w[0].0) {
            return None;
        }
        for (i, q) in p.iter().enumerate() {
            let Some(y) = q.1 else { continue };
            let left = if i > 0 { p[i - 1].0 } else { q.0 };
            let right = if i + 1 < p.len() { p[i + 1].0 } else { q.0 };
            v.push((y, (right - left).max(0.0) / 2.0));
        }
        if p.first().is_some_and(|q| q.1.is_none()) {
            keep.extend(p.iter().find_map(|q| q.1));
        }
        if p.last().is_some_and(|q| q.1.is_none()) {
            keep.extend(p.iter().rev().find_map(|q| q.1));
        }
    }
    let total: f64 = v.iter().map(|e| e.1).sum();
    if v.len() < 10 || !(total > 0.0) {
        return None;
    }
    v.sort_by(|a, b| a.0.total_cmp(&b.0));
    let quantile = |f: f64| {
        let mut acc = 0.0;
        for &(y, w) in &v {
            acc += w;
            if acc >= f * total {
                return y;
            }
        }
        v[v.len() - 1].0
    };
    let (lo, hi) = (quantile(0.01), quantile(0.99));
    let w = (hi - lo).max(f64::MIN_POSITIVE);
    let (min, max) = (v[0].0, v[v.len() - 1].0);
    // a spike or dip is cut where the quantile is
    let mut r = (if min >= lo - w / 2.0 { min } else { lo }, if max <= hi + w / 2.0 { max } else { hi });
    for y in keep {
        r = (r.0.min(y), r.1.max(y));
    }
    Some(r)
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
    fn style(&self, look: &Look<'h>, k: usize, point_d: f64) -> Vec<T<'h>> {
        let mut s = vec![self.color(look.color(k))];
        if look.is_joined(k) {
            let w = if look.thickness.is_empty() { CURVE } else { look.thickness[k % look.thickness.len()] };
            s.push(self.call("absolute_thickness", &[self.h.real(w)]));
            let p = look.dash(k).pattern();
            if !p.is_empty() {
                s.push(self.call("absolute_dashing", &[self.reals(p)]));
            }
        } else {
            s.push(self.call("absolute_point_size", &[self.h.real(point_d)]));
        }
        if !look.styles.is_empty() {
            // a list of directives is spliced in: nested in a list of its own it would end before the curve
            let st = look.styles[k % look.styles.len()];
            match self.h.view(st) {
                mc_core::View::App { head, args } if head == self.sym(mc_api::names::LIST) => s.extend_from_slice(args),
                _ => s.push(st),
            }
        }
        s
    }
}

/// The runs of a sampled line between the points without a value and the steps larger than the visible range
/// `window` (y0, y1) (taken for jumps), except steps wholly above or wholly below it, which are not seen. An
/// infinite window never breaks on steps (data).
pub(crate) fn runs(points: &[(f64, Option<f64>)], window: (f64, f64)) -> Vec<Vec<(f64, f64)>> {
    let span = window.1 - window.0;
    let unseen = |a: f64, b: f64| (a > window.1 && b > window.1) || (a < window.0 && b < window.0);
    let mut runs: Vec<Vec<(f64, f64)>> = vec![];
    let mut prev: Option<f64> = None;
    for &(x, y) in points {
        match (y, prev) {
            (Some(y), Some(p)) if (y - p).abs() <= span || unseen(y, p) => {
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
    // error bars widen the ranges to their ends
    let bar_ends = |k: usize| -> Option<(f64, f64)> {
        let v: Vec<f64> = series.iter().flat_map(|s| s.bars.iter()).filter_map(|b| if k == 0 { b.x } else { b.y }).flat_map(|(a, c)| [a, c]).filter(|v| v.is_finite()).collect();
        (!v.is_empty()).then(|| range(v.into_iter()))
    };
    let bx = union(bounds.map(|b| b[0]), bar_ends(0));
    let by = union(bounds.map(|b| b[1]), bar_ends(1));
    let x = union((!xs.is_empty()).then(|| range(xs.into_iter())), bx).unwrap_or((0.0, 1.0));
    let y = union((!ys.is_empty()).then(|| if look.full { range(ys.into_iter()) } else { curve_range(series).unwrap_or_else(|| robust(ys)) }), by)
        .unwrap_or((0.0, 1.0));
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
fn fillings<'h>(b: &B<'h>, series: &[Series], look: &Look<'h>, [_, (y0, y1)]: [(f64, f64); 2]) -> Vec<T<'h>> {
    if !look.fill_each.is_empty() {
        let mut out = vec![];
        for &(k, f) in &look.fill_each {
            if k >= series.len() {
                continue;
            }
            let one = Look { filling: Some(f), fill_each: vec![], colors: look.colors.clone(), scale: look.scale, ..Look::default() };
            let mut part = match f {
                Filling::Between(..) => fillings(b, series, &one, [(0.0, 1.0), (y0, y1)]),
                _ => {
                    let only: Vec<Series> =
                        series.iter().enumerate().map(|(i, s)| Series { points: if i == k { s.points.clone() } else { vec![] }, name: String::new(), bars: vec![] }).collect();
                    fillings(b, &only, &one, [(0.0, 1.0), (y0, y1)])
                }
            };
            out.append(&mut part);
        }
        return out;
    }
    let Some(fill) = look.filling else { return vec![] };
    let window = if look.data { (f64::NEG_INFINITY, f64::INFINITY) } else { (y0, y1) };
    let mut out = vec![];
    let style = |k: usize| vec![b.color(look.color(k)), b.call("opacity", &[b.h.real(FILL_OPACITY)])];
    match fill {
        Filling::Between(i, j) => {
            let (Some(a), Some(c)) = (series.get(i), series.get(j)) else { return vec![] };
            let polys: Vec<T<'h>> = between(&a.points, &c.points, window).iter().map(|p| b.points(p)).collect();
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
                let polys: Vec<T<'h>> = runs(&s.points, window)
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
pub(crate) fn between(a: &[(f64, Option<f64>)], c: &[(f64, Option<f64>)], window: (f64, f64)) -> Vec<Vec<(f64, f64)>> {
    let ra = runs(a, window);
    let rc = runs(c, window);
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
                if cur.last().is_some_and(|l| (l.1 - y).abs() > window.1 - window.0 || (l.2 - z).abs() > window.1 - window.0) {
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
pub(crate) fn figure<'h>(h: &'h Heap, series: &[Series], look: &Look<'h>, extra: &Extra<'h>) -> T<'h> {
    let b = B { h };
    let [(x0, x1), (y0, y1)] = ranges(series, look, extra.bounds);
    // labels at the ends of curves: room for them right of the data (unless the range is given)
    let longest = look
        .curve_labels
        .iter()
        .filter(|&&l| !matches!(h.view(l), mc_core::View::Sym(s) if h.sym_name(s) == "none"))
        .map(|&l| match h.view(l) {
            mc_core::View::Str(t) => t.chars().count(),
            _ => mc_syntax::Printer::new(h).expr(l).chars().count(),
        })
        .max()
        .unwrap_or(0);
    let x1 = if longest > 0 && look.range[0][1].is_none() && !look.exact { x1 + (x1 - x0) * (0.013 * longest as f64 + 0.01).min(0.5) } else { x1 };
    let n: usize = series.iter().map(|s| s.points.len()).sum();
    let point_d = 2.0 * dot_radius(n);
    let mut content = extra.under.clone();
    content.extend(fillings(&b, series, look, [(x0, x1), (y0, y1)]));
    for (k, s) in series.iter().enumerate() {
        let mut part = b.style(look, k, point_d);
        if look.is_joined(k) {
            let mut runs = runs(&s.points, if look.data { (f64::NEG_INFINITY, f64::INFINITY) } else { (y0, y1) });
            runs.retain(|r| r.len() > 1);
            let runs: Vec<T<'h>> = runs.iter().map(|r| b.points(r)).collect();
            part.push(b.call("line", &[b.list(&runs)]));
        } else {
            let pts: Vec<(f64, f64)> = s.points.iter().filter_map(|&(x, y)| y.map(|y| (x, y))).collect();
            part.push(b.call("point", &[b.points(&pts)]));
        }
        if !s.bars.is_empty() {
            part.push(error_bars(&b, &s.bars, [(x0, x1), (y0, y1)]));
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
                let none = b.sym("none");
                let marks = major
                    .iter()
                    .map(|(u, l)| b.list(&[h.real(*u), b.label(l)]))
                    .chain(minor.iter().map(|&u| b.list(&[h.real(u), none, h.real(0.5)])));
                ticks.push(b.list(&marks.collect::<Vec<_>>()));
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
    // plot_labels: each at the right end of its series, in its colour
    for (k, (&l, s)) in look.curve_labels.iter().zip(series).enumerate() {
        if matches!(h.view(l), mc_core::View::Sym(sy) if h.sym_name(sy) == "none") {
            continue;
        }
        let Some(&(x, Some(y))) = s.points.iter().rev().find(|p| p.1.is_some_and(|y| y >= y0 && y <= y1) && p.0 >= x0 && p.0 <= x1) else { continue };
        let at = b.reals(&[x, y]);
        content.push(b.list(&[b.color(look.color(k)), b.call("text", &[l, at, b.reals(&[-1.2, 0.0])])]));
    }
    let markers = |k: usize| look.is_joined(k) || series.is_empty();
    let entries: Vec<T<'h>> = look
        .legend
        .iter()
        .take(series.len().max(look.legend_swatches.len()))
        .enumerate()
        // an empty entry (a curve the legend leaves out) is not drawn
        .filter(|(k, l)| !l.is_empty() || look.legend_terms.get(*k).is_some())
        .map(|(k, l)| {
            if let Some(&c) = look.legend_swatches.get(k) {
                return b.list(&[b.list(&[b.color(c)]), h.string(l), b.sym("swatch")]);
            }
            let marker = b.sym(if markers(k) { "line" } else { "point" });
            let mut style = b.style(look, k, 6.0);
            if let Some(&c) = look.legend_colors.get(k) {
                style.push(b.color(c));
            }
            let label = look.legend_terms.get(k).copied().unwrap_or_else(|| h.string(l));
            b.list(&[b.list(&style), label, marker])
        })
        .collect();
    if !entries.is_empty() {
        opts.push(b.rule("legend", b.list(&entries)));
        opts.push(b.rule("legend_position", b.sym(look.legend_at.name())));
    }
    if look.bare {
        opts.retain(|r| !matches!(rule_name(h, *r).as_deref(), Some("frame" | "axes" | "frame_label" | "axes_label")));
    }
    // the options given for the graphic replace the plot's own of the same name
    let decoration_label = if look.origin_axes { "axes_label" } else { "frame_label" };
    let own_ticks = opts.iter().find(|r| rule_name(h, **r).as_deref() == Some("ticks")).and_then(|r| match h.view(*r) {
        mc_core::View::App { args: &[_, v], .. } => Some(v),
        _ => None,
    });
    for (name, v) in &look.graphics {
        let frame_ticks = name == "frame_ticks";
        let name = match name.as_str() {
            "axes_label" | "frame_label" => decoration_label,
            "frame_ticks" => "ticks",
            n => n,
        };
        opts.retain(|r| rule_name(h, *r).as_deref() != Some(name));
        let v = match name {
            "ticks" => given_ticks(&b, *v, frame_ticks, look.scale, own_ticks),
            "grid_lines" => log_positions(&b, *v, look.scale),
            // positions of the content given in data values, placed on the axes (logarithms on log axes)
            "epilog" | "prolog" if look.scale.x != Axis::Linear || look.scale.y != Axis::Linear => on_axes(&b, *v, look.scale, 0),
            _ => *v,
        };
        opts.push(b.rule(name, v));
    }
    b.call("graphics", &[b.list(&content), b.call("options", &opts)])
}

/// Error bars as one `line` of segments, 1 pt wide, each end with a short cross bar (1 % of the other axis).
fn error_bars<'h>(b: &B<'h>, bars: &[Bar], [(x0, x1), (y0, y1)]: [(f64, f64); 2]) -> T<'h> {
    let (cx, cy) = (0.008 * (x1 - x0), 0.012 * (y1 - y0));
    let mut segs = vec![];
    for bar in bars {
        let (x, y) = bar.at;
        if let Some((lo, hi)) = bar.y {
            segs.push(b.points(&[(x, lo), (x, hi)]));
            segs.push(b.points(&[(x - cx, lo), (x + cx, lo)]));
            segs.push(b.points(&[(x - cx, hi), (x + cx, hi)]));
        }
        if let Some((lo, hi)) = bar.x {
            segs.push(b.points(&[(lo, y), (hi, y)]));
            segs.push(b.points(&[(lo, y - cy), (lo, y + cy)]));
            segs.push(b.points(&[(hi, y - cy), (hi, y + cy)]));
        }
    }
    b.list(&[b.call("absolute_thickness", &[b.h.real(1.0)]), b.call("absolute_dashing", &[b.list(&[])]), b.call("line", &[b.list(&segs)])])
}

/// The ticks option `[x, y]` for ticks given to a plot (`ticks` or, with `frame`, `frame_ticks`): per axis
/// `automatic` (the plot's own ticks from `own`, powers of ten on log axes), `none`, or a list of positions,
/// `[position, label]` or `[position, label, length]` given in data values and placed on the axes. `frame_ticks`
/// may give the sides, `[[left, right], [bottom, top]]`; the left and bottom ones count.
fn given_ticks<'h>(b: &B<'h>, v: T<'h>, frame: bool, scale: Scale, own: Option<T<'h>>) -> T<'h> {
    use mc_core::View;
    let h = b.h;
    let as_list = |t: T<'h>| match h.view(t) {
        View::App { head, args } if matches!(h.view(head), View::Sym(s) if h.sym_name(s) == mc_api::names::LIST) => Some(args),
        _ => None,
    };
    let sym = |t: T<'h>, names: &[&str]| matches!(h.view(t), View::Sym(s) if names.contains(&h.sym_name(s).as_str()));
    let auto = |t: T<'h>| sym(t, &["automatic", "all", "true"]);
    let off = |t: T<'h>| sym(t, &["none", "false"]);
    let num = |t: T<'h>| mc_graphics::num::number(h, t).is_some();
    // one tick: a position, or [position, label] or [position, label, length] (a label is not a number or list)
    let tick = |t: T<'h>| num(t) || as_list(t).is_some_and(|l| (2..=3).contains(&l.len()) && num(l[0]) && !num(l[1]) && as_list(l[1]).is_none());
    let tick_list = |t: T<'h>| as_list(t).is_some_and(|l| l.iter().all(|&e| tick(e)));
    let axis_spec = |t: T<'h>| auto(t) || off(t) || tick_list(t);
    let two = |t: T<'h>| as_list(t).and_then(|l| if let &[a, c] = l { Some([a, c]) } else { None });
    let [x, y] = match two(v) {
        _ if tick_list(v) || !as_list(v).is_some() => [v, v],
        // per side, [[left, right], [bottom, top]]
        Some([a, c]) if frame && [a, c].iter().all(|&t| two(t).is_some_and(|s| s.iter().all(|&e| axis_spec(e)))) => {
            [two(c).map_or(c, |s| s[0]), two(a).map_or(a, |s| s[0])]
        }
        Some([a, c]) if axis_spec(a) && axis_spec(c) => [a, c],
        _ => [v, v],
    };
    let own = own.and_then(as_list).filter(|l| l.len() == 2);
    let axis = |t: T<'h>, k: usize, ax: Axis| -> T<'h> {
        if auto(t) {
            return own.map_or_else(|| b.sym("automatic"), |o| o[k]);
        }
        if off(t) {
            return b.sym("none");
        }
        let Some(items) = as_list(t) else { return t };
        let moved: Vec<T<'h>> = items
            .iter()
            .filter_map(|&e| match as_list(e) {
                Some(&[p, ref rest @ ..]) if !rest.is_empty() => {
                    let u = ax.to(mc_graphics::num::number(h, p)?)?;
                    let mut a = vec![h.real(u)];
                    a.extend_from_slice(rest);
                    Some(b.list(&a))
                }
                Some(_) => None,
                // a bare position is labelled with its value
                None => mc_graphics::num::number(h, e).and_then(|p| ax.to(p)).map(|u| b.list(&[h.real(u), e])),
            })
            .collect();
        b.list(&moved)
    };
    b.list(&[axis(x, 0, scale.x), axis(y, 1, scale.y)])
}

/// Grid lines `[xs, ys]` given in data values, placed on the axes (logarithms on log axes); each a position or
/// `[position, style]`.
fn log_positions<'h>(b: &B<'h>, v: T<'h>, scale: Scale) -> T<'h> {
    use mc_core::View;
    let h = b.h;
    let as_list = |t: T<'h>| match h.view(t) {
        View::App { head, args } if matches!(h.view(head), View::Sym(s) if h.sym_name(s) == mc_api::names::LIST) => Some(args),
        _ => None,
    };
    let Some(&[xs, ys]) = as_list(v) else { return v };
    let axis = |t: T<'h>, ax: Axis| -> T<'h> {
        if ax == Axis::Linear {
            return t;
        }
        let Some(items) = as_list(t) else { return t };
        let moved: Vec<T<'h>> = items
            .iter()
            .filter_map(|&e| match as_list(e) {
                Some(&[p, s]) => mc_graphics::num::number(h, p).and_then(|p| ax.to(p)).map(|u| b.list(&[h.real(u), s])),
                Some(_) => None,
                None => mc_graphics::num::number(h, e).and_then(|p| ax.to(p)).map(|u| h.real(u)),
            })
            .collect();
        b.list(&moved)
    };
    b.list(&[axis(xs, scale.x), axis(ys, scale.y)])
}

/// Graphics content with the positions of its primitives (given in data values) placed on the axes of `scale`.
/// Positions are the coordinate arguments of `point`, `line`, `polygon`, `arrow`, `bezier_curve`, `rectangle`,
/// `disk`, `circle` (the centre), `text` and `inset` (the position); other terms are left as they are.
fn on_axes<'h>(b: &B<'h>, t: T<'h>, scale: Scale, depth: usize) -> T<'h> {
    use mc_core::View;
    let h = b.h;
    if depth > 32 {
        return t;
    }
    let View::App { head, args } = h.view(t) else { return t };
    let View::Sym(s) = h.view(head) else { return t };
    let name = h.sym_name(s);
    let positions: &[usize] = match name.as_str() {
        n if n == mc_api::names::LIST => {
            let a: Vec<T<'h>> = args.iter().map(|&e| on_axes(b, e, scale, depth + 1)).collect();
            return b.list(&a);
        }
        "point" | "line" | "polygon" | "arrow" | "bezier_curve" | "disk" | "circle" => &[0],
        "rectangle" => &[0, 1],
        "text" | "inset" => &[1],
        "style" | "tooltip" | "annotation" => {
            let mut a = args.to_vec();
            if let Some(first) = a.first_mut() {
                *first = on_axes(b, *first, scale, depth + 1);
            }
            return h.app(head, &a);
        }
        _ => return t,
    };
    let mut a = args.to_vec();
    for &k in positions {
        if let Some(e) = a.get_mut(k) {
            *e = place(b, *e, scale, depth);
        }
    }
    h.app(head, &a)
}

/// A position `[x, y]` (or nested lists of them) moved onto the axes; a point with no place (a log of a value not
/// above 0) is left as it is.
fn place<'h>(b: &B<'h>, t: T<'h>, scale: Scale, depth: usize) -> T<'h> {
    use mc_core::View;
    let h = b.h;
    if depth > 32 {
        return t;
    }
    let View::App { head, args } = h.view(t) else { return t };
    if !matches!(h.view(head), View::Sym(s) if h.sym_name(s) == mc_api::names::LIST) {
        return t;
    }
    if let [x, y] = args {
        if let (Some(x), Some(y)) = (mc_graphics::num::number(h, *x), mc_graphics::num::number(h, *y)) {
            return match (scale.x.to(x), scale.y.to(y)) {
                (Some(u), Some(v)) => h.reals(&[u, v]),
                _ => t,
            };
        }
    }
    let a: Vec<T<'h>> = args.iter().map(|&e| place(b, e, scale, depth + 1)).collect();
    b.list(&a)
}

/// The option name of a rule `name -> value`.
fn rule_name<'h>(h: &'h Heap, r: T<'h>) -> Option<String> {
    let mc_core::View::App { args: &[k, _], .. } = h.view(r) else { return None };
    match h.view(k) {
        mc_core::View::Sym(s) => Some(h.sym_name(s)),
        _ => None,
    }
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
        for v in [-1e10, -3.5, -1e-9, 2e-8, 7.0, 1e10] {
            let u = ax.to(v).unwrap();
            assert!((ax.from(u) - v).abs() <= 1e-9 * v.abs().max(1e-8), "{v} -> {u} -> {}", ax.from(u));
            assert_eq!(ax.to(-v).unwrap(), -u);
        }
        assert_eq!((ax.to(0.0), ax.from(0.0), ax.from(0.5)), (Some(0.0), 0.0, 0.0));
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

    /// Gap queue G88: the mapping of the reference's signed-log scale: sign(v) log10(a + k |v|), with values far
    /// below the threshold just inside the threshold's level.
    #[test]
    fn signed_log_maps_as_the_reference_does() {
        let close = |a: f64, b: f64| (a - b).abs() < 1e-9;
        let u = |c: f64, v: f64| Axis::SignedLog(c).to(v).unwrap();
        // threshold 10^-8: a = 32, k = 10^8
        assert!(close(u(1e-8, 1e-12), 32.0001f64.log10()) && close(u(1e-8, 1e-8), 33f64.log10()));
        assert!(close(u(1e-8, 1e-7), 42f64.log10()) && close(u(1e-8, 1.0), (32.0 + 1e8f64).log10()));
        assert!(close(u(1e-8, -1e-8), -(33f64.log10())));
        // other thresholds: 10^-3 (a = 12), 1/2 and 1/10 (a = 4, k = 10), 1 and above (a = k = 1)
        assert!(close(u(1e-3, 1e-3), 13f64.log10()) && close(u(1e-3, 1.0), 1012f64.log10()));
        assert!(close(u(0.5, 0.5), 9f64.log10()) && close(u(0.1, 10.0), 104f64.log10()));
        assert!(close(u(1.0, 1.0), 2f64.log10()) && close(u(2.0, 200.0), 201f64.log10()) && close(u(1000.0, 1000.0), 1001f64.log10()));
        assert!(close(u(1e-20, 1e-20), 81f64.log10()));
        assert_eq!(u(1e-8, 0.0), 0.0);
    }

    #[test]
    fn signed_log_labels_near_zero_do_not_crowd() {
        for (lo, hi, c) in [(-1e10, 1e10, 1e-8), (-1.0, 3.0, 1e-8), (-100.0, 100.0, 1.0)] {
            let ax = Axis::SignedLog(c);
            let (u0, u1) = (ax.to(lo).unwrap(), ax.to(hi).unwrap());
            let (major, _) = axis(u0, u1, ax).unwrap();
            let gap = (u1 - u0) / 12.0;
            assert!(major.windows(2).all(|w| w[1].0 - w[0].0 >= gap - 1e-9), "{lo} {hi} {c}: {major:?}");
        }
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
