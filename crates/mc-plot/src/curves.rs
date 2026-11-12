//! Curves in the plane: `parametric_plot([fx, fy], [t, a, b])` and `polar_plot(r, [theta, a, b])`, sampled
//! adaptively in the parameter, drawn with equal units on both axes (a circle is round).
use crate::figure::{Extra, Look, Series, H, W};
use crate::{bad, finish, plot_options, real, short_name, unwrap_all};
use mc_core::{Heap, Store, Sym, View, T};
use mc_eval::{Kernel, Native, NativeCx, Reply, Res};
use mc_numfn::{NumFn, Options, Var};

/// Register `parametric_plot` and `polar_plot`.
pub(crate) fn install(k: &mut Kernel, h: &Heap) {
    crate::held::register(k, h, "parametric_plot", Box::new(Planar { polar: false }));
    crate::held::register(k, h, "polar_plot", Box::new(Planar { polar: true }));
}

/// The options of plane curves.
const OPTIONS: &[&str] = &["line_style"];
/// Even parameter values before refinement.
const INITIAL: usize = 128;
/// Halvings of an initial interval at most.
const DEPTH: u32 = 10;
/// Evaluations per curve at most.
const BUDGET: u64 = 30_000;
/// Distance of the midpoint from the chord's midpoint, as a fraction of the curve's extent, that asks for a halving.
const TOLERANCE: f64 = 0.001;
/// A step between neighbouring points after every halving, as a fraction of the extent, taken for a jump.
const JUMP: f64 = 0.05;

/// `parametric_plot` (`polar: false`) or `polar_plot`.
struct Planar {
    polar: bool,
}

impl Planar {
    fn name(&self) -> &'static str {
        if self.polar {
            "polar_plot"
        } else {
            "parametric_plot"
        }
    }
}

/// One curve: x and y as functions of the parameter (for a polar curve the same radius twice).
struct Curve<'h> {
    fx: T<'h>,
    fy: T<'h>,
}

impl Native for Planar {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let name = self.name();
        let base = Look { joined: true, equal: true, origin_axes: self.polar, ..Look::default() };
        if !self.polar {
            let (a, o) = crate::more::split(h, args);
            if let [f, s1, s2] = a.as_slice() {
                return region(cx, *f, *s1, *s2, o);
            }
        }
        let (f, spec, look) = match args {
            [f, spec] => (*f, *spec, base),
            [f, spec, o] => match plot_options(cx, *o, base, OPTIONS, name) {
                Some(look) => (*f, *spec, look),
                None => return Ok(bad(h, name, 3)),
            },
            _ => return Ok(Reply::Decline),
        };
        let Some(&[tv, a, b]) = cx.as_list(spec) else { return Ok(bad(h, name, 2)) };
        let View::Sym(t) = h.view(tv) else { return Ok(bad(h, name, 2)) };
        let (Some(a), Some(b)) = (real(cx, a)?, real(cx, b)?) else { return Ok(bad(h, name, 2)) };
        if a >= b {
            return Ok(bad(h, name, 2));
        }
        // legended, labeled, callout and style around the whole or around each curve
        let mut look = look;
        let wrapped = |t: T<'h>| matches!(crate::values::call_of(h, t), Some((n, _)) if matches!(n.as_str(), "legended" | "labeled" | "callout" | "style" | "highlighted"));
        let f = if wrapped(f) {
            let (v, w) = unwrap_all(h, &[f]);
            w.apply(cx, &mut look);
            v[0]
        } else {
            match cx.as_list(f) {
                Some(items) if items.iter().any(|&e| wrapped(e)) => {
                    let (v, w) = unwrap_all(h, items);
                    w.apply(cx, &mut look);
                    cx.list(&v)
                }
                _ => f,
            }
        };
        // the curves: [fx, fy] or a list of them; a radius or a list of radii
        let curves: Vec<Curve<'h>> = if self.polar {
            match cx.as_list(f) {
                Some([]) => return Ok(bad(h, name, 1)),
                Some(rs) => rs.iter().map(|&r| Curve { fx: r, fy: r }).collect(),
                None => vec![Curve { fx: f, fy: f }],
            }
        } else {
            let Some(items) = cx.as_list(f) else { return Ok(bad(h, name, 1)) };
            match items {
                &[x, y] if cx.as_list(x).is_none() && cx.as_list(y).is_none() => vec![Curve { fx: x, fy: y }],
                [] => return Ok(bad(h, name, 1)),
                _ => {
                    let mut cs = vec![];
                    for &c in items {
                        let Some(&[x, y]) = cx.as_list(c) else { return Ok(bad(h, name, 1)) };
                        cs.push(Curve { fx: x, fy: y });
                    }
                    cs
                }
            }
        };
        let mut series = Vec::with_capacity(curves.len());
        for c in &curves {
            let pts = self.sample(cx, c, t, a, b)?;
            let label = if self.polar {
                short_name(h, c.fx)
            } else {
                format!("[{}, {}]", short_name(h, c.fx), short_name(h, c.fy))
            };
            series.push(Series { points: pts, name: label, bars: vec![] });
        }
        if series.iter().all(|s| s.points.iter().all(|p| p.1.is_none())) {
            return Ok(bad(h, name, 1));
        }
        look.size = Some(page_size(&series));
        finish(cx, &series, &look, Extra::default())
    }
}

