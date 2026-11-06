//! Charts of counts and amounts: `histogram(data)` (bins of one round width) and `bar_chart(values)` (a bar per
//! value, or groups of bars side by side).
use crate::figure::{Bins, Extra, Look, Rgb};
use crate::sidecar::{Entry, Kind};
use crate::{bad, bins_option, finish, plot_options, values};
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

/// The numbers of a list, read without evaluation where they are numbers already; quantities, `around` values and
/// intervals as their magnitudes or middles. Items that are not real numbers (`missing()`, `none`, symbols, complex
/// numbers) are NaN: no bar, not counted. `None` when `t` is not a list.
fn numbers<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>) -> Res<Option<Vec<f64>>> {
    let h = cx.heap();
    match h.packed(t) {
        Some(mc_api::Packed::Reals { data, dims }) if dims.len() == 1 => return Ok(Some(data.to_vec())),
        Some(mc_api::Packed::Ints { data, dims }) if dims.len() == 1 => return Ok(Some(data.iter().map(|&k| k as f64).collect())),
        _ => {}
    }
    // an association: its values
    let t = match values::assoc_entries(cx, t)? {
        Some(entries) => {
            let vals: Vec<T<'h>> = entries.into_iter().map(|e| e.1).collect();
            cx.list(&vals)
        }
        None => t,
    };
    let Some(items) = cx.as_list(t) else { return Ok(None) };
    let mut out = Vec::with_capacity(items.len());
    for (i, &e) in items.iter().enumerate() {
        if i % 4096 == 4095 {
            cx.tick(1)?;
        }
        out.push(match mc_graphics::num::number(h, e) {
            Some(v) => v,
            None if cx.as_list(e).is_some() => return Ok(None),
            None => values::read(cx, legended_value(h, e))?.v.unwrap_or(f64::NAN),
        });
    }
    Ok(Some(out))
}

/// The value inside `legended(v, label)`, `labeled(v, label)` or `style(v, ...)`; `t` itself otherwise.
fn legended_value<'h>(h: &'h Heap, t: T<'h>) -> T<'h> {
    let mut t = t;
    for _ in 0..8 {
        match values::call_of(h, t) {
            Some((n, args)) if !args.is_empty() && matches!(n.as_str(), "legended" | "labeled" | "style" | "callout") => t = args[0],
            _ => break,
        }
    }
    t
}

/// Chart data: one list of values or several, with names for the data sets (an association's keys, a dataset's
/// columns) and for the values of one set (an association of numbers).
struct ChartData {
    sets: Vec<Vec<f64>>,
    /// whether the data was a list of lists (several data sets)
    grouped: bool,
    names: Vec<String>,
    keys: Vec<String>,
    /// the keys of the first set, when the sets are associations (the bars within each group)
    inner_keys: Vec<String>,
}

