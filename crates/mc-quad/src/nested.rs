//! Integrals at machine precision, single and multiple: `n_integrate(f, [x, a, b], [y, c(x), d(x)], ...)` is the
//! integral over x of the integral over y, the inner ranges depending on the outer variables (decision N2.8). Each
//! level runs its own adaptive method; the integrand of an outer level is the inner integral, whose error estimate
//! joins the outer one.
use crate::machine::{piece, End, Integrand, Outcome, Piece};
use crate::options::{Goals, Method};
use crate::{de, gk};
use mc_eval::{NativeCx, Res};
use mc_numfn::{NumFn, Scalar};

/// A point of a range: a constant end, or an end depending on the outer variables (a compiled function of
/// them).
pub(crate) enum Bound<'h> {
    /// a number or an infinity
    Const(End),
    /// a function of the variables of the enclosing ranges
    Expr(Box<dyn NumFn<'h, f64> + 'h>),
}

/// How each level integrates.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Plan {
    /// the method asked for, or `None` for the automatic choice
    pub method: Option<Method>,
    /// the goals of the outermost level (inner levels aim ten times tighter)
    pub goals: Goals,
    /// the Gauss–Kronrod limits
    pub limits: gk::Limits,
}

/// A multiple integral of the machine function `f` of every range's variable (outer ones first).
pub(crate) struct Problem<'h, S> {
    /// the integrand, of all variables
    pub f: Box<dyn NumFn<'h, S> + 'h>,
    /// the points of each range (`[a, s1, ..., b]`), outermost first
    pub ranges: Vec<Vec<Bound<'h>>>,
    /// methods and goals
    pub plan: Plan,
    /// evaluations of the integrand so far
    pub calls: u64,
}

impl<'h, S: Scalar> Problem<'h, S> {
    /// The integral over every range.
    pub fn solve(&mut self, cx: &mut NativeCx<'_, '_, 'h>) -> Res<Outcome<S>> {
        let goals = self.plan.goals;
        self.level(cx, 0, &[], goals)
    }

    /// The integral over range `k` and the inner ones, the outer variables at `prefix`. A value that is not numeric
    /// at the midpoint of a piece (the first sample of the Gauss-Kronrod rule: `1/sqrt(|x|)` at 0 on `[-1, 1]`) is
    /// a singular point of the integrand: the piece is split there and the halves integrated again.
    fn level(&mut self, cx: &mut NativeCx<'_, '_, 'h>, k: usize, prefix: &[f64], goals: Goals) -> Res<Outcome<S>> {
        let mut extra: Vec<f64> = vec![];
        loop {
            let out = self.level_split(cx, k, prefix, goals, &extra)?;
            if let Outcome::NotNumeric(pt) = &out {
                if pt.len() == k + 1 && extra.len() < 3 {
                    let x = pt[k];
                    if !extra.contains(&x) && self.is_midpoint(cx, k, prefix, x)? && self.has_numeric_neighbour(cx, k, prefix, x)? {
                        extra.push(x);
                        continue;
                    }
                }
            }
            return Ok(out);
        }
    }

