//! Coordinates under the pointer on plots (decision NB5.2). A plot picture `plot-N.png` may have a description
//! `plot-N.json` next to it (written by the plotting library, version 1): the PNG's size, the plot area in PNG
//! pixels, the axes' ranges and scales, and the series' points in data coordinates. While the pointer is over such
//! a picture the nearest series point (or the nearest place on a line) within a snap radius gets a marker, guide
//! lines and a tooltip with its coordinates; elsewhere on the picture the tooltip gives the pointer's own data
//! coordinates. Points are indexed in a grid of PNG-pixel cells, so a query looks at a few cells whatever the number
//! of points. Pictures without a description (photographs, PDF pages) show nothing.
use mc_ui::fonts::Face;
use crate::theme::Palette;
use eframe::egui::{self, pos2, vec2, Color32, Painter, Pos2, Rect, Stroke};
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

/// The snap radius in points on the screen.
const SNAP: f32 = 12.0;
/// On a line, a place this close (points) to a vertex snaps to the vertex.
const VERTEX_SNAP: f32 = 4.0;
/// The grid cell side in PNG pixels.
const CELL: f32 = 16.0;
/// Points read per series at most (the description promises 20,000; a damaged file cannot make more work).
const MAX_POINTS: usize = 200_000;
/// Seconds between checks of a description's modification time.
const RECHECK: f64 = 0.5;

/// How an axis maps data to its length.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Scale {
    /// proportional
    Linear,
    /// base-10 logarithm (positive values only)
    Log,
    /// `v/c` for `|v| <= c`, else `sign(v) (1 + log10(|v|/c))`: linear near 0, logarithmic far from it (plot
    /// descriptions before round 13, which give only `c`)
    SignedLog(f64),
    /// `sign(v) log10(a + k |v|)`, 0 at 0 (round 13: the description gives `a` and `k` as the renderer maps it)
    SignedLogAk(f64, f64),
}

impl Scale {
    fn fwd(self, v: f64) -> f64 {
        match self {
            Scale::Linear => v,
            Scale::Log => {
                if v > 0.0 {
                    v.log10()
                } else {
                    f64::NAN
                }
            }
            Scale::SignedLogAk(a, k) => {
                if v == 0.0 {
                    0.0
                } else {
                    v.signum() * (a + k * v.abs()).log10()
                }
            }
            Scale::SignedLog(c) => {
                if v.abs() <= c {
                    v / c
                } else {
                    v.signum() * (1.0 + (v.abs() / c).log10())
                }
            }
        }
    }

    fn inv(self, t: f64) -> f64 {
        match self {
            Scale::Linear => t,
            Scale::Log => 10f64.powf(t),
            // between -log10(a) and log10(a) only 0 has a place
            Scale::SignedLogAk(a, k) => t.signum() * ((10f64.powf(t.abs()) - a) / k).max(0.0),
            Scale::SignedLog(c) => {
                if t.abs() <= 1.0 {
                    t * c
                } else {
                    t.signum() * c * 10f64.powf(t.abs() - 1.0)
                }
            }
        }
    }
}

/// One axis: its range and scale.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Axis {
    /// the value at the start of the plot area (left, bottom)
    pub min: f64,
    /// the value at its end
    pub max: f64,
    /// the scale
    pub scale: Scale,
}

impl Axis {
    /// The share (0 at `min`, 1 at `max`) of the axis a value is at; NaN where the scale has no place for it.
    pub fn unit(&self, v: f64) -> f64 {
        let (a, b) = (self.scale.fwd(self.min), self.scale.fwd(self.max));
        (self.scale.fwd(v) - a) / (b - a)
    }

    /// The value at a share of the axis.
    pub fn value(&self, u: f64) -> f64 {
        let (a, b) = (self.scale.fwd(self.min), self.scale.fwd(self.max));
        self.scale.inv(a + u * (b - a))
    }
}

