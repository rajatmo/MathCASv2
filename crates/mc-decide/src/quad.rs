//! The sign of a real polynomial of total degree at most two in several variables, exactly (tier 3b of the
//! decision service).
//!
//! `e(x) = x^T A x + b^T x + c` is written as the symmetric matrix `M = [[A, b/2], [b^T/2, c]]` with the constant
//! last, so that `e(x) = (x, 1)^T M (x, 1)`. Symmetric Gaussian elimination (an exact LDL^T decomposition over the
//! rationals; Golub and Van Loan, *Matrix Computations*, 4.2) decides whether `M` is positive semidefinite: a
//! negative pivot, or a zero pivot with a nonzero rest of its row, refutes it. `M` is positive semidefinite exactly
//! when `e >= 0` for every real `x`, and the last pivot (the Schur complement of `A` in `M`) is then the minimum of
//! `e`, so `e > 0` everywhere exactly when it is positive. This decides `a^2 - a b + b^2 >= 0` and
//! `x^2 - x y + y^2 + 1 > 0` for real variables, which interval bounds cannot.
use mc_arith::num::Rat;
use std::cmp::Ordering;

/// What holds for every real point: `e > 0` or `e >= 0`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Definite {
    /// `e > 0`
    Positive,
    /// `e >= 0` (and `e` vanishes somewhere)
    Nonnegative,
}

/// The decision for `e` given as terms `(exponents, coefficient)` in `n` variables, each of total degree at most
/// two; `None` when `e` is not nonnegative everywhere (or a term has a higher degree).
pub fn definite(n: usize, terms: &[(Vec<u32>, Rat)]) -> Option<Definite> {
    let half = Rat::new(mc_arith::num::Int::from_i64(1), mc_arith::num::Int::from_i64(2));
    let z = Rat::from_i64(0);
    let mut m = vec![vec![z.clone(); n + 1]; n + 1];
    for (e, c) in terms {
        let idx: Vec<usize> = e.iter().enumerate().flat_map(|(i, &k)| std::iter::repeat_n(i, k as usize)).collect();
        match idx.as_slice() {
            [] => m[n][n] = m[n][n].add(c),
            [i] => {
                let h = c.mul(&half);
                m[*i][n] = m[*i][n].add(&h);
                m[n][*i] = m[n][*i].add(&h);
            }
            [i, j] if i == j => m[*i][*i] = m[*i][*i].add(c),
            [i, j] => {
                let h = c.mul(&half);
                m[*i][*j] = m[*i][*j].add(&h);
                m[*j][*i] = m[*j][*i].add(&h);
            }
            _ => return None,
        }
    }
    let size = n + 1;
    for k in 0..size {
        let d = m[k][k].clone();
        match d.cmp_value(&z) {
            Ordering::Less => return None,
            Ordering::Equal => {
                if (k + 1..size).any(|j| !m[k][j].is_zero()) {
                    return None;
                }
                if k == n {
                    return Some(Definite::Nonnegative);
                }
            }
            Ordering::Greater => {
                if k == n {
                    return Some(Definite::Positive);
                }
                let inv = d.recip().expect("a nonzero pivot");
                for i in k + 1..size {
                    if m[i][k].is_zero() {
                        continue;
                    }
                    let f = m[i][k].mul(&inv);
                    let pivot_row = m[k].clone();
                    for (x, p) in m[i].iter_mut().zip(&pivot_row).skip(k + 1) {
                        *x = x.sub(&f.mul(p));
                    }
                }
            }
        }
    }
    Some(Definite::Nonnegative)
}

/// The multipliers tried by [`definite_on`].
const LAMBDAS: &[(i64, i64)] = &[(1, 1), (1, 2), (2, 1), (1, 4), (3, 4), (5, 4), (3, 2), (4, 1), (1, 8), (8, 1), (1, 16), (16, 1)];

