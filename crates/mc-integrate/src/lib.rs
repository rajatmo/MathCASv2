//! MathCAS v2 indefinite integration over a differential tower (roadmap item 14A).
//!
//! The integrand's differential field is built by [`tower`]; two methods run over it:
//!
//! - [`risch`]: the Risch decision procedure for towers of height one over `C(x)` (one exponential or one
//!   logarithm; trigonometric integrands through `exp(i u)`), with the Risch differential equation of [`rde`]. It
//!   either finds the antiderivative, or proves that none is elementary ([`Outcome::NonElementary`], with the
//!   reason), or declines.
//! - [`norman`]: the parallel Risch–Norman ansatz over any tower, the special functions of [`special`] included as
//!   candidates, so `erf`, `Ei`, `li`, `Si`, `Ci`, `Shi`, `Chi`, Fresnel, incomplete gamma and elliptic answers come
//!   out of the same linear algebra as elementary ones.
//! - [`algebraic`]: the algebra of integrands with roots (item 14B): Chebyshev's theorem on binomial
//!   differentials, Hermite reduction in a simple radical extension with a proof of non-elementarity for remainders
//!   of the second kind, and the invariants of Goursat's involutions; `mc-calculus` drives the substitutions.
//!
//! Both are exact; every method runs under its own [`budget::Budget`] (the evaluator's fuel, the interrupt, a
//! deadline). The driver (`mc-calculus`, `integrate`) checks every answer by differentiation before it is returned.
//!
//! Literature: M. Bronstein, *Symbolic Integration I: Transcendental Functions*, 2nd ed., Springer 2005 (chapters
//! 2, 5, 6); M. Bronstein, "Parallel integration", Programming and Computer Software 32 (2006), and the "pmint"
//! notes (2007); A. C. Norman and P. M. A. Moore, "Implementing the new Risch integration algorithm", 1977.
pub mod algebraic;
pub mod budget;
pub mod field;
pub mod linsolve;
pub mod norman;
pub mod rde;
pub mod risch;
pub mod special;
pub mod terms;
pub mod tower;
pub mod uni;
pub mod write;

use budget::{Budget, Stop};
use mc_api::{Store, T};
use mc_arith::Arith;
use mc_core::Heap;
use mc_eval::{NativeCx, Res};
use mc_poly::ratfun::RatFun;
use mc_poly::work::{Halt, Meter};
use risch::Decision;
use std::time::Duration;
use tower::{Kind, Mode, Tower};
use write::Writer;

/// The outcome of the decision procedure.
#[derive(Clone, Debug)]
pub enum Outcome<'h> {
    /// `int f = part + int rest`, `rest` a rational function of the integration variable (zero when there is none)
    Found {
        /// the antiderivative found
        part: T<'h>,
        /// what is left, for the rational integrator
        rest: T<'h>,
    },
    /// proven: no elementary antiderivative
    NonElementary(String),
    /// outside the procedure, or the time ran out
    Unknown,
}

/// The Risch decision procedure on `f` in `x`, within `time`.
pub fn risch<'h>(cx: &mut NativeCx<'_, '_, 'h>, ar: &Arith<'h, Heap>, f: T<'h>, x: T<'h>, time: Duration) -> Res<Outcome<'h>> {
    let mut bud = Budget::new(cx, time);
    let tw = {
        let mut m = Meter::new(&mut bud);
        tower::build(ar, f, x, Mode::Exp, &mut |_| {}, &mut m)
    };
    let tw = match tw {
        Ok(Some(t)) => t,
        Ok(None) => return Ok(Outcome::Unknown),
        Err(_) => return pass_on(&mut bud).map(|_| Outcome::Unknown),
    };
    match risch::decide(&tw, &mut bud) {
        Ok(Decision::Found { part, logs, rest }) => Ok(found(&tw, ar, &part, &logs, &rest).unwrap_or(Outcome::Unknown)),
        Ok(Decision::NonElementary(why)) => Ok(Outcome::NonElementary(why)),
        Ok(Decision::Unknown) => Ok(Outcome::Unknown),
        Err(Halt::Stopped | Halt::Memory) => pass_on(&mut bud).map(|_| Outcome::Unknown),
    }
}

/// An abort of the evaluator is passed on; a time-out or a give-up is not.
fn pass_on(bud: &mut Budget<'_, '_, '_, '_>) -> Res<()> {
    match bud.stop() {
        Stop::Abort(a) => Err(a),
        Stop::GiveUp => Ok(()),
    }
}