/// One list of values or several: `[v1, ...]` is one data set, `[[...], [...]]` several; an association of lists is
/// a data set per key, an association of numbers one set labelled by its keys; a dataset a set per column.
fn sets<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>) -> Res<Option<ChartData>> {
    let h = cx.heap();
    let (t, names, keys) = match values::dataset_columns(cx, t)? {
        Some(cols) => {
            let names: Vec<String> = cols.iter().map(|c| c.0.clone()).collect();
            let data: Vec<T<'h>> = cols.iter().map(|c| c.1).collect();
            (cx.list(&data), names, vec![])
        }
        None => match values::assoc_entries(cx, t)? {
            Some(entries) => {
                let lists = entries.iter().all(|e| cx.as_list(e.1).is_some() || values::call_of(h, e.1).is_some_and(|(n, _)| n == "assoc"));
                let data: Vec<T<'h>> = entries.iter().map(|e| e.1).collect();
                let names: Vec<String> = entries.into_iter().map(|e| e.0).collect();
                if lists {
                    (cx.list(&data), names, vec![])
                } else {
                    (cx.list(&data), vec![], names)
                }
            }
            None => (t, vec![], vec![]),
        },
    };
    let Some(items) = cx.as_list(t) else { return Ok(None) };
    if !items.is_empty() && items.iter().all(|&e| cx.as_list(e).is_some() || values::call_of(h, e).is_some_and(|(n, _)| n == "assoc")) {
        let mut out = vec![];
        let inner_keys = match items.first() {
            Some(&f) => values::assoc_entries(cx, f)?.map(|e| e.into_iter().map(|e| e.0).collect()).unwrap_or_default(),
            None => vec![],
        };
        for &e in items {
            match numbers(cx, e)? {
                Some(v) => out.push(v),
                None => return Ok(None),
            }
        }
        return Ok(Some(ChartData { sets: out, grouped: true, names, keys, inner_keys }));
    }
    Ok(numbers(cx, t)?.map(|v| ChartData { sets: vec![v], grouped: false, names, keys, inner_keys: vec![] }))
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
        let (pos, o) = crate::more::split(h, args);
        let mut look = match o {
            Some(o) => match plot_options(cx, o, base, &["bins", "normalized"], name) {
                Some(look) => look,
                None => return Ok(bad(h, name, pos.len() as i64 + 1)),
            },
            None => base,
        };
        let (data, bspec, hspec) = match pos.as_slice() {
            [d] => (*d, None, None),
            [d, b] => (*d, Some(*b), None),
            [d, b, hs] => (*d, Some(*b), Some(*hs)),
            _ => return Ok(Reply::Decline),
        };
        if let Some(b) = bspec {
            match bins_option(cx, b) {
                Some(b) => look.bins = b,
                None => return Ok(bad(h, name, 2)),
            }
        }
        let height = match hspec {
            None if look.normalized => Height::Pdf,
            None => Height::Count,
            Some(hs) => match height_spec(h, hs) {
                Some(k) => k,
                None => return Ok(bad(h, name, 3)),
            },
        };
        let Some(chart) = sets(cx, data)? else { return Ok(bad(h, name, 1)) };
        if (look.legend.is_empty() || look.legend.first().is_some_and(|l| l == crate::AUTO_LEGEND)) && chart.names.len() > 1 {
            look.legend = chart.names.clone();
        }
        let mut sets = chart.sets;
        for s in &mut sets {
            s.retain(|v| v.is_finite());
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
            let n = s.len().max(1) as f64;
            let mut acc = 0.0;
            let heights: Vec<f64> = counts
                .iter()
                .map(|&c| {
                    acc += c as f64;
                    match height {
                        Height::Count => c as f64,
                        Height::Probability => c as f64 / n,
                        Height::Pdf => c as f64 / (n * w),
                        Height::CumulativeCount => acc,
                        Height::Cdf => acc / n,
                    }
                })
                .collect();
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
        look.legend_swatches = swatches;
        finish(cx, &[], &look, extra)
    }
}

/// What the height of a histogram's bar is.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Height {
    Count,
    Probability,
    Pdf,
    CumulativeCount,
    Cdf,
}

/// A histogram's height specification: `count`, `probability`, `pdf` (also `probability_density`), `cdf`,
/// `cumulative_count`, as a symbol or a string (case and underscores ignored).
fn height_spec<'h>(h: &'h Heap, t: T<'h>) -> Option<Height> {
    let name = match h.view(t) {
        mc_core::View::Sym(s) => h.sym_name(s).to_string(),
        mc_core::View::Str(s) => s.to_string(),
        _ => return None,
    };
    let key: String = name.chars().filter(|c| *c != '_' && *c != ' ').flat_map(char::to_lowercase).collect();
    Some(match key.as_str() {
        "count" | "automatic" => Height::Count,
        "probability" => Height::Probability,
        "pdf" | "probabilitydensity" => Height::Pdf,
        "cdf" => Height::Cdf,
        "cumulativecount" => Height::CumulativeCount,
        _ => return None,
    })
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
            [d, o] => match plot_options(cx, *o, Look::default(), &["labels"], name) {
                Some(look) => (*d, look),
                None => return Ok(bad(h, name, 2)),
            },
            _ => return Ok(Reply::Decline),
        };
        let Some(chart) = sets(cx, data)? else { return Ok(bad(h, name, 1)) };
        // one list of values: one group per value, one bar each; several lists: a group per list
        let grouped = chart.grouped;
        let mut look = look;
        if look.labels.is_empty() {
            // the keys of an association label the bars (or the groups)
            look.labels = if grouped { chart.names.clone() } else { chart.keys.clone() };
        }
        // `plot_legends: automatic`: the bars of a group by the keys of its association
        if look.legend.first().is_some_and(|l| l == crate::AUTO_LEGEND) {
            look.legend = if grouped { chart.inner_keys.clone() } else { chart.keys.clone() };
        }
        let look = look;
        let groups: Vec<Vec<f64>> = if grouped { chart.sets } else { chart.sets[0].iter().map(|&v| vec![v]).collect() };
        if groups.iter().flatten().all(|v| !v.is_finite()) {
            return Ok(bad(h, name, 1));
        }
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
                    if !v.is_finite() {
                        return None;
                    }
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