/// A page shaped like the curves (equal units): 640 wide for wide curves, 400 high for tall ones.
fn page_size(series: &[Series]) -> (f64, f64) {
    let (mut x0, mut x1, mut y0, mut y1) = (f64::INFINITY, f64::NEG_INFINITY, f64::INFINITY, f64::NEG_INFINITY);
    for (x, y) in series.iter().flat_map(|s| s.points.iter()).filter_map(|&(x, y)| y.map(|y| (x, y))) {
        (x0, x1, y0, y1) = (x0.min(x), x1.max(x), y0.min(y), y1.max(y));
    }
    let r = if x1 > x0 && y1 > y0 { ((y1 - y0) / (x1 - x0)).clamp(0.2, 5.0) } else { 1.0 };
    // the margins take about 60 points across and 40 down
    if r <= (H - 40.0) / (W - 60.0) {
        (W, ((W - 60.0) * r + 40.0).max(200.0))
    } else {
        (((H - 40.0) / r + 60.0).max(240.0), H)
    }
}

/// A function of the parameter, compiled.
type Fun<'h> = Box<dyn NumFn<'h, f64> + 'h>;

/// Evaluates a curve at parameter values.
struct Sampler<'h> {
    fx: Fun<'h>,
    fy: Option<Fun<'h>>,
    polar: bool,
    evals: u64,
}

impl<'h> Sampler<'h> {
    /// The point at parameter `t`, `None` where either coordinate is not a finite real.
    fn at(&mut self, cx: &mut NativeCx<'_, '_, 'h>, t: f64) -> Res<Option<(f64, f64)>> {
        self.evals += 1;
        if self.evals.is_multiple_of(64) {
            cx.tick(1)?;
        }
        let mut out = [0.0];
        if !self.fx.eval(cx, &[t], &mut out)? || !out[0].is_finite() {
            return Ok(None);
        }
        let first = out[0];
        if self.polar {
            return Ok(Some((first * t.cos(), first * t.sin())));
        }
        let Some(fy) = self.fy.as_mut() else { return Ok(None) };
        if !fy.eval(cx, &[t], &mut out)? || !out[0].is_finite() {
            return Ok(None);
        }
        Ok(Some((first, out[0])))
    }

