//! Arithmetic modulo a big odd number: the [`Ring`] interface the primality tests, modular powers and the
//! factoring methods are written against, with three implementations chosen by size ([`with_ring`]):
//!
//! - [`Mont`]: Montgomery multiplication (CIOS) on fixed arrays of `L` limbs, for moduli up to 32 limbs;
//! - [`Barrett`]: products by `mc-arith`'s multiplication (Karatsuba and the NTT) reduced by a precomputed Newton
//!   reciprocal ([`Divisor`]), for larger moduli.
use mc_arith::num::{Divisor, Nat};

/// Arithmetic in Z/nZ for an odd modulus n > 1.
pub trait Ring {
    /// An element (in the implementation's internal form).
    type E: Clone + PartialEq + std::fmt::Debug;
    /// The modulus.
    fn modulus(&self) -> &Nat;
    /// The element of x (any natural number).
    fn elem(&self, x: &Nat) -> Self::E;
    /// The least non-negative residue of an element.
    fn to_nat(&self, a: &Self::E) -> Nat;
    /// The element of a machine word.
    fn elem_u64(&self, x: u64) -> Self::E {
        self.elem(&Nat::from_u64(x))
    }
    /// Zero.
    fn zero(&self) -> Self::E;
    /// One.
    fn one(&self) -> Self::E;
    /// a b.
    fn mul(&self, a: &Self::E, b: &Self::E) -> Self::E;
    /// a^2.
    fn sqr(&self, a: &Self::E) -> Self::E {
        self.mul(a, a)
    }
    /// a + b.
    fn add(&self, a: &Self::E, b: &Self::E) -> Self::E;
    /// a - b.
    fn sub(&self, a: &Self::E, b: &Self::E) -> Self::E;
    /// a / 2 (n is odd).
    fn half(&self, a: &Self::E) -> Self::E;
    /// Is a zero?
    fn is_zero(&self, a: &Self::E) -> bool;
    /// The number of limbs of the modulus (for work estimates).
    fn limbs(&self) -> usize {
        self.modulus().len()
    }
    /// a^e for an exponent given by its bits (most significant first through [`Nat::bit`]).
    fn pow(&self, a: &Self::E, e: &Nat) -> Self::E {
        let mut r = self.one();
        let mut i = e.bits();
        while i > 0 {
            i -= 1;
            r = self.sqr(&r);
            if e.bit(i) {
                r = self.mul(&r, a);
            }
        }
        r
    }
}

/// Code generic over the ring, run by [`with_ring`] on the implementation that suits the modulus.
pub trait RingTask {
    /// The result.
    type Out;
    /// Run on ring `r`.
    fn run<R: Ring>(self, r: &R) -> Self::Out;
}

/// Run `task` with the ring for the odd modulus `n > 1`: Montgomery arithmetic on a fixed number of limbs up to 32
/// limbs (the size is rounded up to one of 1, 2, 3, 4, 6, 8, 12, 16, 24, 32), Newton reciprocals above.
pub fn with_ring<T: RingTask>(n: &Nat, task: T) -> T::Out {
    match n.len() {
        1 => task.run(&Mont::<1>::new(n)),
        2 => task.run(&Mont::<2>::new(n)),
        3 => task.run(&Mont::<3>::new(n)),
        4 => task.run(&Mont::<4>::new(n)),
        5..=6 => task.run(&Mont::<6>::new(n)),
        7..=8 => task.run(&Mont::<8>::new(n)),
        9..=12 => task.run(&Mont::<12>::new(n)),
        13..=16 => task.run(&Mont::<16>::new(n)),
        17..=24 => task.run(&Mont::<24>::new(n)),
        25..=32 => task.run(&Mont::<32>::new(n)),
        _ => task.run(&Barrett::new(n)),
    }
}

/// Montgomery arithmetic modulo an odd n of at most `L` limbs, R = 2^(64 L).
pub struct Mont<const L: usize> {
    n: [u64; L],
    nat: Nat,
    /// -n^(-1) mod 2^64
    inv: u64,
    /// R^2 mod n
    r2: [u64; L],
    one: [u64; L],
}

/// a >= b on L limbs.
#[inline]
fn ge<const L: usize>(a: &[u64; L], b: &[u64; L]) -> bool {
    for i in (0..L).rev() {
        if a[i] != b[i] {
            return a[i] > b[i];
        }
    }
    true
}

