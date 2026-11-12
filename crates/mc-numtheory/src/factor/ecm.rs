//! Lenstra's elliptic-curve method on Montgomery curves B y^2 = x^3 + A x^2 + x in projective x-only coordinates
//! (decision J.12).
//!
//! Curves come from Suyama's parametrisation (sigma = 6, 7, ...), which gives a group order divisible by 12. Stage 1
//! multiplies the starting point by every prime power up to B1 with the Montgomery ladder; stage 2 is the standard
//! continuation over the primes q = m D +- j in (B1, B2] with D = 2310: baby steps `[j]Q` for j < D/2 coprime to D,
//! giant steps [m D]Q, and the product of X_(mD) Z_j - X_j Z_(mD) over the pairs, one gcd per curve.
use crate::budget::Budget;
use crate::ring::{Ring, RingTask};
use crate::sieve::primes_in;
use mc_arith::num::{Int, Nat};
use mc_eval::Res;

/// The giant step of stage 2.
const D: u64 = 2310;

/// A point (X : Z).
#[derive(Clone)]
struct Pt<E> {
    x: E,
    z: E,
}

/// The curve's arithmetic: a24 = (A + 2)/4.
struct Curve<'r, R: Ring> {
    r: &'r R,
    a24: R::E,
}

impl<R: Ring> Curve<'_, R> {
    /// [2]P.
    fn dbl(&self, p: &Pt<R::E>) -> Pt<R::E> {
        let r = self.r;
        let s = r.sqr(&r.add(&p.x, &p.z));
        let d = r.sqr(&r.sub(&p.x, &p.z));
        let t = r.sub(&s, &d);
        Pt { x: r.mul(&s, &d), z: r.mul(&t, &r.add(&d, &r.mul(&self.a24, &t))) }
    }
    /// P + Q given P - Q.
    fn add(&self, p: &Pt<R::E>, q: &Pt<R::E>, diff: &Pt<R::E>) -> Pt<R::E> {
        let r = self.r;
        let u = r.mul(&r.sub(&p.x, &p.z), &r.add(&q.x, &q.z));
        let v = r.mul(&r.add(&p.x, &p.z), &r.sub(&q.x, &q.z));
        let s = r.sqr(&r.add(&u, &v));
        let t = r.sqr(&r.sub(&u, &v));
        Pt { x: r.mul(&diff.z, &s), z: r.mul(&diff.x, &t) }
    }
    /// [k]P by the Montgomery ladder (k >= 1).
    fn mul(&self, p: &Pt<R::E>, k: u64) -> Pt<R::E> {
        if k == 1 {
            return p.clone();
        }
        let (mut r0, mut r1) = (p.clone(), self.dbl(p));
        let mut i = 63 - k.leading_zeros();
        while i > 0 {
            i -= 1;
            if (k >> i) & 1 == 1 {
                r0 = self.add(&r1, &r0, p);
                r1 = self.dbl(&r1);
            } else {
                r1 = self.add(&r1, &r0, p);
                r0 = self.dbl(&r0);
            }
        }
        r0
    }
}

/// a^(-1) mod n, or the gcd when it is not 1.
fn inverse(a: &Nat, n: &Nat) -> Result<Nat, Nat> {
    let (mut r0, mut r1) = (Int::from_nat(n.clone()), Int::from_nat(a.divrem(n).1));
    let (mut t0, mut t1) = (Int::from_i64(0), Int::from_i64(1));
    // Euclid with cofactors: bounded by the bits of n (moduli here are at most a few thousand bits)
    while !r1.is_zero() {
        let (q, rem) = r0.mag.divrem(&r1.mag);
        let q = Int::from_nat(q);
        let nr = Int::from_nat(rem);
        r0 = std::mem::replace(&mut r1, nr);
        let nt = t0.sub(&q.mul(&t1));
        t0 = std::mem::replace(&mut t1, nt);
    }
    if r0.mag != Nat::from_u64(1) {
        return Err(r0.mag);
    }
    Ok(t0.divrem_floor(n).1)
}

/// The outcome of one curve.
enum Outcome {
    Factor(Nat),
    Nothing,
}

/// ECM with bounds (B1, B2) on `curves` curves starting at sigma: a factor of n, or `None`.
pub struct Ecm<'b> {
    /// stage 1 bound
    pub b1: u64,
    /// stage 2 bound
    pub b2: u64,
    /// number of curves
    pub curves: u64,
    /// the first Suyama parameter
    pub sigma: u64,
    /// the budget
    pub b: &'b mut dyn Budget,
}

impl RingTask for Ecm<'_> {
    type Out = Res<Option<Nat>>;
    fn run<R: Ring>(self, r: &R) -> Res<Option<Nat>> {
        let b = self.b;
        let primes1 = primes_in(2, self.b1 + 1, b)?;
        let primes2 = primes_in(self.b1 + 1, self.b2 + 1, b)?;
        for c in 0..self.curves {
            if let Outcome::Factor(f) = curve(r, self.sigma + c, self.b1, &primes1, &primes2, b)? {
                return Ok(Some(f));
            }
        }
        Ok(None)
    }
}

