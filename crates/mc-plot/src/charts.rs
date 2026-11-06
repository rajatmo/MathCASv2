//! Charts of counts and amounts: `histogram(data)` (bins of one round width) and `bar_chart(values)` (a bar per
//! value, or groups of bars side by side).
use crate::figure::{Bins, Extra, Look, Rgb};
use crate::sidecar::{Entry, Kind};
use crate::{bad, finish, plot_options, real};
use mc_core::{Heap, Store, T};
use mc_eval::{Attributes, Kernel, Native, NativeCx, Reply, Res};

/// Register the natives.
pub(crate) fn install(k: &mut Kernel, h: &Heap) {
    k.register(h.sym("histogram"), Attributes::default(), Box::new(Histogram));
    k.register(h.sym("bar_chart"), Attributes::default(), Box::new(BarChart));
}

/// The most bins of a histogram.
const MAX_BINS: usize = 10_000;
/// The width of a bar (or of a group of bars) in a bar chart, the spacing being 1.
const BAR_WIDTH: f64 = 0.8;
/// The opacity of histograms drawn over each other.
const OVERLAID: f64 = 0.55;

/// The numbers of a list, read without evaluation where they are numbers already. `None` when something in it is
/// not a real number.
fn numbers<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>) -> Res<Option<Vec<f64>>> {
    let h = cx.heap();
    match h.packed(t) {
        Some(mc_api::Packed::Reals { data, dims }) if dims.len() == 1 => return Ok(Some(data.to_vec())),
        Some(mc_api::Packed::Ints { data, dims }) if dims.len() == 1 => return Ok(Some(data.iter().map(|&k| k as f64).collect())),
        _ => {}
    }
    let Some(items) = cx.as_list(t) else { return Ok(None) };
    let mut out = Vec::with_capacity(items.len());
    for (i, &e) in items.iter().enumerate() {
        if i % 4096 == 4095 {
            cx.tick(1)?;
        }
        match mc_graphics::num::number(h, e) {
            Some(v) => out.push(v),
            None => match real(cx, e)? {
                Some(v) => out.push(v),
                None => return Ok(None),
            },
        }
    }
    Ok(Some(out))
}

/// One number or several lists of them: `[v1, ...]` is one data set, `[[...], [...]]` several.
fn sets<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>) -> Res<Option<Vec<Vec<f64>>>> {
    let Some(items) = cx.as_list(t) else { return Ok(None) };
    if !items.is_empty() && items.iter().all(|&e| cx.as_list(e).is_some()) {
        let mut out = vec![];
        for &e in items {
            match numbers(cx, e)? {
                Some(v) => out.push(v),
                None => return Ok(None),
            }
        }
        return Ok(Some(out));
    }
    Ok(numbers(cx, t)?.map(|v| vec![v]))
}

/// The smallest round number (1, 2, 2.5 or 5 times a power of ten) at least `w` (> 0).
pub(crate) fn round_up(w: f64) -> f64 {
    let p = 10f64.powf(w.log10().floor());
    [1.0, 2.0, 2.5, 5.0, 10.0].iter().map(|m| m * p).find(|v| *v >= w * (1.0 - 1e-12)).unwrap_or(10.0 * p)
}

/// The bin edges for `data` (sorted, not empty): the first and the width.
fn bin_edges(sorted: &[f64], bins: Bins) -> Option<(f64, f64, usize)> {
    let (min, max) = (sorted[0], sorted[sorted.len() - 1]);
    let n = sorted.len() as f64;
    let span = max - min;
    let w = match bins {
        Bins::Range(lo, hi, w) => return Some((lo, w, ((hi - lo) / w).ceil().max(1.0) as usize)).filter(|b| b.2 <= MAX_BINS),
        Bins::Width(w) => w,
        Bins::Count(k) if span > 0.0 => round_up(span / k as f64),
        Bins::Count(_) => 1.0,
        Bins::Auto => {
            // Freedman and Diaconis: twice the interquartile range over the cube root of the count
            let q = |p: f64| sorted[((n - 1.0) * p).round() as usize];
            let iqr = q(0.75) - q(0.25);
            let raw = if iqr > 0.0 {
                2.0 * iqr / n.cbrt()
            } else if span > 0.0 {
                span / (n.log2().ceil() + 1.0)
            } else {
                1.0
            };
            round_up(raw)
        }
    };
    let lo = (min / w).floor() * w;
    let count = (((max - lo) / w).floor() as usize + 1).max(1);
    (count <= MAX_BINS).then_some((lo, w, count))
}

