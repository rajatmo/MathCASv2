#![allow(clippy::type_complexity)]
//! Charts read through [`crate::chartdata`]: `sector_chart`, `rectangle_chart`, `paired_bar_chart`, `bubble_chart`,
//! `box_whisker_chart`, `smooth_histogram`, and `complex_list_plot`'s data. Data in any of the forms the chart data
//! reader takes (associations, quantities, labelled and styled entries, entries that are not numbers left out);
//! data with no numbers at all gives an empty chart with a message, as an empty list does.
use crate::chartdata::{self, Node};
use crate::figure::{Extra, Look};
use crate::{finish, finish_graphic, plot_options};
use mc_api::names;
use mc_core::{Heap, Store, View, T};
use mc_eval::{NativeCx, Reply, Res};
use mc_graphics::color::Rgba;

pub(crate) fn list<'h>(h: &'h Heap, items: &[T<'h>]) -> T<'h> {
    h.call(h.sym(names::LIST), items)
}

pub(crate) fn rgb((r, g, b): (u8, u8, u8)) -> Rgba {
    Rgba::rgb(r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0)
}

pub(crate) fn rule<'h>(h: &'h Heap, k: &str, v: T<'h>) -> T<'h> {
    h.call(h.sym(names::RULE), &[h.sym_term(h.sym(k)), v])
}

pub(crate) const OPTS: &[&str] = &[
    "chart_labels", "chart_style", "chart_element_function", "chart_elements", "chart_layout", "bar_spacing", "bar_origin", "chart_base_style", "labeling_function",
    "bubble_sizes", "bubble_scale", "sector_origin", "sector_spacing", "polar_axes", "polar_grid_lines", "spacings", "color_function", "plot_points",
    "whisker", "bar_size", "method", "performance_goal", "label_style", "scaling_functions", "color_function_scaling", "legend_appearance",
];

pub(crate) fn look_of<'h>(cx: &NativeCx<'_, '_, 'h>, o: Option<T<'h>>, base: Look<'h>, who: &'static str) -> Look<'h> {
    let mut look = match o {
        Some(o) => plot_options(cx, o, base, OPTS, who).unwrap_or(Look { who, ..Look::default() }),
        None => Look { who, ..base },
    };
    // options the charts read themselves, or styling they take no notice of
    look.ignored.retain(|k| !OPTS.iter().any(|o| k == o || k.starts_with(&format!("{o}:"))));
    look
}

/// The options term's rule for `key`, if any.
pub(crate) fn option<'h>(cx: &NativeCx<'_, '_, 'h>, o: Option<T<'h>>, key: &str) -> Option<T<'h>> {
    let h = cx.heap();
    let View::App { args, .. } = h.view(o?) else { return None };
    args.iter().find_map(|&r| match h.view(r) {
        View::App { args: &[k, v], .. } if matches!(h.view(k), View::Sym(s) if h.sym_name(s) == key) => Some(v),
        _ => None,
    })
}

/// An empty chart (nothing to draw): the frame over `bounds`, with a message.
pub(crate) fn empty<'h>(cx: &mut NativeCx<'_, '_, 'h>, look: &Look<'h>, who: &str) -> Res<Reply<'h>> {
    let h = cx.heap();
    cx.emit(h.call(h.sym("message"), &[h.sym_term(h.sym(who)), h.string("no numeric data to chart")]))?;
    finish(cx, &[], look, Extra { under: vec![], bounds: Some([(0.0, 1.0), (0.0, 1.0)]), entries: vec![] })
}

/// The directives of an item: its colour, then the styles of its group and its own (which override).
fn styled<'h>(h: &'h Heap, base: Rgba, group: Option<&Node<'h>>, n: &Node<'h>, edge: bool) -> Vec<T<'h>> {
    let mut v = vec![mc_graphics_kernel::rgb_term(h, base)];
    if edge {
        v.push(h.call(h.sym("edge_form"), &[mc_graphics_kernel::rgb_term(h, Rgba::rgb(base.r * 0.7, base.g * 0.7, base.b * 0.7))]));
    }
    if let Some(g) = group {
        v.extend_from_slice(&g.style);
    }
    v.extend_from_slice(&n.style);
    v
}

