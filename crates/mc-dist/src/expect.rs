//! `expectation(f, d)` and `expectation(e, x, d)`: E[f(X)] for X distributed as d (decision DI.8).
//!
//! In order:
//! 1. f free of x: f itself;
//! 2. f a polynomial in x (degree at most 200): the sum of its coefficients times the raw moments, exact;
//! 3. a binomial distribution with an exact number of trials up to 10^4: the finite sum over the support;
//! 4. numeric parameters: `n_integrate` of f times the density over the support (continuous), or the series
//!    summed in machine arithmetic until the remaining probability is negligible (Poisson, geometric);
//! 5. exact parameters, continuous: the symbolic `integrate` of f times the density, when it finds one;
//!
//! otherwise the call stays.
use crate::build::B;
use crate::family::{Dist, Family};
use crate::machine::Mach;
use crate::symbolic::{self, MAX_TERMS};
use mc_api::{Store, View, T};
use mc_arith::num::Rat;
use mc_arith::{Arith, Num, Real};
use mc_eval::{NativeCx, Reply, Res};

/// The variable `expectation(f, d)` applies f to (any name works: it is bound inside the native only).
const VAR: &str = "expectation$x";

/// The largest exact number of binomial trials summed term by term.
const MAX_BINOMIAL_SUM: i64 = 10_000;

/// The most terms of a numeric series over a discrete support.
const MAX_SERIES: usize = 1_000_000;

fn is_true<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>) -> Res<bool> {
    let v = cx.eval(t)?;
    Ok(cx.is_true(v))
}

/// `expectation(f, d)`, `expectation(e, x, d)`.
pub fn expectation<'h>(cx: &mut NativeCx<'_, '_, 'h>, name: &str, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let b = B::new(h);
    let (e, x, dt) = match *args {
        [f, d] => {
            let x = b.sym(VAR);
            (cx.eval(h.app(f, &[x]))?, x, d)
        }
        [e, x, d] => {
            if !matches!(h.view(x), View::Sym(_)) {
                return Ok(Reply::Done(b.bad(name, "the variable must be a symbol")));
            }
            (e, x, d)
        }
        _ => return Ok(Reply::Decline),
    };
    let Some(d) = Dist::read(h, dt) else { return Ok(Reply::Decline) };
    // 1. a constant
    if is_true(cx, b.call("free_of", &[e, x]))? {
        return Ok(Reply::Again(e));
    }
    // 2. a polynomial: coefficients times moments
    let cl = cx.eval(b.call("coefficient_list", &[e, x]))?;
    if let Some(cs) = cx.as_list(cl) {
        if !cs.is_empty() && cs.len() as i64 <= MAX_TERMS + 1 {
            let mut poly = true;
            for &c in cs {
                if !is_true(cx, b.call("free_of", &[c, x]))? {
                    poly = false;
                    break;
                }
            }
            if poly {
                let terms: Vec<T<'h>> = cs
                    .iter()
                    .enumerate()
                    .map(|(k, &c)| b.mul(&[c, b.call("moment", &[dt, b.int(k as i64)])]))
                    .collect();
                return Ok(Reply::Again(b.add(&terms)));
            }
        }
    }
    let ar = Arith::new(h);
    let at = |k: T<'h>| b.call("rewrite", &[e, b.call(mc_api::names::RULE, &[x, k])]);
    // 3. a finite binomial sum
    if d.fam == Family::Binomial {
        if let Some(Num::Real(Real::Exact(n))) = ar.num(d.p[0]) {
            if let Some(n) = n.num.to_i64().filter(|&n| n <= MAX_BINOMIAL_SUM) {
                cx.tick(n as u64 + 1)?;
                let ex = [Some(Rat::from_i64(n)), None];
                let terms: Vec<T<'h>> = (0..=n)
                    .map(|k| b.mul(&[symbolic::pdf(b, &d, &ex, b.int(k), Some(&Rat::from_i64(k))), at(b.int(k))]))
                    .collect();
                return Ok(Reply::Again(b.add(&terms)));
            }
        }
    }
    let machine = d.p.iter().any(|&p| matches!(ar.num(p), Some(Num::Real(Real::Machine(_)))));
    let all_numbers = d.p.iter().all(|&p| matches!(ar.num(p), Some(Num::Real(_))));
    // 4. numeric parameters with a machine real: numeric integration or summation
    if machine && all_numbers {
        if let Some((body, lo, hi)) = symbolic::density(b, &d, x) {
            let range = b.list(&[x, lo, hi]);
            return Ok(Reply::Again(b.call("n_integrate", &[b.mul(&[e, body]), range])));
        }
        let ps: Option<Vec<f64>> = d
            .p
            .iter()
            .map(|&p| match ar.num(p) {
                Some(Num::Real(r)) => Some(r.to_f64()),
                _ => None,
            })
            .collect();
        let Some(m) = ps.and_then(|p| Mach::new(d.fam, &p)) else { return Ok(Reply::Decline) };
        return series(cx, &m, &at);
    }
    // 5. a symbolic integral over the support
    if !d.fam.is_discrete() {
        if let Some((body, lo, hi)) = symbolic::density(b, &d, x) {
            // the parameters are in their domains: no conditions of convergence (item 14D)
            let no_conditions = b.call("options", &[b.call("rule", &[b.sym("generate_conditions"), b.sym("false")])]);
            let r = cx.eval(b.call("integrate", &[b.mul(&[e, body]), b.list(&[x, lo, hi]), no_conditions]))?;
            if is_true(cx, b.call("free_of", &[r, b.sym("integrate")]))? {
                return Ok(Reply::Done(r));
            }
        }
    }
    Ok(Reply::Decline)
}

/// sum_k P(k) f(k) over a discrete support in machine arithmetic, until the probability left beyond k is below
/// 1e-17 of the sum and the terms decrease.
fn series<'h>(cx: &mut NativeCx<'_, '_, 'h>, m: &Mach, at: &dyn Fn(T<'h>) -> T<'h>) -> Res<Reply<'h>> {
    let h = cx.heap();
    let ar = Arith::new(h);
    let mut acc = 0.0f64;
    let top = match m.fam {
        Family::Binomial => m.p[0] as usize,
        _ => MAX_SERIES,
    };
    for k in 0..=top {
        cx.tick(1)?;
        let w = m.pdf(k as f64);
        let left = m.cdf(k as f64).1;
        if w > 0.0 {
            let v = cx.eval(h.call(h.sym("n"), &[at(h.int(k as i64))]))?;
            let Some(Num::Real(r)) = ar.num(v) else { return Ok(Reply::Decline) };
            let term = w * r.to_f64();
            acc += term;
            if left < 1e-17 && term.abs() <= 1e-17 * acc.abs().max(f64::MIN_POSITIVE) {
                break;
            }
        } else if left == 0.0 {
            break;
        }
    }
    Ok(Reply::Done(h.real(acc)))
}
