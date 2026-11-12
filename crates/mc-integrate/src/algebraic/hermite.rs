//! Hermite reduction in a simple radical extension (Ostrogradsky's undetermined coefficients).
//!
//! For `y = g^(1/q)`, `g` square-free of degree `d`, an exponent `s` with `-1 < s < 0`, an integer `w >= 0` and
//! polynomials `P`, `E` with `gcd(E, g) = 1`:
//!
//! `int P/(E g^w) g^s dx = A/E_red g^(s+1-w) + int B/E_sf g^s dx`,
//!
//! where `E = prod e_i^i` (square-free decomposition), `E_red = prod e_i^(i-1)`, `E_sf = prod e_i`, and
//! `deg B < deg E_sf + d - 1`. Differentiating and multiplying by `E g^(w-s)`, with `H = E_sf E_red' / E_red` (a
//! polynomial),
//!
//! `P = (E_sf A' - A H) g + (s + 1 - w) A g' E_sf + B E_red g^w`,
//!
//! one linear system in the coefficients of `A` and `B`. The remainder `B g^s dx` is holomorphic above the roots of
//! `g` (its order there is `q s + q - 1 >= 0`), so the poles of order two and more above the roots of `g` (the factor
//! `g^w`) are removed as well. The solution is unique: an exact differential `d(A/E_red g^(s+1-w))` has poles of order
//! two or more at the roots of `E_red` (and above those of `g` when `w > 0`) and, at infinity, a numerator of degree
//! at least `d - 1` (the leading coefficient `lc(g) (i + (s + 1) d)` never vanishes), so it is never of the
//! remainder's form. This is the reduction of Hermite (1872) and Ostrogradsky (1845) carried to the function field
//! `Q(x)[y]` with the integral basis `1, y, ..., y^(q-1)` (Trager 1984, chapter 4; Bronstein's notes): the poles of
//! order two and more at finite places, and the excess degree at infinity, are removed.
//!
//! **A proof of non-elementarity** ([`second_kind`]). For `q = 2`, `E` constant and `d >= 3` the remainder
//! `B g^(-1/2) dx` (`deg B <= d - 2`) has poles only above infinity. When its residues there vanish (always for odd
//! `d`, one place at infinity; for even `d` when the coefficient of `1/x` in the expansion of `B g^(-1/2)` at
//! infinity is zero) it is a differential of the second kind, and it is not exact (by the uniqueness above). By
//! Liouville's theorem an elementary integral of a differential with zero residues is `v_0 + sum c_i log v_i` with
//! every `d v_i / v_i` free of residues, so every `v_i` is constant and the differential is exact: a contradiction.
//! So a nonzero remainder of the second kind proves that the integral is not elementary.
use super::QPoly;
use crate::linsolve::{self, Row};
use mc_arith::num::Rat;
use mc_poly::ring::{CoeffRing, Rationals};
use mc_poly::unipoly as up;
use mc_poly::work::{Meter, WorkResult};

/// The result of the reduction: the algebraic part `A/E_red g^(s+1)` and the remainder `B/E_sf g^s`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reduction {
    /// the numerator of the algebraic part
    pub a: QPoly,
    /// its denominator
    pub e_red: QPoly,
    /// the numerator of the remainder
    pub b: QPoly,
    /// its denominator (square-free)
    pub e_sf: QPoly,
}

/// The most unknowns of one reduction.
const MAX_UNKNOWNS: usize = 120;

