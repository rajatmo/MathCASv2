//! Painting a [`Graphic`] on a [`Surface`]: the plot range, the page size, the margins the decorations need, and
//! then the background, grid lines, primitives, frame or axes with ticks, labels and the legend.
//!
//! Page units are points; y grows downwards on the page and upwards in plot coordinates.
use crate::color::Rgba;
use crate::label::{typeset, Label, Typeset};
use crate::model::{Aspect, Corner, Face, Graphic, Grid, Item, Marker, Pt, Shape, Style, Ticks, POLL};
use crate::surface::{Cap, Path, Pen, Role, Surface, Text};
use crate::{ticks, Failure};

/// The default width of a graphic, in points.
pub const DEFAULT_WIDTH: f64 = 360.0;
/// Frame, axes and tick marks.
pub(crate) const FRAME: Rgba = Rgba::rgb(60.0 / 255.0, 60.0 / 255.0, 60.0 / 255.0);
/// Label text: 85% black.
pub(crate) const TEXT: Rgba = Rgba::rgb(0.15, 0.15, 0.15);
/// Grid lines without a style of their own.
pub(crate) const GRID: Rgba = Rgba::rgb(0.9, 0.9, 0.9);
/// Tick marks' length.
const TICK: f32 = 4.0;
/// Space between a tick mark and its label, and between labels.
const GAP: f32 = 4.0;
/// The margin when nothing else needs room.
const EDGE: f32 = 4.0;
/// The colour bar: its distance from the plot area, and its width.
const BAR_GAP: f32 = 14.0;
const BAR_W: f32 = 14.0;
/// Cells of the gradient image in a colour bar.
const BAR_CELLS: usize = 256;

/// Everything about the page that follows from the graphic before painting.
#[derive(Clone, Debug)]
pub struct Plan {
    /// page width in points
    pub width: f32,
    /// page height in points
    pub height: f32,
    /// the plot area: x, y, w, h
    pub area: [f32; 4],
    /// the x and y plot range
    pub range: [(f64, f64); 2],
    /// x and y ticks with their typeset labels
    ticks: [Vec<(f64, Option<Typeset>)>; 2],
    /// x and y minor tick marks (half length, no labels)
    minor: [Vec<f64>; 2],
    /// where the axes cross: y of the x axis and x of the y axis, in plot coordinates
    origin: (f64, f64),
    /// clip the primitives to the plot area (an explicit plot range)
    clip: bool,
    /// the colour bar's ticks (values with typeset labels) and its typeset label
    bar: Option<ColorBarPlan>,
}

/// A colour bar's ticks (values with typeset labels) and its typeset label.
type ColorBarPlan = (Vec<(f64, Typeset)>, Option<Typeset>);

impl Plan {
    /// Page x of plot coordinate `x`.
    fn px(&self, x: f64) -> f32 {
        let (x0, x1) = self.range[0];
        self.area[0] + ((x - x0) / (x1 - x0)) as f32 * self.area[2]
    }

    /// Page y of plot coordinate `y`.
    fn py(&self, y: f64) -> f32 {
        let (y0, y1) = self.range[1];
        self.area[1] + ((y1 - y) / (y1 - y0)) as f32 * self.area[3]
    }

    /// A position on the page.
    fn at(&self, p: Pt) -> (f32, f32) {
        if p.scaled {
            (self.area[0] + p.x as f32 * self.area[2], self.area[1] + (1.0 - p.y as f32) * self.area[3])
        } else {
            (self.px(p.x), self.py(p.y))
        }
    }

    /// Page lengths of plot lengths `(dx, dy)`.
    fn scale(&self, (dx, dy): (f64, f64)) -> (f32, f32) {
        let ((x0, x1), (y0, y1)) = (self.range[0], self.range[1]);
        ((dx / (x1 - x0)) as f32 * self.area[2], (dy / (y1 - y0)) as f32 * self.area[3])
    }
}

/// The smallest and largest coordinates of the primitives (positions given as `scaled` left out).
pub(crate) fn bounds(items: &[Item], stop: &dyn Fn() -> bool) -> Result<[(f64, f64); 2], Failure> {
    let mut b = [(f64::INFINITY, f64::NEG_INFINITY); 2];
    let mut add = |p: &Pt| {
        if !p.scaled {
            b[0] = (b[0].0.min(p.x), b[0].1.max(p.x));
            b[1] = (b[1].0.min(p.y), b[1].1.max(p.y));
        }
    };
    let mut n = 0usize;
    let mut poll = |k: usize| -> Result<(), Failure> {
        n += k;
        if n >= POLL {
            n = 0;
            if stop() {
                return Err(Failure::new("interrupted", ""));
            }
        }
        Ok(())
    };
    for it in items {
        match &it.shape {
            Shape::Points(ps) | Shape::Bezier(ps) => {
                ps.iter().for_each(&mut add);
                poll(ps.len())?;
            }
            Shape::Line(rs) | Shape::Polygon(rs) | Shape::Arrow(rs) => {
                for r in rs {
                    r.iter().for_each(&mut add);
                    poll(r.len())?;
                }
            }
            Shape::Rect(a, c) => {
                add(a);
                add(c);
            }
            Shape::Ellipse { c, r, .. } => {
                if !c.scaled {
                    add(&Pt { x: c.x - r.0, y: c.y - r.1, scaled: false });
                    add(&Pt { x: c.x + r.0, y: c.y + r.1, scaled: false });
                }
            }
            Shape::Text { at, .. } => add(at),
            Shape::Inset { at, natural, offset, width: Some(w), .. } if !at.scaled => {
                // its rectangle in plot coordinates (its height as its natural shape says)
                let ht = w * natural.1 / natural.0.max(1e-9);
                let left = at.x - (1.0 + offset.0) / 2.0 * w;
                let bottom = at.y - (1.0 + offset.1) / 2.0 * ht;
                add(&Pt { x: left, y: bottom, scaled: false });
                add(&Pt { x: left + w, y: bottom + ht, scaled: false });
            }
            Shape::Inset { at, .. } => add(at),
            Shape::Raster { from, to, .. } => {
                add(from);
                add(to);
            }
        }
        poll(1)?;
    }
    Ok(b)
}

