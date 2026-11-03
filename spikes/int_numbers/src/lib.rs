//! Integration spike I3: spike 3's numbers (own 64-bit-limb naturals with schoolbook, Karatsuba and NTT
//! multiplication; balls with an own-mantissa midpoint and a 32-bit-mantissa radius) living as terms in spike 1b's
//! collected, hash-consed arena (generational, moving, creation order, symbols and 30-bit integers immediate).
//!
//! Modules copied from spike 3 (`spikes/numbers`): [`nat`], [`ntt`], [`int`], [`ball`], [`consts`] (unchanged apart
//! from three slice primitives of `nat` made public). New here:
//! - [`gcd`]: Lehmer's gcd on 64-bit limbs (canonical rationals need it; spike 3 had none);
//! - [`rat`]: canonical rationals outside the store (semantics 2.4), with Knuth's gcd-sparing sum and product;
//! - [`store`]: spike 1b's generational store with number payloads, in the placements and hashing modes compared;
//! - [`work`]: the workloads, each written once against the store and once against the bare numbers.
pub mod ball;
pub mod consts;
pub mod gcd;
pub mod int;
pub mod nat;
pub mod ntt;
pub mod rat;
pub mod store;
pub mod work;

use std::sync::atomic::{AtomicBool, Ordering};

/// Cooperative stop flag (semantics 10): long loops poll it and return early with an unspecified value, which the
/// caller must discard. The harness raises it from a watchdog.
pub static STOP: AtomicBool = AtomicBool::new(false);

#[inline]
pub fn stopped() -> bool {
    STOP.load(Ordering::Relaxed)
}

/// SplitMix64 (spike 3's generator): reproducible test inputs.
pub struct Rng(pub u64);

impl Rng {
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
    /// Uniform in 0..n (n > 0); the modulo bias is irrelevant for test data.
    pub fn below(&mut self, n: u64) -> u64 {
        self.next_u64() % n
    }
    /// Uniform in [0, 1).
    pub fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// Spike 3's representation-independent fingerprint of a natural number given as little-endian 64-bit limbs without
/// trailing zeros.
pub fn fingerprint(limbs: &[u64]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325 ^ limbs.len() as u64;
    for &l in limbs {
        h = (h ^ l).wrapping_mul(0x0000_0100_0000_01b3);
        h ^= h >> 29;
    }
    h
}

const FK: u64 = 0x9E37_79B9_7F4A_7C15;

/// Combines fingerprints (order-sensitive).
pub fn fp_mix(h: u64, v: u64) -> u64 {
    (h.rotate_left(5) ^ v).wrapping_mul(FK)
}

/// Store-independent fingerprints of values: the store's term fingerprint and the bare numbers' fingerprint use the
/// same functions, so a store result agrees with the bare result exactly when the values are equal.
pub mod fp {
    use super::{fingerprint, fp_mix};
    use crate::ball::Ball;
    use crate::int::Int;
    use crate::rat::Rat;

    pub fn int_parts(neg: bool, limbs: &[u64]) -> u64 {
        fp_mix(0x11 ^ neg as u64, fingerprint(limbs))
    }
    pub fn int(x: &Int) -> u64 {
        int_parts(x.neg, &x.mag.limbs)
    }
    pub fn rat_parts(num: u64, den: u64) -> u64 {
        fp_mix(fp_mix(0x22, num), den)
    }
    pub fn rat(x: &Rat) -> u64 {
        if x.is_int() {
            return int(&x.num);
        }
        rat_parts(int(&x.num), int_parts(false, &x.den.limbs))
    }
    pub fn ball_parts(neg: bool, man: &[u64], exp: i64, rm: u64, re: i64) -> u64 {
        let mut h = fp_mix(0x33 ^ neg as u64, fingerprint(man));
        h = fp_mix(h, exp as u64);
        h = fp_mix(h, rm);
        fp_mix(h, re as u64)
    }
    pub fn ball(b: &Ball) -> u64 {
        ball_parts(b.mid.neg, &b.mid.man.limbs, b.mid.exp, b.rad.m, b.rad.e)
    }
    pub fn sym(name: &str) -> u64 {
        fp_mix(0x44, crate::str_hash(name))
    }
    /// A compound: the head's fingerprint and the arguments' fingerprints in order.
    pub fn app(head: u64, args: impl IntoIterator<Item = u64>) -> u64 {
        let mut h = fp_mix(0x55, head);
        for a in args {
            h = fp_mix(h, a);
        }
        h
    }
}

pub fn str_hash(s: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.bytes() {
        h = (h ^ b as u64).wrapping_mul(0x100_0000_01b3);
    }
    h
}
