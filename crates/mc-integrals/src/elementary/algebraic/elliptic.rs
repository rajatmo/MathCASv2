//! Elliptic integrals in Legendre's form, when no elementary antiderivative is found.
//!
//! With `Y = sqrt(p_0) sqrt(1 - alpha x^2) sqrt(1 - beta x^2)` (an even quartic `p_4 x^4 + p_2 x^2 + p_0`, `alpha`
//! and `beta` the roots of `p_0 w^2 + p_2 w + p_4`, or a product of two even quadratics), `s = sqrt(alpha)`,
//! `m = beta/alpha` and `phi = asin(s x)`:
//!
//! - `int dx/Y = F(phi, m)/(s sqrt(p_0))`,
//! - `int x^2 dx/Y = (F(phi, m) - E(phi, m))/(beta s sqrt(p_0))` (from `1 - beta x^2 = sqrt(1 - beta x^2)^2`),
//! - `int dx/((x^2 - z_0) Y) = -Pi(1/(z_0 alpha), phi, m)/(z_0 s sqrt(p_0))`,
//!
//! so an even `R(x)/Y` with `R(x) = S(x^2)` is integrated term by term from the partial fractions of `S` (a
//! polynomial part of degree at most one, which Hermite reduction leaves, and simple poles); an odd part goes to
//! the substitution `u = x^2`. Both orders of `alpha`, `beta` are tried (the verification and the size decide).
//! A cubic radicand with a rational root `e` becomes an even quartic by `x = e + t^2`
//! (`P(e + t^2) = t^2 P_1(t^2)`), and a quartic with a rational root a cubic by `x = e + 1/u` (A. M. Legendre,
//! *Traité des fonctions elliptiques*, 1825; P. F. Byrd and M. D. Friedman, *Handbook of Elliptic Integrals for
//! Engineers and Scientists*, 2nd ed., 1971).
use super::{change, qterm, rat, sub, Cands, Level, Radical};
use crate::elementary::{factors, rewrite, subst};
use crate::{apply, args_of, is_free};
use mc_calculus::term::Build;
use mc_api::T;
use mc_arith::num::Rat;
use mc_eval::{NativeCx, Res};
use mc_integrate::algebraic::{parts, q, QPoly};
use mc_poly::ring::Rationals;
use mc_poly::unipoly as up;
use mc_poly::work::Meter;