/// `histogram(data)`: counts in bins of equal width.
struct Histogram;

impl Native for Histogram {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let name = "histogram";
        let base = Look::default();
        let (data, look) = match args {
            [d] => (*d, base),
            [d, o] => match plot_options(cx, *o, base, &["bins", "normalized"]) {
                Some(look) => (*d, look),
                None => return Ok(bad(h, name, 2)),
            },
            _ => return Ok(Reply::Decline),
        };
        let Some(mut sets) = sets(cx, data)? else { return Ok(bad(h, name, 1)) };
        for s in &mut sets {
            s.sort_by(f64::total_cmp);
        }
        let all: Vec<f64> = {
            let mut v: Vec<f64> = sets.iter().flatten().copied().collect();
            v.sort_by(f64::total_cmp);
            v
        };
        if all.is_empty() {
            return Ok(bad(h, name, 1));
        }
        let Some((lo, w, count)) = bin_edges(&all, look.bins) else { return Ok(bad(h, name, 2)) };
        let several = sets.len() > 1;
        let mut extra = Extra::default();
        let mut top = 0.0f64;
        let mut swatches = vec![];
        for (k, s) in sets.iter().enumerate() {
            cx.tick(1)?;
            let mut counts = vec![0usize; count];
            for &v in s {
                let b = ((v - lo) / w).floor();
                if b >= 0.0 && (b as usize) < count {
                    counts[b as usize] += 1;
                } else if v == lo + w * count as f64 && count > 0 {
                    // the upper end of an explicit range belongs to the last bin
                    counts[count - 1] += 1;
                }
            }
            let scale = if look.normalized { 1.0 / (s.len().max(1) as f64 * w) } else { 1.0 };
            let heights: Vec<f64> = counts.iter().map(|&c| c as f64 * scale).collect();
            top = top.max(heights.iter().copied().fold(0.0, f64::max));
            let color = look.color(k);
            swatches.push(color);
            let bars: Vec<(f64, f64, f64)> =
                heights.iter().enumerate().filter(|(_, v)| **v > 0.0).map(|(b, &v)| (lo + w * b as f64, lo + w * (b + 1) as f64, v)).collect();
            extra.under.push(bars_term(h, &bars, color, if several { OVERLAID } else { 1.0 }));
            extra.entries.push(Entry {
                name: look.legend.get(k).cloned().unwrap_or_else(|| if several { format!("data {}", k + 1) } else { "data".into() }),
                color,
                kind: Kind::Points,
                points: bars.iter().map(|&(a, b, v)| (0.5 * (a + b), v)).collect(),
            });
        }
        extra.bounds = Some([(lo, lo + w * count as f64), (0.0, if top > 0.0 { top } else { 1.0 })]);
        let mut look = look;
        look.legend_swatches = swatches;
        finish(cx, &[], &look, extra)
    }
}

/// Bars `(x0, x1, height)` from 0 in `color` (with this opacity), separated by thin white edges.
fn bars_term<'h>(h: &'h Heap, bars: &[(f64, f64, f64)], color: Rgb, opacity: f64) -> T<'h> {
    let list = |items: &[T<'h>]| h.call(h.sym(mc_api::names::LIST), items);
    let f = |v: u8| v as f64 / 255.0;
    let mut part = vec![
        h.call(h.sym("rgb_color"), &[h.real(f(color.0)), h.real(f(color.1)), h.real(f(color.2))]),
        h.call(h.sym("opacity"), &[h.real(opacity)]),
        h.call(h.sym("edge_form"), &[list(&[h.sym_term(h.sym("white")), h.call(h.sym("absolute_thickness"), &[h.real(0.6)])])]),
    ];
    for &(a, b, v) in bars {
        part.push(h.call(h.sym("rectangle"), &[h.reals(&[a, 0.0_f64.min(v)]), h.reals(&[b, 0.0_f64.max(v)])]));
    }
    list(&part)
}

/// `bar_chart(values)`: bars at 1, 2, ...; groups of bars side by side.
struct BarChart;

