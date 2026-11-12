//! The logarithmic part of a hyperelliptic integral with poles only at infinity: Abel's continued fractions.
//!
//! For `y^2 = P`, `P` square-free of even degree `2g + 2` with a square leading coefficient, the two places above
//! infinity are rational. After Hermite reduction a remainder `rho(x) dx / y` with poles only there is
//! elementary exactly when it is a constant multiple of `d log(A + B y)` for a unit `A + B y` of `Q[x, y]`
//! (`A^2 - B^2 P` a nonzero constant), plus nothing: the rest would be of the second kind and not exact
//! ([`super::hermite`]). Such a unit exists exactly when the divisor `inf_+ - inf_-` has finite order, and then (Abel
//! 1826; Chebyshev 1860) the continued fraction of `sqrt(P)` in `Q((1/x))` is periodic and a convergent `A/B` gives
//! it; `d log(A + B y) = (A'/B) dx / y`. [`unit`] runs the expansion within a bound on the steps and the degree, so
//! a unit of large order is not found (the integral is then undecided, never misjudged). This is the torsion test
//! of Davenport (*On the Integration of Algebraic Functions*, 1981, chapter 7) for this case, with the classical
//! recurrence `a_k = [(P_k + a_0)/Q_k]`, `P_(k+1) = a_k Q_k - P_k`, `Q_(k+1) = (P - P_(k+1)^2)/Q_k`,
//! `p_(k-1)^2 - P q_(k-1)^2 = (-1)^k Q_k` (the polynomial analogue of the expansion of a quadratic irrational; see
//! also van der Poorten, "Non-periodic continued fractions in hyperelliptic function fields", 2001).
use super::QPoly;
use mc_arith::num::Rat;
use mc_poly::ring::{CoeffRing, Rationals};
use mc_poly::unipoly as up;
use mc_poly::work::{Meter, WorkResult};

/// The most partial quotients computed.
const MAX_STEPS: usize = 40;
/// The largest degree of a convergent.
const MAX_DEGREE: usize = 64;

/// A unit `A + B y` (`A^2 - B^2 P` a nonzero constant, `B` nonzero) from the continued fraction of `sqrt(P)`, the
/// first one within the bounds; `None` when `P` does not qualify or none is found.
pub fn unit(p: &[Rat], m: &mut Meter) -> WorkResult<Option<(QPoly, QPoly)>> {
    let r = &Rationals;
    let Some(d) = up::deg(p) else { return Ok(None) };
    if d < 4 || d % 2 == 1 || !super::square_free(p, m)? {
        return Ok(None);
    }
    let Some(a0) = poly_sqrt_part(p, m)? else { return Ok(None) };
    // P_0 = 0, Q_0 = 1; convergents p_-1 = 1, q_-1 = 0, p_-2 = 0, q_-2 = 1
    let (mut pk, mut qk): (QPoly, QPoly) = (vec![], vec![r.one()]);
    let (mut p1, mut q1): (QPoly, QPoly) = (vec![r.one()], vec![]);
    let (mut p2, mut q2): (QPoly, QPoly) = (vec![], vec![r.one()]);
    for _ in 0..MAX_STEPS {
        m.step(1)?;
        let Some((ak, _)) = up::divrem(r, &up::add(r, &pk, &a0), &qk, m)? else { return Ok(None) };
        let pn = up::add(r, &up::mul(r, &ak, &p1, m)?, &p2);
        let qn = up::add(r, &up::mul(r, &ak, &q1, m)?, &q2);
        if up::deg(&pn).unwrap_or(0) > MAX_DEGREE {
            return Ok(None);
        }
        // the next P, Q
        let pk1 = up::sub(r, &up::mul(r, &ak, &qk, m)?, &pk);
        let Some(qk1) = up::div_exact(r, &up::sub(r, p, &up::mul(r, &pk1, &pk1, m)?), &qk, m)? else { return Ok(None) };
        (p2, q2, p1, q1) = (p1, q1, pn, qn);
        (pk, qk) = (pk1, qk1);
        // p_k^2 - P q_k^2 = +- Q_(k+1): a constant gives the unit
        if up::deg(&qk) == Some(0) && !q1.is_empty() {
            return Ok(Some((p1, q1)));
        }
    }
    Ok(None)
}

/// The polynomial part of `sqrt(P)` in `Q((1/x))` (degree `deg P / 2`), when the leading coefficient of `P` is the
/// square of a rational.
fn poly_sqrt_part(p: &[Rat], m: &mut Meter) -> WorkResult<Option<QPoly>> {
    let r = &Rationals;
    let d = p.len() - 1;
    let h = d / 2;
    let Some(c) = rat_sqrt(&p[d]) else { return Ok(None) };
    let two_c = c.add(&c);
    let Some(inv) = two_c.recip() else { return Ok(None) };
    let mut a = vec![r.zero(); h + 1];
    a[h] = c;
    // the coefficient of x^(h + k) of a^2 equals p_(h + k) for k = h - 1, ..., 0
    for k in (0..h).rev() {
        m.step(h as u64)?;
        // the ordered pairs i + j = h + k with both above k (the pair (h, k) gives the 2 a_h a_k)
        let mut tot = r.zero();
        for i in (k + 1)..h {
            tot = tot.add(&a[i].mul(&a[h + k - i]));
        }
        a[k] = p[h + k].sub(&tot).mul(&inv);
    }
    Ok(Some(up::trim(r, a)))
}

/// The square root of a rational when it is rational.
pub fn rat_sqrt(q: &Rat) -> Option<Rat> {
    let (n, d) = super::parts(q)?;
    if n < 0 {
        return None;
    }
    let isqrt = |v: i64| {
        let s = (v as f64).sqrt().round() as i64;
        (s >= 0 && s.checked_mul(s) == Some(v)).then_some(s)
    };
    Some(super::q(isqrt(n)?, isqrt(d)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mc_poly::work::Unlimited;

    fn z(v: &[i64]) -> QPoly {
        up::trim(&Rationals, v.iter().map(|&k| Rat::from_i64(k)).collect())
    }

    #[test]
    fn chebyshev_pseudo_elliptic_unit() {
        let mut w = Unlimited;
        let mut m = Meter::new(&mut w);
        // P = x^4 + 10 x^2 - 96 x - 71 (Chebyshev's example): a unit of degree 8
        let p = z(&[-71, -96, 10, 0, 1]);
        let (a, b) = unit(&p, &mut m).unwrap().unwrap();
        let r = &Rationals;
        let n = up::sub(r, &up::mul(r, &a, &a, &mut m).unwrap(), &up::mul(r, &up::mul(r, &b, &b, &mut m).unwrap(), &p, &mut m).unwrap());
        assert_eq!(up::deg(&n), Some(0));
        assert_eq!(up::deg(&a), Some(8));
        // A'/B is a constant multiple of x
        let da = up::derivative(r, &a, &mut m).unwrap();
        let (qt, rem) = up::divrem(r, &da, &b, &mut m).unwrap().unwrap();
        assert!(rem.is_empty());
        assert_eq!(up::deg(&qt), Some(1));
        assert!(qt[0].is_zero());
        // x^4 + 1: the unit x^2 + y at once
        assert_eq!(unit(&z(&[1, 0, 0, 0, 1]), &mut m).unwrap(), Some((z(&[0, 0, 1]), z(&[1]))));
    }
}