/// A degenerate or empty extent made into a range: around a single value by a tenth of `other` (the other axis's
/// length) or by 1.
fn widen((lo, hi): (f64, f64), other: f64) -> (f64, f64) {
    if !lo.is_finite() {
        return (0.0, 1.0);
    }
    if hi - lo > f64::EPSILON * lo.abs().max(1.0) * 4.0 {
        return (lo, hi);
    }
    let d = if other.is_finite() && other > 0.0 { other / 10.0 } else { lo.abs().max(1.0) / 10.0 };
    (lo - d, hi + d)
}

/// Typeset tick labels of `size`.
fn typeset_ticks(t: Vec<(f64, Option<Label>)>, size: f32) -> Vec<(f64, Option<Typeset>)> {
    t.into_iter().map(|(u, l)| (u, l.map(|l| typeset(&l, size)))).collect()
}

/// The tick count for an axis `len` points long.
fn tick_count(len: f64) -> usize {
    (len / 60.0).round().clamp(3.0, 6.0) as usize
}

/// Work out the page.
pub fn plan(g: &Graphic, stop: &dyn Fn() -> bool) -> Result<Plan, Failure> {
    let o = &g.opts;
    let data = bounds(&g.items, stop)?;
    // the plot range: given ends, else the data's, padded
    let pad = o.padding.unwrap_or(0.02);
    let mut range = [(0.0, 1.0); 2];
    let spans = [data[0].1 - data[0].0, data[1].1 - data[1].0];
    for k in 0..2 {
        let (lo, hi) = widen(data[k], spans[1 - k]);
        let given = o.plot_range[k];
        let len = hi - lo;
        let lo = given[0].unwrap_or(lo - pad * len);
        let hi = given[1].unwrap_or(hi + pad * len);
        range[k] = if hi > lo { (lo, hi) } else { widen((lo, lo), 0.0) };
    }
    let clip = o.clip.unwrap_or_else(|| o.plot_range.iter().flatten().any(Option::is_some));
    let fs = o.font_size as f32;
    // the plot area's shape (height over width)
    let ratio = match o.aspect {
        Aspect::Auto => (range[1].1 - range[1].0) / (range[0].1 - range[0].0),
        Aspect::Ratio(r) => r,
        Aspect::Full => match o.image_size {
            (Some(w), Some(h)) => h / w,
            _ => 0.618_034,
        },
    }
    .clamp(0.02, 50.0);
    // ticks are laid out for a first guess of the area's size, then the margins follow from their labels
    let guess_w = o.image_size.0.unwrap_or(DEFAULT_WIDTH);
    let guess_h = o.image_size.1.unwrap_or(guess_w * ratio);
    let decorated = |k: usize| o.frame || o.axes[k];
    let mut tk: [Vec<(f64, Option<Label>)>; 2] = [vec![], vec![]];
    let mut minor: [Vec<f64>; 2] = [vec![], vec![]];
    for k in 0..2 {
        if !(decorated(0) || decorated(1)) || !(o.frame || o.axes[k]) {
            continue;
        }
        let (lo, hi) = range[k];
        tk[k] = match &o.ticks[k] {
            Ticks::Auto => {
                let n = tick_count(if k == 0 { guess_w } else { guess_h });
                // a frame has unlabelled minor ticks between the labelled ones
                if o.frame {
                    minor[k] = ticks::minor(lo, hi, n);
                }
                ticks::auto(lo, hi, n)
            }
            Ticks::None => vec![],
            Ticks::List(v, mn) => {
                minor[k] = mn.iter().copied().filter(|u| *u >= lo && *u <= hi).collect();
                v.iter().filter(|(u, _)| *u >= lo && *u <= hi).cloned().collect()
            }
        };
    }
    let [tx, ty] = tk;
    let ticks = [typeset_ticks(tx, fs), typeset_ticks(ty, fs)];
    // where the axes cross: at 0, or at the range's lower end when 0 is outside it
    let cross = |(lo, hi): (f64, f64)| if lo <= 0.0 && 0.0 <= hi { 0.0 } else { lo };
    let origin = match o.origin {
        Some((y, x)) => (y.clamp(range[1].0, range[1].1), x.clamp(range[0].0, range[0].1)),
        None => (cross(range[1]), cross(range[0])),
    };
    let max_w = |v: &[(f64, Option<Typeset>)]| v.iter().filter_map(|t| t.1.as_ref()).map(|t| t.width).fold(0.0, f32::max);
    let max_h = |v: &[(f64, Option<Typeset>)]| v.iter().filter_map(|t| t.1.as_ref()).map(|t| t.height).fold(0.0, f32::max);
    // margins: left, top, right, bottom
    let mut m = [o.edge.map_or(EDGE, |e| e as f32); 4];
    let (xl, yl) = (max_h(&ticks[0]), max_w(&ticks[1]));
    // x tick labels below the frame (or an x axis at the bottom edge), y labels left of it
    let x_at_edge = o.frame || (o.axes[0] && origin.0 == range[1].0);
    let y_at_edge = o.frame || (o.axes[1] && origin.1 == range[0].0);
    if (o.frame || o.axes[0]) && x_at_edge && xl > 0.0 {
        m[3] += TICK.min(2.0) + xl;
    }
    if (o.frame || o.axes[1]) && y_at_edge && yl > 0.0 {
        m[0] += GAP + yl;
    }
    if !ticks[1].is_empty() {
        // the top y label sticks out half its height
        m[1] = m[1].max(max_h(&ticks[1]) / 2.0 + 1.0);
    }
    if !ticks[0].is_empty() {
        let last = ticks[0].last().and_then(|t| t.1.as_ref()).map_or(0.0, |t| t.width);
        m[2] = m[2].max(last / 2.0 + 1.0);
    }
    let lfs = o.label_size.map_or(fs, |v| v as f32);
    let labels = |l: &Option<Label>| l.as_ref().map(|l| typeset(l, lfs));
    let frame_labels = [labels(&o.frame_label[0]), labels(&o.frame_label[1])];
    let axes_labels = [labels(&o.axes_label[0]), labels(&o.axes_label[1])];
    if o.frame {
        if let Some(t) = &frame_labels[0] {
            m[3] += GAP + t.height;
        }
        if let Some(t) = &frame_labels[1] {
            m[0] += GAP + t.height;
        }
    } else {
        if let (true, Some(t)) = (o.axes[0], &axes_labels[0]) {
            m[2] += GAP + t.width;
        }
        if let (true, Some(t)) = (o.axes[1], &axes_labels[1]) {
            m[1] += GAP + t.height;
        }
    }
    if let Some(l) = &o.plot_label {
        m[1] += GAP + typeset(l, fs * 1.15).height;
    }
    // the colour bar right of the plot area: the bar, its ticks and labels, its own label
    let bar = o.color_bar.as_ref().map(|cb| {
        let (lo, hi) = cb.range;
        let t: Vec<(f64, Typeset)> = ticks::auto(lo, hi, tick_count(guess_h))
            .into_iter()
            .filter_map(|(u, l)| Some((u, typeset(&l?, fs))))
            .collect();
        (t, cb.label.as_ref().map(|l| typeset(l, fs)))
    });
    if let Some((t, l)) = &bar {
        let tw = t.iter().map(|t| t.1.width).fold(0.0, f32::max);
        m[2] += BAR_GAP + BAR_W + TICK + 2.0 + tw + l.as_ref().map_or(0.0, |l| GAP + l.height);
        if let Some(top) = t.iter().map(|t| t.1.height).reduce(f32::max) {
            m[1] = m[1].max(top / 2.0 + 1.0);
            m[3] = m[3].max(top / 2.0 + 1.0);
        }
    }
    // the page and the plot area
    let (mh, mv) = ((m[0] + m[2]) as f64, (m[1] + m[3]) as f64);
    let (width, height, area) = match o.image_size {
        (Some(w), Some(h)) => {
            let (aw, ah) = ((w - mh).max(1.0), (h - mv).max(1.0));
            let (fw, fh) = if o.aspect == Aspect::Full {
                (aw, ah)
            } else if ah / aw > ratio {
                (aw, aw * ratio)
            } else {
                (ah / ratio, ah)
            };
            let (x, y) = (m[0] as f64 + (aw - fw) / 2.0, m[1] as f64 + (ah - fh) / 2.0);
            (w, h, [x, y, fw, fh])
        }
        (None, Some(h)) => {
            let ah = (h - mv).max(1.0);
            let aw = (ah / ratio).min(crate::model::MAX_SIDE);
            (aw + mh, h, [m[0] as f64, m[1] as f64, aw, ah])
        }
        (w, None) => {
            let w = w.unwrap_or(DEFAULT_WIDTH);
            let aw = (w - mh).max(1.0);
            let ah = (aw * ratio).min(crate::model::MAX_SIDE);
            (w, ah + mv, [m[0] as f64, m[1] as f64, aw, ah])
        }
    };
    Ok(Plan {
        width: width as f32,
        height: height as f32,
        area: area.map(|v| v as f32),
        range,
        ticks,
        minor,
        origin,
        clip,
        bar,
    })
}