/// The decision for `e` on the set where `g >= 0` (`g > 0` when `strict`; both of degree at most two in the same `n`
/// variables), by the
/// S-procedure (Yakubovich 1971; Polik and Terlaky, "A survey of the S-lemma", SIAM Review 2007): if
/// `e - lambda g` is nonnegative (positive) everywhere for some `lambda >= 0`, then `e >= 0` (`e > 0`) wherever
/// `g >= 0`, and `e > 0` wherever `g > 0` (every multiplier tried is positive). The multiplier is searched on a fixed
/// set of rationals; each candidate is checked exactly, so a found certificate is a proof, and none found means
/// "unknown".
pub fn definite_on(n: usize, e: &[(Vec<u32>, Rat)], g: &[(Vec<u32>, Rat)], strict: bool) -> Option<Definite> {
    let mut best = None;
    for &(p, q) in LAMBDAS {
        let l = Rat::new(mc_arith::num::Int::from_i64(p), mc_arith::num::Int::from_i64(q));
        let mut terms: Vec<(Vec<u32>, Rat)> = e.to_vec();
        for (x, c) in g {
            let v = c.mul(&l).neg();
            match terms.iter_mut().find(|(y, _)| y == x) {
                Some((_, d)) => *d = d.add(&v),
                None => terms.push((x.clone(), v)),
            }
        }
        match definite(n, &terms) {
            Some(Definite::Positive) => return Some(Definite::Positive),
            Some(Definite::Nonnegative) if strict => return Some(Definite::Positive),
            Some(Definite::Nonnegative) => best = Some(Definite::Nonnegative),
            None => {}
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(e: &[u32], c: i64) -> (Vec<u32>, Rat) {
        (e.to_vec(), Rat::from_i64(c))
    }

    #[test]
    fn quadratic_forms() {
        // a^2 - a b + b^2 >= 0
        assert_eq!(definite(2, &[t(&[2, 0], 1), t(&[1, 1], -1), t(&[0, 2], 1)]), Some(Definite::Nonnegative));
        // x^2 - x y + y^2 + 1 > 0
        assert_eq!(definite(2, &[t(&[2, 0], 1), t(&[1, 1], -1), t(&[0, 2], 1), t(&[0, 0], 1)]), Some(Definite::Positive));
        // x^2 - 3 x y + y^2 is indefinite
        assert_eq!(definite(2, &[t(&[2, 0], 1), t(&[1, 1], -3), t(&[0, 2], 1)]), None);
        // x^2 + 2 x + 2 = (x + 1)^2 + 1 > 0; x^2 + 2 x + 1 >= 0
        assert_eq!(definite(1, &[t(&[2], 1), t(&[1], 2), t(&[0], 2)]), Some(Definite::Positive));
        assert_eq!(definite(1, &[t(&[2], 1), t(&[1], 2), t(&[0], 1)]), Some(Definite::Nonnegative));
        // 1 + (x - y)^2 > 0 although the quadratic part is singular
        assert_eq!(definite(2, &[t(&[2, 0], 1), t(&[1, 1], -2), t(&[0, 2], 1), t(&[0, 0], 1)]), Some(Definite::Positive));
        // x is not bounded below
        assert_eq!(definite(1, &[t(&[1], 1)]), None);
    }

    #[test]
    fn s_procedure() {
        // (x - 1)^2 + (y - 2)^2 - 3/2 >= 0 on the unit disk 1 - x^2 - y^2 >= 0
        let half = Rat::new(mc_arith::num::Int::from_i64(7), mc_arith::num::Int::from_i64(2));
        let e = vec![t(&[2, 0], 1), t(&[1, 0], -2), t(&[0, 2], 1), t(&[0, 1], -4), (vec![0, 0], half)];
        let g = vec![t(&[0, 0], 1), t(&[2, 0], -1), t(&[0, 2], -1)];
        assert_eq!(definite_on(2, &e, &g, false), Some(Definite::Positive));
        // 2 - x - y > 0 on the unit disk
        let e = vec![t(&[0, 0], 2), t(&[1, 0], -1), t(&[0, 1], -1)];
        assert_eq!(definite_on(2, &e, &g, false), Some(Definite::Positive));
        // x is not positive on the disk
        assert_eq!(definite_on(2, &[t(&[1, 0], 1)], &g, false), None);
    }
}
