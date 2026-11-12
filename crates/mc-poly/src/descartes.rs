//! Real root isolation of integer polynomials in one variable (decision 3B.7; moved here from `mc-solve` by item
//! F2 so that `mc-algebra` and the decision procedures can use it below `mc-solve`).
//!
//! Descartes' rule of signs with bisection (the Vincent–Collins–Akritas method): the roots in (0, B) and (-B, 0),
//! B a power of two above the Cauchy bound, are mapped to (0, 1); an interval whose transformed polynomial
//! `(x+1)^n q(1/(x+1))` has no sign variation holds no root, one variation exactly one root, otherwise it is
//! bisected (a root at the midpoint is recorded exactly). Every interval is open with rational endpoints that are
//! not roots, or a single exact rational root. Refinement bisects with exact sign evaluations.
use mc_arith::num::{Int, Nat, Rat};
use crate::work::{Halt, Meter, WorkResult};
use crate::Z;
use std::cmp::Ordering;

/// An isolating interval of a real root: the exact root `lo` when `lo == hi`, else the open interval (lo, hi).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Interval {
    /// the lower end
    pub lo: Rat,
    /// the upper end
    pub hi: Rat,
}

impl Interval {
    /// Is it a single exact root?
    pub fn is_exact(&self) -> bool {
        self.lo == self.hi
    }
    /// The midpoint.
    pub fn mid(&self) -> Rat {
        self.lo.add(&self.hi).mul(&Rat::new(Int::from_i64(1), Int::from_i64(2)))
    }
}

/// The sign of `p` (dense, index = degree) at the rational x.
pub fn sign_at(p: &[Z], x: &Rat) -> i32 {
    // sum c_i a^i b^(n-i) for x = a/b, b > 0
    let a = Z::from_int(x.num.clone());
    let b = Z::from_int(Int::from_nat(x.den.clone()));
    let n = p.len();
    let mut acc = Z::ZERO;
    let mut bpow = Z::ONE;
    // Horner in a with powers of b: acc = (((c_n) a + c_{n-1} b) a + c_{n-2} b^2) ...
    for (k, c) in p.iter().enumerate().rev() {
        if k == n - 1 {
            acc = c.clone();
        } else {
            bpow = bpow.mul(&b);
            acc = acc.mul(&a).add(&c.mul(&bpow));
        }
    }
    acc.signum()
}

/// The sign of p just right of x: p(x), or p'(x) at a (simple) root. Endpoints of isolating intervals may be
/// exact roots found at bisection points.
fn sign_right(p: &[Z], x: &Rat) -> i32 {
    match sign_at(p, x) {
        0 => {
            let d: Vec<Z> = p.iter().enumerate().skip(1).map(|(i, c)| c.mul_i64(i as i64)).collect();
            sign_at(&d, x)
        }
        s => s,
    }
}

/// Sign variations of a coefficient sequence (zeros skipped).
fn variations(c: &[Z]) -> usize {
    let mut last = 0;
    let mut v = 0;
    for x in c {
        let s = x.signum();
        if s != 0 {
            if last != 0 && s != last {
                v += 1;
            }
            last = s;
        }
    }
    v
}

/// p(x + 1) in place (Taylor shift).
fn shift_one(p: &mut [Z], m: &mut Meter) -> WorkResult<()> {
    let n = p.len();
    for i in 0..n {
        m.step((n - i) as u64)?;
        for j in (i..n - 1).rev() {
            let s = p[j].add(&p[j + 1]);
            p[j] = s;
        }
    }
    Ok(())
}

/// Upper bound of the number of roots in (0, 1): variations of (x+1)^n q(1/(x+1)).
fn descartes01(q: &[Z], m: &mut Meter) -> WorkResult<usize> {
    let mut r: Vec<Z> = q.iter().rev().cloned().collect();
    shift_one(&mut r, m)?;
    Ok(variations(&r))
}

fn pow2(k: u64) -> Z {
    Z::from_int(Int::from_nat(Nat::from_u64(1).shl(k)))
}

fn rat_pow2(c: &Z, k: i64) -> Rat {
    // c * 2^k
    if k >= 0 {
        Rat::from_int(c.mul(&pow2(k as u64)).to_int())
    } else {
        Rat::new(c.to_int(), pow2((-k) as u64).to_int())
    }
}

/// The isolating intervals of the real roots of a square-free p (dense, nonzero), in increasing order.
pub fn isolate(p: &[Z], m: &mut Meter) -> WorkResult<Vec<Interval>> {
    let mut p: Vec<Z> = p.to_vec();
    while p.last().is_some_and(|c| c.is_zero()) {
        p.pop();
    }
    let mut out = vec![];
    let mut zero = false;
    while p.len() > 1 && p[0].is_zero() {
        p.remove(0);
        zero = true;
    }
    if p.len() <= 1 {
        if zero {
            out.push(Interval { lo: Rat::from_i64(0), hi: Rat::from_i64(0) });
        }
        return Ok(out);
    }
    // B = 2^k above 1 + max |a_i / a_n|
    let lead = p.last().expect("nonzero").bits();
    let k = p.iter().map(|c| c.bits()).max().unwrap_or(0).saturating_sub(lead) + 2;
    let neg: Vec<Z> = p.iter().enumerate().map(|(i, c)| if i % 2 == 1 { c.neg() } else { c.clone() }).collect();
    let mut negatives: Vec<Interval> = positive_roots(&neg, k, m)?
        .into_iter()
        .map(|iv| Interval { lo: iv.hi.neg(), hi: iv.lo.neg() })
        .collect();
    negatives.reverse();
    out.extend(negatives);
    if zero {
        out.push(Interval { lo: Rat::from_i64(0), hi: Rat::from_i64(0) });
    }
    out.extend(positive_roots(&p, k, m)?);
    Ok(out)
}

