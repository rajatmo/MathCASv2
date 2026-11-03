//! Pollard's rho method with Brent's cycle detection: x -> x^2 + c, products of differences accumulated over blocks
//! of 128 steps between gcds, and a step-by-step replay of the last block when the gcd overshoots to n.
use crate::budget::Budget;
use crate::ring::{Ring, RingTask};
use crate::word::{gcd_u64, Mont64};
use mc_arith::num::Nat;
use mc_eval::Res;

/// Steps between two gcds.
const BLOCK: u64 = 128;

/// A nontrivial factor of the odd composite word n (not a prime power of a tiny prime).
pub fn rho_u64(n: u64, b: &mut dyn Budget) -> Res<u64> {
    let m = Mont64::new(n);
    let mut c = 1u64;
    loop {
        if let Some(f) = brent_u64(&m, m.to_mont(c), b)? {
            return Ok(f);
        }
        c += 1;
    }
}

/// One run of Brent's method with constant c (Montgomery form); `None` when the cycle closes without a factor.
fn brent_u64(m: &Mont64, c: u64, b: &mut dyn Budget) -> Res<Option<u64>> {
    let n = m.n;
    let f = |x: u64| m.add(m.mul(x, x), c);
    let (mut y, mut r, mut q) = (m.to_mont(2), 1u64, m.one());
    let (mut x, mut ys) = (y, y);
    let mut g = 1;
    // bounded: the sequence modulo the smallest prime factor p cycles within about sqrt(p) steps, and r doubles
    while g == 1 {
        x = y;
        for _ in 0..r {
            y = f(y);
        }
        let mut k = 0;
        while k < r && g == 1 {
            ys = y;
            for _ in 0..BLOCK.min(r - k) {
                y = f(y);
                q = m.mul(q, x.abs_diff(y));
            }
            g = gcd_u64(m.from_mont(q), n);
            k += BLOCK;
        }
        b.charge(r / 512 + 1)?;
        r *= 2;
    }
    if g == n {
        // replay the last block one step at a time
        loop {
            ys = f(ys);
            g = gcd_u64(x.abs_diff(ys), n);
            if g > 1 {
                break;
            }
        }
    }
    Ok((g != n).then_some(g))
}

/// Brent's method on a big n for at most `steps` iterations per constant, two constants: a factor or `None`.
pub struct RhoBig<'b> {
    /// the iteration limit
    pub steps: u64,
    /// the budget
    pub b: &'b mut dyn Budget,
}

impl RingTask for RhoBig<'_> {
    type Out = Res<Option<Nat>>;
    fn run<R: Ring>(self, r: &R) -> Res<Option<Nat>> {
        let n = r.modulus().clone();
        let b = self.b;
        for c in 1..=2u64 {
            let ce = r.elem_u64(c);
            let f = |x: &R::E| r.add(&r.sqr(x), &ce);
            let (mut y, mut len, mut q) = (r.elem_u64(2), 1u64, r.one());
            let (mut x, mut ys) = (y.clone(), y.clone());
            let mut g = Nat::from_u64(1);
            let mut total = 0u64;
            let one = Nat::from_u64(1);
            while g == one && total < self.steps {
                x = y.clone();
                for _ in 0..len {
                    y = f(&y);
                }
                let mut k = 0;
                while k < len && g == one {
                    ys = y.clone();
                    for _ in 0..BLOCK.min(len - k) {
                        y = f(&y);
                        q = r.mul(&q, &r.sub(&x, &y));
                    }
                    g = crate::gcd::gcd(&r.to_nat(&q), &n, b)?;
                    k += BLOCK;
                }
                b.charge(crate::budget::limb_work(r.limbs(), len) / 4 + 1)?;
                total += 2 * len;
                len *= 2;
            }
            if g == n {
                // replay one step at a time; a cycle modulo every factor at once gives nothing with this constant
                loop {
                    ys = f(&ys);
                    g = crate::gcd::gcd(&r.to_nat(&r.sub(&x, &ys)), &n, b)?;
                    if g != one {
                        break;
                    }
                }
            }
            if g != one && g != n {
                return Ok(Some(g));
            }
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::budget::Unlimited;
    use crate::ring::with_ring;

    #[test]
    fn splits_words() {
        for n in [15u64, 91, 8051, 10403, 1_000_000_016_000_000_063, 4_294_967_297, 18_446_744_073_709_551_613] {
            let f = rho_u64(n, &mut Unlimited).unwrap();
            assert!(f > 1 && f < n && n % f == 0, "{n}: {f}");
        }
    }

    #[test]
    fn splits_big() {
        // (2^61 - 1) * 1000003
        let n = Nat::from_u64((1 << 61) - 1).mul(&Nat::from_u64(1_000_003));
        let f = with_ring(&n, RhoBig { steps: 1 << 20, b: &mut Unlimited }).unwrap().unwrap();
        assert!(f == Nat::from_u64(1_000_003) || f == Nat::from_u64((1 << 61) - 1));
    }
}
