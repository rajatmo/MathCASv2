//! Binary splitting of hypergeometric-type series (spike 3): sums whose consecutive terms have a rational ratio
//! `p(k) / (q(k) 2^shift)` with small integer polynomials `p` and `q`. The partial products and sums of a range of
//! terms are exact integers, combined pairwise, so a sum of N terms costs O(M(n) log N) for n-bit results instead
//! of N full-precision operations.
//!
//! The recursion depth is log2 of the number of terms (bounded, decision 1.6 concerns recursion over terms). Every
//! split polls the stop flag; a stopped split returns garbage, which the caller discards after checking
//! [`stopped`].
use mc_arith::num::{stopped, Ball, Float, Int, Mag, Nat};

/// The term ratio of a series: term(k) = term(k - 1) p(k) / (q(k) 2^shift) for k >= 1, term(0) = 1.
pub(crate) trait Ratio {
    /// The numerator polynomial at k >= 1.
    fn p(&self, k: u64) -> Int;
    /// The denominator polynomial at k >= 1 (positive).
    fn q(&self, k: u64) -> Nat;
    /// A power of two in every denominator (the bits of a dyadic argument), kept as a shift.
    fn shift(&self) -> u64 {
        0
    }
}

/// The exact integers of the range [a, b): P = prod p(k), Q = prod q(k), and T with
/// sum_{k=a}^{b-1} prod_{j=a}^{k} p(j) / (q(j) 2^shift) = T / (Q 2^(shift (b - a))).
struct Split {
    p: Int,
    q: Nat,
    t: Int,
}

fn split(r: &dyn Ratio, a: u64, b: u64, need_p: bool) -> Split {
    if b == a + 1 {
        let p = r.p(a);
        return Split { t: p.clone(), q: r.q(a), p };
    }
    let m = a + (b - a) / 2;
    let left = split(r, a, m, true);
    if stopped() {
        return left;
    }
    let right = split(r, m, b, need_p);
    // T = T1 Q2 2^(s n2) + P1 T2
    let t = left.t.mul(&Int::from_nat(right.q.clone())).shl(r.shift() * (b - m)).add(&left.p.mul(&right.t));
    // the right spine never needs P: skipping it saves one large product per level
    let p = if need_p { left.p.mul(&right.p) } else { Int::default() };
    Split { p, q: left.q.mul(&right.q), t }
}

/// sum_{k=0}^{n-1} term(k) as a ball at precision `prec`, enclosing the exact partial sum (the caller adds the
/// tail bound). n >= 1.
pub(crate) fn partial_sum(r: &dyn Ratio, n: u64, prec: u64) -> Ball {
    let one = Ball::from_i64(1);
    if n <= 1 {
        return one;
    }
    let s = split(r, 1, n, false);
    if stopped() || s.q.is_zero() {
        return one; // garbage after a stop, discarded by the caller
    }
    let num = Float::new(s.t.neg, s.t.mag, 0);
    let den = Float::new(false, s.q, (r.shift() * (n - 1)) as i64);
    let (m, e) = num.div(&den, prec);
    one.add(&Ball { mid: m, rad: e }, prec)
}

/// A lower bound of log2(n!): the sum of floor(log2 k) for k <= n.
pub(crate) fn log2_factorial_lower(n: u64) -> u64 {
    (2..=n).map(|k| 63 - k.leading_zeros() as u64).sum()
}

/// The ball b widened by an absolute error bound `err`.
pub(crate) fn widen(b: Ball, err: &Mag) -> Ball {
    Ball { rad: b.rad.add(err), mid: b.mid }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// sum 1/k!: e.
    struct InvFact;
    impl Ratio for InvFact {
        fn p(&self, _: u64) -> Int {
            Int::from_i64(1)
        }
        fn q(&self, k: u64) -> Nat {
            Nat::from_u64(k)
        }
    }

    /// sum (1/2)^k: 2 - 2^(1-n), with the shift path.
    struct Halves;
    impl Ratio for Halves {
        fn p(&self, _: u64) -> Int {
            Int::from_i64(1)
        }
        fn q(&self, _: u64) -> Nat {
            Nat::from_u64(1)
        }
        fn shift(&self) -> u64 {
            1
        }
    }

    #[test]
    fn geometric_sum_is_exact() {
        for n in [1u64, 2, 3, 7, 64, 100] {
            let s = partial_sum(&Halves, n, 200);
            let want = Float::from_i64(2).sub_exact(&Float::new(false, Nat::from_u64(1), 1 - n as i64));
            assert_eq!(s.mid.cmp_exact(&want), std::cmp::Ordering::Equal, "{n}");
            assert!(s.rad.is_zero());
        }
    }

    #[test]
    fn e_by_partial_sums() {
        let s = partial_sum(&InvFact, 60, 200);
        let (d, _) = s.mid.to_sci(30);
        assert_eq!(d, "271828182845904523536028747135");
        assert_eq!(log2_factorial_lower(1), 0);
        assert_eq!(log2_factorial_lower(5), 1 + 1 + 2 + 2);
    }
}
