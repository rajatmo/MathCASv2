//! Integration of rational functions of `x` (package 2C, item 2). OWNED BY THE RATIONAL-INTEGRATION WORK PACKAGE.
//!
//! Bronstein, *Symbolic Integration I*, chapter 2:
//!
//! 1. the integrand is read as `n/d` in lowest terms by `mc-poly`, as polynomials in `x` over Q, or over the
//!    rational functions of the parameters (the other kernels, free of `x`: module `params`);
//! 2. the polynomial part by division: `n = p d + a`, `∫ p` term by term;
//! 3. the rational part by Hermite reduction (module `hermite`): `∫ a/d = Σ b/v^j + ∫ a*/d*`, `d*` square-free;
//! 4. the logarithmic part by Rothstein–Trager (module `logpart`), with real `log`/`atan` output for residues of
//!    degree 2 (and even quartics, split over a real quadratic field).
//!
//! The arithmetic is exact and dense over a field (module `upoly`); every loop charges the evaluator's fuel, so a large
//! integrand can be interrupted. Whatever is not covered (inexact coefficients, residues of degree 3 or above 4)
//! gives `None`: the integral stays unevaluated.
mod hermite;
mod logpart;
mod params;
mod upoly;

use crate::term::Build;
use logpart::Base;
use mc_api::T;
use mc_arith::num::Rat;
use mc_arith::Num;
use mc_eval::{NativeCx, Res};
use mc_poly::convert::{Conv, Heads};
use mc_poly::work::{Halt, Meter, Work, WorkResult};
use mc_poly::{Poly, Z};
use upoly::{deg, divrem, monic, q, Fld, Stop, Ticker, P, Q, R};

/// The evaluator's fuel as a [`Ticker`]: one unit of fuel per [`BLOCK`] coefficient operations; also polls
/// `mc-arith`'s stop flag.
struct Tick<'c, 'a, 'k, 'h> {
    cx: &'c mut NativeCx<'a, 'k, 'h>,
    steps: u64,
}

/// Coefficient operations per unit of fuel.
const BLOCK: u64 = 1024;

impl Ticker for Tick<'_, '_, '_, '_> {
    fn step(&mut self, n: u64) -> R<()> {
        self.steps += n;
        if self.steps >= BLOCK {
            let units = self.steps / BLOCK;
            self.steps %= BLOCK;
            self.cx.tick(units).map_err(Stop::Abort)?;
            if mc_arith::num::stopped() {
                return Err(Stop::GiveUp);
            }
        }
        Ok(())
    }
}

/// `mc-poly`'s work budget over a [`Ticker`].
pub(super) struct Budget<'a> {
    tk: &'a mut dyn Ticker,
    err: Option<Stop>,
}

impl<'a> Budget<'a> {
    fn new(tk: &'a mut dyn Ticker) -> Budget<'a> {
        Budget { tk, err: None }
    }
    /// The stop behind a failed `mc-poly` kernel.
    fn stop(&mut self) -> Stop {
        self.err.take().unwrap_or(Stop::GiveUp)
    }
}

impl Work for Budget<'_> {
    fn charge(&mut self, units: u64) -> WorkResult<()> {
        self.tk.step(units.saturating_mul(BLOCK)).map_err(|e| {
            self.err = Some(e);
            Halt::Stopped
        })
    }
}

/// An antiderivative of `f` when it is a rational function of `x` (coefficients free of `x`), else `None`.
pub fn integrate<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, x: T<'h>, _depth: u32) -> Res<Option<T<'h>>> {
    let mut tk = Tick { cx, steps: 0 };
    match run(b, f, x, &mut tk) {
        Ok(r) => Ok(r),
        Err(Stop::Abort(a)) => Err(a),
        Err(Stop::GiveUp) => Ok(None),
    }
}

