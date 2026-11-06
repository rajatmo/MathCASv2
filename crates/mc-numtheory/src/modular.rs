//! Modular arithmetic and multiplicative functions: modular powers and inverses, the extended gcd, the Jacobi
//! symbol, primitive roots, multiplicative orders, and the functions computed from a factorisation (Euler's phi,
//! the divisors, the divisor sums, the Moebius function).
use crate::budget::Budget;
use crate::factor::{factor, prime_power};
use crate::prime::to_u64;
use crate::ring::{with_ring, Barrett, Ring, RingTask};
use mc_arith::num::{Int, Nat};
use mc_eval::Res;

/// base^e on a ring, charging the budget every 64 bits of the exponent.
fn ring_pow<R: Ring>(r: &R, base: &R::E, e: &Nat, b: &mut dyn Budget) -> Res<R::E> {
    let mut x = r.one();
    let mut i = e.bits();
    let units = crate::budget::limb_work(r.limbs(), 64) + 1;
    while i > 0 {
        let lo = i.saturating_sub(64);
        while i > lo {
            i -= 1;
            x = r.sqr(&x);
            if e.bit(i) {
                x = r.mul(&x, base);
            }
        }
        b.charge(units)?;
    }
    Ok(x)
}

struct PowTask<'a> {
    a: &'a Nat,
    e: &'a Nat,
    b: &'a mut dyn Budget,
}

impl RingTask for PowTask<'_> {
    type Out = Res<Nat>;
    fn run<R: Ring>(self, r: &R) -> Res<Nat> {
        let x = r.elem(self.a);
        let y = ring_pow(r, &x, self.e, self.b)?;
        Ok(r.to_nat(&y))
    }
}

/// a^e mod m for m >= 1 (a reduced first; 0^0 = 1).
pub fn pow_mod(a: &Nat, e: &Nat, m: &Nat, b: &mut dyn Budget) -> Res<Nat> {
    if *m == Nat::from_u64(1) {
        return Ok(Nat::zero());
    }
    if m.bit(0) {
        return with_ring(m, PowTask { a, e, b });
    }
    PowTask { a, e, b }.run(&Barrett::new(m))
}

/// (g, s, t) with g = gcd(a, b) = s a + t b (g >= 0), by Euclid with cofactors.
pub fn extended_gcd(a: &Int, b: &Int, budget: &mut dyn Budget) -> Res<(Nat, Int, Int)> {
    let (mut r0, mut r1) = (a.clone(), b.clone());
    let (mut s0, mut s1) = (Int::from_i64(1), Int::from_i64(0));
    let (mut t0, mut t1) = (Int::from_i64(0), Int::from_i64(1));
    let mut steps = 0u64;
    // bounded by the bits of the smaller operand
    while !r1.is_zero() {
        let (q, rem) = floor_divrem(&r0, &r1);
        r0 = std::mem::replace(&mut r1, rem);
        let ns = s0.sub(&q.mul(&s1));
        s0 = std::mem::replace(&mut s1, ns);
        let nt = t0.sub(&q.mul(&t1));
        t0 = std::mem::replace(&mut t1, nt);
        steps += 1;
        if steps.is_multiple_of(64) {
            budget.charge(crate::budget::limb_work(r0.mag.len(), 64))?;
        }
    }
    if r0.neg {
        Ok((r0.mag, s0.neg(), t0.neg()))
    } else {
        Ok((r0.mag, s0, t0))
    }
}

/// floor(a / b) and a - b floor(a / b) for b != 0.
pub fn floor_divrem(a: &Int, b: &Int) -> (Int, Int) {
    let (q, r) = a.mag.divrem(&b.mag);
    let (q, r) = (Int::new(a.neg != b.neg, q), Int::new(a.neg, r));
    if !r.is_zero() && r.neg != b.neg {
        (q.sub(&Int::from_i64(1)), r.add(b))
    } else {
        (q, r)
    }
}

/// a^(-1) mod m (m >= 1), if gcd(a, m) = 1.
pub fn inverse_mod(a: &Int, m: &Nat, b: &mut dyn Budget) -> Res<Option<Nat>> {
    let (g, s, _) = extended_gcd(a, &Int::from_nat(m.clone()), b)?;
    if g != Nat::from_u64(1) {
        return Ok(None);
    }
    Ok(Some(s.divrem_floor(m).1))
}