    /// Push the points of (p, q], refined; `p` is already pushed. Recursion depth is at most `DEPTH`.
    #[allow(clippy::too_many_arguments)]
    fn refine(
        &mut self,
        cx: &mut NativeCx<'_, '_, 'h>,
        p: (f64, Option<(f64, f64)>),
        q: (f64, Option<(f64, f64)>),
        span: (f64, f64),
        depth: u32,
        out: &mut Vec<(f64, Option<(f64, f64)>)>,
    ) -> Res<()> {
        let dist = |a: (f64, f64), b: (f64, f64)| ((a.0 - b.0) / span.0).hypot((a.1 - b.1) / span.1);
        if depth > 0 && self.evals < BUDGET {
            let tm = 0.5 * (p.0 + q.0);
            let m = (tm, self.at(cx, tm)?);
            let split = match (p.1, m.1, q.1) {
                (Some(a), Some(c), Some(b)) => dist(c, (0.5 * (a.0 + b.0), 0.5 * (a.1 + b.1))) > TOLERANCE,
                (None, None, None) => false,
                _ => true,
            };
            if split {
                self.refine(cx, p, m, span, depth - 1, out)?;
                return self.refine(cx, m, q, span, depth - 1, out);
            }
            out.push(m);
        } else if let (0, Some(a), Some(b)) = (depth, p.1, q.1) {
            if dist(a, b) > JUMP {
                out.push((0.5 * (p.0 + q.0), None));
            }
        }
        out.push(q);
        Ok(())
    }
}

impl Planar {
    /// The points of curve `c` for the parameter `t` in [a, b], as plot points (a missing point breaks the line).
    fn sample<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, c: &Curve<'h>, t: Sym, a: f64, b: f64) -> Res<Vec<(f64, Option<f64>)>> {
        let vars = [Var::scalar(t)];
        let fx = mc_numfn::compile_expr(cx, c.fx, &vars, &Options::default())?;
        let fy = if self.polar { None } else { Some(mc_numfn::compile_expr(cx, c.fy, &vars, &Options::default())?) };
        let mut s = Sampler { fx, fy, polar: self.polar, evals: 0 };
        let mut grid = Vec::with_capacity(INITIAL);
        for k in 0..INITIAL {
            let tk = a + (b - a) * k as f64 / (INITIAL - 1) as f64;
            grid.push((tk, s.at(cx, tk)?));
        }
        // the extent of the coarse curve sets the tolerance in each direction
        let ext = |f: fn(&(f64, f64)) -> f64| {
            let v: Vec<f64> = grid.iter().filter_map(|p| p.1.as_ref().map(f)).collect();
            let (lo, hi) = crate::figure::robust(v);
            if hi > lo {
                hi - lo
            } else {
                1.0
            }
        };
        let span = (ext(|p| p.0), ext(|p| p.1));
        let span = (span.0.max(span.1 * 1e-3), span.1.max(span.0 * 1e-3));
        let mut pts = vec![grid[0]];
        for w in grid.windows(2) {
            s.refine(cx, w[0], w[1], span, DEPTH, &mut pts)?;
        }
        Ok(pts.into_iter().map(|(_, p)| match p {
            Some((x, y)) => (x, Some(y)),
            None => (0.0, None),
        }).collect())
    }
}