/// Paint the graphic on `s` as `p` plans.
pub fn paint(g: &Graphic, p: &Plan, s: &mut dyn Surface, stop: &dyn Fn() -> bool) -> Result<(), Failure> {
    let o = &g.opts;
    if let Some(bg) = o.background {
        s.role(if o.themed_background { Role::Background } else { Role::Data });
        s.fill(&Path::rect(0.0, 0.0, p.width, p.height), bg);
        s.role(Role::Data);
    }
    let [ax, ay, aw, ah] = p.area;
    // grid lines, behind everything
    for k in 0..2 {
        let lines: Vec<(f64, Option<usize>)> = match &o.grid[k] {
            Grid::None => continue,
            Grid::Auto => {
                let (lo, hi) = p.range[k];
                let n = tick_count(if k == 0 { aw } else { ah } as f64);
                let t = if p.ticks[k].is_empty() { ticks::positions(lo, hi, n) } else { p.ticks[k].iter().map(|t| t.0).collect() };
                t.into_iter().map(|u| (u, None)).collect()
            }
            Grid::List(v) => v.clone(),
        };
        let (lo, hi) = p.range[k];
        for (u, style) in lines.into_iter().filter(|(u, _)| *u >= lo && *u <= hi) {
            let pen = match style {
                Some(i) => {
                    s.role(g.styles[i].role);
                    pen_of(&g.styles[i], aw)
                }
                None => {
                    s.role(Role::Grid);
                    Pen { color: GRID, width: 1.0, dash: vec![], cap: Cap::Butt }
                }
            };
            let path = if k == 0 {
                Path::polyline(&[(p.px(u), ay), (p.px(u), ay + ah)], false)
            } else {
                Path::polyline(&[(ax, p.py(u)), (ax + aw, p.py(u))], false)
            };
            s.stroke(&path, &pen);
        }
    }
    s.role(Role::Data);
    // the primitives
    if p.clip {
        s.clip(p.area);
    }
    for (i, it) in g.items.iter().enumerate() {
        if i % POLL == POLL - 1 && stop() {
            return Err(Failure::new("interrupted", ""));
        }
        item(g, p, it, s);
    }
    s.role(Role::Data);
    if p.clip {
        s.unclip();
    }
    decorations(g, p, s);
    legend(g, p, s);
    color_bar(g, p, s);
    s.role(Role::Data);
    Ok(())
}