/// The Jacobi symbol (a / n) for odd n > 0.
pub fn jacobi(a: &Int, n: &Nat) -> i32 {
    let mut a = a.divrem_floor(n).1;
    let mut n = n.clone();
    let mut t = 1;
    // bounded: each round reduces the pair like Euclid
    while !a.is_zero() {
        let z = a.trailing_zeros();
        a = a.shr(z);
        let n8 = n.limbs[0] % 8;
        if z % 2 == 1 && (n8 == 3 || n8 == 5) {
            t = -t;
        }
        if a.limbs[0] % 4 == 3 && n.limbs[0] % 4 == 3 {
            t = -t;
        }
        std::mem::swap(&mut a, &mut n);
        a = a.divrem(&n).1;
    }
    if n == Nat::from_u64(1) {
        t
    } else {
        0
    }
}

/// The distinct primes of p - 1 for the primes p of a factorisation (for orders and primitive roots).
fn factor_minus_one(p: &Nat, b: &mut dyn Budget) -> Res<Vec<Nat>> {
    Ok(factor(&p.sub_u64(1), b)?.into_iter().map(|(q, _)| q).collect())
}

/// Is g a primitive root modulo the odd prime power p^k (k >= 1), given the primes of p - 1?
fn is_primitive(g: &Nat, p: &Nat, k: u64, qs: &[Nat], b: &mut dyn Budget) -> Res<bool> {
    let pm1 = p.sub_u64(1);
    let gp = g.divrem(p).1;
    if gp.is_zero() {
        return Ok(false);
    }
    for q in qs {
        if pow_mod(&gp, &pm1.div(q), p, b)? == Nat::from_u64(1) {
            return Ok(false);
        }
    }
    // modulo p^2 (and so every higher power) g^(p-1) must not be 1
    Ok(k == 1 || pow_mod(g, &pm1, &p.square(), b)? != Nat::from_u64(1))
}

/// The smallest primitive root modulo n >= 1, or `None` when there is none (n not 1, 2, 4, p^k or 2 p^k).
pub fn primitive_root(n: &Nat, b: &mut dyn Budget) -> Res<Option<Nat>> {
    // modulo 2 p^k: the root g of p^k when it is odd, else g + p^k (the usual construction; decision P6N.4)
    if !n.bit(0) && n.bit(1) && to_u64(n).is_none_or(|v| v > 4) {
        let odd = n.shr(1);
        let Some(g) = primitive_root_from(&odd, 2, b)? else { return Ok(None) };
        return Ok(Some(if g.bit(0) { g } else { g.add(&odd) }));
    }
    primitive_root_from(n, 2, b)
}

/// The smallest primitive root modulo n that is at least `start` (word-sized candidates), or `None` when n has
/// none.
pub fn primitive_root_from(n: &Nat, start: u64, b: &mut dyn Budget) -> Res<Option<Nat>> {
    match to_u64(n) {
        Some(1) if start <= 2 => return Ok(Some(Nat::zero())),
        Some(2) if start <= 2 => return Ok(Some(Nat::from_u64(1))),
        Some(4) if start <= 3 => return Ok(Some(Nat::from_u64(3))),
        Some(1 | 2 | 4) => return Ok(None),
        _ => {}
    }
    let (odd, twice) = if n.bit(0) {
        (n.clone(), false)
    } else if n.bit(1) {
        (n.shr(1), true)
    } else {
        return Ok(None);
    };
    let Some((p, k)) = prime_power(&odd, b)? else { return Ok(None) };
    let qs = factor_minus_one(&p, b)?;
    // modulo 2 p^k the root must also be odd
    let mut g = start.max(2);
    if twice && g.is_multiple_of(2) {
        g += 1;
    }
    loop {
        b.charge(1)?;
        if is_primitive(&Nat::from_u64(g), &p, k, &qs, b)? {
            return Ok(Some(Nat::from_u64(g)));
        }
        g += if twice { 2 } else { 1 };
    }
}

/// The factorisation of Carmichael's lambda(n) for the factorisation of n: lcm of lambda(p^e).
fn carmichael_factors(fs: &[(Nat, u64)], b: &mut dyn Budget) -> Res<Vec<(Nat, u64)>> {
    let mut acc: Vec<(Nat, u64)> = vec![];
    let mut merge = |q: Nat, e: u64| match acc.iter_mut().find(|(r, _)| *r == q) {
        Some(x) => x.1 = x.1.max(e),
        None => acc.push((q, e)),
    };
    for (p, e) in fs {
        if *p == Nat::from_u64(2) {
            let l = match e {
                1 => 0,
                2 => 1,
                _ => e - 2,
            };
            if l > 0 {
                merge(Nat::from_u64(2), l);
            }
            continue;
        }
        if *e > 1 {
            merge(p.clone(), e - 1);
        }
        for (q, f) in factor(&p.sub_u64(1), b)? {
            merge(q, f);
        }
    }
    acc.sort();
    Ok(acc)
}