fn run<'h>(b: &Build<'h>, f: T<'h>, x: T<'h>, tk: &mut dyn Ticker) -> R<Option<T<'h>>> {
    let hd = Heads::new(b.heap());
    let conv = Conv::rational(&b.ar, &hd);
    let mut budget = Budget::new(tk);
    let read = {
        let mut m = Meter::new(&mut budget);
        conv.ring(&[f], &[x], &mut m).and_then(|(ring, _)| {
            // every kernel but x must be free of x, and exact (an inexact number is not a coefficient)
            let coefficient = |k| crate::integrate::is_free(b, k, x) && b.ar.num(k).is_none();
            let ok = ring.kernels.iter().all(|&k| k == x || coefficient(k));
            if !ok {
                return Ok(None);
            }
            Ok(mc_poly::ratfun::read(&conv, f, &ring, &mut m)?.ok().map(|rf| (ring, rf)))
        })
    };
    let (ring, rf) = match read {
        Ok(Some(r)) => r,
        Ok(None) => return Ok(None),
        Err(_) => return Err(budget.stop()),
    };
    let xi = ring.var(x).ok_or(Stop::GiveUp)?;
    if ring.nvars() == 1 {
        let dense = |p: &Poly| -> P<Rat> {
            upoly::norm(&Q, p.to_dense(0).iter().map(|z| Rat::from_int(z.to_int())).collect())
        };
        let et = |c: &Rat| rat_term(b, c);
        return solve(b, x, &Q, &et, dense(&rf.num), dense(&rf.den), tk).map(Some);
    }
    let k = params::Params::new(&conv, &ring, xi);
    let num = k.dense_in_x(&rf.num);
    let den = k.dense_in_x(&rf.den);
    let et = |c: &params::Elem| k.term(c);
    let r = solve(b, x, &k, &et, num, den, tk)?;
    if k.failed() {
        return Err(Stop::GiveUp);
    }
    Ok(Some(r))
}

/// `∫ num/den` over the field `k`, whose elements have the terms `et`.
fn solve<'h, K: Base>(
    b: &Build<'h>,
    x: T<'h>,
    k: &K,
    et: &dyn Fn(&K::E) -> T<'h>,
    num: P<K::E>,
    den: P<K::E>,
    tk: &mut dyn Ticker,
) -> R<T<'h>> {
    let lc = den.last().cloned().ok_or(Stop::GiveUp)?;
    let d = monic(k, &den)?;
    let n = upoly::scale(k, &k.inv(&lc).ok_or(Stop::GiveUp)?, &num);
    let (p, a) = divrem(k, &n, &d, tk)?;
    let mut out: Vec<T<'h>> = vec![];
    // the polynomial part
    for (i, c) in p.iter().enumerate() {
        if !k.is_zero(c) {
            let c1 = k.fdiv(c, &k.of(&q(i as i64 + 1)))?;
            out.push(b.mul(&[et(&c1), b.powi(x, i as i64 + 1)]));
        }
    }
    // the rational part
    let (fracs, a, d) = hermite::reduce(k, &a, &d, tk)?;
    for fr in &fracs {
        // with v = vi / mu, vi a primitive integer polynomial: b / v^j = b mu^j / vi^j
        let (num, den) = match rational_prim(k, &fr.v) {
            Some((mu, vi)) => {
                let bn = upoly::scale(k, &k.of(&mu.pow_u64(fr.j as u64)), &fr.b);
                (poly_term(b, k, &bn, x, et), poly_term(b, &Q, &vi, x, &|c| rat_term(b, c)))
            }
            None => (poly_term(b, k, &fr.b, x, et), poly_term(b, k, &fr.v, x, et)),
        };
        out.push(b.div(num, b.powi(den, fr.j as i64)));
    }
    // the logarithmic part
    if deg(&d).unwrap_or(0) > 0 {
        logpart::integrate(b, x, k, et, &a, &d, tk, &mut out)?;
    } else if !a.is_empty() {
        return Err(Stop::GiveUp);
    }
    Ok(b.add(&out))
}