/// The colour bar: the gradient from the bottom (the low end) to the top, framed, with ticks and labels on its
/// right and its label beyond them, read upwards.
fn color_bar(g: &Graphic, p: &Plan, s: &mut dyn Surface) {
    let (Some(cb), Some((ticks, label))) = (&g.opts.color_bar, &p.bar) else { return };
    let [ax, ay, aw, ah] = p.area;
    let x = ax + aw + BAR_GAP;
    let mut cells = Vec::with_capacity(BAR_CELLS * 4);
    for k in (0..BAR_CELLS).rev() {
        cells.extend_from_slice(&cb.gradient.at((k as f64 + 0.5) / BAR_CELLS as f64).bytes());
    }
    s.role(Role::Data);
    s.image([x, ay, BAR_W, ah], 1, BAR_CELLS, &cells);
    let thin = Pen { color: FRAME, width: 1.0, dash: vec![], cap: Cap::Butt };
    s.role(Role::Frame);
    s.stroke(&Path::rect(x, ay, BAR_W, ah), &thin);
    let (lo, hi) = cb.range;
    let mut marks = Path::default();
    let mut widest = 0.0f32;
    for (u, t) in ticks {
        let y = ay + ((hi - u) / (hi - lo)) as f32 * ah;
        marks.add_polyline(&[(x + BAR_W, y), (x + BAR_W + TICK, y)], false);
        chrome_label(s, t, (x + BAR_W + TICK + 2.0, y - t.height / 2.0), false);
        widest = widest.max(t.width);
    }
    s.role(Role::Frame);
    s.stroke(&marks, &thin);
    if let Some(t) = label {
        let left = x + BAR_W + TICK + 2.0 + widest + GAP;
        chrome_label(s, t, (left, ay + ah / 2.0 + t.width / 2.0), true);
    }
}

/// The pen of a style for lines.
fn pen_of(st: &Style, width: f32) -> Pen {
    let w = width as f64;
    let dash: Vec<f32> = st.dashing.iter().map(|d| d.at(w) as f32).collect();
    // round caps turn the short dashes of a dotted line into dots
    let cap = if dash.is_empty() { Cap::Butt } else { Cap::Round };
    Pen { color: st.ink(), width: st.thickness.at(w) as f32, dash, cap }
}

/// Fill a face and draw its edges, as the style says.
fn face(s: &mut dyn Surface, path: &Path, st: &Style, width: f32) {
    if let Some(c) = st.fill() {
        s.role(if matches!(st.face, Face::Current) { st.role } else { Role::Data });
        s.fill(path, c);
    }
    if let Some(e) = &st.edge {
        s.role(Role::Data);
        let w = width as f64;
        let dash: Vec<f32> = e.dashing.iter().map(|d| d.at(w) as f32).collect();
        let cap = if dash.is_empty() { Cap::Butt } else { Cap::Round };
        s.stroke(path, &Pen { color: e.color, width: e.thickness.at(w) as f32, dash, cap });
    }
}