/// The multiplicative order of a modulo n >= 2, or `None` when gcd(a, n) != 1.
pub fn multiplicative_order(a: &Nat, n: &Nat, b: &mut dyn Budget) -> Res<Option<Nat>> {
    if crate::gcd::gcd(a, n, b)? != Nat::from_u64(1) {
        return Ok(None);
    }
    let lam = carmichael_factors(&factor(n, b)?, b)?;
    let mut ord = lam.iter().fold(Nat::from_u64(1), |acc, (q, e)| acc.mul(&q.pow_u64(*e)));
    let one = Nat::from_u64(1);
    for (q, e) in &lam {
        for _ in 0..*e {
            let cand = ord.div(q);
            if pow_mod(a, &cand, n, b)? == one {
                ord = cand;
            } else {
                break;
            }
        }
    }
    Ok(Some(ord))
}

/// Euler's phi of n >= 1.
pub fn euler_phi(n: &Nat, b: &mut dyn Budget) -> Res<Nat> {
    let mut r = Nat::from_u64(1);
    for (p, e) in factor(n, b)? {
        r = r.mul(&p.pow_u64(e - 1)).mul(&p.sub_u64(1));
    }
    Ok(r)
}

/// The most divisors [`divisors`] lists.
pub const MAX_DIVISORS: u64 = 1 << 24;

/// The positive divisors of n >= 1 in increasing order, or `None` when there are more than [`MAX_DIVISORS`].
pub fn divisors(n: &Nat, b: &mut dyn Budget) -> Res<Option<Vec<Nat>>> {
    let fs = factor(n, b)?;
    let count = fs.iter().try_fold(1u64, |acc, (_, e)| acc.checked_mul(e + 1));
    if count.is_none_or(|c| c > MAX_DIVISORS) {
        return Ok(None);
    }
    let mut ds = vec![Nat::from_u64(1)];
    for (p, e) in fs {
        let len = ds.len();
        let mut pk = Nat::from_u64(1);
        for _ in 0..e {
            pk = pk.mul(&p);
            for i in 0..len {
                ds.push(ds[i].mul(&pk));
            }
            b.charge(len as u64 / 64 + 1)?;
        }
    }
    ds.sort();
    Ok(Some(ds))
}

/// sigma_k(n) = sum of d^k over the divisors d of n >= 1.
pub fn divisor_sigma(k: u64, n: &Nat, b: &mut dyn Budget) -> Res<Nat> {
    let mut r = Nat::from_u64(1);
    for (p, e) in factor(n, b)? {
        if k == 0 {
            r = r.mul_u64(e + 1);
            continue;
        }
        // (p^(k(e+1)) - 1) / (p^k - 1)
        let pk = p.pow_u64(k);
        let num = pk.pow_u64(e + 1).sub_u64(1);
        r = r.mul(&num.div(&pk.sub_u64(1)));
    }
    Ok(r)
}