/// `parametric_plot([fx, fy], [u, a, b], [v, c, d])`: the region the map covers, filled, with its mesh lines.
fn region<'h>(cx: &mut NativeCx<'_, '_, 'h>, f: T<'h>, s1: T<'h>, s2: T<'h>, o: Option<T<'h>>) -> Res<Reply<'h>> {
    let h = cx.heap();
    let name = "parametric_plot";
    let (Some((u, a, b)), Some((v, c, d))) = (crate::fields::spec(cx, s1)?, crate::fields::spec(cx, s2)?) else { return Ok(bad(h, name, 2)) };
    let base = Look { equal: true, ..Look::default() };
    let mut look = match o {
        Some(o) => plot_options(cx, o, base, &["mesh", "exclusions", "mesh_style", "boundary_style", "mesh_functions", "plot_points", "color_function", "region_function"], name).unwrap_or_default(),
        None => Look { who: name, ..base },
    };
    look.who = name;
    let f = crate::strip_wrappers(cx, f);
    let f = cx.eval(f)?;
    let maps: Vec<(T<'h>, T<'h>)> = match cx.as_list(f) {
        Some(&[x, y]) if cx.as_list(x).is_none() => vec![(x, y)],
        Some(items) => items.iter().filter_map(|&p| match cx.as_list(p) {
            Some(&[x, y]) => Some((x, y)),
            _ => None,
        }).collect(),
        None => vec![],
    };
    if maps.is_empty() {
        return Ok(bad(h, name, 1));
    }
    let n = 36;
    let vars = [mc_numfn::Var::scalar(u), mc_numfn::Var::scalar(v)];
    let mut under = vec![];
    let (mut x0, mut x1, mut y0, mut y1) = (f64::INFINITY, f64::NEG_INFINITY, f64::INFINITY, f64::NEG_INFINITY);
    for (k, (fx, fy)) in maps.into_iter().enumerate() {
        let mut gx = mc_numfn::compile_expr(cx, fx, &vars, &mc_numfn::Options::default())?;
        let mut gy = mc_numfn::compile_expr(cx, fy, &vars, &mc_numfn::Options::default())?;
        let mut grid = vec![None; (n + 1) * (n + 1)];
        let mut out = [0.0];
        for j in 0..=n {
            cx.tick(1)?;
            for i in 0..=n {
                let p = [a + (b - a) * i as f64 / n as f64, c + (d - c) * j as f64 / n as f64];
                let x = if gx.eval(cx, &p, &mut out)? { out[0] } else { f64::NAN };
                let y = if gy.eval(cx, &p, &mut out)? { out[0] } else { f64::NAN };
                if x.is_finite() && y.is_finite() {
                    grid[j * (n + 1) + i] = Some([x, y]);
                    x0 = x0.min(x);
                    x1 = x1.max(x);
                    y0 = y0.min(y);
                    y1 = y1.max(y);
                }
            }
        }
        let (r, g, bl) = look.color(k);
        let col = mc_graphics_kernel::rgb_term(h, mc_graphics::color::Rgba::rgb(r as f64 / 255.0, g as f64 / 255.0, bl as f64 / 255.0));
        let mut part = vec![col, h.call(h.sym("opacity"), &[h.real(0.35)])];
        let mut quads = vec![];
        for j in 0..n {
            for i in 0..n {
                let q = [grid[j * (n + 1) + i], grid[j * (n + 1) + i + 1], grid[(j + 1) * (n + 1) + i + 1], grid[(j + 1) * (n + 1) + i]];
                if q.iter().all(Option::is_some) {
                    quads.extend(q.iter().flatten().flatten().copied());
                }
            }
        }
        if !quads.is_empty() {
            part.push(h.call(h.sym("polygon"), &[h.real_array(&[quads.len() / 8, 4, 2], &quads)]));
        }
        under.push(cx.list(&part));
        // mesh lines: every sixth grid line each way
        let mut lines = vec![col, h.call(h.sym("absolute_thickness"), &[h.real(0.6)])];
        for j in (0..=n).step_by(6) {
            let pts: Vec<T<'h>> = (0..=n).filter_map(|i| grid[j * (n + 1) + i]).map(|p| h.reals(&p)).collect();
            lines.push(h.call(h.sym("line"), &[cx.list(&pts)]));
        }
        for i in (0..=n).step_by(6) {
            let pts: Vec<T<'h>> = (0..=n).filter_map(|j| grid[j * (n + 1) + i]).map(|p| h.reals(&p)).collect();
            lines.push(h.call(h.sym("line"), &[cx.list(&pts)]));
        }
        under.push(cx.list(&lines));
    }
    if !x0.is_finite() {
        return Ok(bad(h, name, 1));
    }
    let extra = crate::figure::Extra { under, bounds: Some([(x0, if x1 > x0 { x1 } else { x0 + 1.0 }), (y0, if y1 > y0 { y1 } else { y0 + 1.0 })]), entries: vec![] };
    crate::finish(cx, &[], &look, extra)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pages_follow_the_curves_shape() {
        let s = |pts: &[(f64, f64)]| Series { points: pts.iter().map(|&(x, y)| (x, Some(y))).collect(), name: String::new(), bars: vec![] };
        assert_eq!(page_size(&[s(&[(-1.0, -1.0), (1.0, 1.0)])]), (420.0, 400.0));
        let (w, h) = page_size(&[s(&[(0.0, 0.0), (10.0, 1.0)])]);
        assert_eq!(w, 640.0);
        assert!((200.0..400.0).contains(&h));
        assert_eq!(page_size(&[]), (420.0, 400.0));
    }
}
