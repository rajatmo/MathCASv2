//! Hermite reduction for one radical `g^(j/q)`, `g` a square-free polynomial with rational coefficients
//! (`mc_integrate::algebraic::hermite`).
//!
//! The integrand `R(x) g^(j/q)` is written `P/(E g^w) g^s` with `-1 < s < 0` and `gcd(E, g) = 1`; the reduction
//! gives the algebraic part `A/E_red g^(s+1-w)` (in the integrand's own radical) and the remainder
//! `B/E_sf g^s`, which goes to the other methods (with this one off). For square roots of a radicand of degree
//! three or more with a polynomial `P/E`, a nonzero remainder is of the second kind or has a residue at infinity;
//! in the first case it proves that no elementary antiderivative exists.
use super::{qratfun, qterm, rat, sub_answer, Cands, Level, Radical};
use mc_calculus::term::Build;
use mc_arith::num::Rat;
use mc_eval::{NativeCx, Res};
use mc_integrate::algebraic::{hermite, q, QPoly};
use mc_poly::ring::Rationals;
use mc_poly::unipoly as up;
use mc_poly::work::Meter;

/// The algebraic part, and the remainder to the other methods.
pub fn run<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    b: &Build<'h>,
    c: &mut Cands<'h>,
    rads: &[Radical<'h>],
    lv: Level,
    proof: &mut Option<String>,
) -> Res<()> {
    let (f, x) = (c.f, c.x);
    let [r] = rads else { return Ok(()) };
    let Some(g) = super::qpoly(b, r.base, x) else { return Ok(()) };
    let d = up::deg(&g).unwrap_or(0);
    if d < 2 {
        return Ok(());
    }
    // f = R g^(num/den): R rational
    let rr = b.div(f, r.term);
    if !super::radicals(b, rr, x).is_empty() {
        return Ok(());
    }
    let Some((num, den)) = qratfun(cx, b, rr, x)? else { return Ok(()) };
    let Some(red) = reduce(&g, r.num, r.den, &num, &den) else { return Ok(()) };
    let (s, w, red) = red;
    let alg = b.mul(&[b.div(qterm(b, &red.a, x), qterm(b, &red.e_red, x)), b.pow(r.base, rat(b, &s.add(&Rat::from_i64(1 - w as i64))))]);
    if red.b.is_empty() {
        c.offer(cx, b, alg)?;
        return Ok(());
    }
    let mut wk = mc_integrate::terms::Small(1 << 20);
    let mut m = Meter::new(&mut wk);
    // poles only above infinity with a residue there: the logarithmic part from a unit (Abel's continued fraction)
    let mut log_part = b.zero();
    let mut rb = red.b.clone();
    if r.den == 2 && up::deg(&red.e_sf) == Some(0) {
        if let Some((lp, rest)) = abel_log(b, &g, &red.b, r.base, x, &mut m) {
            log_part = lp;
            rb = rest;
        }
    }
    if rb.is_empty() {
        c.offer(cx, b, b.add(&[alg, log_part]))?;
        return Ok(());
    }
    if proof.is_none() && r.den == 2 && up::deg(&red.e_sf) == Some(0) && hermite::second_kind(&g, &rb, &mut m).unwrap_or(false) {
        *proof = Some(format!(
            "Hermite reduction in the function field of y^2 = {}: the remainder is a nonzero differential of the second kind, \
             not exact, so no elementary antiderivative exists (Liouville)",
            mc_integrate::algebraic::text(&g)
        ));
    }
    let alg = b.add(&[alg, log_part]);
    let rem = b.mul(&[b.div(qterm(b, &rb, x), qterm(b, &red.e_sf, x)), b.pow(r.base, rat(b, &s))]);
    let ra = sub_answer(cx, b, rem, x, Level { depth: lv.depth + 1, no_hermite: true })?;
    if let Some(ri) = ra.found {
        c.offer(cx, b, b.add(&[alg, ri]))?;
    }
    // the algebraic part is elementary: a proof for the remainder is one for f
    if proof.is_none() {
        *proof = ra.proof;
    }
    Ok(())
}

/// The logarithmic part `mu log(A + B y)` of the remainder `rho dx / y` when `A + B y` is a unit
/// (`mc_integrate::algebraic::abel`) and `mu` matches the residues at infinity, with what is left of `rho` (zero
/// residues there).
fn abel_log<'h>(b: &Build<'h>, g: &[Rat], rho: &[Rat], base: mc_api::T<'h>, x: mc_api::T<'h>, m: &mut Meter) -> Option<(mc_api::T<'h>, QPoly)> {
    let r = &Rationals;
    let res = hermite::residue_at_infinity(g, rho, m).ok()??;
    if res.is_zero() {
        return None;
    }
    let (ua, ub) = mc_integrate::algebraic::abel::unit(g, m).ok()??;
    let (q, rest) = up::divrem(r, &up::derivative(r, &ua, m).ok()?, &ub, m).ok()??;
    if !rest.is_empty() {
        return None;
    }
    let rq = hermite::residue_at_infinity(g, &q, m).ok()??;
    let mu = res.mul(&rq.recip()?);
    let left = up::sub(r, rho, &up::scale(r, &q, &mu, m).ok()?);
    let y = b.pow(base, b.rat(1, 2));
    let v = b.add(&[qterm(b, &ua, x), b.mul(&[qterm(b, &ub, x), y])]);
    Some((b.mul(&[rat(b, &mu), b.call("log", &[v])]), left))
}

/// The reduction of `N/D g^(j/q)`: `(s, w, reduction)`.
fn reduce(g: &[Rat], j: i64, qd: i64, num: &[Rat], den: &[Rat]) -> Option<(Rat, usize, hermite::Reduction)> {
    let r = &Rationals;
    let mut wk = mc_integrate::terms::Small(1 << 22);
    let mut m = Meter::new(&mut wk);
    // j/q = s + k with -1 < s < 0, k = ceil(j/q)
    let k = (j + qd - 1).div_euclid(qd);
    let s = q(j - k * qd, qd);
    // N g^k / D = P/(E g^w)
    let (mut p, mut e): (QPoly, QPoly) = (num.to_vec(), den.to_vec());
    if k > 0 {
        p = up::mul(r, &p, &up::pow(r, g, k as u64, &mut m).ok()?, &mut m).ok()?;
    } else if k < 0 {
        e = up::mul(r, &e, &up::pow(r, g, (-k) as u64, &mut m).ok()?, &mut m).ok()?;
    }
    // cancel common powers of g, then w = the power of g left in E
    while let (Some(p2), Some(e2)) = (up::div_exact(r, &p, g, &mut m).ok()?, up::div_exact(r, &e, g, &mut m).ok()?) {
        p = p2;
        e = e2;
    }
    let mut w = 0;
    while let Some(e2) = up::div_exact(r, &e, g, &mut m).ok()? {
        e = e2;
        w += 1;
    }
    let red = hermite::reduce(g, &s, w, &p, &e, &mut m).ok()??;
    Some((s, w, red))
}