/// The parts of the polyline inside the rectangle `[x0, y0, x1, y1]` (Liang and Barsky, 1984), as runs: a segment
/// leaving the rectangle ends a run; points that are not finite break the line.
fn clipped_runs(pts: &[(f32, f32)], r: [f32; 4]) -> Vec<Vec<(f32, f32)>> {
    let inside = |p: (f32, f32)| p.0.is_finite() && p.1.is_finite() && p.0 >= r[0] && p.0 <= r[2] && p.1 >= r[1] && p.1 <= r[3];
    if pts.iter().all(|&p| inside(p)) {
        return vec![pts.to_vec()];
    }
    let mut runs: Vec<Vec<(f32, f32)>> = vec![];
    let mut cur: Vec<(f32, f32)> = vec![];
    for w in pts.windows(2) {
        let (a, b) = (w[0], w[1]);
        let seg = if a.0.is_finite() && a.1.is_finite() && b.0.is_finite() && b.1.is_finite() {
            let (a, b) = ((a.0 as f64, a.1 as f64), (b.0 as f64, b.1 as f64));
            let (d0, d1) = (b.0 - a.0, b.1 - a.1);
            let (mut lo, mut hi) = (0.0f64, 1.0f64);
            let mut ok = true;
            for (q, dq, l, h) in [(a.0, d0, r[0] as f64, r[2] as f64), (a.1, d1, r[1] as f64, r[3] as f64)] {
                if dq == 0.0 {
                    ok &= q >= l && q <= h;
                } else {
                    let (t0, t1) = ((l - q) / dq, (h - q) / dq);
                    lo = lo.max(t0.min(t1));
                    hi = hi.min(t0.max(t1));
                }
            }
            (ok && lo <= hi).then(|| {
                let at = |t: f64| ((a.0 + t * d0) as f32, (a.1 + t * d1) as f32);
                (at(lo), at(hi), lo > 0.0, hi < 1.0)
            })
        } else {
            None
        };
        match seg {
            Some((p, q, cut_start, cut_end)) => {
                if cut_start && !cur.is_empty() {
                    runs.push(std::mem::take(&mut cur));
                }
                if cur.is_empty() {
                    cur.push(p);
                }
                cur.push(q);
                if cut_end {
                    runs.push(std::mem::take(&mut cur));
                }
            }
            None => {
                if !cur.is_empty() {
                    runs.push(std::mem::take(&mut cur));
                }
            }
        }
    }
    if !cur.is_empty() {
        runs.push(cur);
    }
    runs
}

/// Paint one primitive.
fn item(g: &Graphic, p: &Plan, it: &Item, s: &mut dyn Surface) {
    let st = &g.styles[it.style];
    // lines, points and text in the style's colour; faces and images set their own role
    s.role(st.role);
    let aw = p.area[2];
    let run = |r: &[Pt]| r.iter().map(|&q| p.at(q)).collect::<Vec<_>>();
    match &it.shape {
        Shape::Points(ps) => {
            let c: Vec<(f32, f32)> = ps.iter().map(|&q| p.at(q)).collect();
            s.dots(&c, st.point_size.at(aw as f64) as f32, st.ink());
        }
        Shape::Line(rs) => {
            let mut path = Path::default();
            // far outside the page a stroke is not drawn, but its coordinates can overflow the rasterizer's
            // stroker: runs are cut at a rectangle around the page
            let reach = p.area[2].max(p.area[3]) * 4.0;
            let keep = [p.area[0] - reach, p.area[1] - reach, p.area[0] + p.area[2] + reach, p.area[1] + p.area[3] + reach];
            for r in rs {
                for part in clipped_runs(&run(r), keep) {
                    path.add_polyline(&part, false);
                }
            }
            if !path.is_empty() {
                s.stroke(&path, &pen_of(st, aw));
            }
        }
        Shape::Polygon(rs) => {
            let mut path = Path::default();
            for r in rs.iter().filter(|r| r.len() >= 2) {
                path.add_polyline(&run(r), true);
            }
            if !path.is_empty() {
                face(s, &path, st, aw);
            }
        }
        Shape::Rect(a, b) => {
            let ((x0, y0), (x1, y1)) = (p.at(*a), p.at(*b));
            let path = Path::rect(x0.min(x1), y0.min(y1), (x1 - x0).abs(), (y1 - y0).abs());
            face(s, &path, st, aw);
        }
        Shape::Ellipse { c, r, arc, filled } => {
            let centre = p.at(*c);
            let radii = if c.scaled { (r.0 as f32 * aw, r.1 as f32 * p.area[3]) } else { p.scale(*r) };
            let path = match arc {
                None => Path::ellipse(centre.0, centre.1, radii.0, radii.1),
                Some((a0, a1)) => {
                    let mut path = Path::default();
                    if *filled {
                        path.move_to(centre.0, centre.1);
                        path.arc(centre, radii, *a0, *a1, true);
                        path.close();
                    } else {
                        path.arc(centre, radii, *a0, *a1, false);
                    }
                    path
                }
            };
            if *filled {
                face(s, &path, st, aw);
            } else {
                s.stroke(&path, &pen_of(st, aw));
            }
        }
        Shape::Arrow(rs) => {
            let pen = pen_of(st, aw);
            let heads = match st.arrowheads {
                None => [0.0, 4.0 * pen.width + 4.0],
                Some([a, b]) => [a as f32 * aw, b as f32 * aw],
            };
            for r in rs {
                arrow(s, &run(r), &pen, heads);
            }
        }
        Shape::Raster { cols, rows, rgba, from, to } => {
            s.role(Role::Data);
            let ((x0, y0), (x1, y1)) = (p.at(*from), p.at(*to));
            s.image([x0.min(x1), y0.min(y1), (x1 - x0).abs(), (y1 - y0).abs()], *cols, *rows, rgba);
        }
        Shape::Bezier(ps) => {
            let q = run(ps);
            let Some(&(x, y)) = q.first() else { return };
            let mut path = Path::default();
            path.move_to(x, y);
            let mut i = 0;
            while i + 1 < q.len() {
                let left = q.len() - 1 - i;
                let (p0, p1) = (q[i], q[i + 1]);
                if left >= 3 {
                    let (p2, p3) = (q[i + 2], q[i + 3]);
                    path.cubic_to(p1.0, p1.1, p2.0, p2.1, p3.0, p3.1);
                    i += 3;
                } else if left == 2 {
                    // a quadratic as a cubic: control points two thirds of the way to the middle point
                    let p2 = q[i + 2];
                    let c1 = (p0.0 + 2.0 / 3.0 * (p1.0 - p0.0), p0.1 + 2.0 / 3.0 * (p1.1 - p0.1));
                    let c2 = (p2.0 + 2.0 / 3.0 * (p1.0 - p2.0), p2.1 + 2.0 / 3.0 * (p1.1 - p2.1));
                    path.cubic_to(c1.0, c1.1, c2.0, c2.1, p2.0, p2.1);
                    i += 2;
                } else {
                    path.line_to(p1.0, p1.1);
                    i += 1;
                }
            }
            s.stroke(&path, &pen_of(st, aw));
        }
        Shape::Inset { graphic, natural, at, offset, width } => {
            let (x, y) = p.at(*at);
            let w = match width {
                Some(w) if at.scaled => *w as f32 * aw,
                Some(w) => p.scale((*w, 0.0)).0,
                None => natural.0 as f32,
            };
            let k = w / (natural.0 as f32).max(1e-6);
            let ht = natural.1 as f32 * k;
            let left = x - (1.0 + offset.0 as f32) / 2.0 * w;
            let top = y - (1.0 - offset.1 as f32) / 2.0 * ht;
            if let Ok(sub) = plan(graphic, &|| false) {
                s.role(Role::Data);
                let mut placed = crate::surface::Placed { inner: s, dx: left, dy: top, s: k };
                let _ = paint(graphic, &sub, &mut placed, &|| false);
            }
        }
        Shape::Text { label, at, offset } => {
            let size = st.font_size.unwrap_or(g.opts.font_size) as f32;
            let t = typeset(label, size);
            let (x, y) = p.at(*at);
            let left = x - (1.0 + offset.0 as f32) / 2.0 * t.width;
            let top = y - (1.0 - offset.1 as f32) / 2.0 * t.height;
            draw_label(s, &t, (left, top), false, st.ink());
        }
    }
}