/// a - b on L limbs (wrapping), with the borrow out.
#[inline]
fn sub_limbs<const L: usize>(a: &[u64; L], b: &[u64; L]) -> ([u64; L], bool) {
    let mut r = [0u64; L];
    let mut borrow = false;
    for i in 0..L {
        let (d1, b1) = a[i].overflowing_sub(b[i]);
        let (d2, b2) = d1.overflowing_sub(borrow as u64);
        r[i] = d2;
        borrow = b1 || b2;
    }
    (r, borrow)
}

/// a + b on L limbs (wrapping), with the carry out.
#[inline]
fn add_limbs<const L: usize>(a: &[u64; L], b: &[u64; L]) -> ([u64; L], bool) {
    let mut r = [0u64; L];
    let mut carry = false;
    for i in 0..L {
        let (s1, c1) = a[i].overflowing_add(b[i]);
        let (s2, c2) = s1.overflowing_add(carry as u64);
        r[i] = s2;
        carry = c1 || c2;
    }
    (r, carry)
}

fn to_array<const L: usize>(x: &Nat) -> [u64; L] {
    let mut a = [0u64; L];
    a[..x.limbs.len()].copy_from_slice(&x.limbs);
    a
}

impl<const L: usize> Mont<L> {
    /// The arithmetic modulo the odd n > 1 of at most L limbs.
    pub fn new(n: &Nat) -> Mont<L> {
        debug_assert!(n.len() <= L && n.bit(0));
        let n0 = n.limbs[0];
        let mut inv = n0;
        for _ in 0..5 {
            inv = inv.wrapping_mul(2u64.wrapping_sub(n0.wrapping_mul(inv)));
        }
        let r_mod = Nat::pow2(64 * L as u64).divrem(n).1;
        let r2 = r_mod.square().divrem(n).1;
        Mont { n: to_array(n), nat: n.clone(), inv: inv.wrapping_neg(), r2: to_array(&r2), one: to_array(&r_mod) }
    }

    /// a b R^(-1) mod n (CIOS: interleaved multiplication and reduction).
    #[inline]
    fn redc_mul(&self, a: &[u64; L], b: &[u64; L]) -> [u64; L] {
        let mut t = [0u64; L];
        let mut hi: u64 = 0;
        for &bi in b.iter() {
            let mut c: u64 = 0;
            for j in 0..L {
                let s = t[j] as u128 + a[j] as u128 * bi as u128 + c as u128;
                t[j] = s as u64;
                c = (s >> 64) as u64;
            }
            let s = hi as u128 + c as u128;
            hi = s as u64;
            let hi2 = (s >> 64) as u64;
            let m = t[0].wrapping_mul(self.inv);
            let s = t[0] as u128 + m as u128 * self.n[0] as u128;
            let mut c = (s >> 64) as u64;
            for j in 1..L {
                let s = t[j] as u128 + m as u128 * self.n[j] as u128 + c as u128;
                t[j - 1] = s as u64;
                c = (s >> 64) as u64;
            }
            let s = hi as u128 + c as u128;
            t[L - 1] = s as u64;
            hi = hi2 + (s >> 64) as u64;
        }
        if hi != 0 || ge(&t, &self.n) {
            sub_limbs(&t, &self.n).0
        } else {
            t
        }
    }
}

impl<const L: usize> Ring for Mont<L> {
    type E = [u64; L];
    fn modulus(&self) -> &Nat {
        &self.nat
    }
    fn elem(&self, x: &Nat) -> [u64; L] {
        let r = if *x >= self.nat { x.divrem(&self.nat).1 } else { x.clone() };
        self.redc_mul(&to_array(&r), &self.r2)
    }
    fn to_nat(&self, a: &[u64; L]) -> Nat {
        let mut one = [0u64; L];
        one[0] = 1;
        Nat::from_limbs(self.redc_mul(a, &one).to_vec())
    }
    fn zero(&self) -> [u64; L] {
        [0; L]
    }
    fn one(&self) -> [u64; L] {
        self.one
    }
    #[inline]
    fn mul(&self, a: &[u64; L], b: &[u64; L]) -> [u64; L] {
        self.redc_mul(a, b)
    }
    #[inline]
    fn add(&self, a: &[u64; L], b: &[u64; L]) -> [u64; L] {
        let (s, c) = add_limbs(a, b);
        if c || ge(&s, &self.n) {
            sub_limbs(&s, &self.n).0
        } else {
            s
        }
    }
    #[inline]
    fn sub(&self, a: &[u64; L], b: &[u64; L]) -> [u64; L] {
        let (d, borrow) = sub_limbs(a, b);
        if borrow {
            add_limbs(&d, &self.n).0
        } else {
            d
        }
    }
    fn half(&self, a: &[u64; L]) -> [u64; L] {
        let (v, carry) = if a[0] & 1 == 1 { add_limbs(a, &self.n) } else { (*a, false) };
        let mut r = [0u64; L];
        for i in 0..L {
            let next = if i + 1 < L { v[i + 1] } else { carry as u64 };
            r[i] = (v[i] >> 1) | (next << 63);
        }
        r
    }
    fn is_zero(&self, a: &[u64; L]) -> bool {
        a.iter().all(|&w| w == 0)
    }
}

