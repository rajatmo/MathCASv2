//! Plots, first version: `plot` and `list_plot` rendered straight to PNG images.
//!
//! A plot evaluates to `image("path")`, where `path` is a PNG file in the system's temporary folder
//! (`mathcas-plots/`); front ends show it (the Jupyter kernel displays it inline). There is no graphics term model
//! yet: that comes with the data-structure work for file formats. Functions are sampled adaptively (64 even points, refined where the curve bends or stops)
//! through `mc_numfn::compile_expr` (compiled when it pays); a value that is not a finite real breaks the curve, and
//! so does a jump larger than the visible range (a pole). The y range is robust: the 1% and 99% quantiles of the
//! sampled values, widened to the full range when no value lies far outside them.
#![warn(missing_docs)]

mod render;

use mc_api::names;
use mc_core::{Heap, Store, Sym, View, T};
use mc_eval::{Attributes, Kernel, Native, NativeCx, Reply, Res};
use mc_numfn::{NumFn, Options, Var};
use render::Series;
use std::sync::atomic::{AtomicU64, Ordering};

/// Every function of this package with a one-line description.
pub const FUNCTIONS: &[(&str, &str)] = &[
    (
        "plot",
        "plot(f, [x, a, b]): a PNG image of f for x from a to b (plot([f1, f2, ...], [x, a, b]) for several curves); evaluates to image(path)",
    ),
    (
        "list_plot",
        "list_plot(data): a PNG image of the points [y1, y2, ...] (at x = 1, 2, ...) or [[x1, y1], ...]; joined: true draws a line; evaluates to image(path)",
    ),
];


/// Register the natives with `k`.
pub fn install(k: &mut Kernel, h: &Heap) {
    k.register(h.sym("plot"), Attributes::default(), Box::new(Plot));
    k.register(h.sym("list_plot"), Attributes::default(), Box::new(ListPlot));
}

fn failure<'h>(h: &'h Heap, tag: &str, details: &[T<'h>]) -> T<'h> {
    let mut a = vec![h.sym_term(h.sym(tag))];
    a.extend_from_slice(details);
    h.call(h.sym(names::FAILURE), &a)
}

fn bad<'h>(h: &'h Heap, f: &str, k: i64) -> Reply<'h> {
    Reply::Done(failure(h, "argument", &[h.sym_term(h.sym(f)), h.int(k)]))
}

/// A machine real for a number or a closed numeric expression (through `n`).
fn real<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>) -> Res<Option<f64>> {
    let h = cx.heap();
    let direct = |t: T<'h>| match h.view(t) {
        View::Int(i) => Some(i as f64),
        View::Real(x) => Some(x),
        _ => None,
    };
    if let Some(x) = direct(t) {
        return Ok(Some(x).filter(|x| x.is_finite()));
    }
    let v = cx.eval(h.call(h.sym("n"), &[t]))?;
    Ok(direct(v).filter(|x| x.is_finite()))
}

/// The image term for a written PNG file.
fn image<'h>(h: &'h Heap, path: &str) -> T<'h> {
    h.call(h.sym("image"), &[h.string(path)])
}

/// Write the rendered plot to a fresh file and give its image term (or a failure).
fn finish<'h>(cx: &mut NativeCx<'_, '_, 'h>, series: &[Series], joined: bool) -> Res<Reply<'h>> {
    let h = cx.heap();
    // writing a file is an effect: in program order, never speculatively
    cx.effect()?;
    static COUNT: AtomicU64 = AtomicU64::new(0);
    let dir = std::env::temp_dir().join("mathcas-plots");
    let path = dir.join(format!("plot-{}-{}.png", std::process::id(), COUNT.fetch_add(1, Ordering::Relaxed)));
    let written = std::fs::create_dir_all(&dir)
        .map_err(|e| e.to_string())
        .and_then(|_| render::render(series, joined).ok_or_else(|| "rendering failed".to_string()))
        .and_then(|png| std::fs::write(&path, png).map_err(|e| e.to_string()));
    Ok(Reply::Done(match written {
        // forward slashes: the path prints without escapes on every system
        Ok(()) => image(h, &path.to_string_lossy().replace('\\', "/")),
        Err(e) => failure(h, "io", &[h.string(&e)]),
    }))
}

/// `plot(f, [x, a, b])`.
struct Plot;

impl Native for Plot {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let [f, spec] = args else { return Ok(Reply::Decline) };
        let Some(&[xv, a, b]) = cx.as_list(*spec) else { return Ok(bad(h, "plot", 2)) };
        let View::Sym(x) = h.view(xv) else { return Ok(bad(h, "plot", 2)) };
        let (Some(a), Some(b)) = (real(cx, a)?, real(cx, b)?) else { return Ok(bad(h, "plot", 2)) };
        if a >= b {
            return Ok(bad(h, "plot", 2));
        }
        let fs: Vec<T<'h>> = match cx.as_list(*f) {
            Some(fs) if !fs.is_empty() => fs.to_vec(),
            Some(_) => return Ok(bad(h, "plot", 1)),
            None => vec![*f],
        };
        let mut series = Vec::with_capacity(fs.len());
        for f in fs {
            series.push(sample(cx, f, x, a, b)?);
        }
        finish(cx, &series, true)
    }
}