/// Legendre forms for the integrand (see the module).
pub fn run<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, c: &mut Cands<'h>, rads: &[Radical<'h>], lv: Level) -> Res<()> {
    let (f, x) = (c.f, c.x);
    if rads.iter().any(|r| r.den != 2) {
        return Ok(());
    }
    let half = b.rat(1, 2);
    match rads {
        [r] => {
            let Some(cs) = mc_integrate::terms::coeffs_in(&b.ar, r.base, x) else { return Ok(()) };
            if cs.len() == 5 && b.is_zero(cs[1]) && b.is_zero(cs[3]) && !b.is_zero(cs[0]) {
                let (p0, p2, p4) = (cs[0], cs[2], cs[4]);
                let y = b.pow(r.base, half);
                let disc = b.sub(b.powi(p2, 2), b.mul(&[b.int(4), p0, p4]));
                let sd = b.pow(disc, half);
                let two_p0 = b.mul(&[b.int(2), p0]);
                let al = b.div(b.sub(sd, p2), two_p0);
                let be = b.div(b.neg(b.add(&[sd, p2])), two_p0);
                let k = b.pow(p0, b.rat(-1, 2));
                legendre(cx, b, c, b.mul(&[f, y]), y, &[(al, be), (be, al)], k, lv)?;
            } else if cs.len() == 4 || cs.len() == 5 {
                root_route(cx, b, c, r, lv)?;
            }
        }
        [r1, r2] => {
            let (Some(c1), Some(c2)) = (mc_integrate::terms::coeffs_in(&b.ar, r1.base, x), mc_integrate::terms::coeffs_in(&b.ar, r2.base, x)) else {
                return Ok(());
            };
            let even = |v: &[T<'h>]| v.len() == 3 && b.is_zero(v[1]) && !b.is_zero(v[0]);
            if !even(&c1) || !even(&c2) {
                return Ok(());
            }
            let al = b.neg(b.div(c1[2], c1[0]));
            let be = b.neg(b.div(c2[2], c2[0]));
            let k = b.mul(&[b.pow(c1[0], b.rat(-1, 2)), b.pow(c2[0], b.rat(-1, 2))]);
            let y = b.mul(&[b.pow(r1.base, half), b.pow(r2.base, half)]);
            legendre(cx, b, c, b.mul(&[f, y]), y, &[(al, be), (be, al)], k, lv)?;
        }
        _ => {}
    }
    Ok(())
}

/// `int R(x)/Y dx` for the orders `(alpha, beta)` of `pairs`, `Y = sqrt(1 - alpha x^2) sqrt(1 - beta x^2)/k`.
#[allow(clippy::too_many_arguments)]
fn legendre<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    b: &Build<'h>,
    c: &mut Cands<'h>,
    rr: T<'h>,
    y: T<'h>,
    pairs: &[(T<'h>, T<'h>)],
    k: T<'h>,
    lv: Level,
) -> Res<()> {
    let x = c.x;
    if !super::radicals(b, rr, x).is_empty() {
        return Ok(());
    }
    // even and odd parts
    let rm = subst(b, rr, x, b.neg(x));
    let odd = apply(cx, b, "together", &[b.div(b.sub(rr, rm), b.int(2))])?;
    let even = apply(cx, b, "together", &[b.div(b.add(&[rr, rm]), b.int(2))])?;
    let mut odd_part = b.zero();
    if !b.is_zero(odd) {
        match sub(cx, b, b.div(odd, y), x, lv)? {
            Some(r) => odd_part = r,
            None => return Ok(()),
        }
    }
    if b.is_zero(even) {
        if !b.is_zero(odd) {
            c.offer(cx, b, odd_part)?;
        }
        return Ok(());
    }
    // S(z) = even(x), z = x^2, in partial fractions
    let z = super::super::fresh(b, "z", lv.depth);
    let sz = change(b, even, x, &[], b.pow(z, b.rat(1, 2)));
    if !is_free(b, sz, x) || super::has_root_of(b, sz, z) {
        return Ok(());
    }
    let ap = apply(cx, b, "apart", &[sz, z])?;
    let terms: Vec<T<'h>> = args_of(b, ap, b.hd.add).map(|t| t.to_vec()).unwrap_or_else(|| vec![ap]);
    let (mut c0, mut c1) = (vec![], vec![]);
    let mut poles: Vec<(T<'h>, T<'h>)> = vec![];
    for t in terms {
        if is_free(b, t, z) {
            c0.push(t);
            continue;
        }
        let tz = b.div(t, z);
        if is_free(b, tz, z) {
            c1.push(tz);
            continue;
        }
        // coef (l0 + l1 z)^(-1)
        let (mut coef, mut lin) = (vec![], None);
        for g in factors(b, t) {
            if is_free(b, g, z) {
                coef.push(g);
                continue;
            }
            match args_of(b, g, b.hd.pow) {
                Some([base, e]) if *e == b.int(-1) && lin.is_none() => lin = Some(*base),
                _ => return Ok(()),
            }
        }
        let Some(lin) = lin else { return Ok(()) };
        let Some(lc) = mc_integrate::terms::coeffs_in(&b.ar, lin, z) else { return Ok(()) };
        let [l0, l1] = lc.as_slice() else { return Ok(()) };
        poles.push((b.div(b.mul(&coef), *l1), b.neg(b.div(*l0, *l1))));
    }
    let (c0, c1) = (b.add(&c0), b.add(&c1));
    for &(al, be) in pairs {
        if super::late() {
            break;
        }
        if b.is_zero(al) || (b.is_zero(be) && !b.is_zero(c1)) {
            continue;
        }
        let s = b.pow(al, b.rat(1, 2));
        let m = b.div(be, al);
        let phi = b.call("asin", &[b.mul(&[s, x])]);
        let ef = b.call("elliptic_f", &[phi, m]);
        // c0 F + c1 (F - E)/beta
        let mut parts = vec![];
        if b.is_zero(c1) {
            parts.push(b.mul(&[c0, ef]));
        } else {
            let ee = b.call("elliptic_e", &[phi, m]);
            let c1b = b.div(c1, be);
            let cf = apply(cx, b, "together", &[b.add(&[c0, c1b])])?;
            parts.push(b.mul(&[cf, ef]));
            parts.push(b.neg(b.mul(&[c1b, ee])));
        }
        for &(cf, z0) in &poles {
            let n = b.div(b.one(), b.mul(&[z0, al]));
            let pi = b.call("elliptic_pi", &[n, phi, m]);
            parts.push(b.neg(b.mul(&[cf, b.div(pi, z0)])));
        }
        let ans = b.add(&[b.mul(&[k, b.div(b.add(&parts), s)]), odd_part]);
        let ans = cx.eval(ans)?;
        c.offer(cx, b, ans)?;
    }
    Ok(())
}

