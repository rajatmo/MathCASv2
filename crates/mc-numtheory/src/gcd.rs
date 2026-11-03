//! Subquadratic integer gcd by half-gcd reduction (benchmark N9: gcds of numbers with millions of digits; decision
//! J.13).
//!
//! `hgcd(a, b)` returns a matrix M of non-negative integers with determinant +-1 and the pair (alpha, beta) =
//! M^(-1) (a, b), both non-negative, with beta below about the square root of a. It recurses twice on top parts
//! (the leading half, then the leading part of the reduced pair) and applies each matrix found to the full numbers.
//! A matrix found from a top part describes the first quotients of the continued fraction of a/b except possibly
//! the last few; the last quotients are kept, and they are undone one at a time while the reduced pair has a
//! negative entry. Since every matrix is unimodular, gcd(a, b) = gcd(alpha, beta) whatever the quotients, so
//! correctness never depends on how far a top part's quotients reach: only speed does. When no reduction survives,
//! a plain division step is taken. Small operands use `mc-arith`'s Lehmer gcd.
use crate::budget::Budget;
use mc_arith::num::gcd::gcd as lehmer_gcd;
use mc_arith::num::{Int, Nat};
use mc_eval::Res;

/// Below this many limbs the quadratic Lehmer gcd is faster.
const LEHMER_LIMBS: usize = 200;
/// hgcd works on machine words below this many bits.
const BASE_BITS: u64 = 120;
/// Last quotients kept for undoing.
const TAIL: usize = 6;

/// (a; b) = M (alpha; beta) with M = [[m00, m01], [m10, m11]].
#[derive(Clone, Debug)]
struct Reduction {
    m: [Nat; 4],
    /// the determinant is (-1)^odd
    odd: bool,
    /// the last quotients, most recent last
    tail: Vec<Nat>,
}

impl Reduction {
    fn identity() -> Reduction {
        Reduction { m: [Nat::from_u64(1), Nat::zero(), Nat::zero(), Nat::from_u64(1)], odd: false, tail: vec![] }
    }
    fn is_identity(&self) -> bool {
        self.tail.is_empty() && self.m[1].is_zero() && self.m[2].is_zero()
    }
    /// M Q(q), Q(q) = [[q, 1], [1, 0]]: one more quotient.
    fn push(&mut self, q: Nat) {
        let [m00, m01, m10, m11] = &self.m;
        self.m = [m00.mul(&q).add(m01), m00.clone(), m10.mul(&q).add(m11), m10.clone()];
        self.odd = !self.odd;
        self.tail.push(q);
        if self.tail.len() > TAIL {
            self.tail.remove(0);
        }
    }
    /// M Q(q)^(-1) for the last quotient q, if one is kept.
    fn pop(&mut self) -> Option<Nat> {
        let q = self.tail.pop()?;
        let [m00, m01, m10, m11] = &self.m;
        self.m = [m01.clone(), m00.sub(&m01.mul(&q)), m11.clone(), m10.sub(&m11.mul(&q))];
        self.odd = !self.odd;
        Some(q)
    }
    /// M N.
    fn then(&self, n: &Reduction) -> Reduction {
        let [a, b, c, d] = &self.m;
        let [e, f, g, h] = &n.m;
        let m = [a.mul(e).add(&b.mul(g)), a.mul(f).add(&b.mul(h)), c.mul(e).add(&d.mul(g)), c.mul(f).add(&d.mul(h))];
        let mut tail = self.tail.clone();
        tail.extend(n.tail.iter().cloned());
        let cut = tail.len().saturating_sub(TAIL);
        tail.drain(..cut);
        Reduction { m, odd: self.odd != n.odd, tail }
    }
    /// M^(-1) (a; b) = det (m11 a - m01 b; m00 b - m10 a).
    fn inverse_apply(&self, a: &Nat, b: &Nat) -> (Int, Int) {
        let [m00, m01, m10, m11] = &self.m;
        let x = Int::from_nat(m11.mul(a)).sub(&Int::from_nat(m01.mul(b)));
        let y = Int::from_nat(m00.mul(b)).sub(&Int::from_nat(m10.mul(a)));
        if self.odd {
            (x.neg(), y.neg())
        } else {
            (x, y)
        }
    }
}

