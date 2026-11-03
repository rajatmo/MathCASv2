//! Parallel-evaluation spike (plan section 5a, semantics 18).
//!
//! - [`store`]: spike 1's hash-consed store made concurrent: a segmented node arena with per-thread id chunks
//!   ([`arena`]) and three interning tables ([`table`]): one lock, sharded locks, and lock-free lookup with sharded
//!   insertion.
//! - [`memo`]: a concurrent, lossy memo of normal forms (semantics 18.5).
//! - [`eval`]: a small rule language evaluated on a work-stealing pool, strict arguments in parallel under a
//!   granularity rule, with speculative effect ordering (18.2 item 3) and split random streams (18.2 item 5).
//! - [`work`]: the class-M workloads; [`seq`]: spike 1's sequential store and its R10/R11 workloads, for the
//!   single-thread regression check.
pub mod arena;
pub mod eval;
pub mod memo;
pub mod rng;
pub mod seq;
pub mod store;
pub mod table;
pub mod work;

/// A term handle: a stored id, or (top bit set) an immediate 31-bit integer.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[repr(transparent)]
pub struct Term(pub u32);

pub const IMM: u32 = 1 << 31;
pub const IMM_MIN: i64 = -(1 << 30);
pub const IMM_MAX: i64 = (1 << 30) - 1;

#[inline]
pub fn imm_value(t: Term) -> i64 {
    (((t.0 << 1) as i32) >> 1) as i64
}

pub const K: u64 = 0x9E37_79B9_7F4A_7C15;

/// A 64-bit finaliser (SplitMix64's): spreads every input bit over the whole word, so tables may index by low or high
/// bits.
#[inline]
pub fn finalize(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// The hash combiner of spike 1's store (so structural hashes agree with it).
#[inline]
pub fn mix(h: u64, v: u64) -> u64 {
    (h.rotate_left(5) ^ v).wrapping_mul(K)
}

pub use seq::str_hash;
