//! The algebra of integrands with roots (roadmap item 14B): the decisions and the linear algebra of the
//! substitution portfolio that `mc-calculus` (`integrate/elementary/algebraic`) drives.
//!
//! - [`binomial`]: Chebyshev's theorem on binomial differentials `x^m (a + b x^n)^p` (m, n, p rational): the three
//!   integrable cases with their rationalising substitutions, and otherwise a proof that no elementary
//!   antiderivative exists.
//! - [`hermite`]: Hermite reduction in a simple radical extension `y^q = g(x)`, `g` square-free (Ostrogradsky's
//!   method of undetermined coefficients): `int P/E g^s = A/E_red g^(s+1) + int B/E_sf g^s` with the remainder
//!   reduced at the finite places and at infinity; for `q = 2` and `deg g >= 3` a nonzero remainder of the second
//!   kind proves non-elementarity (Liouville's theorem on the curve; see the module).
//! - [`abel`]: the logarithmic part of a hyperelliptic remainder with poles only at infinity, from the continued
//!   fraction of `sqrt(P)` (Abel, Chebyshev; Davenport's torsion test), bounded.
//! - [`goursat`]: rational functions invariant under the involution `x -> c/x` written in `u = x + c/x` (the
//!   reduction behind Goursat's pseudo-elliptic integrals of quartic radicals).
//!
//! Everything here is exact (rational coefficients), metered by a `mc-poly` [`Meter`], and returns `None` when a
//! method does not apply; the driver verifies every antiderivative it builds from these pieces by differentiation.
//!
//! Literature: P. L. Chebyshev, "Sur l'intégration des différentielles irrationnelles", J. Math. Pures Appl. 18
//! (1853); E. Goursat, "Note sur quelques intégrales pseudo-elliptiques", Bull. SMF 15 (1887); M. V. Ostrogradsky
//! (1845) and C. Hermite (1872) on the rational part; B. M. Trager, *Integration of Algebraic Functions*, PhD thesis,
//! MIT 1984 (Hermite reduction with an integral basis, the residue criterion); J. H. Davenport, *On the Integration
//! of Algebraic Functions*, LNCS 102, Springer 1981; M. Bronstein, "Symbolic integration tutorial" (ISSAC 1998) and
//! his notes on the algebraic case.
pub mod abel;
pub mod binomial;
pub mod goursat;
pub mod hermite;

use mc_arith::num::Rat;
use mc_poly::ring::{CoeffRing, Rationals};
use mc_poly::unipoly as up;
use mc_poly::work::{Meter, WorkResult};

/// A univariate polynomial with rational coefficients, constant term first, trimmed (zero is empty).
pub type QPoly = Vec<Rat>;

/// The rational `n / d`.
pub fn q(n: i64, d: i64) -> Rat {
    Rat::new(mc_arith::num::Int::from_i64(n), mc_arith::num::Int::from_i64(d))
}

/// The polynomial of the coefficients (constant first), trimmed.
pub fn poly(cs: &[Rat]) -> QPoly {
    up::trim(&Rationals, cs.to_vec())
}

/// Is `g` square-free (gcd with its derivative a constant)?
pub fn square_free(g: &[Rat], m: &mut Meter) -> WorkResult<bool> {
    let d = up::derivative(&Rationals, g, m)?;
    let c = up::gcd(&Rationals, g, &d, m)?;
    Ok(up::deg(&c).unwrap_or(0) == 0)
}

/// Numerator and denominator of a rational with machine-sized parts.
pub fn parts(r: &Rat) -> Option<(i64, i64)> {
    let (n, d) = mc_poly::qpoly::rat_parts(r);
    Some((n.as_i64()?, d.as_i64()?))
}

/// A polynomial in `x` as text, highest degree first (`x^3 + 1`, `-1/2 x^2 + 3`).
pub fn text(p: &[Rat]) -> String {
    let mut out = String::new();
    for (k, c) in p.iter().enumerate().rev() {
        if c.is_zero() {
            continue;
        }
        let (n, d) = parts(c).unwrap_or((0, 1));
        let neg = n < 0;
        let (n, mag_one) = (n.abs(), n.abs() == 1 && d == 1);
        if out.is_empty() {
            if neg {
                out.push('-');
            }
        } else {
            out.push_str(if neg { " - " } else { " + " });
        }
        let coef = if d == 1 { format!("{n}") } else { format!("{n}/{d}") };
        match k {
            0 => out.push_str(&coef),
            _ => {
                if !mag_one {
                    out.push_str(&coef);
                    out.push(' ');
                }
                out.push('x');
                if k > 1 {
                    out.push_str(&format!("^{k}"));
                }
            }
        }
    }
    if out.is_empty() {
        out.push('0');
    }
    out
}

/// `p` as a polynomial in `h` (`deg h >= 1`): the `h`-adic digits of `p` when they are all constants, else `None`.
pub fn in_terms_of(p: &[Rat], h: &[Rat], m: &mut Meter) -> WorkResult<Option<QPoly>> {
    let r = &Rationals;
    if up::deg(h).unwrap_or(0) == 0 {
        return Ok(None);
    }
    let mut out = vec![];
    let mut cur = p.to_vec();
    while !cur.is_empty() {
        let Some((q, rem)) = up::divrem(r, &cur, h, m)? else { return Ok(None) };
        if up::deg(&rem).unwrap_or(0) > 0 {
            return Ok(None);
        }
        out.push(rem.first().cloned().unwrap_or_else(|| r.zero()));
        cur = q;
    }
    Ok(Some(up::trim(r, out)))
}