/// The curve of `f` in `x` on [a, b], sampled adaptively: `INITIAL` even points, then each interval is halved
/// while its midpoint lies more than `TOLERANCE` (as a fraction of the visible y range, about half a pixel) off the
/// chord, or while the value is a number at one end and not at the other (to find where it stops), up to `DEPTH`
/// halvings and `BUDGET` evaluations in all. `None` where the value is not a finite real.
fn sample<'h>(cx: &mut NativeCx<'_, '_, 'h>, f: T<'h>, x: Sym, a: f64, b: f64) -> Res<Series> {
    let fun = mc_numfn::compile_expr(cx, f, &[Var::scalar(x)], &Options::default())?;
    let mut s = Sampler { fun, evals: 0 };
    let mut grid = Vec::with_capacity(INITIAL);
    for k in 0..INITIAL {
        let xk = a + (b - a) * k as f64 / (INITIAL - 1) as f64;
        grid.push((xk, s.at(cx, xk)?));
    }
    // the visible y range of the coarse curve sets the tolerance
    let span = {
        let (lo, hi) = render::robust(grid.iter().filter_map(|p| p.1).collect());
        if hi > lo {
            hi - lo
        } else {
            1.0
        }
    };
    let mut pts = vec![grid[0]];
    for w in grid.windows(2) {
        s.refine(cx, w[0], w[1], span, DEPTH, &mut pts)?;
    }
    Ok(Series { points: pts })
}

/// Even points before refinement.
const INITIAL: usize = 64;
/// Halvings of an initial interval at most.
const DEPTH: u32 = 10;
/// Evaluations per curve at most.
const BUDGET: u64 = 20_000;
/// Deviation from the chord, as a fraction of the visible y range, that asks for a halving.
const TOLERANCE: f64 = 0.0015;

struct Sampler<'h> {
    fun: Box<dyn NumFn<'h, f64> + 'h>,
    evals: u64,
}

impl<'h> Sampler<'h> {
    fn at(&mut self, cx: &mut NativeCx<'_, '_, 'h>, x: f64) -> Res<Option<f64>> {
        self.evals += 1;
        if self.evals.is_multiple_of(64) {
            cx.tick(1)?;
        }
        let mut out = [0.0];
        let ok = self.fun.eval(cx, &[x], &mut out)?;
        Ok(if ok && out[0].is_finite() { Some(out[0]) } else { None })
    }

    /// Push the points of (p, q], refined; `p` is already pushed. Recursion depth is at most `DEPTH`.
    fn refine(
        &mut self,
        cx: &mut NativeCx<'_, '_, 'h>,
        p: (f64, Option<f64>),
        q: (f64, Option<f64>),
        span: f64,
        depth: u32,
        out: &mut Vec<(f64, Option<f64>)>,
    ) -> Res<()> {
        if depth > 0 && self.evals < BUDGET {
            let xm = 0.5 * (p.0 + q.0);
            let m = (xm, self.at(cx, xm)?);
            let split = match (p.1, m.1, q.1) {
                (Some(a), Some(c), Some(b)) => (c - 0.5 * (a + b)).abs() > TOLERANCE * span,
                (None, None, None) => false,
                _ => true,
            };
            if split {
                self.refine(cx, p, m, span, depth - 1, out)?;
                return self.refine(cx, m, q, span, depth - 1, out);
            }
            out.push(m);
        }
        out.push(q);
        Ok(())
    }
}

/// `list_plot(data)`, `list_plot(data, joined: true)`.
struct ListPlot;

impl Native for ListPlot {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let (data, joined) = match args {
            [d] => (*d, false),
            [d, o] => match joined_option(cx, *o) {
                Some(j) => (*d, j),
                None => return Ok(bad(h, "list_plot", 2)),
            },
            _ => return Ok(Reply::Decline),
        };
        let Some(items) = cx.as_list(data) else { return Ok(bad(h, "list_plot", 1)) };
        let mut pts = Vec::with_capacity(items.len());
        for (i, &it) in items.iter().enumerate() {
            if i % 4096 == 0 {
                cx.tick(1)?;
            }
            let p = match cx.as_list(it) {
                Some(&[px, py]) => (real(cx, px)?, real(cx, py)?),
                Some(_) => return Ok(bad(h, "list_plot", 1)),
                None => (Some(i as f64 + 1.0), real(cx, it)?),
            };
            match p {
                (Some(px), y) => pts.push((px, y)),
                (None, _) => return Ok(bad(h, "list_plot", 1)),
            }
        }
        if pts.iter().all(|p| p.1.is_none()) {
            return Ok(bad(h, "list_plot", 1));
        }
        finish(cx, &[Series { points: pts }], joined)
    }
}

/// `options(joined -> true | false)`: the option's value, or `None` for anything else.
fn joined_option<'h>(cx: &NativeCx<'_, '_, 'h>, o: T<'h>) -> Option<bool> {
    let h = cx.heap();
    let View::App { head, args } = h.view(o) else { return None };
    if head != h.sym_term(h.sym("options")) {
        return None;
    }
    let mut joined = false;
    for &r in args {
        let View::App { head, args: kv } = h.view(r) else { return None };
        if head != h.sym_term(h.sym(names::RULE)) || kv.len() != 2 || kv[0] != h.sym_term(h.sym("joined")) {
            return None;
        }
        joined = kv[1] == h.sym_term(h.sym("true"));
    }
    Some(joined)
}