fn labels_wanted<'h>(cx: &NativeCx<'_, '_, 'h>, o: Option<T<'h>>) -> bool {
    let h = cx.heap();
    ["plot_labels", "chart_labels", "labels"].iter().any(|k| option(cx, o, k).is_some_and(|v| !matches!(h.view(v), View::Sym(s) if h.sym_name(s) == "none")))
}

/// Explicit labels given by `chart_labels: [...]` / `plot_labels: [...]`, as text.
fn given_labels<'h>(cx: &NativeCx<'_, '_, 'h>, o: Option<T<'h>>) -> Vec<String> {
    let h = cx.heap();
    for k in ["chart_labels", "plot_labels", "labels"] {
        if let Some(v) = option(cx, o, k) {
            let v = match crate::values::call_of(h, v) {
                Some((n, a)) if (n == "placed" || n == "callout") && !a.is_empty() => a[0],
                _ => v,
            };
            if let Some(items) = cx.as_list(v) {
                let items = match items.first().and_then(|&f| cx.as_list(f)) {
                    Some(inner) if items.len() <= 2 => inner,
                    _ => items,
                };
                return items
                    .iter()
                    .map(|&t| match h.view(t) {
                        View::Str(s) => s.to_string(),
                        _ => mc_syntax::Printer::new(h).expr(t),
                    })
                    .collect();
            }
        }
    }
    vec![]
}

fn text_label<'h>(h: &'h Heap, s: &str, at: [f64; 2]) -> T<'h> {
    h.call(h.sym("text"), &[h.string(s), h.reals(&at)])
}

/// The label of item `k` of a group: given labels, else the entry's own when labels are wanted or it has one.
fn item_label<'h>(h: &'h Heap, n: &Node<'h>, k: usize, given: &[String], wanted: bool) -> Option<String> {
    if let Some(s) = given.get(k) {
        return Some(s.clone());
    }
    let own = n.label_text(h);
    if wanted || own.is_some() {
        return own;
    }
    None
}

// ---- sector chart

/// A filled sector of the ring r0..r about `c` from angle a to b, as a polygon.
pub(crate) fn sector<'h>(h: &'h Heap, c: [f64; 2], r0: f64, r: f64, a: f64, b: f64) -> T<'h> {
    let n = ((b - a).abs() / 0.05).ceil().max(2.0) as usize;
    let mut pts = vec![];
    for k in 0..=n {
        let t = a + (b - a) * k as f64 / n as f64;
        pts.push(h.reals(&[c[0] + r * t.cos(), c[1] + r * t.sin()]));
    }
    for k in (0..=n).rev() {
        let t = a + (b - a) * k as f64 / n as f64;
        pts.push(h.reals(&[c[0] + r0 * t.cos(), c[1] + r0 * t.sin()]));
    }
    h.call(h.sym("polygon"), &[list(h, &pts)])
}

pub(crate) fn picture<'h>(cx: &mut NativeCx<'_, '_, 'h>, prims: Vec<T<'h>>, mut opts: Vec<T<'h>>, look: &Look<'h>, who: &str) -> Res<Reply<'h>> {
    let h = cx.heap();
    for (k, v) in &look.graphics {
        if !opts.iter().any(|r| matches!(h.view(*r), View::App { args: &[kk, _], .. } if matches!(h.view(kk), View::Sym(s) if h.sym_name(s) == *k))) {
            opts.push(rule(h, k, *v));
        }
    }
    let g = h.call(h.sym("graphics"), &[list(h, &prims), h.call(h.sym("options"), &opts)]);
    finish_graphic(cx, g, who, &look.ignored)
}

