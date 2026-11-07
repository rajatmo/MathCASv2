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

    /// The integral over range `k` and the inner ones, the outer variables at `prefix`.
    fn level(&mut self, cx: &mut NativeCx<'_, '_, 'h>, k: usize, prefix: &[f64], goals: Goals) -> Res<Outcome<S>> {
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
        let pieces: Vec<Piece> = ends.windows(2).filter_map(|w| piece(w[0], w[1])).collect();
        let plan = self.plan;
        let mut f = Level { p: self, k, prefix: prefix.to_vec() };
        let de_level = de::MAX_LEVEL;
        match plan.method {
            Some(Method::GaussKronrod) => gk::integrate(cx, &mut f, &pieces, goals, plan.limits),
            Some(Method::DoubleExponential) => de::integrate(cx, &mut f, &pieces, goals, de_level),
            Some(_) | None => {
                // the automatic choice (decision N2.7): the double-exponential rule first when the integrand is not
                // numeric at a finite end (an endpoint singularity), Gauss–Kronrod first otherwise; the other
                // method when the first does not converge, keeping the better estimate
                let singular_end = f.singular_end(cx, &pieces)?;
                let first = if singular_end {
                    de::integrate(cx, &mut f, &pieces, goals, de_level)?
                } else {
                    gk::integrate(cx, &mut f, &pieces, goals, plan.limits)?
                };
                if first.converged() {
                    return Ok(first);
                }
                let second = if singular_end {
                    gk::integrate(cx, &mut f, &pieces, goals, plan.limits)?
                } else {
                    de::integrate(cx, &mut f, &pieces, goals, de_level)?
                };
                Ok(match (&first, &second) {
                    (_, Outcome::Done { converged: true, .. }) => second,
                    (Outcome::NotNumeric(_), Outcome::Done { .. }) => second,
                    (Outcome::Done { .. }, Outcome::Done { .. }) if second.err() < first.err() => second,
                    _ => first,
                })
            }
        }
    }
}

/// The integrand of level `k` with the outer variables at `prefix`: `f` itself at the innermost level, the inner
/// integral otherwise.
struct Level<'p, 'h, S> {
    p: &'p mut Problem<'h, S>,
    k: usize,
    prefix: Vec<f64>,
}

impl<'h, S: Scalar> Level<'_, 'h, S> {
    /// Is the integrand not numeric at a finite end of a piece?
    fn singular_end(&mut self, cx: &mut NativeCx<'_, '_, 'h>, pieces: &[Piece]) -> Res<bool> {
        let mut xs = Vec::new();
        for p in pieces {
            if p.map == crate::machine::Map::Id {
                xs.extend([p.t0, p.t1]);
            } else if let Some((x, _)) = p.map.apply(0.0) {
                xs.push(x);
            }
        }
        for x in xs {
            let (mut y, mut e) = ([S::real(0.0)], [0.0]);
            if self.eval(cx, &[x], &mut y, &mut e)?.is_err() {
                return Ok(true);
            }
        }
        Ok(false)
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
            // find the first point that is not numeric
            for &x in xs {
                let mut y = [S::real(0.0)];
                if !self.p.f.eval(cx, &scalars::<S>(&point(&self.prefix, x)), &mut y)? {
                    return Ok(Err(point(&self.prefix, x)));
                }
            }
            // a batch that failed although every point passes alone (a declined batch): use the single values
            for (i, &x) in xs.iter().enumerate() {
                let mut y = [S::real(0.0)];
                self.p.f.eval(cx, &scalars::<S>(&point(&self.prefix, x)), &mut y)?;
                out[i] = y[0];
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
