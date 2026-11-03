//! Numbers spike: an own natural-number layer (schoolbook, Karatsuba and three-prime NTT multiplication, Newton
//! division and square root, divide-and-conquer radix conversion), ball arithmetic on top of it, binary-splitting
//! constants and adaptive precision (semantics 2.3 to 2.6).
//!
//! With the `v1` feature the crate also compiles v1's number modules (`bigint`, `bigfloat`, `interrupt`, copied
//! unchanged by build.rs) so the harness can compare against them. They must sit at the crate root because their
//! code refers to `crate::bigint` and `crate::interrupt`.

pub mod adaptive;
pub mod ball;
pub mod consts;
pub mod int;
pub mod nat;
pub mod ntt;

#[cfg(feature = "v1")]
#[allow(warnings, clippy::all)]
pub mod bigint {
    include!(concat!(env!("OUT_DIR"), "/v1_bigint.rs"));
}
#[cfg(feature = "v1")]
#[allow(warnings, clippy::all)]
pub mod bigfloat {
    include!(concat!(env!("OUT_DIR"), "/v1_bigfloat.rs"));
}
#[cfg(feature = "v1")]
#[allow(warnings, clippy::all)]
pub mod interrupt {
    include!(concat!(env!("OUT_DIR"), "/v1_interrupt.rs"));
}

use std::sync::atomic::{AtomicBool, Ordering};

/// Cooperative stop flag (semantics 10): long loops poll it and return early with an unspecified value, which the
/// caller must discard. The harness sets it from a watchdog when a case exceeds its time limit.
pub static STOP: AtomicBool = AtomicBool::new(false);

#[inline]
pub fn stopped() -> bool {
    STOP.load(Ordering::Relaxed)
}

/// SplitMix64: a small, fast, reproducible generator for test inputs.
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
}

/// Representation-independent fingerprint of a natural number given as little-endian 64-bit limbs without
/// trailing zeros: every implementation is reduced to these limbs before hashing, so equal values hash equally.
pub fn fingerprint(limbs: &[u64]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325 ^ limbs.len() as u64;
    for &l in limbs {
        h = (h ^ l).wrapping_mul(0x0000_0100_0000_01b3);
        h ^= h >> 29;
    }
    h
}
