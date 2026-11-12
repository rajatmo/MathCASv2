//! Functions invariant under the involution `x -> c/x`, written in `u = x + c/x`.
//!
//! Goursat (1887) showed that `R(x) dx / sqrt(P(x))`, `P` a quartic, is pseudo-elliptic (has an elementary
//! integral although the curve has genus one) when the differential is invariant under a Möbius involution that
//! permutes the roots of `P`: with `u` an invariant function of degree two, the differential is `G(u) du` with
//! `G` rational in `u` and `sqrt(Q(u))`, `Q` quadratic, and the integral is elementary. For the involutions
//! `x -> c/x` (`c` rational; `c < 0` gives `u = x - |c|/x`) the invariant is `u = x + c/x`: a quartic with
//! `p_0 = c^2 p_4` and `p_1 = c p_3` is `x^2 Q(u)`, and an invariant rational function is a rational function of `u`.
//! [`laurent_in_u`] and [`ratfun_in_u`] compute these; they return `None` when the input is not invariant, so the
//! driver tests and reduces in one step. (The involution `x -> -x` is the substitution `u = x^2` of the driver.)
use super::QPoly;
use mc_arith::num::Rat;
use mc_poly::ring::{CoeffRing, Rationals};
use mc_poly::unipoly as up;
use mc_poly::work::{Meter, WorkResult};
use std::collections::BTreeMap;

/// A Laurent polynomial: exponent to coefficient (no zero coefficients).
pub type Laurent = BTreeMap<i64, Rat>;

/// The Laurent polynomial `x^shift p(x)`.
pub fn laurent(p: &[Rat], shift: i64) -> Laurent {
    p.iter().enumerate().filter(|(_, c)| !c.is_zero()).map(|(k, c)| (k as i64 + shift, c.clone())).collect()
}

/// `l` as a polynomial in `u = x + c/x` (constant first) when `l` is invariant under `x -> c/x`, else `None`.
pub fn laurent_in_u(l: &Laurent, c: &Rat, m: &mut Meter) -> WorkResult<Option<QPoly>> {
    let r = &Rationals;
    let mut l = l.clone();
    let Some(&top) = l.keys().next_back() else { return Ok(Some(vec![])) };
    if top < 0 {
        return Ok(None);
    }
    let mut out = vec![r.zero(); top as usize + 1];
    // (x + c/x)^j = sum_i C(j, i) c^i x^(j - 2i)
    for j in (1..=top).rev() {
        m.step(j as u64)?;
        let Some(lj) = l.get(&j).cloned() else { continue };
        out[j as usize] = lj.clone();
        let mut binom = Rat::from_i64(1);
        let mut ci = Rat::from_i64(1);
        for i in 0..=j {
            let e = j - 2 * i;
            let v = lj.mul(&binom).mul(&ci);
            let nv = l.get(&e).cloned().unwrap_or_else(|| r.zero()).sub(&v);
            if nv.is_zero() {
                l.remove(&e);
            } else {
                l.insert(e, nv);
            }
            binom = binom.mul(&Rat::from_i64(j - i)).mul(&super::q(1, i + 1));
            ci = ci.mul(c);
        }
    }
    // what is left must be a constant
    if l.keys().any(|&e| e != 0) {
        return Ok(None);
    }
    out[0] = l.get(&0).cloned().unwrap_or_else(|| r.zero());
    Ok(Some(up::trim(r, out)))
}

/// `x^shift N(x) / D(x)` as `n(u) / s(u)` when it is invariant under `x -> c/x`, else `None`.
pub fn ratfun_in_u(num: &[Rat], shift: i64, den: &[Rat], c: &Rat, m: &mut Meter) -> WorkResult<Option<(QPoly, QPoly)>> {
    let r = &Rationals;
    let Some(dd) = up::deg(den) else { return Ok(None) };
    // D~(x) = x^dd D(c/x); D D~ / x^dd and x^shift N D~ / x^dd are invariant when N/D is
    let mut ct = Rat::from_i64(1);
    let mut dt = vec![r.zero(); dd + 1];
    for (k, dk) in den.iter().enumerate() {
        dt[dd - k] = dk.mul(&ct);
        ct = ct.mul(c);
    }
    let dt = up::trim(r, dt);
    let s = laurent(&up::mul(r, den, &dt, m)?, -(dd as i64));
    let n = laurent(&up::mul(r, num, &dt, m)?, shift - dd as i64);
    let (Some(su), Some(nu)) = (laurent_in_u(&s, c, m)?, laurent_in_u(&n, c, m)?) else { return Ok(None) };
    if su.is_empty() {
        return Ok(None);
    }
    // lowest terms
    let g = up::gcd(r, &nu, &su, m)?;
    let (Some(nu), Some(su)) = (up::div_exact(r, &nu, &g, m)?, up::div_exact(r, &su, &g, m)?) else { return Ok(None) };
    Ok(Some((nu, su)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::algebraic::q;
    use mc_poly::work::Unlimited;

    fn z(v: &[i64]) -> QPoly {
        up::trim(&Rationals, v.iter().map(|&k| Rat::from_i64(k)).collect())
    }

    #[test]
    fn palindromic_quartic_and_invariant_ratio() {
        let mut w = Unlimited;
        let mut m = Meter::new(&mut w);
        // (x^4 + 3x^2 + 1)/x^2 = u^2 + 1 with u = x + 1/x
        let l = laurent(&z(&[1, 0, 3, 0, 1]), -2);
        assert_eq!(laurent_in_u(&l, &q(1, 1), &mut m).unwrap(), Some(z(&[1, 0, 1])));
        // x^4 + 1 with c = -1: (x^4 + 1)/x^2 = (x - 1/x)^2 + 2
        let l = laurent(&z(&[1, 0, 0, 0, 1]), -2);
        assert_eq!(laurent_in_u(&l, &q(-1, 1), &mut m).unwrap(), Some(z(&[2, 0, 1])));
        // x^3 + 1 is not invariant
        assert_eq!(laurent_in_u(&laurent(&z(&[1, 0, 0, 1]), -2), &q(1, 1), &mut m).unwrap(), None);
        // x (x^2 + 1)/(x^2 - 1)... (x^2 + 1)/(x^2 + 3x + 1) = (u)/(u + 3)
        let (n, s) = ratfun_in_u(&z(&[1, 0, 1]), 0, &z(&[1, 3, 1]), &q(1, 1), &mut m).unwrap().unwrap();
        assert_eq!((n, s), (z(&[0, 1]), z(&[3, 1])));
        // x/(x^2 + 1) is invariant: 1/u
        let (n, s) = ratfun_in_u(&z(&[1]), 1, &z(&[1, 0, 1]), &q(1, 1), &mut m).unwrap().unwrap();
        assert_eq!((n, s), (z(&[1]), z(&[0, 1])));
        // x is not
        assert!(ratfun_in_u(&z(&[1]), 1, &z(&[1]), &q(1, 1), &mut m).unwrap().is_none());
    }
}