/// Apply `r`, found for the top parts (a >> k, b >> k) and reducing them to (xh, yh), to (a, b), undoing last
/// quotients while the pair has a negative entry; `None` when nothing valid is left. M^(-1) (a, b) is
/// (xh, yh) 2^k + M^(-1) (a mod 2^k, b mod 2^k), which multiplies the matrix by the low parts only. The pair returned
/// is ordered (alpha >= beta), with a swap recorded as quotient 0.
fn apply(top: (Reduction, Nat, Nat), k: u64, a: &Nat, b: &Nat) -> Option<(Reduction, Nat, Nat)> {
    let (mut r, xh, yh) = top;
    let (lx, ly) = r.inverse_apply(&a.low_bits(k), &b.low_bits(k));
    let (mut x, mut y) = (Int::from_nat(xh.shl(k)).add(&lx), Int::from_nat(yh.shl(k)).add(&ly));
    while x.neg || y.neg {
        // (alpha, beta) of M is Q(q) (alpha', beta') = (q alpha' + beta', alpha') of M Q(q)^(-1)
        let q = r.pop()?;
        let nx = x.mul(&Int::from_nat(q)).add(&y);
        y = x;
        x = nx;
    }
    if r.is_identity() {
        return None;
    }
    let (mut x, mut y) = (x.mag, y.mag);
    if x < y {
        r.push(Nat::zero());
        std::mem::swap(&mut x, &mut y);
    }
    Some((r, x, y))
}

/// One division step on an ordered pair.
fn step(r: &mut Reduction, a: &mut Nat, b: &mut Nat) {
    let (q, rem) = a.divrem(b);
    r.push(q);
    *a = std::mem::replace(b, rem);
}

/// Reduce the ordered pair (a, b), a >= b, until b < 2^s with s = bits(a)/2 + 1, by machine-word Euclid.
fn hgcd_base(a: u128, b: u128, s: u64) -> (Reduction, Nat, Nat) {
    let (mut a, mut b) = (a, b);
    let mut r = Reduction::identity();
    // bounded: each step at least halves a every two steps
    while b != 0 && 128 - b.leading_zeros() as u64 > s {
        let q = a / b;
        (a, b) = (b, a - q * b);
        r.push(Nat::from_u128(q));
    }
    (r, Nat::from_u128(a), Nat::from_u128(b))
}

/// Reduce the ordered pair (a, b) until b < 2^s by Lehmer steps: Euclid on the leading 120 bits gives a reduction
/// with entries below 2^60, which is applied to the full pair (undoing its last quotients if needed).
fn hgcd_lehmer(a: &Nat, b: &Nat, s: u64) -> (Reduction, Nat, Nat) {
    let (mut x, mut y) = (a.clone(), b.clone());
    let mut r = Reduction::identity();
    // bounded: every round takes a valid quotient or a division step
    while y.bits() > s {
        let mx = x.bits();
        if mx <= BASE_BITS {
            let (r2, nx, ny) = hgcd_base(x.to_u128().unwrap(), y.to_u128().unwrap(), s);
            return (r.then(&r2), nx, ny);
        }
        let sh = mx - BASE_BITS;
        // the leading parts reduce to half their size, but not below the target
        let target = (BASE_BITS / 2 + 1).max(s.saturating_sub(sh) + 1);
        let top = hgcd_base(x.shr(sh).to_u128().unwrap(), y.shr(sh).to_u128().unwrap(), target);
        match apply(top, sh, &x, &y) {
            Some((r2, nx, ny)) => {
                r = r.then(&r2);
                x = nx;
                y = ny;
            }
            None => step(&mut r, &mut x, &mut y),
        }
    }
    (r, x, y)
}

/// Pairs up to this size are reduced by Lehmer steps.
const LEHMER_BITS: u64 = 64 * 24;

/// The half-gcd reduction of the ordered pair (a, b), a >= b: beta < 2^(bits(a)/2 + 1) when the steps allow.
fn hgcd(a: &Nat, b: &Nat, budget: &mut dyn Budget) -> Res<(Reduction, Nat, Nat)> {
    let m = a.bits();
    let s = m / 2 + 1;
    if b.bits() <= s {
        return Ok((Reduction::identity(), a.clone(), b.clone()));
    }
    if m <= BASE_BITS {
        return Ok(hgcd_base(a.to_u128().unwrap(), b.to_u128().unwrap(), s));
    }
    if m <= LEHMER_BITS {
        return Ok(hgcd_lehmer(a, b, s));
    }
    budget.charge(crate::budget::limb_work(a.len(), 1).min(a.len() as u64) + 1)?;
    // the leading half: its reduction holds for (a, b) except perhaps for its last quotients
    let k = m / 2;
    let top = hgcd(&a.shr(k), &b.shr(k), budget)?;
    let (mut r, mut x, mut y) = match apply(top, k, a, b) {
        Some(v) => v,
        None => (Reduction::identity(), a.clone(), b.clone()),
    };
    // then leading parts of the reduced pair, sized so that their reduction ends near 2^s; division steps when a
    // reduction makes no progress
    let mut guard = 0;
    while y.bits() > s {
        guard += 1;
        let mx = x.bits();
        let k2 = (2 * s).saturating_sub(mx);
        let progressed = if guard < 4 && mx > k2 + BASE_BITS / 2 && 2 * s > mx {
            let top = hgcd(&x.shr(k2), &y.shr(k2), budget)?;
            match apply(top, k2, &x, &y) {
                Some((r2, nx, ny)) => {
                    r = r.then(&r2);
                    x = nx;
                    y = ny;
                    true
                }
                None => false,
            }
        } else {
            false
        };
        if !progressed {
            step(&mut r, &mut x, &mut y);
        }
    }
    Ok((r, x, y))
}