pub(crate) fn sector_chart<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>], o: Option<T<'h>>) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [d, ..] = a else { return Ok(Reply::Decline) };
    let data = chartdata::read(cx, *d)?;
    let look = look_of(cx, o, Look::default(), "sector_chart");
    let groups = data.tuple_groups(2);
    if groups.is_empty() {
        return empty(cx, &look, "sector_chart");
    }
    let (given, wanted) = (given_labels(cx, o), labels_wanted(cx, o));
    let rmax = groups.iter().flat_map(|g| g.1.iter().map(|t| t.1[1].abs())).fold(0.0f64, f64::max).max(1e-300);
    let mut prims = vec![h.call(h.sym("edge_form"), &[h.sym_term(h.sym("white"))])];
    for (gi, (g, items)) in groups.iter().enumerate() {
        let total: f64 = items.iter().map(|t| t.1[0].abs()).sum::<f64>().max(1e-300);
        let c = [2.4 * gi as f64, 0.0];
        let mut ang = std::f64::consts::FRAC_PI_2;
        for (k, (n, t)) in items.iter().enumerate() {
            let span = std::f64::consts::TAU * t[0].abs() / total;
            let r = t[1].abs() / rmax;
            let mut part = styled(h, rgb(look.color(k)), *g, n, false);
            part.push(sector(h, c, 0.0, r, ang - span, ang));
            prims.push(list(h, &part));
            if let Some(s) = item_label(h, n, k, &given, wanted) {
                let m = ang - span / 2.0;
                prims.push(text_label(h, &s, [c[0] + (r + 0.12) * m.cos(), c[1] + (r + 0.12) * m.sin()]));
            }
            ang -= span;
        }
    }
    picture(cx, prims, vec![rule(h, "aspect_ratio", h.sym_term(h.sym("automatic")))], &look, "sector_chart")
}

// ---- rectangle chart

pub(crate) fn rectangle_chart<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>], o: Option<T<'h>>) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [d, ..] = a else { return Ok(Reply::Decline) };
    let data = chartdata::read(cx, *d)?;
    let mut look = look_of(cx, o, Look::default(), "rectangle_chart");
    let groups = data.tuple_groups(2);
    if groups.is_empty() {
        return empty(cx, &look, "rectangle_chart");
    }
    let (given, wanted) = (given_labels(cx, o), labels_wanted(cx, o));
    let wsum: f64 = groups.iter().flat_map(|g| g.1.iter().map(|t| t.1[0].abs())).sum::<f64>().max(1e-300);
    let gap = wsum * 0.01;
    let mut x = 0.0;
    let mut under = vec![];
    let mut ticks = vec![];
    let (mut lo, mut hi) = (0.0f64, 0.0f64);
    for (g, items) in &groups {
        let start = x;
        for (k, (n, t)) in items.iter().enumerate() {
            let col = if groups.len() > 1 { look.color(k) } else { look.color(0) };
            let mut part = styled(h, rgb(col), *g, n, true);
            part.push(h.call(h.sym("rectangle"), &[h.reals(&[x, 0.0]), h.reals(&[x + t[0].abs(), t[1]])]));
            under.push(list(h, &part));
            if let Some(s) = item_label(h, n, k, &given, wanted) {
                under.push(text_label(h, &s, [x + t[0].abs() / 2.0, t[1] + 0.0]));
            }
            lo = lo.min(t[1]);
            hi = hi.max(t[1]);
            x += t[0].abs() + gap;
        }
        if let Some(s) = g.and_then(|g| g.label_text(h)) {
            ticks.push(((start + x) / 2.0, s));
        }
        x += gap * 4.0;
    }
    if !ticks.is_empty() && look.x_ticks.is_none() {
        look.x_ticks = Some(ticks);
    }
    let extra = Extra { under, bounds: Some([(0.0, x), (lo, hi * 1.08)]), entries: vec![] };
    finish(cx, &[], &look, extra)
}

// ---- paired bar chart