/// A polyline with arrowheads `heads` long (points) at its start and its end (0: none there).
fn arrow(s: &mut dyn Surface, pts: &[(f32, f32)], pen: &Pen, heads: [f32; 2]) {
    let n = pts.len();
    if n < 2 {
        return;
    }
    let mut shaft = pts.to_vec();
    let mut tips = vec![];
    for (end, len) in [(false, heads[0]), (true, heads[1])] {
        let (a, b) = if end { (pts[n - 2], pts[n - 1]) } else { (pts[1], pts[0]) };
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let d = (dx * dx + dy * dy).sqrt();
        if d == 0.0 || len <= 0.0 {
            continue;
        }
        let (ux, uy) = (dx / d, dy / d);
        let len = len.min(d);
        let half = 0.4 * len;
        // the shaft stops under the head
        let at = if end { n - 1 } else { 0 };
        shaft[at] = (b.0 - ux * len * 0.8, b.1 - uy * len * 0.8);
        let base = (b.0 - ux * len, b.1 - uy * len);
        tips.push([b, (base.0 - uy * half, base.1 + ux * half), (base.0 + uy * half, base.1 - ux * half)]);
    }
    s.stroke(&Path::polyline(&shaft, false), pen);
    for head in tips {
        s.fill(&Path::polyline(&head, true), pen.color);
    }
}

/// Draw a typeset label with its top-left corner at `origin` (turned upwards: see [`Text::place`]).
fn draw_label(s: &mut dyn Surface, t: &Typeset, origin: (f32, f32), up: bool, color: Rgba) {
    s.text(&Text { glyphs: &t.glyphs, rules: &t.rules, text: &t.text, origin, up, color });
}

/// A chrome label: [`draw_label`] in [`TEXT`] as [`Role::Text`].
fn chrome_label(s: &mut dyn Surface, t: &Typeset, origin: (f32, f32), up: bool) {
    s.role(Role::Text);
    draw_label(s, t, origin, up, TEXT);
}