/// The Moebius function of n >= 1.
pub fn moebius_mu(n: &Nat, b: &mut dyn Budget) -> Res<i32> {
    let fs = factor(n, b)?;
    if fs.iter().any(|(_, e)| *e > 1) {
        return Ok(0);
    }
    Ok(if fs.len() % 2 == 0 { 1 } else { -1 })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::budget::Unlimited;

    fn n(v: u64) -> Nat {
        Nat::from_u64(v)
    }

    #[test]
    fn powers_and_inverses() {
        let u = &mut Unlimited;
        assert_eq!(pow_mod(&n(2), &n(10), &n(1000), u).unwrap(), n(24));
        assert_eq!(pow_mod(&n(3), &n(0), &n(7), u).unwrap(), n(1));
        assert_eq!(pow_mod(&n(5), &n(117), &n(1), u).unwrap(), n(0));
        let m = Nat::pow2(127).sub_u64(1);
        // Fermat on a Mersenne prime, odd and even moduli
        assert_eq!(pow_mod(&n(3), &m.sub_u64(1), &m, u).unwrap(), n(1));
        assert_eq!(pow_mod(&n(3), &n(1000), &Nat::pow2(100), u).unwrap(), Nat::from_u64(3).pow_u64(1000).low_bits(100));
        assert_eq!(inverse_mod(&Int::from_i64(3), &n(11), u).unwrap(), Some(n(4)));
        assert_eq!(inverse_mod(&Int::from_i64(-3), &n(11), u).unwrap(), Some(n(7)));
        assert_eq!(inverse_mod(&Int::from_i64(6), &n(9), u).unwrap(), None);
        let (g, s, t) = extended_gcd(&Int::from_i64(240), &Int::from_i64(46), u).unwrap();
        assert_eq!(g, n(2));
        assert_eq!(s.mul(&Int::from_i64(240)).add(&t.mul(&Int::from_i64(46))), Int::from_i64(2));
        assert_eq!(jacobi(&Int::from_i64(1001), &n(9907)), -1);
        assert_eq!(jacobi(&Int::from_i64(-1), &n(7)), -1);
        assert_eq!(jacobi(&Int::from_i64(30), &n(15)), 0);
    }

    #[test]
    fn roots_orders_and_multiplicative_functions() {
        let u = &mut Unlimited;
        let pr = |v: u64| primitive_root(&n(v), &mut Unlimited).unwrap().map(|g| to_u64(&g).unwrap());
        assert_eq!(pr(1), Some(0));
        assert_eq!(pr(2), Some(1));
        assert_eq!(pr(4), Some(3));
        assert_eq!(pr(7), Some(3));
        assert_eq!(pr(8), None);
        assert_eq!(pr(15), None);
        // modulo 2 p^k: the root g of p^k, plus p^k when g is even (decision P6N.4)
        assert_eq!(pr(18), Some(11));
        assert_eq!(pr(10), Some(7));
        assert_eq!(pr(25), Some(2));
        assert_eq!(pr(40487), Some(5));
        // 40487: 5 is a primitive root modulo p but not modulo p^2, so 10 is the smallest modulo p^2
        assert_eq!(pr(40487 * 40487), Some(10));
        assert_eq!(pr(2 * 3u64.pow(5)), Some(2 + 3u64.pow(5)));
        assert_eq!(multiplicative_order(&n(2), &n(7), u).unwrap(), Some(n(3)));
        let brute = (1..10007u64).find(|&k| crate::word::pow_mod(10, k, 10007) == 1).unwrap();
        assert_eq!(multiplicative_order(&n(10), &n(10007), u).unwrap(), Some(n(brute)));
        assert_eq!(multiplicative_order(&n(2), &n(6), u).unwrap(), None);
        assert_eq!(euler_phi(&n(36), u).unwrap(), n(12));
        assert_eq!(euler_phi(&n(1), u).unwrap(), n(1));
        let ds: Vec<u64> = divisors(&n(60), u).unwrap().unwrap().iter().map(|d| to_u64(d).unwrap()).collect();
        assert_eq!(ds, vec![1, 2, 3, 4, 5, 6, 10, 12, 15, 20, 30, 60]);
        assert_eq!(divisor_sigma(1, &n(12), u).unwrap(), n(28));
        assert_eq!(divisor_sigma(0, &n(12), u).unwrap(), n(6));
        assert_eq!(divisor_sigma(2, &n(10), u).unwrap(), n(130));
        assert_eq!(moebius_mu(&n(30), u).unwrap(), -1);
        assert_eq!(moebius_mu(&n(12), u).unwrap(), 0);
        assert_eq!(moebius_mu(&n(1), u).unwrap(), 1);
    }

    #[test]
    fn brute_force_small_moduli() {
        let u = &mut Unlimited;
        for m in 1u64..200 {
            // the smallest primitive root by brute force
            let units: Vec<u64> = (1..=m).filter(|&a| crate::word::gcd_u64(a % m, m) == 1).collect();
            let phi = units.len() as u64;
            let order = |a: u64| (1..=phi).find(|&k| crate::word::pow_mod(a, k, m) == 1 % m).unwrap_or(0);
            let smallest = if m == 1 { Some(0) } else { units.iter().copied().find(|&a| order(a) == phi) };
            // modulo 2 p^k (m > 4): the root g of p^k when odd, else g + p^k
            let want = match smallest {
                Some(_) if m % 4 == 2 && m > 4 => {
                    let odd = m / 2;
                    let g = primitive_root(&n(odd), u).unwrap().map(|g| to_u64(&g).unwrap()).unwrap();
                    assert_eq!(order(if g % 2 == 1 { g } else { g + odd }), phi);
                    Some(if g % 2 == 1 { g } else { g + odd })
                }
                w => w,
            };
            let got = primitive_root(&n(m), u).unwrap().map(|g| to_u64(&g).unwrap());
            assert_eq!(got, want, "primitive root of {m}");
            assert_eq!(to_u64(&euler_phi(&n(m), u).unwrap()), Some(phi), "phi({m})");
            for &a in units.iter().take(5) {
                if m > 1 {
                    assert_eq!(multiplicative_order(&n(a), &n(m), u).unwrap(), Some(n(order(a))));
                }
            }
        }
    }
}