/// The reduction of `int P/(E g^w) g^s` (see the module); `None` when `gcd(E, g)` is not constant, `g` is not
/// square-free, `s` is not in (-1, 0), the system is too large, or (it should not happen) inconsistent.
pub fn reduce(g: &[Rat], s: &Rat, w: usize, p: &[Rat], e: &[Rat], m: &mut Meter) -> WorkResult<Option<Reduction>> {
    let r = &Rationals;
    let Some(d) = up::deg(g) else { return Ok(None) };
    if d == 0 || up::deg(e).is_none() {
        return Ok(None);
    }
    if !(s.cmp_value(&Rat::from_i64(0)).is_lt() && s.cmp_value(&Rat::from_i64(-1)).is_gt()) {
        return Ok(None);
    }
    if !super::square_free(g, m)? || up::deg(&up::gcd(r, e, g, m)?).unwrap_or(0) > 0 {
        return Ok(None);
    }
    // E = prod e_i^i: E_sf = prod e_i, E_red = E / E_sf
    let mut e_sf: QPoly = vec![r.one()];
    for (f, _) in up::square_free(r, e, m)? {
        e_sf = up::mul(r, &e_sf, &f, m)?;
    }
    let e_sf = up::monic(r, &e_sf, m)?;
    let Some(e_red) = up::div_exact(r, e, &e_sf, m)? else { return Ok(None) };
    let de_red = up::derivative(r, &e_red, m)?;
    let Some(hh) = up::div_exact(r, &up::mul(r, &e_sf, &de_red, m)?, &e_red, m)? else { return Ok(None) };
    let dg = up::derivative(r, g, m)?;
    let s1 = s.add(&Rat::from_i64(1 - w as i64));
    let gw = up::pow(r, g, w as u64, m)?;
    let (dp, dsf, dred) = (up::deg(p).unwrap_or(0) as i64, up::deg(&e_sf).unwrap_or(0) as i64, up::deg(&e_red).unwrap_or(0) as i64);
    let na = (dp - dsf - d as i64 + 1).max(dred + (w * d) as i64).max(0) as usize + 1;
    let nb = dsf as usize + d - 1;
    if na + nb > MAX_UNKNOWNS {
        return Ok(None);
    }
    // the columns: the image of x^i in A, then of x^j in B
    let mut cols: Vec<QPoly> = Vec::with_capacity(na + nb);
    for i in 0..na {
        let xi = up::shift_up(r, &[r.one()], i);
        let dxi: QPoly = if i == 0 { vec![] } else { up::shift_up(r, &[r.of_i64(i as i64)], i - 1) };
        let t1 = up::add(r, &up::mul(r, &dxi, g, m)?, &up::scale(r, &up::mul(r, &xi, &dg, m)?, &s1, m)?);
        let t1 = up::mul(r, &e_sf, &t1, m)?;
        let t2 = up::mul(r, &up::mul(r, &xi, g, m)?, &hh, m)?;
        cols.push(up::sub(r, &t1, &t2));
    }
    let red_gw = up::mul(r, &e_red, &gw, m)?;
    for j in 0..nb {
        cols.push(up::shift_up(r, &red_gw, j));
    }
    let top = cols.iter().map(|c| c.len()).chain(std::iter::once(p.len())).max().unwrap_or(0);
    let mut rows = Vec::with_capacity(top);
    for k in 0..top {
        let mut row = vec![];
        for (j, c) in cols.iter().enumerate() {
            if let Some(v) = c.get(k) {
                if !v.is_zero() {
                    row.push((j, v.clone()));
                }
            }
        }
        rows.push(Row { cols: row, rhs: p.get(k).cloned().unwrap_or_else(|| r.zero()) });
    }
    let Some(u) = linsolve::solve(r, rows, na + nb, m)? else { return Ok(None) };
    let a = up::trim(r, u[..na].to_vec());
    let b = up::trim(r, u[na..].to_vec());
    Ok(Some(Reduction { a, e_red, b, e_sf }))
}

/// For `q = 2`, a constant `E` and `deg g >= 3`: is the remainder `B g^(-1/2) dx` nonzero and of the second kind
/// (zero residues above infinity)? Then no elementary antiderivative exists (see the module).
pub fn second_kind(g: &[Rat], b: &[Rat], m: &mut Meter) -> WorkResult<bool> {
    let Some(d) = up::deg(g) else { return Ok(false) };
    if d < 3 || up::deg(b).is_none() || !super::square_free(g, m)? {
        return Ok(false);
    }
    if d % 2 == 1 {
        return Ok(true);
    }
    Ok(residue_at_infinity(g, b, m)?.is_some_and(|v| v.is_zero()))
}

/// For even `deg g`: `sqrt(lc(g))` times the residue of `B g^(-1/2) dx` at one place above infinity (the other has
/// the opposite one); `None` for odd `deg g`. With `g = lc x^d (1 + w(1/x))`, the coefficient of `1/x` in
/// `x^(-d/2) B(x) (1 + w)^(-1/2)` is `sum_j b_j c_(j - d/2 + 1)`, `c_k` the coefficient of `x^-k` in
/// `(1 + w)^(-1/2)`.
pub fn residue_at_infinity(g: &[Rat], b: &[Rat], m: &mut Meter) -> WorkResult<Option<Rat>> {
    let r = &Rationals;
    let Some(d) = up::deg(g) else { return Ok(None) };
    if d % 2 == 1 {
        return Ok(None);
    }
    let lc = up::lc(r, g);
    let Some(inv) = lc.recip() else { return Ok(None) };
    // w as a series in z = 1/x: w_k = g_(d-k) / lc
    let n = (d + 1).max(b.len() + 1);
    let w: Vec<Rat> = (0..n).map(|k| if k == 0 || k > d { r.zero() } else { g[d - k].mul(&inv) }).collect();
    let c = binomial_series(&w, n, m)?;
    let mut res = r.zero();
    for (j, bj) in b.iter().enumerate() {
        let k = j as i64 - (d / 2) as i64 + 1;
        if k >= 0 && (k as usize) < c.len() {
            res = res.add(&bj.mul(&c[k as usize]));
        }
    }
    Ok(Some(res))
}