/// The frame or axes, their ticks and labels, the frame and axes labels and the plot label.
fn decorations(g: &Graphic, p: &Plan, s: &mut dyn Surface) {
    let o = &g.opts;
    let [ax, ay, aw, ah] = p.area;
    let fs = o.font_size as f32;
    let lfs = o.label_size.map_or(fs, |v| v as f32);
    let thin = Pen { color: FRAME, width: 1.0, dash: vec![], cap: Cap::Butt };
    if o.frame {
        s.role(Role::Frame);
        s.stroke(&Path::rect(ax, ay, aw, ah), &thin);
        let mut marks = Path::default();
        for (u, l) in &p.ticks[0] {
            let x = p.px(*u);
            marks.add_polyline(&[(x, ay + ah), (x, ay + ah - TICK)], false);
            marks.add_polyline(&[(x, ay), (x, ay + TICK)], false);
            if let Some(t) = l {
                chrome_label(s, t, (x - t.width / 2.0, ay + ah + 2.0), false);
            }
        }
        for (u, l) in &p.ticks[1] {
            let y = p.py(*u);
            marks.add_polyline(&[(ax, y), (ax + TICK, y)], false);
            marks.add_polyline(&[(ax + aw, y), (ax + aw - TICK, y)], false);
            if let Some(t) = l {
                chrome_label(s, t, (ax - GAP - t.width, y - t.height / 2.0), false);
            }
        }
        for &u in &p.minor[0] {
            let x = p.px(u);
            marks.add_polyline(&[(x, ay + ah), (x, ay + ah - TICK / 2.0)], false);
            marks.add_polyline(&[(x, ay), (x, ay + TICK / 2.0)], false);
        }
        for &u in &p.minor[1] {
            let y = p.py(u);
            marks.add_polyline(&[(ax, y), (ax + TICK / 2.0, y)], false);
            marks.add_polyline(&[(ax + aw, y), (ax + aw - TICK / 2.0, y)], false);
        }
        s.role(Role::Frame);
        s.stroke(&marks, &thin);
        let tick_h = p.ticks[0].iter().filter_map(|t| t.1.as_ref()).map(|t| t.height).fold(0.0, f32::max);
        let tick_w = p.ticks[1].iter().filter_map(|t| t.1.as_ref()).map(|t| t.width).fold(0.0, f32::max);
        if let Some(l) = &o.frame_label[0] {
            let t = typeset(l, lfs);
            chrome_label(s, &t, (ax + aw / 2.0 - t.width / 2.0, ay + ah + 2.0 + tick_h + GAP), false);
        }
        if let Some(l) = &o.frame_label[1] {
            let t = typeset(l, lfs);
            let left = ax - GAP - tick_w - GAP - t.height;
            chrome_label(s, &t, (left, ay + ah / 2.0 + t.width / 2.0), true);
        }
    } else {
        // axes through the origin (or along the lower edge)
        let (oy, ox) = (p.py(p.origin.0), p.px(p.origin.1));
        let mut lines = Path::default();
        if o.axes[0] {
            lines.add_polyline(&[(ax, oy), (ax + aw, oy)], false);
            for (u, l) in &p.ticks[0] {
                let x = p.px(*u);
                lines.add_polyline(&[(x, oy), (x, oy + TICK)], false);
                let crossing = o.axes[1] && *u == p.origin.1 && p.origin.1 != p.range[0].0;
                if let (Some(t), false) = (l, crossing) {
                    chrome_label(s, t, (x - t.width / 2.0, oy + TICK + 1.0), false);
                }
            }
            for &u in &p.minor[0] {
                let x = p.px(u);
                lines.add_polyline(&[(x, oy), (x, oy + TICK / 2.0)], false);
            }
            if let Some(l) = &o.axes_label[0] {
                let t = typeset(l, lfs);
                chrome_label(s, &t, (ax + aw + GAP, oy - t.height / 2.0), false);
            }
        }
        if o.axes[1] {
            lines.add_polyline(&[(ox, ay), (ox, ay + ah)], false);
            for (u, l) in &p.ticks[1] {
                let y = p.py(*u);
                lines.add_polyline(&[(ox, y), (ox - TICK, y)], false);
                let crossing = o.axes[0] && *u == p.origin.0 && p.origin.0 != p.range[1].0;
                if let (Some(t), false) = (l, crossing) {
                    chrome_label(s, t, (ox - TICK - 1.0 - t.width, y - t.height / 2.0), false);
                }
            }
            for &u in &p.minor[1] {
                let y = p.py(u);
                lines.add_polyline(&[(ox, y), (ox - TICK / 2.0, y)], false);
            }
            if let Some(l) = &o.axes_label[1] {
                let t = typeset(l, lfs);
                chrome_label(s, &t, (ox - t.width / 2.0, ay - GAP - t.height), false);
            }
        }
        if !lines.is_empty() {
            s.role(Role::Frame);
            s.stroke(&lines, &thin);
        }
    }
    if let Some(l) = &o.plot_label {
        let t = typeset(l, fs * 1.15);
        let top = if o.axes[1] && o.axes_label[1].is_some() && !o.frame {
            ay - GAP - typeset(o.axes_label[1].as_ref().unwrap_or(l), lfs).height - GAP - t.height
        } else {
            ay - GAP - t.height - if p.ticks[1].is_empty() { 0.0 } else { 2.0 }
        };
        chrome_label(s, &t, (ax + aw / 2.0 - t.width / 2.0, top.max(0.0)), false);
    }
}