impl Native for BarChart {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let name = "bar_chart";
        let (data, look) = match args {
            [d] => (*d, Look::default()),
            [d, o] => match plot_options(cx, *o, Look::default(), &["labels"]) {
                Some(look) => (*d, look),
                None => return Ok(bad(h, name, 2)),
            },
            _ => return Ok(Reply::Decline),
        };
        let Some(groups) = sets(cx, data)? else { return Ok(bad(h, name, 1)) };
        // one list of values: one group per value, one bar each; several lists: a group per list
        let grouped = cx.as_list(data).is_some_and(|items| items.iter().all(|&e| cx.as_list(e).is_some()));
        let groups: Vec<Vec<f64>> = if grouped { groups } else { groups[0].iter().map(|&v| vec![v]).collect() };
        if groups.is_empty() || groups.iter().all(|g| g.is_empty()) {
            return Ok(bad(h, name, 1));
        }
        let per = groups.iter().map(Vec::len).max().unwrap_or(1);
        let w = BAR_WIDTH / per as f64;
        let mut extra = Extra::default();
        let (mut lo, mut hi) = (0.0f64, 0.0f64);
        for k in 0..per {
            let color = if grouped { look.color(k) } else { look.color(0) };
            let bars: Vec<(f64, f64, f64)> = groups
                .iter()
                .enumerate()
                .filter_map(|(g, vs)| {
                    let v = *vs.get(k)?;
                    let x0 = (g + 1) as f64 - BAR_WIDTH / 2.0 + w * k as f64;
                    Some((x0, x0 + w, v))
                })
                .collect();
            for b in &bars {
                (lo, hi) = (lo.min(b.2), hi.max(b.2));
            }
            extra.under.push(bars_term(h, &bars, color, 1.0));
            let name = look.legend.get(k).cloned().unwrap_or_else(|| if grouped { format!("series {}", k + 1) } else { "data".into() });
            extra.entries.push(Entry { name, color, kind: Kind::Points, points: bars.iter().map(|&(a, b, v)| (0.5 * (a + b), v)).collect() });
        }
        let n = groups.len();
        extra.bounds = Some([(0.5, n as f64 + 0.5), (lo, if hi > lo { hi } else { lo + 1.0 })]);
        let mut look = look;
        if grouped {
            look.legend_swatches = (0..per).map(|k| look.color(k)).collect();
        } else {
            look.legend_swatches = vec![look.color(0)];
        }
        if !look.labels.is_empty() || n <= 30 {
            look.x_ticks = Some(
                (0..n)
                    .map(|g| ((g + 1) as f64, look.labels.get(g).cloned().unwrap_or_else(|| if look.labels.is_empty() { (g + 1).to_string() } else { String::new() })))
                    .collect(),
            );
        }
        finish(cx, &[], &look, extra)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn widths_are_round() {
        assert_eq!(round_up(0.3), 0.5);
        assert_eq!(round_up(1.0), 1.0);
        assert_eq!(round_up(2.2), 2.5);
        assert_eq!(round_up(7.0), 10.0);
        assert_eq!(round_up(0.011), 0.02);
    }

    #[test]
    fn bins_cover_the_data() {
        let data: Vec<f64> = (0..100).map(|k| k as f64 / 10.0).collect();
        let (lo, w, n) = bin_edges(&data, Bins::Count(10)).unwrap();
        assert_eq!((lo, w), (0.0, 1.0));
        assert_eq!(n, 10);
        let (lo, w, n) = bin_edges(&data, Bins::Width(2.5)).unwrap();
        assert_eq!((lo, w, n), (0.0, 2.5, 4));
        assert_eq!(bin_edges(&data, Bins::Range(-5.0, 5.0, 1.0)), Some((-5.0, 1.0, 10)));
        let (lo, w, n) = bin_edges(&data, Bins::Auto).unwrap();
        assert!(lo <= 0.0 && lo + w * n as f64 > 9.9 && (4..=30).contains(&n), "{lo} {w} {n}");
        assert_eq!(bin_edges(&[3.0, 3.0], Bins::Auto), Some((3.0, 1.0, 1)));
        assert_eq!(bin_edges(&data, Bins::Width(1e-6)), None);
    }
}
