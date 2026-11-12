//! Lowering the degree of the radicand: `u = x^k`, and Goursat's involutions `x -> c/x` for quartic radicals.
//!
//! - [`powers`]: when every radicand is a polynomial in `x^k` and `f dx = g(x^k) d(x^k)` (`f/(k x^(k-1))` a function
//!   of `x^k`), the substitution `u = x^k` divides the radicand's degree by `k` (`x/sqrt(1 + x^4)`,
//!   `x^2 sqrt(1 + x^3)`, `x sqrt(1 - x^4)`): the involution `x -> -x` and its higher analogues.
//! - [`goursat`]: a quartic `P` with `p_0 = c^2 p_4` is `x^2 Q(u)`, `u = x + c/x`, when it is invariant under
//!   `x -> c/x` (`mc_integrate::algebraic::goursat`); when `R(x) x^(j+2)/(x^2 - c)` is invariant too, the differential
//!   `R(x) P^(j/2) dx` is `r(u) Q(u)^(j/2) du` (for `x > 0`), a quadratic radical. These are Goursat's pseudo-elliptic
//!   integrals (`(1 + x^2)/((1 - x^2) sqrt(1 + x^4))`); answers are written back with `Q(u)^e = P^e x^(-2e)`.
use super::{change, qratfun, qterm, rat, sub, Cands, Level, Radical};
use crate::elementary::{rewrite, subst};
use crate::{args_of, is_free};
use mc_calculus::term::Build;
use mc_arith::num::Rat;
use mc_eval::{NativeCx, Res};
use mc_integrate::algebraic::{goursat as gs, parts, q, QPoly};
use mc_poly::ring::Rationals;
use mc_poly::unipoly as up;
use mc_poly::work::Meter;

/// `u = x^k` (see the module).
pub fn powers<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, c: &mut Cands<'h>, rads: &[Radical<'h>], lv: Level) -> Res<()> {
    let (f, x) = (c.f, c.x);
    let mut g = 0usize;
    for r in rads {
        let (n, d) = super::num_den(cx, b, r.base)?;
        for p in [n, d] {
            let Some(cs) = mc_integrate::terms::coeffs_in(&b.ar, p, x) else { return Ok(()) };
            for (e, cf) in cs.iter().enumerate().skip(1) {
                if !b.is_zero(*cf) {
                    g = gcd(g, e);
                }
            }
        }
    }
    if g < 2 {
        return Ok(());
    }
    let mut tried = 0;
    for k in (2..=g).rev().filter(|k| g.is_multiple_of(*k)) {
        if tried >= 2 || super::late() {
            break;
        }
        let t = super::super::fresh(b, "p", lv.depth);
        let kt = b.int(k as i64);
        let gx = b.div(f, b.mul(&[kt, b.powi(x, k as i64 - 1)]));
        let gt = change(b, gx, x, &[], b.pow(t, b.rat(1, k as i64)));
        if !is_free(b, gt, x) || super::has_root_of(b, gt, t) {
            continue;
        }
        tried += 1;
        if let Some(rt) = sub(cx, b, gt, t, lv)? {
            c.offer(cx, b, subst(b, rt, t, b.powi(x, k as i64)))?;
        }
    }
    Ok(())
}

fn gcd(a: usize, b: usize) -> usize {
    if b == 0 {
        a
    } else {
        gcd(b, a % b)
    }
}