/// Rows of a side: each a value or a list of values (stacked).
fn rows(n: &Node<'_>) -> Vec<Vec<f64>> {
    if let Some(v) = n.num() {
        return vec![vec![v]];
    }
    n.items().iter().map(|r| if let Some(v) = r.num() { vec![v] } else { r.items().iter().filter_map(Node::num).collect() }).collect()
}

pub(crate) fn paired_bar_chart<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>], o: Option<T<'h>>) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [l, r, ..] = a else { return Ok(Reply::Decline) };
    let (ln, rn) = (chartdata::read(cx, *l)?, chartdata::read(cx, *r)?);
    let look = look_of(cx, o, Look::default(), "paired_bar_chart");
    let (lr, rr) = (rows(&ln), rows(&rn));
    let m = lr.iter().chain(&rr).map(|row| row.iter().map(|v| v.abs()).sum::<f64>()).fold(0.0f64, f64::max);
    if m <= 0.0 {
        return empty(cx, &look, "paired_bar_chart");
    }
    let n = lr.len().max(rr.len());
    let mut under = vec![];
    for (side, rows, node) in [(-1.0, &lr, &ln), (1.0, &rr, &rn)] {
        for (k, row) in rows.iter().enumerate() {
            let y = (n - k) as f64;
            let mut x0 = 0.0;
            for (j, v) in row.iter().enumerate() {
                let col = if row.len() > 1 { look.color(j) } else { look.color(if side < 0.0 { 0 } else { 1 }) };
                let item = node.items().get(k);
                let mut part = vec![mc_graphics_kernel::rgb_term(h, rgb(col)), h.call(h.sym("edge_form"), &[h.sym_term(h.sym("white"))])];
                if let Some(it) = item {
                    part.extend_from_slice(&it.style);
                }
                part.push(h.call(h.sym("rectangle"), &[h.reals(&[side * x0, y - 0.4]), h.reals(&[side * (x0 + v.abs()), y + 0.4])]));
                under.push(list(h, &part));
                x0 += v.abs();
            }
        }
    }
    let labels: Vec<(f64, String)> = ln.items().iter().enumerate().filter_map(|(k, it)| it.label_text(h).map(|s| ((n - k) as f64, s))).collect();
    if !labels.is_empty() {
        for (y, s) in labels {
            under.push(text_label(h, &s, [0.0, y + 0.0]));
        }
    }
    let extra = Extra { under, bounds: Some([(-m * 1.05, m * 1.05), (0.4, n as f64 + 0.6)]), entries: vec![] };
    finish(cx, &[], &look, extra)
}

// ---- bubble chart