/// Arithmetic modulo a large n: products by `mc-arith`'s multiplication, reduced by a Newton reciprocal.
pub struct Barrett {
    n: Nat,
    d: Divisor,
}

impl Barrett {
    /// The arithmetic modulo n > 1.
    pub fn new(n: &Nat) -> Barrett {
        Barrett { n: n.clone(), d: Divisor::new(n) }
    }
    fn reduce(&self, x: &Nat) -> Nat {
        if *x < self.n {
            x.clone()
        } else {
            self.d.divrem(x).1
        }
    }
}

impl Ring for Barrett {
    type E = Nat;
    fn modulus(&self) -> &Nat {
        &self.n
    }
    fn elem(&self, x: &Nat) -> Nat {
        if x.bits() <= 2 * self.n.bits() {
            self.reduce(x)
        } else {
            x.divrem(&self.n).1
        }
    }
    fn to_nat(&self, a: &Nat) -> Nat {
        a.clone()
    }
    fn zero(&self) -> Nat {
        Nat::zero()
    }
    fn one(&self) -> Nat {
        Nat::from_u64(1)
    }
    fn mul(&self, a: &Nat, b: &Nat) -> Nat {
        self.reduce(&a.mul(b))
    }
    fn sqr(&self, a: &Nat) -> Nat {
        self.reduce(&a.square())
    }
    fn add(&self, a: &Nat, b: &Nat) -> Nat {
        let s = a.add(b);
        if s >= self.n {
            s.sub(&self.n)
        } else {
            s
        }
    }
    fn sub(&self, a: &Nat, b: &Nat) -> Nat {
        if a >= b {
            a.sub(b)
        } else {
            a.add(&self.n).sub(b)
        }
    }
    fn half(&self, a: &Nat) -> Nat {
        if a.bit(0) {
            a.add(&self.n).shr(1)
        } else {
            a.shr(1)
        }
    }
    fn is_zero(&self, a: &Nat) -> bool {
        a.is_zero()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use num_bigint::BigUint;

    fn big(x: &Nat) -> BigUint {
        BigUint::from_slice(&x.limbs.iter().flat_map(|&l| [l as u32, (l >> 32) as u32]).collect::<Vec<_>>())
    }

    struct Check(Vec<Nat>);

    impl RingTask for Check {
        type Out = ();
        fn run<R: Ring>(self, r: &R) {
            let n = big(r.modulus());
            for a in &self.0 {
                for b in &self.0 {
                    let (ea, eb) = (r.elem(a), r.elem(b));
                    let (ba, bb) = (big(a) % &n, big(b) % &n);
                    assert_eq!(big(&r.to_nat(&r.mul(&ea, &eb))), (&ba * &bb) % &n);
                    assert_eq!(big(&r.to_nat(&r.add(&ea, &eb))), (&ba + &bb) % &n);
                    assert_eq!(big(&r.to_nat(&r.sub(&ea, &eb))), (&ba + &n - &bb) % &n);
                }
                let ea = r.elem(a);
                let h = r.to_nat(&r.half(&ea));
                assert_eq!((big(&h) * 2u32) % &n, big(a) % &n);
                let e = Nat::from_u64(12345);
                assert_eq!(big(&r.to_nat(&r.pow(&ea, &e))), big(a).modpow(&BigUint::from(12345u32), &n));
            }
        }
    }

    #[test]
    fn rings_agree_with_num_bigint() {
        let mut seed = 0x1234_5678_9abc_def0u64;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        for limbs in [1usize, 2, 3, 5, 7, 11, 16, 20, 31, 40, 200] {
            let mut n = Nat::from_limbs((0..limbs).map(|_| next()).collect());
            if !n.bit(0) {
                n = n.add_u64(1);
            }
            let vals: Vec<Nat> = (0..4)
                .map(|i| Nat::from_limbs((0..limbs + i % 3).map(|_| next()).collect()))
                .chain([Nat::zero(), Nat::from_u64(1), n.sub_u64(1), n.clone()])
                .collect();
            with_ring(&n, Check(vals));
        }
    }
}