/// A series of the plot.
#[derive(Clone, Debug)]
pub struct Series {
    /// its name
    pub name: String,
    /// its colour
    pub color: Color32,
    /// drawn as a line through its points (else as separate points)
    pub line: bool,
    /// the points in data coordinates
    pub data: Vec<[f64; 2]>,
    /// the points in PNG pixels (NaN where a point has no place, as 0 on a log axis)
    px: Vec<Pos2>,
}

/// A plot's description with its point index.
#[derive(Debug)]
pub struct PlotInfo {
    /// the PNG's size in pixels
    pub size: [f32; 2],
    /// the plot area in PNG pixels, y down
    pub area: Rect,
    /// the horizontal axis
    pub x: Axis,
    /// the vertical axis
    pub y: Axis,
    /// the series
    pub series: Vec<Series>,
    /// grid of cells `CELL` PNG pixels wide: (series, index) of the points (point series) or segments (line series,
    /// segment k joins points k and k+1) that pass through each cell
    grid: Vec<Vec<(u32, u32)>>,
    cols: usize,
    rows: usize,
}

/// What the pointer is on.
#[derive(Clone, Debug, PartialEq)]
pub enum Found {
    /// a series point or a place on a line: the series, the place in PNG pixels and its data coordinates
    Point(usize, Pos2, [f64; 2]),
    /// no point near: the pointer's own data coordinates
    Free([f64; 2]),
}

fn number(v: &Value) -> Option<f64> {
    v.as_f64().filter(|x| x.is_finite())
}

fn axis(v: &Value) -> Option<Axis> {
    let min = number(v.get("min")?)?;
    let max = number(v.get("max")?)?;
    let scale = match v.get("scale").and_then(Value::as_str).unwrap_or("linear") {
        "linear" => Scale::Linear,
        "log" => Scale::Log,
        "signed_log" => {
            let ak = (v.get("a").and_then(number), v.get("k").and_then(number));
            match ak {
                (Some(a), Some(k)) if a > 0.0 && k > 0.0 => Scale::SignedLogAk(a, k),
                _ => Scale::SignedLog(v.get("c").and_then(number).filter(|c| *c > 0.0).unwrap_or(1.0)),
            }
        }
        _ => return None,
    };
    let a = Axis { min, max, scale };
    let span = a.scale.fwd(max) - a.scale.fwd(min);
    (span.is_finite() && span != 0.0).then_some(a)
}

fn colour(s: &str) -> Option<Color32> {
    let h = s.strip_prefix('#')?;
    if h.len() != 6 {
        return None;
    }
    let n = u32::from_str_radix(h, 16).ok()?;
    Some(Color32::from_rgb((n >> 16) as u8, (n >> 8) as u8, n as u8))
}

impl PlotInfo {
    /// Read a description (version 1); `None` for anything else.
    pub fn parse(text: &str) -> Option<PlotInfo> {
        let v: Value = serde_json::from_str(text).ok()?;
        if v.get("version")?.as_u64()? != 1 {
            return None;
        }
        let size = [number(v.get("width")?)? as f32, number(v.get("height")?)? as f32];
        let a: Vec<f32> = v.get("plot_area")?.as_array()?.iter().filter_map(number).map(|x| x as f32).collect();
        let [x0, y0, x1, y1] = a[..] else { return None };
        let area = Rect::from_min_max(pos2(x0.min(x1), y0.min(y1)), pos2(x0.max(x1), y0.max(y1)));
        if size[0] <= 0.0 || size[1] <= 0.0 || area.width() <= 0.0 || area.height() <= 0.0 {
            return None;
        }
        let (x, y) = (axis(v.get("x")?)?, axis(v.get("y")?)?);
        let mut series = vec![];
        for s in v.get("series").and_then(Value::as_array).into_iter().flatten() {
            let data: Vec<[f64; 2]> = s
                .get("points")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .take(MAX_POINTS)
                .filter_map(|p| {
                    let p = p.as_array()?;
                    Some([p.first().and_then(number)?, p.get(1).and_then(number)?])
                })
                .collect();
            let px = data
                .iter()
                .map(|&[dx, dy]| {
                    let (u, w) = (x.unit(dx), y.unit(dy));
                    pos2(area.left() + u as f32 * area.width(), area.bottom() - w as f32 * area.height())
                })
                .collect();
            series.push(Series {
                name: s.get("name").and_then(Value::as_str).unwrap_or_default().to_string(),
                color: s.get("color").and_then(Value::as_str).and_then(colour).unwrap_or(Color32::GRAY),
                line: s.get("kind").and_then(Value::as_str) != Some("points"),
                data,
                px,
            });
        }
        let cols = (size[0] / CELL).ceil().max(1.0) as usize;
        let rows = (size[1] / CELL).ceil().max(1.0) as usize;
        let mut info = PlotInfo { size, area, x, y, series, grid: vec![vec![]; cols * rows], cols, rows };
        info.index();
        Some(info)
    }