/// Derivative-divides with a functional decomposition (F2's `decompose`): when every radicand is `G_j(h(x))` for one
/// inner polynomial `h` and `f = R(x) prod G_j(h)^(e_j)` with `R/h'` a rational function of `h`, the substitution
/// `u = h(x)` (`(2x + 1)/sqrt(x^4 + 2x^3 + x^2 + 1)` is `1/sqrt(u^2 + 1)` with `u = x^2 + x`).
pub fn decomposed<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, c: &mut Cands<'h>, rads: &[Radical<'h>], lv: Level) -> Res<()> {
    let (f, x) = (c.f, c.x);
    let Some(r0) = rads.first() else { return Ok(()) };
    let Some(p0) = super::qpoly(b, r0.base, x) else { return Ok(()) };
    if up::deg(&p0).unwrap_or(0) < 4 {
        return Ok(());
    }
    let mut wk = mc_integrate::terms::Small(1 << 20);
    let mut m = Meter::new(&mut wk);
    let Ok(Some(comps)) = mc_poly::decompose::decompose(&Rationals, &p0, None, &mut m) else { return Ok(()) };
    // R = f / prod of the radicals, rational
    let rr = b.div(f, b.mul(&rads.iter().map(|r| r.term).collect::<Vec<_>>()));
    if !super::radicals(b, rr, x).is_empty() {
        return Ok(());
    }
    for i in 1..comps.len() {
        if super::late() {
            break;
        }
        // the inner part h = comps[i] o ... o comps[last]
        let mut h: QPoly = comps[comps.len() - 1].clone();
        for cm in comps[i..comps.len() - 1].iter().rev() {
            let Ok(hh) = up::compose(&Rationals, cm, &h, &mut m) else { return Ok(()) };
            h = hh;
        }
        // a monomial x^k is the substitution of `powers`
        if h.iter().filter(|v| !v.is_zero()).count() == 1 {
            continue;
        }
        let u = super::super::fresh(b, "h", lv.depth);
        let mut reps = vec![];
        for r in rads {
            let Some(pj) = super::qpoly(b, r.base, x) else { return Ok(()) };
            let Ok(Some(gj)) = mc_integrate::algebraic::in_terms_of(&pj, &h, &mut m) else { return Ok(()) };
            reps.push((r.term, b.pow(qterm(b, &gj, u), b.rat(r.num, r.den))));
        }
        let ht = qterm(b, &h, x);
        let Some(dh) = crate::elementary::deriv(cx, b, ht, x)? else { continue };
        let Some((num, den)) = qratfun(cx, b, b.div(rr, dh), x)? else { continue };
        let (Ok(Some(nu)), Ok(Some(du))) = (mc_integrate::algebraic::in_terms_of(&num, &h, &mut m), mc_integrate::algebraic::in_terms_of(&den, &h, &mut m)) else {
            continue;
        };
        let mut parts = vec![b.div(qterm(b, &nu, u), qterm(b, &du, u))];
        parts.extend(reps.iter().map(|(_, t)| *t));
        if let Some(ru) = sub(cx, b, b.mul(&parts), u, lv)? {
            c.offer(cx, b, subst(b, ru, u, ht))?;
        }
    }
    Ok(())
}

/// Goursat's involutions `x -> c/x` (see the module).
pub fn goursat<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, c: &mut Cands<'h>, rads: &[Radical<'h>], lv: Level) -> Res<()> {
    let (f, x) = (c.f, c.x);
    let [r] = rads else { return Ok(()) };
    if r.den != 2 {
        return Ok(());
    }
    let Some(p) = super::qpoly(b, r.base, x) else { return Ok(()) };
    if up::deg(&p) != Some(4) || p[0].is_zero() {
        return Ok(());
    }
    let rr = b.div(f, r.term);
    if !super::radicals(b, rr, x).is_empty() {
        return Ok(());
    }
    let Some((num, den)) = qratfun(cx, b, rr, x)? else { return Ok(()) };
    let Some(inv4) = p[4].recip() else { return Ok(()) };
    let Some(root) = rat_sqrt(&p[0].mul(&inv4)) else { return Ok(()) };
    let mut wk = mc_integrate::terms::Small(1 << 20);
    let mut m = Meter::new(&mut wk);
    for cc in [root.clone(), root.neg()] {
        if super::late() {
            break;
        }
        let Ok(Some(qu)) = gs::laurent_in_u(&gs::laurent(&p, -2), &cc, &mut m) else { continue };
        // R(x) x^(j+2) / (x^2 - c) in u
        let x2c = up::trim(&Rationals, vec![cc.neg(), Rat::from_i64(0), Rat::from_i64(1)]);
        let Ok(den2) = up::mul(&Rationals, &den, &x2c, &mut m) else { continue };
        let Ok(Some((nu, su))) = gs::ratfun_in_u(&num, r.num + 2, &den2, &cc, &mut m) else { continue };
        let u = super::super::fresh(b, "g", lv.depth);
        let qt = qterm(b, &qu, u);
        let e = b.rat(r.num, 2);
        let gu = b.mul(&[b.div(qterm(b, &nu, u), qterm(b, &su, u)), b.pow(qt, e)]);
        if let Some(ru) = sub(cx, b, gu, u, lv)? {
            // Q(u)^e = P^e x^(-2e), then u = x + c/x
            let back = rewrite(b, ru, &mut |s| match args_of(b, s, b.hd.pow) {
                Some([base, e]) if *base == qt => Some(b.mul(&[b.pow(r.base, *e), b.pow(x, b.mul(&[b.int(-2), *e]))])),
                _ => None,
            });
            let ux = b.add(&[x, b.div(rat(b, &cc), x)]);
            c.offer(cx, b, subst(b, back, u, ux))?;
        }
    }
    Ok(())
}

/// The square root of a nonnegative rational when it is rational.
fn rat_sqrt(r: &Rat) -> Option<Rat> {
    let (n, d) = parts(r)?;
    if n < 0 {
        return None;
    }
    let (a, b) = (isqrt(n)?, isqrt(d)?);
    Some(q(a, b))
}

fn isqrt(n: i64) -> Option<i64> {
    let s = (n as f64).sqrt().round() as i64;
    (s >= 0 && s.checked_mul(s) == Some(n)).then_some(s)
}