    /// Is the integrand (at the innermost level) numeric at a point near `x`, on either side? A singular point has
    /// numeric neighbours; an integrand that is not numeric anywhere (an unknown function) has none, and the first
    /// failing point is reported as it is.
    fn has_numeric_neighbour(&mut self, cx: &mut NativeCx<'_, '_, 'h>, k: usize, prefix: &[f64], x: f64) -> Res<bool> {
        if k + 1 != self.ranges.len() {
            return Ok(false);
        }
        let d = if x == 0.0 { 1e-5 } else { 1e-5 * x.abs() };
        for y in [x - d, x + d] {
            let mut pt: Vec<S> = prefix.iter().map(|&v| S::real(v)).collect();
            pt.push(S::real(y));
            let mut out = [S::real(0.0)];
            if self.f.eval(cx, &pt, &mut out)? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Is `x` the midpoint of a finite piece of range `k`?
    fn is_midpoint(&mut self, cx: &mut NativeCx<'_, '_, 'h>, k: usize, prefix: &[f64], x: f64) -> Res<bool> {
        let mut ends = vec![];
        for b in self.ranges[k].iter_mut() {
            match b {
                Bound::Const(End::At(v)) => ends.push(*v),
                Bound::Const(End::Inf(_)) => ends.push(f64::NAN),
                Bound::Expr(g) => {
                    let mut v = [0.0];
                    ends.push(if g.eval(cx, prefix, &mut v)? { v[0] } else { f64::NAN });
                }
            }
        }
        Ok(ends.windows(2).any(|w| w[0].is_finite() && w[1].is_finite() && 0.5 * (w[0] + w[1]) == x))
    }

    fn level_split(&mut self, cx: &mut NativeCx<'_, '_, 'h>, k: usize, prefix: &[f64], goals: Goals, extra: &[f64]) -> Res<Outcome<S>> {
        // the ends of the pieces
        let mut ends = Vec::with_capacity(self.ranges[k].len());
        for b in self.ranges[k].iter_mut() {
            match b {
                Bound::Const(e) => ends.push(*e),
                Bound::Expr(g) => {
                    let mut v = [0.0];
                    if !g.eval(cx, prefix, &mut v)? {
                        return Ok(Outcome::NotNumeric(prefix.to_vec()));
                    }
                    ends.push(End::At(v[0]));
                }
            }
        }
        // the break points found at singular midpoints
        let mut with_extra = Vec::with_capacity(ends.len() + extra.len());
        for (i, e) in ends.iter().enumerate() {
            with_extra.push(*e);
            if let (End::At(a), Some(End::At(b))) = (e, ends.get(i + 1)) {
                let (lo, hi) = (a.min(*b), a.max(*b));
                let mut inside: Vec<f64> = extra.iter().copied().filter(|&x| x > lo && x < hi).collect();
                inside.sort_by(|p, q| p.total_cmp(q));
                if a > b {
                    inside.reverse();
                }
                with_extra.extend(inside.into_iter().map(End::At));
            }
        }
        let ends = with_extra;
        let pieces: Vec<Piece> = ends.windows(2).filter_map(|w| piece(w[0], w[1])).collect();
        let plan = self.plan;
        let mut f = Level { p: self, k, prefix: prefix.to_vec() };
        let de_level = de::MAX_LEVEL;
        match plan.method {
            Some(Method::GaussKronrod) => gk::integrate(cx, &mut f, &pieces, goals, plan.limits),
            Some(Method::DoubleExponential) => de::integrate(cx, &mut f, &pieces, goals, de_level),
            Some(_) | None => {
                // the automatic choice (decision N2.7): the double-exponential rule first when the integrand is not
                // numeric at a finite end (an endpoint singularity) or not smooth there (decision quad-2.2: an
                // unbounded derivative, as x log x or x^x at 0, which defeats the Gauss-Kronrod error estimate),
                // Gauss-Kronrod first otherwise; the other method when the first does not converge, keeping the
                // better estimate
                let (pieces, flags) = f.split_rough(cx, &pieces)?;
                // an oscillating infinite tail: the oscillatory method first (decision quad-2.3)
                let mut osc_tail = false;
                for p in &pieces {
                    let (a, dir) = match p.map {
                        crate::machine::Map::Upper(a) => (a, 1.0),
                        crate::machine::Map::Lower(a) => (a, -1.0),
                        _ => continue,
                    };
                    osc_tail |= crate::osc::oscillating_tail(cx, &mut f, a, dir)?;
                }
                if osc_tail {
                    if let Some(o) = oscillatory(cx, &mut f, &pieces, &flags, goals, plan.limits, de_level)? {
                        if o.converged() {
                            return Ok(o);
                        }
                    }
                }
                let mixed = flags.iter().any(|&b| b) && !flags.iter().all(|&b| b);
                let out = if mixed {
                    // each piece by its own choice, summed
                    let mut sum = S::real(0.0);
                    let (mut err, mut conv) = (0.0, true);
                    let mut bad = None;
                    for (p, &fl) in pieces.iter().zip(&flags) {
                        match auto(cx, &mut f, std::slice::from_ref(p), fl, goals, plan.limits, de_level)? {
                            Outcome::Done { value, err: e, converged } => {
                                sum = sum + value;
                                err += e;
                                conv &= converged;
                            }
                            o @ Outcome::NotNumeric(_) => {
                                bad = Some(o);
                                break;
                            }
                        }
                    }
                    match bad {
                        Some(o) => o,
                        None => {
                            let tol = goals.tol(sum.abs()).max(crate::machine::ROUNDOFF * sum.abs());
                            Outcome::Done { value: sum, err, converged: conv || err <= tol }
                        }
                    }
                } else {
                    auto(cx, &mut f, &pieces, flags.iter().any(|&b| b), goals, plan.limits, de_level)?
                };
                if out.converged() || !pieces.iter().any(|p| p.map != crate::machine::Map::Id) {
                    return Ok(out);
                }
                // an infinite range that neither rule resolves: the oscillatory method on the infinite pieces
                let osc = oscillatory(cx, &mut f, &pieces, &flags, goals, plan.limits, de_level)?;
                Ok(match osc {
                    Some(o @ Outcome::Done { .. }) if o.err() < out.err() || o.converged() => o,
                    _ => out,
                })
            }
        }
    }
}

/// The two-method automatic integration of `pieces` (decision N2.7), double-exponential first if `rough`.
fn auto<'h, S: Scalar>(
    cx: &mut NativeCx<'_, '_, 'h>,
    f: &mut dyn Integrand<'h, S>,
    pieces: &[Piece],
    rough: bool,
    goals: Goals,
    limits: gk::Limits,
    de_level: u32,
) -> Res<Outcome<S>> {
    let first = if rough { de::integrate(cx, f, pieces, goals, de_level)? } else { gk::integrate(cx, f, pieces, goals, limits)? };
    if first.converged() {
        return Ok(first);
    }
    let second = if rough { gk::integrate(cx, f, pieces, goals, limits)? } else { de::integrate(cx, f, pieces, goals, de_level)? };
    Ok(match (&first, &second) {
        (_, Outcome::Done { converged: true, .. }) => second,
        (Outcome::NotNumeric(_), Outcome::Done { .. }) => second,
        (Outcome::Done { .. }, Outcome::Done { .. }) if second.err() < first.err() => second,
        _ => first,
    })
}

/// The finite pieces by the automatic choice and the infinite ones by the oscillatory method (`osc`), summed;
/// `None` when an infinite piece is not oscillatory.
fn oscillatory<'h, S: Scalar>(
    cx: &mut NativeCx<'_, '_, 'h>,
    f: &mut dyn Integrand<'h, S>,
    pieces: &[Piece],
    flags: &[bool],
    goals: Goals,
    limits: gk::Limits,
    de_level: u32,
) -> Res<Option<Outcome<S>>> {
    use crate::machine::Map;
    let mut sum = S::real(0.0);
    let (mut err, mut conv) = (0.0, true);
    for (p, &fl) in pieces.iter().zip(flags) {
        // half-lines (a, direction, the factor of the half-line integral, which runs from a outward)
        let halves: Vec<(f64, f64, f64)> = match p.map {
            Map::Id => {
                match auto(cx, f, std::slice::from_ref(p), fl, goals, limits, de_level)? {
                    Outcome::Done { value, err: e, converged } => {
                        sum = sum + value;
                        err += e;
                        conv &= converged;
                    }
                    o => return Ok(Some(o)),
                }
                continue;
            }
            Map::Upper(a) => vec![(a, 1.0, if p.t1 >= p.t0 { 1.0 } else { -1.0 })],
            Map::Lower(a) => vec![(a, -1.0, if p.t1 >= p.t0 { 1.0 } else { -1.0 })],
            Map::Whole => {
                let s = if p.t1 >= p.t0 { 1.0 } else { -1.0 };
                vec![(0.0, 1.0, s), (0.0, -1.0, -s)]
            }
        };
        for (a, dir, sign) in halves {
            let Some(o) = crate::osc::half_line(cx, f, a, dir, goals)? else { return Ok(None) };
            match o {
                Outcome::Done { value, err: e, converged } => {
                    sum = sum + value * S::real(sign);
                    err += e;
                    conv &= converged;
                }
                o => return Ok(Some(o)),
            }
        }
    }
    Ok(Some(Outcome::Done { value: sum, err, converged: conv || err <= goals.tol(sum.abs()) }))
}