pub(crate) fn bubble_chart<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>], o: Option<T<'h>>) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [d, ..] = a else { return Ok(Reply::Decline) };
    let data = chartdata::read(cx, *d)?;
    let look = look_of(cx, o, Look::default(), "bubble_chart");
    let groups = data.tuple_groups(3);
    if groups.is_empty() {
        return empty(cx, &look, "bubble_chart");
    }
    let (given, wanted) = (given_labels(cx, o), labels_wanted(cx, o));
    let all: Vec<&Vec<f64>> = groups.iter().flat_map(|g| g.1.iter().map(|t| &t.1)).collect();
    let span = |k: usize| {
        let lo = all.iter().map(|r| r[k]).fold(f64::INFINITY, f64::min);
        let hi = all.iter().map(|r| r[k]).fold(f64::NEG_INFINITY, f64::max);
        (lo, if hi > lo { hi } else { lo + 1.0 })
    };
    let ((x0, x1), (y0, y1)) = (span(0), span(1));
    let zmax = all.iter().map(|r| r[2].abs()).fold(0.0f64, f64::max).max(1e-300);
    // bubble_sizes: [smallest, largest] diameters as fractions of the plot width (default 0.02 .. 0.16)
    let (smin, smax) = match option(cx, o, "bubble_sizes").and_then(|v| cx.as_list(v)) {
        Some(&[a, b]) => (mc_graphics::num::number(h, a).unwrap_or(0.02), mc_graphics::num::number(h, b).unwrap_or(0.16)),
        _ => (0.02, 0.16),
    };
    let (kx, ky) = (x1 - x0, y1 - y0);
    let mut under = vec![];
    let mut k = 0;
    for (gi, (g, items)) in groups.iter().enumerate() {
        for (n, r) in items {
            let s = smin + (smax - smin) * (r[2].abs() / zmax).sqrt();
            let c = rgb(look.color(if groups.len() > 1 { gi } else { 0 }));
            let mut part = styled(h, c, *g, n, true);
            part.insert(1, h.call(h.sym("opacity"), &[h.real(0.75)]));
            part.push(h.call(h.sym("disk"), &[h.reals(&[r[0], r[1]]), h.reals(&[kx * s / 2.0, ky * s / 2.0 * 1.6])]));
            under.push(list(h, &part));
            if let Some(l) = item_label(h, n, k, &given, wanted) {
                under.push(text_label(h, &l, [r[0], r[1]]));
            }
            k += 1;
        }
    }
    let (px, py) = (kx * smax * 0.6, ky * smax);
    let extra = Extra { under, bounds: Some([(x0 - px, x1 + px), (y0 - py, y1 + py)]), entries: vec![] };
    finish(cx, &[], &look, extra)
}

// ---- box and whisker chart

/// The q-quantile with linear interpolation between order statistics.
pub(crate) fn quantile(sorted: &[f64], q: f64) -> f64 {
    let n = sorted.len();
    if n == 1 {
        return sorted[0];
    }
    let pos = q * (n - 1) as f64;
    let k = pos.floor() as usize;
    let f = pos - k as f64;
    if k + 1 >= n {
        sorted[n - 1]
    } else {
        sorted[k] + f * (sorted[k + 1] - sorted[k])
    }
}

