//! Binomial differentials `K x^m (a + b x^n)^p` and Chebyshev's theorem (`mc_integrate::algebraic::binomial`).
//!
//! With `u = x^n` the integral is `(K/n) int u^r (a + b u)^p du`, `r = (m + 1)/n - 1`. In the second case of the
//! theorem (`r` an integer) `t^s = a + b u`; in the third (`r + p` an integer) `t^s = b + a/u`, written back as
//! `t = (a + b x^n)^(1/s) x^(-n/s)` so that the answer has the integrand's radical. The first case (`p` an integer,
//! roots of `x` only) is the rationalising substitution's. When none applies the theorem is a proof, and the answer
//! is the hypergeometric form `x^(m+1)/(m+1) B^p (1 + b x^n/a)^(-p) 2F1(-p, k; k + 1; -b x^n/a)`, `k = (m + 1)/n`
//! (from the binomial series), verified like every other.
use super::{sub, Cands, Level, Radical};
use crate::{args_of, is_free};
use mc_calculus::term::Build;
use mc_api::T;
use mc_eval::{NativeCx, Res};
use mc_integrate::algebraic::binomial::{decide, Case, Q};

/// The parts of a binomial differential: `(K, m, a, b, n, p, B)` with `B = a + b x^n`.
struct Parts<'h> {
    k: T<'h>,
    m: Q,
    a: T<'h>,
    bc: T<'h>,
    n: Q,
    p: Q,
    base: T<'h>,
}

/// Candidates by Chebyshev's substitutions, or the proof that there is no elementary antiderivative.
pub fn run<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    b: &Build<'h>,
    c: &mut Cands<'h>,
    _rads: &[Radical<'h>],
    lv: Level,
    proof: &mut Option<String>,
) -> Res<()> {
    let x = c.x;
    let Some(bp) = parts(b, c.f, x) else { return Ok(()) };
    let Some(case) = decide(bp.m, bp.n, bp.p) else { return Ok(()) };
    let n = b.rat(bp.n.0, bp.n.1);
    let k_over_n = b.div(bp.k, n);
    // r = (m + 1)/n - 1 as a term
    let r = b.sub(b.div(b.add(&[b.rat(bp.m.0, bp.m.1), b.one()]), n), b.one());
    let t = super::super::fresh(b, "c", lv.depth);
    match case {
        Case::Integer(_) => {}
        Case::NonElementary(why) => {
            *proof = Some(why);
            // the hypergeometric form: K x^(m+1)/(m+1) B^p (1 + b x^n/a)^(-p) 2F1(-p, k; k + 1; -b x^n/a), k = (m+1)/n
            if bp.m != Q(-1, 1) {
                let m1 = b.rat(bp.m.0 + bp.m.1, bp.m.1);
                let kk = b.div(m1, n);
                let p = b.rat(bp.p.0, bp.p.1);
                let w = b.div(b.mul(&[bp.bc, b.pow(x, n)]), bp.a);
                let f21 = b.call("hypergeometric_2f1", &[b.neg(p), kk, b.add(&[kk, b.one()]), b.neg(w)]);
                let r = b.mul(&[bp.k, b.div(b.pow(x, m1), m1), b.pow(bp.base, p), b.pow(b.add(&[b.one(), w]), b.neg(p)), f21]);
                c.offer(cx, b, r)?;
            }
        }
        Case::Second(s) => {
            // u = (t^s - a)/b, du = s t^(s-1)/b dt, (a + b u)^p = t^(p s)
            let ts = b.powi(t, s);
            let u = b.div(b.sub(ts, bp.a), bp.bc);
            let du = b.div(b.mul(&[b.int(s), b.powi(t, s - 1)]), bp.bc);
            let g = b.mul(&[k_over_n, b.pow(u, r), b.powi(t, bp.p.0 * s / bp.p.1), du]);
            if let Some(rt) = sub(cx, b, g, t, lv)? {
                let back = b.pow(bp.base, b.rat(1, s));
                c.offer(cx, b, super::super::subst(b, rt, t, back))?;
            }
        }
        Case::Third(s) => {
            // u = a/(t^s - b), du = -a s t^(s-1)/(t^s - b)^2 dt, (a + b u)^p = u^p t^(p s)
            let ts = b.powi(t, s);
            let den = b.sub(ts, bp.bc);
            let u = b.div(bp.a, den);
            let du = b.neg(b.div(b.mul(&[bp.a, b.int(s), b.powi(t, s - 1)]), b.powi(den, 2)));
            let rp = b.add(&[r, b.rat(bp.p.0, bp.p.1)]);
            let g = b.mul(&[k_over_n, b.pow(u, rp), b.powi(t, bp.p.0 * s / bp.p.1), du]);
            let g = crate::apply(cx, b, "together", &[g])?;
            if let Some(rt) = sub(cx, b, g, t, lv)? {
                let back = b.mul(&[b.pow(bp.base, b.rat(1, s)), b.pow(x, b.neg(b.div(n, b.int(s))))]);
                let w = if bp.n.0 > 0 { super::SINGULAR } else { 100 };
                c.offer_weighted(cx, b, super::super::subst(b, rt, t, back), w)?;
            }
        }
    }
    Ok(())
}

/// `f` as `K x^m (a + b x^n)^p` (a, b nonzero and free of x, p not an integer).
fn parts<'h>(b: &Build<'h>, f: T<'h>, x: T<'h>) -> Option<Parts<'h>> {
    let mut k = vec![];
    let mut m = Q(0, 1);
    let mut bin = None;
    for g in super::super::factors(b, f) {
        if is_free(b, g, x) {
            k.push(g);
            continue;
        }
        if let Some(e) = x_power(b, g, x) {
            m = Q::new(m.0 * e.1 + e.0 * m.1, m.1 * e.1);
            continue;
        }
        let Some([base, e]) = args_of(b, g, b.hd.pow) else { return None };
        let (pn, pd) = super::rat_exp(b, *e)?;
        if pd == 1 || bin.is_some() {
            return None;
        }
        bin = Some((*base, Q::new(pn, pd)));
    }
    let (base, p) = bin?;
    let terms = args_of(b, base, b.hd.add)?;
    let [t1, t2] = terms else { return None };
    let (a, other) = if is_free(b, *t1, x) { (*t1, *t2) } else if is_free(b, *t2, x) { (*t2, *t1) } else { return None };
    // other = coefficient x^n
    let mut bc = vec![];
    let mut n = None;
    for g in super::super::factors(b, other) {
        if is_free(b, g, x) {
            bc.push(g);
        } else if n.is_none() {
            n = Some(x_power(b, g, x)?);
        } else {
            return None;
        }
    }
    let n = n?;
    Some(Parts { k: b.mul(&k), m, a, bc: b.mul(&bc), n, p, base })
}

/// `e` when `g` is `x^e` (`x` itself: 1).
fn x_power<'h>(b: &Build<'h>, g: T<'h>, x: T<'h>) -> Option<Q> {
    if g == x {
        return Some(Q(1, 1));
    }
    let [base, e] = args_of(b, g, b.hd.pow)? else { return None };
    if *base != x {
        return None;
    }
    let (n, d) = super::rat_exp(b, *e)?;
    Some(Q::new(n, d))
}