/// The integrand of level `k` with the outer variables at `prefix`: `f` itself at the innermost level, the inner
/// integral otherwise.
struct Level<'p, 'h, S> {
    p: &'p mut Problem<'h, S>,
    k: usize,
    prefix: Vec<f64>,
}

impl<'h, S: Scalar> Level<'_, 'h, S> {
    /// The pieces with a flag each: the integrand is rough at a finite end (not numeric there, or with a derivative
    /// that grows without bound toward it). An infinite piece rough at its finite end is split one unit (or |a|)
    /// from it, so the double-exponential rule takes the rough end and the infinite tail keeps its own choice.
    fn split_rough(&mut self, cx: &mut NativeCx<'_, '_, 'h>, pieces: &[Piece]) -> Res<(Vec<Piece>, Vec<bool>)> {
        use crate::machine::Map;
        let mut out = Vec::with_capacity(pieces.len() + 1);
        let mut flags = Vec::with_capacity(pieces.len() + 1);
        for p in pieces {
            match p.map {
                Map::Id => {
                    let (a, b) = (p.t0, p.t1);
                    let rough = self.rough_at(cx, a, b - a)? || self.rough_at(cx, b, a - b)?;
                    out.push(*p);
                    flags.push(rough);
                }
                Map::Upper(a) | Map::Lower(a) => {
                    let dir = if matches!(p.map, Map::Upper(_)) { 1.0 } else { -1.0 };
                    if self.rough_at(cx, a, dir)? {
                        let c = a + dir * a.abs().max(1.0);
                        // [a, c] in the direction of the piece, then the tail from c (t runs over [0, 1) either way)
                        let forward = p.t1 >= p.t0;
                        let fin = if forward { Piece { map: Map::Id, t0: a, t1: c } } else { Piece { map: Map::Id, t0: c, t1: a } };
                        let tail_map = if dir > 0.0 { Map::Upper(c) } else { Map::Lower(c) };
                        let tail = Piece { map: tail_map, t0: p.t0, t1: p.t1 };
                        out.extend([fin, tail]);
                        flags.extend([true, false]);
                    } else {
                        out.push(*p);
                        flags.push(false);
                    }
                }
                Map::Whole => {
                    // the two half-lines from 0, so no node falls on x = 0 (sin(x)/x there is 0/0) and each tail
                    // keeps its own choice
                    let forward = p.t1 >= p.t0;
                    let (lo, hi) = if forward { ((1.0, 0.0), (0.0, 1.0)) } else { ((0.0, 1.0), (1.0, 0.0)) };
                    let rough = self.rough_at(cx, 0.0, 1.0)? || self.rough_at(cx, 0.0, -1.0)?;
                    out.push(Piece { map: Map::Lower(0.0), t0: lo.0, t1: lo.1 });
                    out.push(Piece { map: Map::Upper(0.0), t0: hi.0, t1: hi.1 });
                    flags.extend([rough, rough]);
                }
            }
        }
        Ok((out, flags))
    }

    /// Is the integrand rough at the end `a` of a piece extending `len` (signed) from it: not numeric at a, or a
    /// difference quotient that keeps growing as the step shrinks from 2^-6 to 2^-40 of the piece (an unbounded
    /// derivative)? A smooth integrand's quotients settle to f'(a).
    fn rough_at(&mut self, cx: &mut NativeCx<'_, '_, 'h>, a: f64, len: f64) -> Res<bool> {
        let mut xs = vec![a];
        let hs: Vec<f64> = [6, 14, 22, 30, 40].iter().map(|&k| len * 2f64.powi(-k)).collect();
        for &h in &hs {
            xs.push(a + h);
            xs.push(a + 2.0 * h);
        }
        let mut y = vec![S::real(0.0); xs.len()];
        let mut e = vec![0.0; xs.len()];
        if self.eval(cx, &xs, &mut y, &mut e)?.is_err() {
            // some point is not numeric: at a itself?
            let (mut y0, mut e0) = ([S::real(0.0)], [0.0]);
            return Ok(self.eval(cx, &[a], &mut y0, &mut e0)?.is_err());
        }
        let scale = y.iter().fold(0.0f64, |m, v| m.max(v.abs())).max(f64::MIN_POSITIVE);
        let q: Vec<f64> = (0..hs.len()).map(|k| (y[2 + 2 * k] - y[1 + 2 * k]).abs() / hs[k].abs()).collect();
        // the roundoff level of a quotient at step h
        let noise = |k: usize| 64.0 * f64::EPSILON * scale / hs[k].abs();
        let growing = q.windows(2).enumerate().all(|(k, w)| w[1] > 1.2 * w[0] && w[1] > noise(k + 1));
        Ok(growing && q[q.len() - 1] > 3.0 * q[0])
    }
}

