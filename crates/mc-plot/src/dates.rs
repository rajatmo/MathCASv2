#![allow(clippy::type_complexity)]
//! Plots against dates: `date_list_plot`, `date_list_log_plot`, `date_list_step_plot` and
//! `stacked_date_list_plot`. Dates are `[y]`, `[y, m]`, `[y, m, d]`, `[y, m, d, h, min, s]` lists or strings
//! (`"August 15, 2006"`, `"September 2010"`, `"2006-08-15"`); the x axis is in days, with ticks at round years,
//! months or days labelled as dates. Values without dates are placed at successive days from a start date (default
//! 1 January 1900), or spread over a `[start, end]` range, or stepped by `[start, automatic, "Month" | "Week" |
//! "Day" | "Year" | [y, m, d]]`.
use crate::chartdata::{self, Node};
use crate::charts3::{empty, look_of};
use crate::figure::{Axis, Extra, Look, Series};
use crate::finish;
use mc_core::{Heap, Store, View, T};
use mc_eval::{Attributes, Kernel, Native, NativeCx, Reply, Res};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Line,
    Log,
    Step,
    Stacked,
}

struct DatePlot(&'static str, Kind);

/// Register the natives.
pub(crate) fn install(k: &mut Kernel, h: &Heap) {
    for (name, kind) in [("date_list_plot", Kind::Line), ("date_list_log_plot", Kind::Log), ("date_list_step_plot", Kind::Step), ("stacked_date_list_plot", Kind::Stacked)] {
        k.register(h.sym(name), Attributes::default(), Box::new(DatePlot(name, kind)));
    }
}

/// Days from 1 January 1970 of a civil date (proleptic Gregorian; H. Hinnant's algorithm), fractional for times.
pub(crate) fn civil(y: i64, m: i64, d: f64) -> f64 {
    let (y, m) = (y + (m - 1).div_euclid(12), (m - 1).rem_euclid(12) + 1);
    let y2 = if m <= 2 { y - 1 } else { y };
    let era = y2.div_euclid(400);
    let yoe = y2 - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    (era * 146097 + doe - 719468) as f64 + d - 1.0
}

/// The civil date (y, m, d) of a day number.
fn from_days(z: f64) -> (i64, i64, i64) {
    let z = z.floor() as i64 + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/// A date string: month names, day and year in any order, or `y-m-d`.
fn parse_date(s: &str) -> Option<f64> {
    let words: Vec<String> = s.split(|c: char| c == ',' || c.is_whitespace() || c == '/').filter(|w| !w.is_empty()).map(str::to_lowercase).collect();
    if words.len() == 1 && words[0].contains('-') {
        let p: Vec<i64> = words[0].split('-').filter_map(|x| x.parse().ok()).collect();
        if p.iter().any(|v| v.abs() > 12_000_000) || p.first().is_some_and(|y| y.abs() > 1_000_000) {
            return None;
        }
        return match p.as_slice() {
            [y, m, d, ..] => Some(civil(*y, *m, *d as f64)),
            [y, m] => Some(civil(*y, *m, 1.0)),
            _ => None,
        };
    }
    let (mut y, mut m, mut d) = (None, None, None);
    for w in &words {
        if let Some(k) = MONTHS.iter().position(|mn| w.starts_with(&mn.to_lowercase())) {
            m = Some(k as i64 + 1);
        } else if let Ok(n) = w.trim_end_matches(|c: char| c.is_alphabetic()).parse::<i64>() {
            if n > 31 || d.is_some() {
                y = Some(n);
            } else {
                d = Some(n);
            }
        }
    }
    let y = y.or_else(|| d.filter(|_| m.is_none()))?;
    if y.abs() > 1_000_000 {
        return None;
    }
    Some(civil(y, m.unwrap_or(1), d.filter(|_| m.is_some()).unwrap_or(1) as f64))
}

/// A date given as a list of numbers or a string, in days.
fn date_of(n: &Node<'_>, h: &Heap, t: Option<T<'_>>) -> Option<f64> {
    if let Some(View::Str(s)) = t.map(|t| h.view(t)) {
        return parse_date(s);
    }
    let v: Vec<f64> = n.items().iter().map(Node::num).collect::<Option<_>>()?;
    let (y, m, d) = (*v.first()? as i64, v.get(1).copied().unwrap_or(1.0) as i64, v.get(2).copied().unwrap_or(1.0));
    // a component far outside any calendar (2^64) is no date: the tick loops and the day arithmetic would not end
    // or would overflow (X23-12)
    let bad = |t: f64| t.is_nan() || t.abs() > 1e9;
    if y.abs() > 1_000_000 || m.abs() > 12_000_000 || bad(d) || v.iter().skip(3).any(|&t| bad(t)) {
        return None;
    }
    let secs = v.get(3).copied().unwrap_or(0.0) * 3600.0 + v.get(4).copied().unwrap_or(0.0) * 60.0 + v.get(5).copied().unwrap_or(0.0);
    (n.is_list() && !n.items().is_empty() && v.len() <= 6).then(|| civil(y, m, d) + secs / 86400.0)
}

/// Ticks for a range of days: years, months or days, labelled as dates.
fn ticks(lo: f64, hi: f64) -> Vec<(f64, String)> {
    let span = hi - lo;
    let (y0, m0, _) = from_days(lo);
    let mut out = vec![];
    if span > 365.0 * 3.0 {
        let years = span / 365.25;
        let step = [1, 2, 5, 10, 20, 50, 100, 200, 500].into_iter().find(|&s| years / s as f64 <= 7.0).unwrap_or(1000);
        let mut y = y0 - y0.rem_euclid(step);
        while civil(y, 1, 1.0) <= hi {
            let t = civil(y, 1, 1.0);
            if t >= lo {
                out.push((t, y.to_string()));
            }
            y += step;
        }
    } else if span > 60.0 {
        let months = span / 30.44;
        let step = [1, 2, 3, 6, 12].into_iter().find(|&s| months / s as f64 <= 7.0).unwrap_or(12);
        let (mut y, mut m) = (y0, m0);
        m -= (m - 1) % step;
        loop {
            let t = civil(y, m, 1.0);
            if t > hi {
                break;
            }
            if t >= lo {
                out.push((t, if m == 1 || out.is_empty() { format!("{} {}", MONTHS[(m - 1) as usize], y) } else { MONTHS[(m - 1) as usize].to_string() }));
            }
            m += step;
            if m > 12 {
                m -= 12;
                y += 1;
            }
        }
    } else {
        let step = [1.0, 2.0, 7.0, 14.0].into_iter().find(|&s| span / s <= 8.0).unwrap_or(28.0);
        let mut t = lo.ceil();
        if step == 7.0 || step == 14.0 {
            // Mondays (1 January 1970 was a Thursday)
            t += (11.0 - t.rem_euclid(7.0)).rem_euclid(7.0);
        }
        while t <= hi {
            let (_, m, d) = from_days(t);
            out.push((t, format!("{} {}", MONTHS[(m - 1) as usize], d)));
            t += step;
        }
    }
    out
}

/// The x of the k-th value of a series without dates, from the start specification.
struct Spacing {
    start: f64,
    step: Step,
}

enum Step {
    Days(f64),
    Months(i64),
}

impl Spacing {
    fn at(&self, k: usize, n: usize, end: Option<f64>) -> f64 {
        if let Some(e) = end {
            return self.start + (e - self.start) * k as f64 / (n.max(2) - 1) as f64;
        }
        match self.step {
            Step::Days(d) => self.start + d * k as f64,
            Step::Months(m) => {
                let (y, mo, d) = from_days(self.start);
                civil(y, mo + m * k as i64, d as f64)
            }
        }
    }
}

impl Native for DatePlot {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let (a, o) = crate::more::split(h, args);
        let (who, kind) = (self.0, self.1);
        let [d, rest @ ..] = a.as_slice() else { return Ok(Reply::Decline) };
        let inner = match crate::values::call_of(h, *d) {
            Some((n, &[x, _])) if n == mc_api::names::RULE => x,
            _ => *d,
        };
        if matches!(h.view(inner), View::Sym(_)) {
            return Ok(crate::bad(h, who, 1));
        }
        let mut look = look_of(cx, o, Look { joined: kind != Kind::Stacked, ..Look::default() }, who);
        if kind == Kind::Log {
            look.scale.y = Axis::Log;
        }
        // the start: a date, [start, end], or [start, automatic, step]
        let (mut spacing, mut end) = (Spacing { start: civil(1900, 1, 1.0), step: Step::Days(1.0) }, None);
        if let Some(&s) = rest.first() {
            let sn = chartdata::read(cx, s)?;
            if let Some(t) = date_of(&sn, h, Some(s)) {
                spacing.start = t;
            } else {
                let parts = cx.as_list(s).map(|l| l.to_vec()).unwrap_or_default();
                let one = |cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>| -> Res<Option<f64>> { Ok(date_of(&chartdata::read(cx, t)?, h, Some(t))) };
                let d0 = match parts.first() {
                    Some(&p) => one(cx, p)?,
                    None => None,
                };
                let d1 = match parts.get(1) {
                    Some(&p) => one(cx, p)?,
                    None => None,
                };
                match (d0, d1, parts.len()) {
                    (Some(a), Some(b), 2) => {
                        spacing.start = a;
                        end = Some(b);
                    }
                    (Some(a), _, _) => spacing.start = a,
                    (None, Some(b), _) => spacing.start = b,
                    _ => {}
                }
                if let Some(&p2) = parts.get(2) {
                    spacing.step = match h.view(p2) {
                        View::Str(w) => match w.to_lowercase().as_str() {
                            "month" => Step::Months(1),
                            "quarter" => Step::Months(3),
                            "year" => Step::Months(12),
                            "week" => Step::Days(7.0),
                            "hour" => Step::Days(1.0 / 24.0),
                            _ => Step::Days(1.0),
                        },
                        _ => {
                            let v = chartdata::read(cx, p2)?.numbers().unwrap_or_default();
                            let g = |k: usize| v.get(k).copied().unwrap_or(0.0);
                            if g(0) != 0.0 || g(1) != 0.0 {
                                Step::Months((g(0) * 12.0 + g(1)) as i64)
                            } else {
                                Step::Days(g(2) + g(3) / 24.0 + g(4) / 1440.0 + g(5) / 86400.0)
                            }
                        }
                    };
                }
            }
        }
        // data sets: values, or [date, value] pairs; several sets
        let node = chartdata::read(cx, inner)?;
        let raw = cx.as_list(inner).map(|l| l.to_vec()).unwrap_or_default();
        let pairs_of = |n: &Node<'h>, terms: &[T<'h>], cx: &mut NativeCx<'_, '_, 'h>| -> Res<Option<Vec<(f64, f64)>>> {
            let items = n.items();
            if items.is_empty() {
                return Ok(None);
            }
            // [date, value] pairs
            let mut out = vec![];
            let mut all_pairs = true;
            for (k, e) in items.iter().enumerate() {
                match e.items() {
                    [dt, v] if dt.is_list() || matches!(terms.get(k).and_then(|&t| cx.as_list(t)).and_then(|l| l.first()).map(|&x| h.view(x)), Some(View::Str(_))) => {
                        let dterm = terms.get(k).and_then(|&t| cx.as_list(t)).and_then(|l| l.first().copied());
                        match (date_of(dt, h, dterm), v.num()) {
                            (Some(x), Some(y)) => out.push((x, y)),
                            (Some(_), None) => {}
                            _ => all_pairs = false,
                        }
                    }
                    _ => all_pairs = false,
                }
            }
            if all_pairs && !out.is_empty() {
                return Ok(Some(out));
            }
            // plain values
            if items.iter().all(|e| !e.is_list()) {
                let n = items.len();
                return Ok(Some(items.iter().enumerate().filter_map(|(k, e)| e.num().map(|y| (spacing.at(k, n, end), y))).collect()));
            }
            Ok(None)
        };
        let mut sets: Vec<Vec<(f64, f64)>> = vec![];
        if let Some(s) = pairs_of(&node, &raw, cx)? {
            sets.push(s);
        } else {
            for (k, sub) in node.items().iter().enumerate() {
                let terms = raw.get(k).and_then(|&t| cx.as_list(t)).map(|l| l.to_vec()).unwrap_or_default();
                if let Some(s) = pairs_of(sub, &terms, cx)? {
                    sets.push(s);
                }
            }
        }
        sets.retain(|s| !s.is_empty());
        if sets.is_empty() {
            return empty(cx, &look, who);
        }
        for s in sets.iter_mut() {
            s.sort_by(|a, b| a.0.total_cmp(&b.0));
        }
        let lo = sets.iter().flatten().map(|p| p.0).fold(f64::INFINITY, f64::min);
        let hi = sets.iter().flatten().map(|p| p.0).fold(f64::NEG_INFINITY, f64::max);
        let hi = if hi > lo { hi } else { lo + 1.0 };
        look.x_ticks.get_or_insert_with(|| ticks(lo, hi));
        let mut under = vec![];
        let mut series = vec![];
        let mut base: Vec<f64> = vec![];
        for (k, s) in sets.iter().enumerate() {
            let mut pts: Vec<(f64, Option<f64>)> = vec![];
            match kind {
                Kind::Step => {
                    for (i, &(x, y)) in s.iter().enumerate() {
                        let next = s.get(i + 1).map_or(x + (hi - lo) / s.len().max(1) as f64, |p| p.0);
                        pts.push((x, look.scale.y.to(y)));
                        pts.push((next, look.scale.y.to(y)));
                    }
                }
                Kind::Stacked => {
                    base.resize(s.len(), 0.0);
                    let top: Vec<(f64, f64)> = s.iter().enumerate().map(|(i, &(x, y))| (x, base[i] + y)).collect();
                    let mut poly: Vec<T<'h>> = top.iter().map(|&(x, y)| h.reals(&[x, y])).collect();
                    for i in (0..s.len()).rev() {
                        poly.push(h.reals(&[s[i].0, base[i]]));
                    }
                    let (r, g, b) = look.color(k);
                    under.push(cx.list(&[mc_graphics_kernel::rgb_term(h, mc_graphics::color::Rgba::rgb(r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0)), h.call(h.sym("opacity"), &[h.real(0.35)]), h.call(h.sym("polygon"), &[cx.list(&poly)])]));
                    for (i, &(x, y)) in top.iter().enumerate() {
                        base[i] = y;
                        pts.push((x, Some(y)));
                    }
                }
                _ => pts = s.iter().map(|&(x, y)| (x, look.scale.y.to(y))).collect(),
            }
            series.push(Series { points: pts, name: format!("series {}", k + 1), bars: vec![] });
        }
        if kind == Kind::Stacked {
            look.joined = true;
        }
        let extra = Extra { under, bounds: None, entries: vec![] };
        finish(cx, &series, &look, extra)
    }
}