pub(crate) fn box_whisker_chart<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>], o: Option<T<'h>>) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [d, rest @ ..] = a else { return Ok(Reply::Decline) };
    let notched = rest.iter().any(|&s| matches!(h.view(s), View::Str(s) if s.contains("Notched")));
    let outliers = !rest.iter().any(|&s| matches!(h.view(s), View::Str(s) if s.contains("Basic")));
    let data = chartdata::read(cx, *d)?;
    let mut look = look_of(cx, o, Look::default(), "box_whisker_chart");
    // groups of data sets: a set; a list of sets; a list of lists of sets
    let mut groups: Vec<(Option<&Node<'h>>, Vec<(&Node<'h>, Vec<f64>)>)> = vec![];
    if let Some(v) = data.numbers() {
        groups.push((None, vec![(&data, v)]));
    } else if data.items().iter().any(|n| n.numbers().is_some()) {
        groups.push((None, data.items().iter().filter_map(|n| n.numbers().map(|v| (n, v))).collect()));
    } else {
        for g in data.items() {
            let sets: Vec<(&Node<'h>, Vec<f64>)> = g.items().iter().filter_map(|n| n.numbers().map(|v| (n, v))).collect();
            if !sets.is_empty() {
                groups.push((Some(g), sets));
            }
        }
    }
    groups.iter_mut().for_each(|g| {
        g.1.iter_mut().for_each(|s| {
            s.1.retain(|v| v.is_finite());
            s.1.sort_by(f64::total_cmp);
        });
        g.1.retain(|s| !s.1.is_empty());
    });
    groups.retain(|g| !g.1.is_empty());
    if groups.is_empty() {
        return empty(cx, &look, "box_whisker_chart");
    }
    let mut under = vec![];
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    let mut ticks = vec![];
    let mut x = 0.0;
    let several = groups.len() > 1;
    for (g, sets) in &groups {
        let start = x + 1.0;
        for (k, (n, s)) in sets.iter().enumerate() {
            x += 1.0;
            let (q1, med, q3) = (quantile(s, 0.25), quantile(s, 0.5), quantile(s, 0.75));
            let iqr = q3 - q1;
            let (flo, fhi) = (q1 - 1.5 * iqr, q3 + 1.5 * iqr);
            let wlo = s.iter().copied().find(|&v| v >= flo).unwrap_or(q1);
            let whi = s.iter().rev().copied().find(|&v| v <= fhi).unwrap_or(q3);
            lo = lo.min(s[0]);
            hi = hi.max(s[s.len() - 1]);
            let c = rgb(look.color(if several { k } else { 0 }));
            let dark = mc_graphics_kernel::rgb_term(h, Rgba::rgb(c.r * 0.6, c.g * 0.6, c.b * 0.6));
            let w = 0.3;
            let mut part = styled(h, c, *g, n, true);
            if notched {
                let nh = 1.57 * iqr / (s.len() as f64).sqrt();
                let pts = [[x - w, q1], [x + w, q1], [x + w, med - nh], [x + w * 0.5, med], [x + w, med + nh], [x + w, q3], [x - w, q3], [x - w, med + nh], [x - w * 0.5, med], [x - w, med - nh]];
                part.push(h.call(h.sym("polygon"), &[list(h, &pts.iter().map(|p| h.reals(p)).collect::<Vec<_>>())]));
            } else {
                part.push(h.call(h.sym("rectangle"), &[h.reals(&[x - w, q1]), h.reals(&[x + w, q3])]));
            }
            under.push(list(h, &part));
            let seg = |a: [f64; 2], b: [f64; 2]| h.call(h.sym("line"), &[list(h, &[h.reals(&a), h.reals(&b)])]);
            let mut lines = vec![dark, h.call(h.sym("absolute_thickness"), &[h.real(1.2)])];
            lines.push(seg([x, q3], [x, whi]));
            lines.push(seg([x, q1], [x, wlo]));
            lines.push(seg([x - w / 2.0, whi], [x + w / 2.0, whi]));
            lines.push(seg([x - w / 2.0, wlo], [x + w / 2.0, wlo]));
            lines.push(h.sym_term(h.sym("white")));
            lines.push(h.call(h.sym("absolute_thickness"), &[h.real(2.0)]));
            lines.push(seg([x - w, med], [x + w, med]));
            under.push(list(h, &lines));
            let out: Vec<T<'h>> = s.iter().copied().filter(|&v| v < flo || v > fhi).map(|v| h.reals(&[x, v])).collect();
            if outliers && !out.is_empty() {
                under.push(list(h, &[dark, h.call(h.sym("absolute_point_size"), &[h.real(4.0)]), h.call(h.sym("point"), &[list(h, &out)])]));
            }
            if !several {
                ticks.push((x, n.label_text(h).unwrap_or_else(|| (k + 1).to_string())));
            }
        }
        if several {
            ticks.push(((start + x) / 2.0, g.and_then(|g| g.label_text(h)).unwrap_or_else(|| ticks.len().to_string())));
            x += 0.6;
        }
    }
    if several {
        for (k, t) in ticks.iter_mut().enumerate() {
            if t.1.parse::<usize>().is_ok() {
                t.1 = (k + 1).to_string();
            }
        }
    }
    if look.x_ticks.is_none() {
        look.x_ticks = Some(ticks);
    }
    let pad = (hi - lo).abs().max(1e-9) * 0.08;
    let extra = Extra { under, bounds: Some([(0.4, x + 0.6), (lo - pad, hi + pad)]), entries: vec![] };
    finish(cx, &[], &look, extra)
}

// ---- smooth histogram