    fn cell_of(&self, p: Pos2) -> Option<usize> {
        if !(p.x.is_finite() && p.y.is_finite()) || p.x < 0.0 || p.y < 0.0 {
            return None;
        }
        let (c, r) = ((p.x / CELL) as usize, (p.y / CELL) as usize);
        (c < self.cols && r < self.rows).then_some(r * self.cols + c)
    }

    /// Put every point (point series) or segment (line series) into the cells it passes through.
    fn index(&mut self) {
        let mut grid = std::mem::take(&mut self.grid);
        for (si, s) in self.series.iter().enumerate() {
            if !s.line || s.px.len() == 1 {
                for (k, &p) in s.px.iter().enumerate() {
                    if let Some(c) = self.cell_of(p) {
                        grid[c].push((si as u32, k as u32));
                    }
                }
                continue;
            }
            for k in 0..s.px.len().saturating_sub(1) {
                let (a, b) = (s.px[k], s.px[k + 1]);
                if !(a.x.is_finite() && a.y.is_finite() && b.x.is_finite() && b.y.is_finite()) {
                    continue;
                }
                // samples along the segment, at most half a cell apart, clipped to the picture
                let steps = (((b - a).length() / (CELL * 0.5)).ceil() as usize).clamp(1, 4 * (self.cols + self.rows));
                let mut last = usize::MAX;
                for j in 0..=steps {
                    let p = a + (b - a) * (j as f32 / steps as f32);
                    if let Some(c) = self.cell_of(p).filter(|&c| c != last) {
                        grid[c].push((si as u32, k as u32));
                        last = c;
                    }
                }
            }
        }
        self.grid = grid;
    }

    /// The data coordinates of a place in PNG pixels.
    pub fn data_at(&self, p: Pos2) -> [f64; 2] {
        let u = f64::from((p.x - self.area.left()) / self.area.width());
        let w = f64::from((self.area.bottom() - p.y) / self.area.height());
        [self.x.value(u), self.y.value(w)]
    }