/// A cubic (or quartic) radicand with a rational root `e`: `x = e + t^2` (or `x = e + 1/u`), and the portfolio on
/// the result.
fn root_route<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, c: &mut Cands<'h>, r: &Radical<'h>, lv: Level) -> Res<()> {
    let (f, x) = (c.f, c.x);
    let Some(p) = super::qpoly(b, r.base, x) else { return Ok(()) };
    let mut wk = mc_integrate::terms::Small(1 << 20);
    let mut m = Meter::new(&mut wk);
    if !mc_integrate::algebraic::square_free(&p, &mut m).unwrap_or(false) {
        return Ok(());
    }
    let Some(e) = rational_root(&p, &mut m) else { return Ok(()) };
    // S(z) = P(e + z), S(0) = 0
    let Ok(sp) = up::compose(&Rationals, &p, &up::trim(&Rationals, vec![e.clone(), Rat::from_i64(1)]), &mut m) else { return Ok(()) };
    let et = rat(b, &e);
    let xe = b.sub(x, et);
    let d = up::deg(&p).unwrap_or(0);
    if d == 3 {
        // x = e + sg t^2 (sg = 1, -1: the branch with x - e of the sign of sg at the points of the check);
        // P(e + sg t^2) = t^2 P1(t^2), P1(z) = S(sg z)/z
        for sg in [1i64, -1] {
            if super::late() || !c.found.is_empty() {
                break;
            }
            let t = super::super::fresh(b, if sg > 0 { "q" } else { "w" }, lv.depth);
            let p1: QPoly = sp[1..].iter().enumerate().map(|(k, v)| if sg < 0 && k % 2 == 0 { v.neg() } else { v.clone() }).collect();
            let p1t = qterm(b, &p1, b.powi(t, 2));
            let p1t = apply(cx, b, "expand", &[p1t])?;
            let y1 = b.mul(&[t, b.pow(p1t, b.rat(1, 2))]);
            let xt = b.add(&[et, b.mul(&[b.int(sg), b.powi(t, 2)])]);
            let g = b.mul(&[change(b, f, x, &[(r.term, b.powi(y1, r.num))], xt), b.int(2 * sg), t]);
            let sxe = b.mul(&[b.int(sg), xe]);
            if let Some(rt) = sub(cx, b, g, t, lv)? {
                let back = rewrite(b, rt, &mut |s| match args_of(b, s, b.hd.pow) {
                    Some([base, ex]) if *base == p1t => Some(b.mul(&[b.pow(r.base, *ex), b.pow(sxe, b.neg(*ex))])),
                    _ => None,
                });
                c.offer(cx, b, subst(b, back, t, b.pow(sxe, b.rat(1, 2))))?;
            }
        }
    } else if d == 4 {
        // u^4 P(e + 1/u) = C(u), a cubic: c_(4-k) = s_k
        let u = super::super::fresh(b, "v", lv.depth);
        let mut cu: QPoly = vec![Rat::from_i64(0); 5];
        for (kk, sk) in sp.iter().enumerate() {
            cu[4 - kk] = sk.clone();
        }
        let cu = up::trim(&Rationals, cu);
        let ct = qterm(b, &cu, u);
        let yu = b.mul(&[b.powi(u, -2), b.pow(ct, b.rat(1, 2))]);
        let xt = b.add(&[et, b.div(b.one(), u)]);
        let g = b.mul(&[change(b, f, x, &[(r.term, b.powi(yu, r.num))], xt), b.neg(b.powi(u, -2))]);
        if let Some(ru) = sub(cx, b, g, u, lv)? {
            let back = rewrite(b, ru, &mut |s| match args_of(b, s, b.hd.pow) {
                Some([base, ex]) if *base == ct => Some(b.mul(&[b.pow(r.base, *ex), b.pow(xe, b.mul(&[b.int(-4), *ex]))])),
                _ => None,
            });
            c.offer(cx, b, subst(b, back, u, b.div(b.one(), xe)))?;
        }
    }
    Ok(())
}

/// A rational root of `p` (rational root theorem on the integer multiple of `p`), the smallest in absolute value.
fn rational_root(p: &[Rat], m: &mut Meter) -> Option<Rat> {
    if p.first().is_some_and(|c| c.is_zero()) {
        return Some(Rat::from_i64(0));
    }
    let mut l = 1i64;
    for c in p {
        let (_, d) = parts(c)?;
        l = mc_integrate::algebraic::binomial::lcm(l, d);
    }
    let ints: Option<Vec<i64>> = p.iter().map(|c| parts(&c.mul(&Rat::from_i64(l))).map(|(n, _)| n)).collect();
    let ints = ints?;
    let (a0, an) = (ints[0].abs(), ints.last()?.abs());
    if a0 > 1_000_000 || an > 1_000_000 {
        return None;
    }
    let divs = |n: i64| (1..=n).filter(move |k| n % k == 0);
    let mut cands: Vec<Rat> = vec![];
    for pn in divs(a0) {
        for qd in divs(an) {
            cands.push(q(pn, qd));
            cands.push(q(-pn, qd));
        }
    }
    cands.sort_by(|a, b| a.mul(a).cmp_value(&b.mul(b)));
    cands.into_iter().find(|cnd| up::eval(&Rationals, p, cnd, m).is_ok_and(|v| v.is_zero()))
}