/// The roots of p in (0, 2^k), increasing.
fn positive_roots(p: &[Z], k: u64, m: &mut Meter) -> WorkResult<Vec<Interval>> {
    // q(x) = p(2^k x)
    let q: Vec<Z> = p.iter().enumerate().map(|(i, c)| c.mul(&pow2(k * i as u64))).collect();
    // (q, c, j): q(x) proportional to p(2^k (c + x) / 2^j), interval (c/2^j, (c+1)/2^j) times 2^k
    let mut stack: Vec<(Vec<Z>, Z, u64)> = vec![(q, Z::ZERO, 0)];
    let mut out = vec![];
    while let Some((q, c, j)) = stack.pop() {
        let n = q.len() - 1;
        m.step(n as u64 + 1)?;
        if j > 100_000 {
            return Err(Halt::Memory);
        }
        let v = descartes01(&q, m)?;
        let at = |num: &Z, jj: u64| rat_pow2(num, k as i64 - jj as i64);
        if v == 0 {
            continue;
        }
        if v == 1 {
            out.push(Interval { lo: at(&c, j), hi: at(&c.add(&Z::ONE), j) });
            continue;
        }
        // left half: 2^n q(x/2); right half: left(x + 1)
        let left: Vec<Z> = q.iter().enumerate().map(|(i, a)| a.mul(&pow2((n - i) as u64))).collect();
        let mut right = left.clone();
        shift_one(&mut right, m)?;
        let c2 = c.mul_i64(2);
        if right[0].is_zero() {
            // the midpoint is a root
            out.push(Interval { lo: at(&c2.add(&Z::ONE), j + 1), hi: at(&c2.add(&Z::ONE), j + 1) });
            right.remove(0);
            right.push(Z::ZERO);
            while right.last().is_some_and(|x| x.is_zero()) && right.len() > 1 {
                right.pop();
            }
        }
        stack.push((right, c2.add(&Z::ONE), j + 1));
        stack.push((left, c2, j + 1));
    }
    out.sort_by(|a, b| a.lo.cmp_value(&b.lo));
    Ok(out)
}

/// Bisect an isolating interval of a root of p until its width is at most `width`.
pub fn refine(p: &[Z], iv: &Interval, width: &Rat, m: &mut Meter) -> WorkResult<Interval> {
    let mut iv = iv.clone();
    if iv.is_exact() {
        return Ok(iv);
    }
    let slo = sign_right(p, &iv.lo);
    while iv.hi.sub(&iv.lo).cmp_value(width) == Ordering::Greater {
        m.step(p.len() as u64 * 4)?;
        let mid = iv.mid();
        let s = sign_at(p, &mid);
        if s == 0 {
            return Ok(Interval { lo: mid.clone(), hi: mid });
        }
        if s == slo {
            iv.lo = mid;
        } else {
            iv.hi = mid;
        }
    }
    Ok(iv)
}

/// Halve an interval once (the half that holds the root).
pub fn halve(p: &[Z], iv: &Interval) -> Interval {
    if iv.is_exact() {
        return iv.clone();
    }
    let mid = iv.mid();
    let s = sign_at(p, &mid);
    if s == 0 {
        return Interval { lo: mid.clone(), hi: mid };
    }
    if s == sign_right(p, &iv.lo) {
        Interval { lo: mid, hi: iv.hi.clone() }
    } else {
        Interval { lo: iv.lo.clone(), hi: mid }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::work::Unlimited;

    fn z(v: &[i64]) -> Vec<Z> {
        v.iter().map(|&x| Z::S(x)).collect()
    }

    #[test]
    fn isolates() {
        let mut w = Unlimited;
        let mut m = Meter::new(&mut w);
        // (x - 1)(x + 2)(x - 1/2) * 2 = 2x^3 + x^2 - 5x + 2
        let ivs = isolate(&z(&[2, -5, 1, 2]), &mut m).unwrap();
        assert_eq!(ivs.len(), 3);
        let w = Rat::new(Int::from_i64(1), Int::from_i64(1 << 20));
        let r: Vec<f64> = ivs
            .iter()
            .map(|iv| {
                let iv = refine(&z(&[2, -5, 1, 2]), iv, &w, &mut m).unwrap();
                let x = iv.mid();
                x.num.to_i64().unwrap() as f64 / x.den.to_u128().unwrap() as f64
            })
            .collect();
        assert!((r[0] + 2.0).abs() < 1e-5 && (r[1] - 0.5).abs() < 1e-5 && (r[2] - 1.0).abs() < 1e-5);
        // x^5 - x - 1: one real root; x^2 + 1: none; x (x^2 - 2): three
        assert_eq!(isolate(&z(&[-1, -1, 0, 0, 0, 1]), &mut m).unwrap().len(), 1);
        assert_eq!(isolate(&z(&[1, 0, 1]), &mut m).unwrap().len(), 0);
        let ivs = isolate(&z(&[0, -2, 0, 1]), &mut m).unwrap();
        assert_eq!(ivs.len(), 3);
        assert!(ivs[1].is_exact());
        // Wilkinson-like: (x-1)(x-2)...(x-8)
        let mut p = z(&[1]);
        for r in 1..=8 {
            let mut q = vec![Z::ZERO; p.len() + 1];
            for (i, c) in p.iter().enumerate() {
                q[i + 1] = q[i + 1].add(c);
                q[i] = q[i].sub(&c.mul_i64(r));
            }
            p = q;
        }
        assert_eq!(isolate(&p, &mut m).unwrap().len(), 8);
    }
}