/// gcd(a, b).
pub fn gcd(a: &Nat, b: &Nat, budget: &mut dyn Budget) -> Res<Nat> {
    let (mut a, mut b) = if a >= b { (a.clone(), b.clone()) } else { (b.clone(), a.clone()) };
    loop {
        if b.is_zero() {
            return Ok(a);
        }
        if b.len() <= LEHMER_LIMBS {
            if a.len() > b.len() + 1 {
                a = a.divrem(&b).1;
                if a.is_zero() {
                    return Ok(b);
                }
            }
            budget.charge(crate::budget::limb_work(b.len(), 4))?;
            return Ok(lehmer_gcd(&a, &b));
        }
        // operands of very different sizes: a division first
        if a.bits() > b.bits() + 64 {
            let r = a.divrem(&b).1;
            a = std::mem::replace(&mut b, r);
            continue;
        }
        let (r, x, y) = hgcd(&a, &b, budget)?;
        if r.is_identity() {
            let rem = a.divrem(&b).1;
            a = std::mem::replace(&mut b, rem);
        } else {
            a = x;
            b = y;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::budget::Unlimited;
    use num_bigint::BigUint;
    use num_traits::Zero;

    fn big(x: &Nat) -> BigUint {
        BigUint::from_slice(&x.limbs.iter().flat_map(|&l| [l as u32, (l >> 32) as u32]).collect::<Vec<_>>())
    }

    fn euclid(mut a: BigUint, mut b: BigUint) -> BigUint {
        while !b.is_zero() {
            let r = &a % &b;
            a = std::mem::replace(&mut b, r);
        }
        a
    }

    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }
        fn nat(&mut self, limbs: usize) -> Nat {
            Nat::from_limbs((0..limbs).map(|_| self.next()).collect())
        }
    }

    #[test]
    fn hgcd_reduces_and_is_unimodular() {
        let mut rng = Rng(77);
        for limbs in [2usize, 3, 5, 9, 17, 40] {
            for _ in 0..20 {
                let (mut a, mut b) = (rng.nat(limbs), rng.nat(limbs));
                if a < b {
                    std::mem::swap(&mut a, &mut b);
                }
                let (r, x, y) = hgcd(&a, &b, &mut Unlimited).unwrap();
                let [m00, m01, m10, m11] = &r.m;
                assert_eq!(m00.mul(&x).add(&m01.mul(&y)), a);
                assert_eq!(m10.mul(&x).add(&m11.mul(&y)), b);
                assert!(x >= y);
                assert!(y.bits() <= a.bits() / 2 + 1, "{} {}", y.bits(), a.bits());
            }
        }
    }

    #[test]
    fn gcd_agrees_with_euclid() {
        let mut rng = Rng(12345);
        for limbs in [1usize, 2, 50, 199, 201, 300, 700] {
            for _ in 0..3 {
                let g = rng.nat(limbs / 3 + 1);
                let a = rng.nat(limbs).mul(&g);
                let b = rng.nat(limbs + limbs % 7).mul(&g);
                let got = gcd(&a, &b, &mut Unlimited).unwrap();
                assert_eq!(big(&got), euclid(big(&a), big(&b)), "{limbs} limbs");
            }
        }
        // larger sizes against the quadratic Lehmer gcd of mc-arith
        for limbs in [3000usize, 12000] {
            let g = rng.nat(limbs / 2);
            let (a, b) = (rng.nat(limbs).mul(&g), rng.nat(limbs - 5).mul(&g));
            assert_eq!(gcd(&a, &b, &mut Unlimited).unwrap(), lehmer_gcd(&a, &b));
        }
        // Fibonacci neighbours: the longest remainder sequences
        let (mut f0, mut f1) = (Nat::from_u64(1), Nat::from_u64(1));
        for _ in 0..30_000 {
            let f2 = f0.add(&f1);
            f0 = std::mem::replace(&mut f1, f2);
        }
        assert_eq!(gcd(&f1, &f0, &mut Unlimited).unwrap(), Nat::from_u64(1));
        assert_eq!(gcd(&Nat::zero(), &f0, &mut Unlimited).unwrap(), f0);
    }
}