    /// What is at `p` (PNG pixels): the nearest point or place on a line within `radius` PNG pixels (a vertex when
    /// within `vertex` pixels of it), else the place's own coordinates.
    pub fn find(&self, p: Pos2, radius: f32, vertex: f32) -> Found {
        let mut best: Option<(f32, usize, Pos2, [f64; 2])> = None;
        let r = (radius / CELL).ceil() as i64;
        let (pc, pr) = ((p.x / CELL).floor() as i64, (p.y / CELL).floor() as i64);
        for row in (pr - r).max(0)..=(pr + r).min(self.rows as i64 - 1) {
            for col in (pc - r).max(0)..=(pc + r).min(self.cols as i64 - 1) {
                for &(si, k) in &self.grid[row as usize * self.cols + col as usize] {
                    let (si, k) = (si as usize, k as usize);
                    let s = &self.series[si];
                    let (at, data) = if s.line && s.px.len() > 1 {
                        let (a, b) = (s.px[k], s.px[k + 1]);
                        let ab = b - a;
                        let t = if ab.length_sq() > 0.0 { ((p - a).dot(ab) / ab.length_sq()).clamp(0.0, 1.0) } else { 0.0 };
                        let q = a + ab * t;
                        if (q - a).length() <= vertex {
                            (a, s.data[k])
                        } else if (q - b).length() <= vertex {
                            (b, s.data[k + 1])
                        } else {
                            (q, self.data_at(q))
                        }
                    } else {
                        (s.px[k], s.data[k])
                    };
                    let d = (at - p).length();
                    if d <= radius && best.is_none_or(|(bd, ..)| d < bd) {
                        best = Some((d, si, at, data));
                    }
                }
            }
        }
        match best {
            Some((_, si, at, data)) => Found::Point(si, at, data),
            None => Found::Free(self.data_at(p)),
        }
    }
}

/// A number with 6 significant digits, without trailing zeros (`0.333333`, `1.5`, `2.5e-7`).
pub fn format_number(v: f64) -> String {
    if v == 0.0 {
        return "0".into();
    }
    if !v.is_finite() {
        return if v.is_nan() { "?".into() } else if v > 0.0 { "∞".into() } else { "-∞".into() };
    }
    let e = v.abs().log10().floor() as i32;
    let trim = |s: String| {
        if s.contains('.') {
            s.trim_end_matches('0').trim_end_matches('.').to_string()
        } else {
            s
        }
    };
    if (-4..6).contains(&e) {
        trim(format!("{:.*}", (5 - e).max(0) as usize, v))
    } else {
        let s = format!("{v:.5e}");
        let (m, x) = s.split_once('e').unwrap_or((&s, "0"));
        format!("{}e{x}", trim(m.to_string()))
    }
}

/// The plot descriptions read so far, by picture path.
#[derive(Default)]
pub struct Plots {
    known: HashMap<PathBuf, Entry>,
}

struct Entry {
    stamp: Option<SystemTime>,
    checked: f64,
    info: Option<Arc<PlotInfo>>,
}

impl Plots {
    /// The description of the picture at `picture` (its `.json` sibling), read again when the file changes.
    pub fn get(&mut self, picture: &Path, now: f64) -> Option<Arc<PlotInfo>> {
        let json = picture.with_extension("json");
        if let Some(e) = self.known.get(picture) {
            if now - e.checked < RECHECK {
                return e.info.clone();
            }
        }
        let stamp = std::fs::metadata(&json).and_then(|m| m.modified()).ok();
        if let Some(e) = self.known.get_mut(picture) {
            e.checked = now;
            if e.stamp == stamp {
                return e.info.clone();
            }
        }
        let info = stamp.and_then(|_| std::fs::read_to_string(&json).ok()).and_then(|t| PlotInfo::parse(&t)).map(Arc::new);
        if self.known.len() > 256 {
            self.known.clear();
        }
        self.known.insert(picture.to_path_buf(), Entry { stamp, checked: now, info: info.clone() });
        info
    }
}