/// The legend: a framed box in a corner of the plot area with a sample of each entry's style and its label.
fn legend(g: &Graphic, p: &Plan, s: &mut dyn Surface) {
    let o = &g.opts;
    if o.legend.is_empty() {
        return;
    }
    let fs = o.legend_font_size.unwrap_or(o.font_size) as f32;
    const PAD: f32 = 8.0;
    // the sample grows with the text, so that big labels keep a sample to match
    let sample = 24.0f32.max(fs * 2.4);
    let labels: Vec<Typeset> = o.legend.iter().map(|e| typeset(&e.label, fs)).collect();
    let has_sample = o.legend.iter().any(|e| e.marker != Marker::None);
    let sample_w = if has_sample { sample + 6.0 } else { 0.0 };
    let title = o.legend_title.as_ref().map(|l| typeset(l, fs));
    let title_h = title.as_ref().map_or(0.0, |t| t.height.max(fs * 1.4) + 2.0);
    let text_w = labels.iter().map(|t| t.width).fold(0.0, f32::max);
    let text_w = text_w.max(title.as_ref().map_or(0.0, |t| t.width - sample_w));
    let rows: Vec<f32> = labels.iter().map(|t| t.height.max(fs * 1.4)).collect();
    // in a row: each entry its sample and label wide, SPACE between entries
    const SPACE: f32 = 12.0;
    let row_w: f32 = labels.iter().map(|t| sample_w + t.width).sum::<f32>() + SPACE * labels.len().saturating_sub(1) as f32;
    let row_h = rows.iter().copied().fold(0.0, f32::max);
    let (w, h) = if o.legend_row {
        (PAD + row_w.max(title.as_ref().map_or(0.0, |t| t.width)) + PAD, 2.0 * PAD - 4.0 + row_h + title_h)
    } else {
        (PAD + sample_w + text_w + PAD, 2.0 * PAD - 4.0 + rows.iter().sum::<f32>() + title_h)
    };
    let [ax, ay, aw, ah] = p.area;
    let x = match o.legend_position {
        Corner::TopLeft | Corner::BottomLeft => ax + 10.0,
        Corner::TopRight | Corner::BottomRight => ax + aw - 10.0 - w,
    };
    let y = match o.legend_position {
        Corner::TopLeft | Corner::TopRight => ay + 10.0,
        Corner::BottomLeft | Corner::BottomRight => ay + ah - 10.0 - h,
    };
    let bx = Path::rounded_rect(x, y, w, h, o.legend_frame.unwrap_or(0.0) as f32);
    s.role(Role::Panel);
    s.fill(&bx, Rgba::WHITE);
    if o.legend_frame.is_some() {
        s.role(Role::Frame);
        s.stroke(&bx, &Pen { color: FRAME, width: 1.0, dash: vec![], cap: Cap::Butt });
    }
    s.clip([x, y, w, h]);
    let mut top = y + PAD - 2.0;
    if let Some(t) = &title {
        chrome_label(s, t, (x + (w - t.width) / 2.0, top + (title_h - 2.0 - t.height) / 2.0), false);
        top += title_h;
        let rule = Pen { color: Rgba { a: 0.35, ..FRAME }, width: 0.5, dash: vec![], cap: Cap::Butt };
        s.role(Role::Frame);
        s.stroke(&Path::polyline(&[(x + PAD / 2.0, top - 2.0), (x + w - PAD / 2.0, top - 2.0)], false), &rule);
    }
    let mut left = x + PAD;
    for ((e, t), row) in o.legend.iter().zip(&labels).zip(&rows) {
        let row = if o.legend_row { &row_h } else { row };
        let mid = top + row / 2.0;
        let st = &g.styles[e.style];
        let x0 = left;
        s.role(st.role);
        match e.marker {
            Marker::Line => {
                let mut pen = pen_of(st, aw);
                pen.width = pen.width.min(row / 2.0);
                s.stroke(&Path::polyline(&[(x0, mid), (x0 + sample, mid)], false), &pen);
            }
            Marker::Point => {
                let d = (st.point_size.at(aw as f64) as f32).clamp(2.0, row * 0.8);
                s.dots(&[(x0 + sample / 2.0, mid)], d, st.ink());
            }
            Marker::Swatch => {
                let q = row * 0.7;
                let path = Path::rect(x0 + sample / 2.0 - q / 2.0, mid - q / 2.0, q, q);
                face(s, &path, st, aw);
            }
            Marker::None => {}
        }
        chrome_label(s, t, (x0 + sample_w, mid - t.height / 2.0), false);
        if o.legend_row {
            left += sample_w + t.width + SPACE;
        } else {
            top += row;
        }
    }
    s.unclip();
}

#[cfg(test)]
mod tests {
    use super::clipped_runs;

    #[test]
    fn far_segments_are_cut_at_the_rectangle() {
        let r = [0.0, 0.0, 100.0, 100.0];
        // a steep rise out of the rectangle and back: two runs, every point inside
        let runs = clipped_runs(&[(10.0, 50.0), (11.0, 1.0e30), (12.0, 50.0), (13.0, 60.0)], r);
        assert_eq!(runs.len(), 2);
        assert!(runs.iter().flatten().all(|p| p.0 >= 0.0 && p.0 <= 100.0 && p.1 >= 0.0 && p.1 <= 100.0));
        // inside: unchanged; not finite: a break
        assert_eq!(clipped_runs(&[(1.0, 1.0), (2.0, 2.0)], r), vec![vec![(1.0, 1.0), (2.0, 2.0)]]);
        assert_eq!(clipped_runs(&[(1.0, 1.0), (2.0, f32::INFINITY), (3.0, 3.0)], r).len(), 0);
    }
}
