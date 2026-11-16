//! The axes of a picture that `show` combines from plots, marked afresh for the combined range: the range is
//! settled first (the union of the plots' ranges, or the `plot_range` given to `show`, which is in data values and
//! is placed on logarithmic axes here), then the ticks and the grid lines asked for are worked out for that range
//! exactly as one plot of that range marks them ([`figure::axis_marks`]). The axes are the first plot's (its
//! scales, frame or axes through the origin). Options given to `show` win; ticks a plot was given (and date ticks)
//! are kept as they are; grid lines appear only when `show` or a plot asks for them.
use crate::figure::{self, Axis, Look, Scale};
use crate::sidecar::{self, Ask, Marks};
use mc_core::{Heap, Store, View, T};

/// The marks of a plot drawn as `look`, remembered with its picture.
pub(crate) fn marks<'h>(h: &'h Heap, look: &Look<'h>) -> Marks {
    let given = |k: &str| look.graphics.iter().find(|(n, _)| n == k).map(|(_, v)| *v);
    let own = given("ticks").is_some() || given("frame_ticks").is_some();
    let grid = match given("grid_lines") {
        None => [Ask::None; 2],
        Some(v) if is_auto(h, v) => [Ask::Auto; 2],
        Some(v) => match pair(h, v) {
            Some(xy) => xy.map(|e| if is_auto(h, e) { Ask::Auto } else if is_none(h, e) { Ask::None } else { Ask::Given }),
            None if is_none(h, v) => [Ask::None; 2],
            None => [Ask::Given; 2],
        },
    };
    Marks { origin_axes: look.origin_axes, own_ticks: [own || look.x_ticks.is_some(), own], grid }
}

fn is_auto(h: &Heap, t: T<'_>) -> bool {
    matches!(h.view(t), View::Sym(s) if matches!(h.sym_name(s).as_str(), "automatic" | "true" | "all"))
}

fn is_none(h: &Heap, t: T<'_>) -> bool {
    matches!(h.view(t), View::Sym(s) if matches!(h.sym_name(s).as_str(), "none" | "false"))
}

/// The two elements of a list of two.
fn pair<'h>(h: &'h Heap, t: T<'h>) -> Option<[T<'h>; 2]> {
    match h.view(t) {
        View::App { head, args: &[x, y] } if matches!(h.view(head), View::Sym(s) if h.sym_name(s) == mc_api::names::LIST) => Some([x, y]),
        _ => None,
    }
}

/// A range `[[x0, x1], [y0, y1]]` of numbers.
fn range(h: &Heap, t: T<'_>) -> Option<[(f64, f64); 2]> {
    let [x, y] = pair(h, t)?;
    let ends = |t| -> Option<(f64, f64)> {
        let [a, b] = pair(h, t)?;
        Some((mc_graphics::num::number(h, a)?, mc_graphics::num::number(h, b)?))
    };
    Some([ends(x)?, ends(y)?])
}

/// [`mc_graphics_kernel::Reaxis`]: the combined options `rules` of a `show` of the graphics drawn to `paths`
/// (`given`: the options `show` was given) with the axes marked for the combined range.
pub(crate) fn reaxis<'h>(h: &'h Heap, paths: &[Option<String>], rules: &mut Vec<(String, T<'h>)>, given: &[String]) {
    let plots: Vec<(Scale, Marks)> = paths.iter().flatten().filter_map(|p| sidecar::marks_of(p)).collect();
    let Some(&(scale, first)) = plots.first() else { return };
    let axes = [scale.x, scale.y];
    let gave = |k: &str| given.iter().any(|g| g == k);
    let get = |rules: &Vec<(String, T<'h>)>, k: &str| rules.iter().find(|(n, _)| n == k).map(|r| r.1);
    let set = |rules: &mut Vec<(String, T<'h>)>, k: &str, v: Option<T<'h>>| {
        rules.retain(|(n, _)| n != k);
        if let Some(v) = v {
            rules.push((k.to_string(), v));
        }
    };
    let list = |items: &[T<'h>]| h.call(h.sym(mc_api::names::LIST), items);
    // a range given to `show` is in data values: placed on logarithmic axes as a plot places its own
    if gave("plot_range") && axes.iter().any(|a| *a != Axis::Linear) {
        if let Some(r) = get(rules, "plot_range").and_then(|v| range(h, v)) {
            let on = |(a, b): (f64, f64), ax: Axis| Some((ax.to(a)?, ax.to(b)?));
            if let (Some(x), Some(y)) = (on(r[0], scale.x), on(r[1], scale.y)) {
                set(rules, "plot_range", Some(list(&[h.reals(&[x.0, x.1]), h.reals(&[y.0, y.1])])));
            }
        }
    }
    let Some(r) = get(rules, "plot_range").and_then(|v| range(h, v)) else { return };
    let marks: Vec<(T<'h>, T<'h>)> = (0..2).map(|k| figure::axis_marks(h, r[k].0, r[k].1, axes[k], first.origin_axes)).collect();
    // ticks: those given to `show` as they are; per axis those a plot was given, else afresh
    if !gave("ticks") && !gave("frame_ticks") {
        let old = get(rules, "ticks").and_then(|v| pair(h, v));
        let auto = h.sym_term(h.sym("automatic"));
        let t: Vec<T<'h>> =
            (0..2).map(|k| if first.own_ticks[k] { old.map_or(auto, |o| o[k]) } else { marks[k].0 }).collect();
        set(rules, "ticks", Some(list(&t)));
    }
    // grid lines: those `show` asks for, on the combined axes; else per axis afresh where a plot asks for its own,
    // the given positions where a plot gives them, none otherwise
    let own_grid = [marks[0].1, marks[1].1];
    if gave("grid_lines") {
        if let Some(v) = get(rules, "grid_lines") {
            set(rules, "grid_lines", Some(figure::asked_grid(h, v, &own_grid, scale)));
        }
        return;
    }
    let none = h.sym_term(h.sym("none"));
    let old = get(rules, "grid_lines").and_then(|v| pair(h, v));
    let ask = |k: usize| {
        let asks = plots.iter().map(|p| p.1.grid[k]);
        if asks.clone().any(|a| a == Ask::Auto) {
            Ask::Auto
        } else if asks.into_iter().any(|a| a == Ask::Given) {
            Ask::Given
        } else {
            Ask::None
        }
    };
    let g: Vec<T<'h>> = (0..2)
        .map(|k| match ask(k) {
            Ask::Auto => own_grid[k],
            Ask::Given => old.map_or(none, |o| o[k]),
            Ask::None => none,
        })
        .collect();
    set(rules, "grid_lines", (g[0] != none || g[1] != none).then(|| list(&g)));
}