/// Silverman's rule of thumb bandwidth of sorted data.
pub(crate) fn bandwidth(sorted: &[f64]) -> f64 {
    let n = sorted.len() as f64;
    let mean = sorted.iter().sum::<f64>() / n;
    let sd = (sorted.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (n - 1.0).max(1.0)).sqrt();
    let iqr = quantile(sorted, 0.75) - quantile(sorted, 0.25);
    let s = if iqr > 0.0 { sd.min(iqr / 1.34) } else { sd };
    (0.9 * s * n.powf(-0.2)).max(1e-9 * (1.0 + mean.abs())).max(if s == 0.0 { 0.1 } else { 0.0 })
}

/// The Gaussian kernel density estimate of `data` at `x`, and its distribution function.
pub(crate) fn kde(data: &[f64], bw: f64, x: f64) -> (f64, f64) {
    let n = data.len() as f64;
    let (mut p, mut c) = (0.0, 0.0);
    for v in data {
        let z = (x - v) / bw;
        p += (-0.5 * z * z).exp();
        c += 0.5 * (1.0 + crate::charts3::erf(z / std::f64::consts::SQRT_2));
    }
    (p / (n * bw * (std::f64::consts::TAU).sqrt()), c / n)
}

/// The error function (Abramowitz and Stegun 7.1.26, |error| < 1.5e-7).
pub(crate) fn erf(x: f64) -> f64 {
    let t = 1.0 / (1.0 + 0.3275911 * x.abs());
    let y = 1.0 - (((((1.061405429 * t - 1.453152027) * t) + 1.421413741) * t - 0.284496736) * t + 0.254829592) * t * (-x * x).exp();
    if x >= 0.0 {
        y
    } else {
        -y
    }
}

pub(crate) fn smooth_histogram<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>], o: Option<T<'h>>) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [d, rest @ ..] = a else { return Ok(Reply::Decline) };
    let data = chartdata::read(cx, *d)?;
    let mut sets: Vec<(Option<String>, Vec<f64>)> = data.data_sets().into_iter().map(|(n, v)| (n.label_text(h), v)).collect();
    if sets.len() == 1 && sets[0].0.is_none() {
        sets[0].0 = None;
    }
    // bandwidth: a number, or automatic
    let bw_given = rest.first().and_then(|&b| crate::values::call_of(h, b).map_or(mc_graphics::num::number(h, b), |_| None));
    let kind = rest.get(1).map(|&k| match h.view(k) {
        View::Str(s) => s.to_uppercase(),
        View::Sym(s) => h.sym_name(s).to_uppercase(),
        _ => "PDF".into(),
    });
    let mut curves = vec![];
    let mut names = vec![];
    for (name, mut s) in sets {
        s.retain(|v| v.is_finite());
        if s.is_empty() {
            continue;
        }
        s.sort_by(f64::total_cmp);
        let bw = bw_given.filter(|b| *b > 0.0).unwrap_or_else(|| bandwidth(&s));
        let (lo, hi) = (s[0] - 3.0 * bw, s[s.len() - 1] + 3.0 * bw);
        let mut pts = Vec::with_capacity(201);
        for k in 0..=200 {
            let x = lo + (hi - lo) * k as f64 / 200.0;
            let (p, c) = kde(&s, bw, x);
            let y = match kind.as_deref() {
                Some("CDF") => c,
                Some("SF") => 1.0 - c,
                Some("HF") => p / (1.0 - c).max(1e-12),
                Some("CHF") => -(1.0 - c).max(1e-300).ln(),
                _ => p,
            };
            pts.push(h.reals(&[x, y]));
        }
        curves.push(list(h, &pts));
        names.push(name);
    }
    if curves.is_empty() {
        let look = look_of(cx, o, Look::default(), "smooth_histogram");
        return empty(cx, &look, "smooth_histogram");
    }
    let filled = !matches!(kind.as_deref(), Some("CDF" | "SF" | "HF" | "CHF"));
    let mut args = vec![list(h, &curves)];
    let mut opts = vec![];
    if filled {
        opts.push(rule(h, "filling", h.sym_term(h.sym("axis"))));
    }
    if names.iter().all(Option::is_some) && names.len() > 1 && option(cx, o, "plot_legends").is_none() {
        opts.push(rule(h, "plot_legends", list(h, &names.iter().map(|n| h.string(n.as_deref().unwrap_or(""))).collect::<Vec<_>>())));
    }
    if let Some(o) = o {
        if let View::App { args: rs, .. } = h.view(o) {
            opts.extend(rs.iter().copied());
        }
    }
    args.push(h.call(h.sym("options"), &opts));
    Ok(Reply::Again(h.call(h.sym("list_line_plot"), &args)))
}