/// The terms of a decision's answer; `None` when an exponential of `i u` cannot be written in real form.
fn found<'h>(tw: &Tower<'h>, ar: &Arith<'h, Heap>, part: &RatFun, logs: &[(RatFun, RatFun)], rest: &RatFun) -> Option<Outcome<'h>> {
    let w = Writer { tw, ar };
    let h = ar.store();
    // an exponential of i u (a trigonometric integrand read through exp)
    let complex_exp = tw.vars.iter().any(|v| matches!(v.kind, Kind::Exp { .. }) && terms::has_complex(ar, v.term));
    if complex_exp && !logs.is_empty() {
        return None;
    }
    let p = if complex_exp { real_form(tw, ar, part)? } else { w.ratfun(part) };
    let mut terms = vec![p];
    for (c, g) in logs {
        terms.push(ar.mul(&[w.ratfun(c), h.call(h.sym("log"), &[w.ratfun(g)])]));
    }
    let rest = w.ratfun(rest);
    Some(Outcome::Found { part: ar.add(&terms), rest })
}

/// A Laurent polynomial in `t = exp(i u)` written with `cos(n u)` and `sin(n u)`: `c_n t^n + c_-n t^-n =
/// (c_n + c_-n) cos(n u) + i (c_n - c_-n) sin(n u)`.
fn real_form<'h>(tw: &Tower<'h>, ar: &Arith<'h, Heap>, p: &RatFun) -> Option<T<'h>> {
    let fld = &tw.fld;
    let h = ar.store();
    let w = Writer { tw, ar };
    let t = tw.vars.iter().position(|v| matches!(v.kind, Kind::Exp { .. }))?;
    let i = fld.imag?;
    let mut wk = terms::Small(1 << 16);
    let mut m = Meter::new(&mut wk);
    let j = p.den.degree(t).unwrap_or(0);
    if p.den.low_degree(t).unwrap_or(0) != j {
        return None;
    }
    let c: mc_poly::Poly = mc_poly::Poly::from_terms(
        fld.nv,
        p.den
            .terms()
            .map(|(e, k)| {
                let mut f = e.to_vec();
                f[t] = 0;
                (f, k.clone())
            })
            .collect(),
    );
    let mut coef: std::collections::BTreeMap<i64, RatFun> = Default::default();
    for (k, a) in p.num.coeffs_in(t) {
        coef.insert(k as i64 - j as i64, fld.quo(a, c.clone(), &mut m).ok()?);
    }
    // u: the generator of exp(i u) divided by i
    let gen = terms::fun(h, tw.vars[t].term).and_then(|(_, a)| a.first().copied())?;
    let im = ar.complex(ar.int(0), ar.int(1));
    let u = ar.div(gen, im);
    let zero = fld.int(0);
    let mut out = vec![];
    let ns: Vec<i64> = coef.keys().map(|k| k.abs()).collect::<std::collections::BTreeSet<_>>().into_iter().collect();
    for n in ns {
        let cp = coef.get(&n).cloned().unwrap_or_else(|| zero.clone());
        let cm = coef.get(&-n).cloned().unwrap_or_else(|| zero.clone());
        if n == 0 {
            out.push(w.ratfun(&cp));
            continue;
        }
        let re = fld.addm(&cp, &cm, &mut m).ok()?;
        let im_part = fld.mulm(&fld.var(i), &fld.subm(&cp, &cm, &mut m).ok()?, &mut m).ok()?;
        let nu = ar.mul(&[ar.int(n), u]);
        if !re.num.is_zero() {
            out.push(ar.mul(&[w.ratfun(&re), h.call(h.sym("cos"), &[nu])]));
        }
        if !im_part.num.is_zero() {
            out.push(ar.mul(&[w.ratfun(&im_part), h.call(h.sym("sin"), &[nu])]));
        }
    }
    Some(ar.add(&out))
}

/// The ansatz on `f` in `x` within `time`, with the special-function candidates when `special`.
pub fn norman<'h>(cx: &mut NativeCx<'_, '_, 'h>, ar: &Arith<'h, Heap>, f: T<'h>, x: T<'h>, time: Duration, special: bool) -> Res<Option<T<'h>>> {
    let mut bud = Budget::new(cx, time);
    let tw = {
        let mut m = Meter::new(&mut bud);
        tower::build(
            ar,
            f,
            x,
            Mode::Pair,
            &mut |b| {
                if special {
                    special::add(b, f)
                }
            },
            &mut m,
        )
    };
    let mut tw = match tw {
        Ok(Some(t)) => t,
        Ok(None) => return Ok(None),
        Err(_) => return pass_on(&mut bud).map(|_| None),
    };
    match norman::integrate(&mut tw, ar, &mut bud) {
        Ok(r) => Ok(r),
        Err(_) => pass_on(&mut bud).map(|_| None),
    }
}