/// gcd(x, n) as a factor outcome: a proper factor, or nothing.
fn check<R: Ring>(r: &R, x: &R::E, b: &mut dyn Budget) -> Res<Outcome> {
    let n = r.modulus();
    let g = crate::gcd::gcd(&r.to_nat(x), n, b)?;
    Ok(if g != Nat::from_u64(1) && g != *n { Outcome::Factor(g) } else { Outcome::Nothing })
}

fn curve<R: Ring>(r: &R, sigma: u64, b1: u64, primes1: &[u64], primes2: &[u64], b: &mut dyn Budget) -> Res<Outcome> {
    let n = r.modulus();
    // Suyama: u = sigma^2 - 5, v = 4 sigma, x0 = u^3, z0 = v^3, a24 = (v - u)^3 (3u + v) / (16 u^3 v)
    let s = r.elem_u64(sigma);
    let u = r.sub(&r.sqr(&s), &r.elem_u64(5));
    let v = r.mul(&r.elem_u64(4), &s);
    let u3 = r.mul(&r.sqr(&u), &u);
    let v3 = r.mul(&r.sqr(&v), &v);
    let vmu = r.sub(&v, &u);
    let num = r.mul(&r.mul(&r.sqr(&vmu), &vmu), &r.add(&r.mul(&r.elem_u64(3), &u), &v));
    let den = r.mul(&r.mul(&r.elem_u64(16), &u3), &v);
    let inv = match inverse(&r.to_nat(&den), n) {
        Ok(i) => r.elem(&i),
        Err(g) => return Ok(if g != *n { Outcome::Factor(g) } else { Outcome::Nothing }),
    };
    let cv = Curve { r, a24: r.mul(&num, &inv) };
    let mut q = Pt { x: u3, z: v3 };
    // stage 1: every prime power up to B1
    let units = crate::budget::limb_work(r.limbs(), 16) + 1;
    for (i, &p) in primes1.iter().enumerate() {
        let mut pk = p;
        while pk <= b1 / p {
            pk *= p;
        }
        q = cv.mul(&q, pk);
        if i % 64 == 63 {
            b.charge(units * 64)?;
        }
    }
    if let o @ Outcome::Factor(_) = check(r, &q.z, b)? {
        return Ok(o);
    }
    if primes2.is_empty() {
        return Ok(Outcome::Nothing);
    }
    // stage 2: baby steps [j]Q, j odd below D/2 and coprime to D
    let q2 = cv.dbl(&q);
    let half = (D / 2) as usize;
    let mut baby: Vec<Option<Pt<R::E>>> = vec![None; half];
    let (mut prev, mut cur) = (q.clone(), cv.add(&q2, &q, &q)); // [1]Q, [3]Q
    baby[1] = Some(prev.clone());
    let mut j = 3;
    while j < half {
        if gcd_small(j as u64, D) == 1 {
            baby[j] = Some(cur.clone());
        }
        // [j + 2]Q = [j]Q + [2]Q, difference [j - 2]Q
        let next = cv.add(&cur, &q2, &prev);
        prev = std::mem::replace(&mut cur, next);
        j += 2;
    }
    // giant steps [m D]Q for m from floor(B1 / D)
    let gd = cv.mul(&q, D);
    let m0 = (primes2[0] / D).max(1);
    let mut g_prev = if m0 == 1 { q.clone() } else { cv.mul(&q, (m0 - 1) * D) };
    let mut g_cur = cv.mul(&q, m0 * D);
    // for m0 = 1 the "previous" giant step [0 D]Q is the point at infinity; handle the first step by the ladder
    let mut m = m0;
    let mut acc = r.one();
    let mut idx = 0;
    let mut since = 0u64;
    while idx < primes2.len() {
        let lo = m * D - D / 2;
        let hi = m * D + D / 2;
        while idx < primes2.len() && primes2[idx] < hi {
            let p = primes2[idx];
            if p >= lo {
                let jj = p.abs_diff(m * D) as usize;
                if let Some(Some(bp)) = baby.get(jj) {
                    let t = r.sub(&r.mul(&g_cur.x, &bp.z), &r.mul(&bp.x, &g_cur.z));
                    acc = r.mul(&acc, &t);
                }
            }
            idx += 1;
        }
        // next giant step: [(m + 1) D]Q = [m D]Q + [D]Q with difference [(m - 1) D]Q
        let next = if m == 1 && m0 == 1 { cv.dbl(&gd) } else { cv.add(&g_cur, &gd, &g_prev) };
        g_prev = std::mem::replace(&mut g_cur, next);
        m += 1;
        since += 1;
        if since.is_multiple_of(16) {
            b.charge(units * 16)?;
        }
    }
    check(r, &acc, b)
}

fn gcd_small(a: u64, b: u64) -> u64 {
    crate::word::gcd_u64(a, b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::budget::Unlimited;
    use crate::ring::with_ring;

    #[test]
    fn finds_medium_factors() {
        // a 12-digit prime times a 25-digit prime
        let p = Nat::from_u64(100_000_000_003);
        let q = Nat::from_decimal("1000000000000000000000007").unwrap();
        let n = p.mul(&q);
        let f = with_ring(&n, Ecm { b1: 2000, b2: 200_000, curves: 60, sigma: 6, b: &mut Unlimited }).unwrap();
        let f = f.expect("a factor");
        assert!(f == p || f == q, "{}", f.to_decimal());
    }
}