/// The first `n` coefficients of `(1 + w(z))^(-1/2)`, `w(0) = 0`, by the recurrence of `y' (1 + w) = -w' y / 2`.
fn binomial_series(w: &[Rat], n: usize, m: &mut Meter) -> WorkResult<Vec<Rat>> {
    let r = &Rationals;
    let mut y = vec![r.one()];
    let half = super::q(-1, 2);
    for k in 1..n {
        m.step(k as u64)?;
        // k y_k + sum_{j>=1} (k - j) w_j y_(k-j) = -1/2 sum_{j>=1} j w_j y_(k-j)
        let mut acc = r.zero();
        for j in 1..=k {
            let wj = w.get(j).cloned().unwrap_or_else(|| r.zero());
            if wj.is_zero() {
                continue;
            }
            let coef = half.mul(&Rat::from_i64(j as i64)).sub(&Rat::from_i64((k - j) as i64));
            acc = acc.add(&coef.mul(&wj).mul(&y[k - j]));
        }
        y.push(acc.mul(&super::q(1, k as i64)));
    }
    Ok(y)
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
    fn rational_part_of_a_cubic_radical() {
        let mut w = Unlimited;
        let mut m = Meter::new(&mut w);
        // int (5 x^3 + 2)/sqrt(x^3 + 1) = 2 x sqrt(x^3 + 1): P = (5x^3 + 2)/2 ... with s = -1/2, A = 2x
        let g = z(&[1, 0, 0, 1]);
        let red = reduce(&g, &q(-1, 2), 0, &z(&[2, 0, 0, 5]), &z(&[1]), &mut m).unwrap().unwrap();
        assert_eq!(red.a, z(&[0, 2]));
        assert!(red.b.is_empty());
        // int sqrt(x^3 + 1) = int (x^3 + 1)/sqrt(x^3 + 1): a remainder of the second kind
        let red = reduce(&g, &q(-1, 2), 0, &z(&[1, 0, 0, 1]), &z(&[1]), &mut m).unwrap().unwrap();
        assert!(!red.b.is_empty());
        assert!(second_kind(&g, &red.b, &mut m).unwrap());
    }

    #[test]
    fn residue_at_infinity_of_an_even_quartic() {
        let mut w = Unlimited;
        let mut m = Meter::new(&mut w);
        // x^2 / sqrt(x^4 + x^3 + 1) has a residue at infinity; 1/sqrt(x^4 + 1) and x^2/sqrt(x^4 + 1) do not
        assert!(!second_kind(&z(&[1, 0, 0, 1, 1]), &z(&[0, 0, 1]), &mut m).unwrap());
        assert!(second_kind(&z(&[1, 0, 0, 0, 1]), &z(&[1]), &mut m).unwrap());
        assert!(second_kind(&z(&[1, 0, 0, 0, 1]), &z(&[0, 0, 1]), &mut m).unwrap());
        // x / sqrt(x^4 + 1): a residue (it is asinh(x^2)/2)
        assert!(!second_kind(&z(&[1, 0, 0, 0, 1]), &z(&[0, 1]), &mut m).unwrap());
    }

    #[test]
    fn finite_poles() {
        let mut w = Unlimited;
        let mut m = Meter::new(&mut w);
        // int 1/(x^2 sqrt(x^2 - 1)) = sqrt(x^2 - 1)/x: A = 1, E_red = x
        let red = reduce(&z(&[-1, 0, 1]), &q(-1, 2), 0, &z(&[1]), &z(&[0, 0, 1]), &mut m).unwrap().unwrap();
        assert!(red.b.is_empty());
        assert_eq!(red.e_red, z(&[0, 1]));
        assert_eq!(red.a, z(&[1]));
        // int (1 + x^2)^(-3/2) = x (1 + x^2)^(-1/2): w = 1, A = x
        let red = reduce(&z(&[1, 0, 1]), &q(-1, 2), 1, &z(&[1]), &z(&[1]), &mut m).unwrap().unwrap();
        assert!(red.b.is_empty());
        assert_eq!(red.a, z(&[0, 1]));
    }
}