// ---- complex list plot

/// Complex data as points [re, im], keeping the wrappers, labels and sets the list plots read.
pub(crate) fn complex_points<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, depth: usize) -> Res<T<'h>> {
    let h = cx.heap();
    if depth > 8 {
        return Ok(t);
    }
    if let Some(entries) = crate::values::assoc_entries(cx, t)? {
        let mut out = vec![];
        for (k, v) in entries {
            let p = complex_points(cx, v, depth + 1)?;
            out.push(h.call(h.sym("callout"), &[p, h.string(&k)]));
        }
        return Ok(list(h, &out));
    }
    if let Some((n, args)) = crate::values::call_of(h, t) {
        match (n.as_str(), args) {
            ("labeled" | "callout" | "legended" | "style" | "tooltip", &[x, ref rest @ ..]) => {
                let x = complex_points(cx, x, depth + 1)?;
                let mut a = vec![x];
                a.extend_from_slice(rest);
                let head = if n == "tooltip" { "labeled" } else { n.as_str() };
                return Ok(h.call(h.sym(head), &a));
            }
            (r, &[x, l]) if r == names::RULE => {
                if let (Some(xs), Some(ls)) = (cx.as_list(x), cx.as_list(l)) {
                    if xs.len() == ls.len() {
                        let (xs, ls) = (xs.to_vec(), ls.to_vec());
                        let mut out = vec![];
                        for (x, l) in xs.into_iter().zip(ls) {
                            let p = complex_points(cx, x, depth + 1)?;
                            out.push(h.call(h.sym("callout"), &[p, l]));
                        }
                        return Ok(list(h, &out));
                    }
                }
                let p = complex_points(cx, x, depth + 1)?;
                return Ok(h.call(h.sym("callout"), &[p, l]));
            }
            _ => {}
        }
    }
    if let Some(items) = cx.as_list(t) {
        let items = items.to_vec();
        let mut out = Vec::with_capacity(items.len());
        for e in items {
            let p = complex_points(cx, e, depth + 1)?;
            // entries that are not numbers are left out
            if !matches!(h.view(p), View::Sym(_)) || cx.as_list(p).is_some() {
                out.push(p);
            }
        }
        return Ok(list(h, &out));
    }
    let v = cx.eval(h.call(h.sym("n"), &[t]))?;
    Ok(match h.view(v) {
        View::Int(a) => h.reals(&[a as f64, 0.0]),
        View::Real(a) => h.reals(&[a, 0.0]),
        View::App { head, args: &[a, b] } if matches!(h.view(head), View::Sym(s) if h.sym_name(s) == names::COMPLEX) => match (mc_graphics::num::number(h, a), mc_graphics::num::number(h, b)) {
            (Some(a), Some(b)) => h.reals(&[a, b]),
            _ => h.sym_term(h.sym("none")),
        },
        _ => h.sym_term(h.sym("none")),
    })
}

pub(crate) fn complex_list_plot<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>], o: Option<T<'h>>) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [d] = a else { return Ok(Reply::Decline) };
    let data = complex_points(cx, *d, 0)?;
    // one point alone is a list of one
    let data = if cx.as_list(data).is_some_and(|l| l.len() == 2 && l.iter().all(|&e| matches!(h.view(e), View::Real(_)))) { list(h, &[data]) } else { data };
    let mut args = vec![data];
    args.extend(o);
    Ok(Reply::Again(h.call(h.sym("list_plot"), &args)))
}