/// Paint the hover marks for the pointer at `pointer` (screen) over a plot drawn in `rect` (screen): marker, guide
/// lines and the tooltip.
pub fn paint(ctx: &egui::Context, painter: &Painter, info: &PlotInfo, rect: Rect, pointer: Pos2, p: &Palette) {
    let k = vec2(rect.width() / info.size[0], rect.height() / info.size[1]);
    let to_screen = |q: Pos2| rect.min + vec2(q.x * k.x, q.y * k.y);
    let to_png = |q: Pos2| pos2((q.x - rect.left()) / k.x, (q.y - rect.top()) / k.y);
    let scale = 1.0 / k.x.max(1e-6);
    let found = info.find(to_png(pointer), SNAP * scale, VERTEX_SNAP * scale);
    let area = Rect::from_min_max(to_screen(info.area.min), to_screen(info.area.max));
    let guide = Stroke::new(1.0, p.over_bg(p.weak, 0.55));
    let (text, color, at) = match &found {
        Found::Point(si, q, [x, y]) => {
            let s = &info.series[*si];
            let at = to_screen(*q);
            painter.vline(at.x, area.y_range(), guide);
            painter.hline(area.x_range(), at.y, guide);
            painter.circle(at, 4.0, s.color, Stroke::new(1.5, p.bg));
            let name = if info.series.len() > 1 && !s.name.is_empty() { format!("{}: ", s.name) } else { String::new() };
            (format!("{name}x = {}, y = {}", format_number(*x), format_number(*y)), p.text, at)
        }
        Found::Free([x, y]) => {
            if area.contains(pointer) {
                painter.vline(pointer.x, area.y_range(), guide);
                painter.hline(area.x_range(), pointer.y, guide);
            }
            (format!("x ≈ {}, y ≈ {}", format_number(*x), format_number(*y)), p.weak, pointer)
        }
    };
    // the tooltip above the layer of the notebook, beside the point, kept inside the window
    let layer = egui::LayerId::new(egui::Order::Tooltip, egui::Id::unique("mc-notebook-plot-hover"));
    let tip = ctx.layer_painter(layer);
    let font = Face::Ui.font(12.0);
    let g = tip.layout_no_wrap(text, font, color);
    let pad = vec2(6.0, 3.0);
    let size = g.size() + pad * 2.0;
    let screen = ctx.content_rect();
    let mut min = at + vec2(12.0, -size.y - 10.0);
    if min.x + size.x > screen.right() {
        min.x = at.x - 12.0 - size.x;
    }
    if min.y < screen.top() {
        min.y = at.y + 14.0;
    }
    let r = Rect::from_min_size(min, size);
    tip.rect(r, 3.0, p.bg, Stroke::new(1.0, p.border_strong()), egui::StrokeKind::Inside);
    tip.galley(r.min + pad, g, color);
}

#[cfg(test)]
mod tests {
    use super::*;