impl<'h, S: Scalar> Integrand<'h, S> for Level<'_, 'h, S> {
    fn eval(&mut self, cx: &mut NativeCx<'_, '_, 'h>, xs: &[f64], out: &mut [S], err: &mut [f64]) -> Res<Result<(), Vec<f64>>> {
        let point = |prefix: &[f64], x: f64| {
            let mut v = prefix.to_vec();
            v.push(x);
            v
        };
        if self.k + 1 == self.p.ranges.len() {
            // the innermost level: one batch call of f
            let n = self.prefix.len() + 1;
            let mut args = Vec::with_capacity(xs.len() * n);
            for &x in xs {
                args.extend(self.prefix.iter().map(|&v| S::real(v)));
                args.push(S::real(x));
            }
            self.p.calls += xs.len() as u64;
            if self.p.f.eval_flat(cx, &args, out)? {
                err.fill(0.0);
                return Ok(Ok(()));
            }
            // the batch failed: point by point. A point that is not numeric but has numeric neighbours on both
            // sides (sin(x)/x at the midpoint 0 of [-1, 1]; a removable singularity, or a single undefined sample of
            // an integrable one) takes the mean of its neighbours at a relative distance 1e-7: an isolated sample
            // does not change an integral
            for (i, &x) in xs.iter().enumerate() {
                let mut y = [S::real(0.0)];
                if self.p.f.eval(cx, &scalars::<S>(&point(&self.prefix, x)), &mut y)? {
                    out[i] = y[0];
                    continue;
                }
                let d = if x == 0.0 { 1e-7 } else { 1e-7 * x.abs() };
                let (mut ya, mut yb) = ([S::real(0.0)], [S::real(0.0)]);
                let a_ok = self.p.f.eval(cx, &scalars::<S>(&point(&self.prefix, x - d)), &mut ya)?;
                let b_ok = a_ok && self.p.f.eval(cx, &scalars::<S>(&point(&self.prefix, x + d)), &mut yb)?;
                if !b_ok {
                    return Ok(Err(point(&self.prefix, x)));
                }
                // only a removable singularity: the neighbours agree (a pole, or an integrable singularity such as
                // 1/sqrt(|x|), is split at instead, by the caller)
                let scale = ya[0].abs().max(yb[0].abs()).max(1.0);
                if (ya[0] - yb[0]).abs() > 1e-3 * scale {
                    return Ok(Err(point(&self.prefix, x)));
                }
                out[i] = (ya[0] + yb[0]) * S::real(0.5);
            }
            err.fill(0.0);
            return Ok(Ok(()));
        }
        // an outer level: an inner integral per point, ten times more accurate than this level's goals
        let g = self.p.plan.goals;
        let inner = Goals { abs: g.abs * 0.1, rel: g.rel * 0.1 };
        for (i, &x) in xs.iter().enumerate() {
            match self.p.level(cx, self.k + 1, &point(&self.prefix, x), inner)? {
                Outcome::Done { value, err: e, .. } => {
                    out[i] = value;
                    err[i] = e;
                }
                Outcome::NotNumeric(pt) => return Ok(Err(pt)),
            }
        }
        Ok(Ok(()))
    }
}

/// Machine reals as scalars of the integrand's type.
fn scalars<S: Scalar>(v: &[f64]) -> Vec<S> {
    v.iter().map(|&x| S::real(x)).collect()
}