impl Base for Q {
    fn factor(&self, p: &[Rat], tk: &mut dyn Ticker) -> R<Vec<P<Rat>>> {
        let zs: Vec<Z> = upoly::clear_denominators(p).into_iter().map(Z::from_int).collect();
        let poly = Poly::from_dense(1, 0, &zs);
        let mut budget = Budget::new(tk);
        let res = {
            let mut m = Meter::new(&mut budget);
            mc_poly::factor::factor(&poly, false, &mut m)
        };
        let fs = res.map_err(|_| budget.stop())?;
        let mut out = vec![];
        for (f, _) in &fs.factors {
            let dense: Vec<Rat> = f.to_dense(0).iter().map(|z| Rat::from_int(z.to_int())).collect();
            out.push(monic(&Q, &dense)?);
        }
        Ok(out)
    }
}

/// The canonical term of a rational.
fn rat_term<'h>(b: &Build<'h>, c: &Rat) -> T<'h> {
    b.ar.number(&Num::rat(c.clone()))
}

/// The term `Σ c_k x^k` with the coefficient terms given by `et`.
fn poly_term<'h, K: Fld>(b: &Build<'h>, k: &K, p: &[K::E], x: T<'h>, et: &dyn Fn(&K::E) -> T<'h>) -> T<'h> {
    let terms: Vec<T<'h>> = p
        .iter()
        .enumerate()
        .filter(|(_, c)| !k.is_zero(c))
        .map(|(i, c)| b.mul(&[et(c), b.powi(x, i as i64)]))
        .collect();
    b.add(&terms)
}

/// `(mu, mu p)` with `mu p` an integer polynomial with coprime coefficients and a positive leading coefficient,
/// when the coefficients of `p` are rational.
fn rational_prim<K: Fld>(k: &K, p: &[K::E]) -> Option<(Rat, P<Rat>)> {
    let pq: Vec<Rat> = p.iter().map(|c| k.as_rat(c)).collect::<Option<_>>()?;
    let ints = upoly::clear_denominators(&pq);
    let mut g = mc_arith::num::Nat::zero();
    for c in &ints {
        g = mc_arith::num::gcd::gcd(&g, &c.mag);
    }
    let neg = ints.last().is_some_and(|c| c.neg);
    let gi = mc_arith::num::Int::new(neg, g);
    if gi.is_zero() {
        return None;
    }
    let out: P<Rat> = ints.iter().map(|c| Rat::new(c.clone(), gi.clone())).collect();
    let mu = out.last()?.mul(&pq.last()?.recip()?);
    Some((mu, out))
}

/// The term of `p` with its content outside: `c * (integer polynomial)` when `p` is a multiple of a rational
/// polynomial, else `p` expanded.
fn scaled_term<'h, K: Fld>(b: &Build<'h>, k: &K, p: &[K::E], x: T<'h>, et: &dyn Fn(&K::E) -> T<'h>) -> T<'h> {
    let Some(lc) = p.last() else { return b.zero() };
    if let Ok(m) = monic(k, p) {
        if let Some((mu, pi)) = rational_prim(k, &m) {
            if let Some(c) = k.div(lc, &k.of(&mu)) {
                return b.mul(&[et(&c), poly_term(b, &Q, &pi, x, &|c| rat_term(b, c))]);
            }
        }
    }
    poly_term(b, k, p, x, et)
}

/// The term of `p` up to a constant factor (for a `log` argument): a primitive integer polynomial when it can be,
/// else `p` made monic.
fn log_arg<'h, K: Fld>(b: &Build<'h>, k: &K, p: &[K::E], x: T<'h>, et: &dyn Fn(&K::E) -> T<'h>) -> T<'h> {
    let m = monic(k, p).unwrap_or_else(|_| p.to_vec());
    match rational_prim(k, &m) {
        Some((_, pi)) => poly_term(b, &Q, &pi, x, &|c| rat_term(b, c)),
        None => poly_term(b, k, &m, x, et),
    }
}
