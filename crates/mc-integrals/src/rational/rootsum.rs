//! The logarithmic part over residues of degree 3 or more: a `root_sum` term over `mc-algebra`'s number fields
//! (roadmap F1; decision F1.4 of `mc-algebra`).
//!
//! For an irreducible factor `r` of the Rothstein–Trager resultant with root `α`, `v = gcd(d, a - α d')` over
//! `K = Q(α)` (modular gcd). Two forms, the smaller always:
//!
//! - `v` linear, `v = x - β(α)`: the roots `β` of `d` whose residues are the roots of `r` are the roots of
//!   `q = minpoly(β)`, and the part is `root_sum(t => q(t), t => (a(t)/d'(t)) log(x - t))`: the classical residue
//!   formula, with the residue written as a reduced quotient `A(t)/B(t)` (`A = a mod q`, `B = d' mod q`) or as the
//!   polynomial `c(t) = A B^-1 mod q`, whichever has fewer coefficient bits;
//! - otherwise (several roots of `d` share a residue): `root_sum(t => r(t), t => t log(v(t, x)))` with `v` monic in
//!   `x` and its coefficients polynomials in `t`.
//!
//! In the first case, when `q` is a binomial `x^n - c` with `n` in 3, 4, 6 and a real root, the part is done over
//! the real radical field instead (module `radical`): real `log` and `atan` terms in `c^(1/n)`.
use super::upoly::{Stop, Ticker, R};
use super::Budget;
use mc_calculus::term::Build;
use mc_algebra::field::NumberField;
use mc_algebra::{kpoly, qpoly};
use mc_api::{Store, T};
use mc_arith::num::Rat;
use mc_arith::Num;
use mc_poly::work::{Halt, Meter};

/// The bound variable of the functions in the result: `t`, or `u` when the integration variable is `t`.
fn bound<'h>(b: &Build<'h>, x: T<'h>) -> T<'h> {
    let h = b.heap();
    let t = h.sym_term(h.sym("t"));
    if t == x {
        h.sym_term(h.sym("u"))
    } else {
        t
    }
}

/// `x => body`.
fn function<'h>(b: &Build<'h>, v: T<'h>, body: T<'h>) -> T<'h> {
    let h = b.heap();
    h.call(h.sym("fn"), &[b.app(b.sym(b.hd.list), &[v]), body])
}

/// `sum c_i v^i`.
fn poly_term<'h>(b: &Build<'h>, p: &[Rat], v: T<'h>) -> T<'h> {
    let parts: Vec<T<'h>> = p
        .iter()
        .enumerate()
        .filter(|(_, c)| !c.is_zero())
        .map(|(i, c)| b.mul(&[b.ar.number(&Num::rat(c.clone())), b.powi(v, i as i64)]))
        .collect();
    b.add(&parts)
}

/// The primitive integer form of `p` as a term in `v`.
fn primitive_term<'h>(b: &Build<'h>, p: &[Rat], v: T<'h>) -> T<'h> {
    poly_term(b, &qpoly::from_ints(&qpoly::primitive(p)), v)
}

fn stop_of(h: Halt, budget: &mut Budget) -> Stop {
    match h {
        Halt::Stopped => budget.stop(),
        Halt::Memory => Stop::GiveUp,
    }
}

/// The `root_sum` term of the residues that are the roots of the monic irreducible `r` (degree 3 or more),
/// appended to `out`; the number of roots of `d` it accounts for.
#[allow(clippy::too_many_arguments)]
pub fn residues<'h>(
    b: &Build<'h>,
    x: T<'h>,
    r: &[Rat],
    a: &[Rat],
    d: &[Rat],
    dp: &[Rat],
    tk: &mut dyn Ticker,
    out: &mut Vec<T<'h>>,
) -> R<usize> {
    let mut budget = Budget::new(tk);
    let res = {
        let mut m = Meter::new(&mut budget);
        build(b, x, r, a, d, dp, &mut m)
    };
    let res = match res {
        Ok(r) => r,
        Err(h) => return Err(stop_of(h, &mut budget)),
    };
    let Some((t, n, q)) = res else { return Err(Stop::GiveUp) };
    // a binomial factor of the denominator: real log/atan terms over its real radical field instead
    if let Some(q) = q {
        if super::radical::integrate(b, x, &q, a, d, tk, out)? {
            return Ok(n);
        }
    }
    out.push(t);
    Ok(n)
}

/// A built part: the term, the number of roots of the denominator it covers, the denominator factor (residue form).
type Part<'h> = (T<'h>, usize, Option<Vec<Rat>>);

fn build<'h>(
    b: &Build<'h>,
    x: T<'h>,
    r: &[Rat],
    a: &[Rat],
    d: &[Rat],
    dp: &[Rat],
    m: &mut Meter,
) -> Result<Option<Part<'h>>, Halt> {
    let k = NumberField::new(r.to_vec(), None);
    let alpha = k.gen();
    let (ak, dk, dpk) = (kpoly::lift(a), kpoly::lift(d), kpoly::lift(dp));
    let g = kpoly::sub(&k, &ak, &kpoly::scale(&k, &dpk, &alpha));
    let v = kpoly::gcd(&k, &dk, &g, m)?;
    let Some(nv) = kpoly::deg(&v) else { return Ok(None) };
    if nv == 0 {
        return Ok(None);
    }
    let t = bound(b, x);
    let n = k.degree();
    if nv == 1 {
        // x - beta
        let beta = k.neg(&v[0]);
        let q = k.minpoly(&beta, m)?;
        if q.len() - 1 == n {
            let (aq, bq) = (qpoly::rem(a, &q, m)?, qpoly::rem(dp, &q, m)?);
            let gq = qpoly::gcd(&aq, &bq, m)?;
            let aq = qpoly::divrem(&aq, &gq, m)?.0;
            let bq = qpoly::divrem(&bq, &gq, m)?.0;
            let kq = NumberField::new(q.clone(), None);
            let Some(c) = kq.div(&kq.reduce(&aq), &kq.reduce(&bq)) else { return Ok(None) };
            let log = b.call("log", &[b.sub(x, t)]);
            let coef = if qpoly::bit_size(&c) <= qpoly::bit_size(&aq) + qpoly::bit_size(&bq) {
                poly_term(b, &c, t)
            } else {
                // A / B with B's content moved to A: B primitive over the integers
                let bi = qpoly::from_ints(&qpoly::primitive(&bq));
                let s = qpoly::rdiv(&bi[bi.len() - 1], &bq[bq.len() - 1]);
                b.div(poly_term(b, &qpoly::scale(&aq, &s), t), poly_term(b, &bi, t))
            };
            let g = function(b, t, b.mul(&[coef, log]));
            let f = function(b, t, primitive_term(b, &q, t));
            return Ok(Some((b.call("root_sum", &[f, g]), n, Some(q))));
        }
    }
    // t log(v(t, x)), v monic in x
    let terms: Vec<T<'h>> = v
        .iter()
        .enumerate()
        .filter(|(_, c)| !c.is_empty())
        .map(|(i, c)| b.mul(&[poly_term(b, c, t), b.powi(x, i as i64)]))
        .collect();
    let g = function(b, t, b.mul(&[t, b.call("log", &[b.add(&terms)])]));
    let f = function(b, t, primitive_term(b, r, t));
    Ok(Some((b.call("root_sum", &[f, g]), nv * n, None)))
}