    const SINE: &str = r##"{"version":1, "width":640, "height":400, "plot_area":[60, 20, 620, 360],
        "x":{"min":0, "max":10, "scale":"linear"}, "y":{"min":-1, "max":1, "scale":"linear"},
        "series":[{"name":"a", "color":"#1f77b4", "kind":"line", "points":[[0,0],[5,1],[10,0]]},
                  {"name":"b", "color":"#ff7f0e", "kind":"points", "points":[[2,-0.5]]}]}"##;

    #[test]
    fn descriptions_are_read_and_rejected() {
        let p = PlotInfo::parse(SINE).unwrap();
        assert_eq!(p.series.len(), 2);
        assert_eq!(p.series[0].color, Color32::from_rgb(0x1f, 0x77, 0xb4));
        assert!(p.series[0].line && !p.series[1].line);
        assert!(PlotInfo::parse("{}").is_none());
        assert!(PlotInfo::parse(&SINE.replace("\"version\":1", "\"version\":2")).is_none());
        assert!(PlotInfo::parse(&SINE.replace("\"max\":10", "\"max\":0")).is_none());
    }

    #[test]
    fn the_nearest_vertex_line_place_or_free_coordinates() {
        let p = PlotInfo::parse(SINE).unwrap();
        // the vertex (5, 1) at x = 60 + 280, y = 20
        assert_eq!(p.find(pos2(342.0, 23.0), 12.0, 4.0), Found::Point(0, pos2(340.0, 20.0), [5.0, 1.0]));
        // halfway along the first segment: interpolated
        match p.find(pos2(200.0, 105.0), 12.0, 4.0) {
            Found::Point(0, _, [x, y]) => assert!((x - 2.5).abs() < 0.05 && (y - 0.5).abs() < 0.05, "{x} {y}"),
            f => panic!("{f:?}"),
        }
        // the lone point of series b at (2, -0.5): x = 60 + 112, y = 20 + 255
        assert_eq!(p.find(pos2(170.0, 270.0), 12.0, 4.0), Found::Point(1, pos2(172.0, 275.0), [2.0, -0.5]));
        // nothing near: the place's own coordinates
        match p.find(pos2(620.0, 360.0), 12.0, 4.0) {
            Found::Free([x, y]) => assert!((x - 10.0).abs() < 1e-9 && (y + 1.0).abs() < 1e-9),
            f => panic!("{f:?}"),
        }
    }

    #[test]
    fn log_and_signed_log_axes_map_both_ways() {
        let log = Axis { min: 1.0, max: 1000.0, scale: Scale::Log };
        assert!((log.unit(10.0) - 1.0 / 3.0).abs() < 1e-12);
        assert!((log.value(2.0 / 3.0) - 100.0).abs() < 1e-9);
        assert!(log.unit(-1.0).is_nan());
        let sl = Axis { min: -100.0, max: 100.0, scale: Scale::SignedLog(1.0) };
        assert!((sl.unit(0.0) - 0.5).abs() < 1e-12);
        for v in [-50.0, -0.3, 0.0, 2.0, 99.0] {
            assert!((sl.value(sl.unit(v)) - v).abs() < 1e-9, "{v}");
        }
        // round 13: sign(v) log10(a + k |v|) with the description's constants (threshold 1e-8: k = 1e8, a = 32)
        let ak = axis(&serde_json::json!({"min": -1000.0, "max": 1000.0, "scale": "signed_log", "c": 1e-8, "a": 32.0, "k": 1e8}))
            .unwrap();
        assert!(matches!(ak.scale, Scale::SignedLogAk(..)));
        assert!((ak.unit(0.0) - 0.5).abs() < 1e-12);
        for v in [-1000.0, -3e-5, 1e-8, 0.5, 700.0] {
            assert!((ak.value(ak.unit(v)) - v).abs() < 1e-9 * v.abs().max(1.0), "{v}");
        }
        let expect = |v: f64| (32.0 + 1e8 * v).log10();
        let span = 2.0 * expect(1000.0);
        assert!((ak.unit(1.0) - (0.5 + expect(1.0) / span)).abs() < 1e-12);
    }

    #[test]
    fn a_hundred_thousand_points_answer_quickly() {
        let pts: Vec<String> = (0..100_000).map(|k| format!("[{},{}]", k as f64 / 1e4, (k as f64 / 1e3).sin())).collect();
        let text = format!(
            r##"{{"version":1,"width":800,"height":500,"plot_area":[50,10,790,480],"x":{{"min":0,"max":10}},"y":{{"min":-1,"max":1}},"series":[{{"name":"s","color":"#000000","kind":"line","points":[{}]}}]}}"##,
            pts.join(",")
        );
        let p = PlotInfo::parse(&text).unwrap();
        let t = std::time::Instant::now();
        for k in 0..1000 {
            let _ = p.find(pos2(50.0 + (k % 740) as f32, 10.0 + (k % 470) as f32), 12.0, 4.0);
        }
        let per = t.elapsed().as_secs_f64() / 1000.0;
        println!("hover query over 100,000 points: {:.1} us", per * 1e6);
        assert!(per < 2e-3, "{per}");
    }

    #[test]
    fn numbers_have_six_significant_digits() {
        assert_eq!(format_number(1.0 / 3.0), "0.333333");
        assert_eq!(format_number(1.5), "1.5");
        assert_eq!(format_number(123456.7), "123457");
        assert_eq!(format_number(1234567.0), "1.23457e6");
        assert_eq!(format_number(-2.5e-7), "-2.5e-7");
        assert_eq!(format_number(0.0), "0");
        assert_eq!(format_number(4.1234567), "4.12346");
    }
}
