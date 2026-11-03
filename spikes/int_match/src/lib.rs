//! Integration piece I2: spike 2's matcher and rule dispatch on spike 4's concurrent store and work-stealing
//! evaluator.
//!
//! - [`heap`]: spike 4's concurrent store C (lock-free sharded interning, segmented arena with per-thread chunks),
//!   extended with what the matcher needs from spike 2's store: machine reals, the canonical order (semantics 8) and
//!   AC canonicalisation. Every operation takes `&self`, so a condition can allocate while other tasks match.
//! - [`conc`]: single-writer, many-reader building blocks (an append-only segmented vector and a key map), readable
//!   without locks or shared writes while the one writer extends them.
//! - [`pat`], [`matcher`]: spike 2's patterns and CPS matcher (decisions 2.1 to 2.3, 2.8), with conditions that may
//!   call the evaluator, and an abort channel through the continuation.
//! - [`index`]: spike 2's rule sets (discrimination tree, AC key-set filter, rule order through the index; decisions
//!   2.4, 2.5) rebuilt on [`conc`], so a definition made by the frontier task is safe to read concurrently.
//! - [`eval`]: spike 4's evaluator (speculative effect ordering with holes, lazy granularity, lossy seqlock memo
//!   stamped with the start epoch and read set; decisions 4.4 to 4.9) with spike 2's dispatch (argument-head rules,
//!   read sets through memo hits and conditions; decisions 2.6, 2.7).
//! - [`work`]: the workloads (spike 2's R4, R5, R13, R14 and S7, plus R14e and the mid-run redefinition cases).
pub mod arena;
pub mod conc;
pub mod eval;
pub mod heap;
pub mod index;
pub mod matcher;
pub mod memo;
pub mod pat;
pub mod table;
pub mod work;

#[cfg(test)]
mod tests;

use heap::{Heap, View};

/// A term handle: a stored id, or (top bit set) an immediate 31-bit integer.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
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

/// SplitMix64's finaliser.
#[inline]
pub fn finalize(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

#[inline]
pub fn mix(h: u64, v: u64) -> u64 {
    (h.rotate_left(5) ^ v).wrapping_mul(K)
}

/// The mask bit of a symbol index (spike 1: bit `index % 64`).
#[inline]
pub fn sym_bit(i: u32) -> u64 {
    1u64 << (i % 64)
}

pub fn str_hash(s: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.bytes() {
        h = (h ^ b as u64).wrapping_mul(0x100_0000_01b3);
    }
    h
}

/// Combine fingerprints in sequence (spike 2's).
pub fn combine(h: u64, v: u64) -> u64 {
    (h.rotate_left(7) ^ v).wrapping_mul(K)
}

/// Spike 2's structural fingerprint, built from symbol names and values, not ids: equal for equal terms in any store,
/// any schedule, and in spike 2's sequential store. Iterative.
pub fn fingerprint(st: &Heap, t: Term) -> u64 {
    enum Frame {
        Enter(Term),
        Exit(u64, usize),
    }
    let mut work = vec![Frame::Enter(t)];
    let mut vals: Vec<u64> = vec![];
    while let Some(f) = work.pop() {
        match f {
            Frame::Enter(t) => match st.view(t) {
                View::Int(v) => vals.push((v as u64).wrapping_mul(K) ^ 1),
                View::Real(r) => vals.push(r.to_bits().wrapping_mul(K) ^ 4),
                View::Sym(i) => vals.push(str_hash(st.name(i)) ^ 2),
                View::App(h, args) => {
                    work.push(Frame::Exit(fingerprint(st, h), args.len()));
                    for a in args.iter().rev() {
                        work.push(Frame::Enter(*a));
                    }
                }
            },
            Frame::Exit(hh, n) => {
                let start = vals.len() - n;
                let mut h = hh.wrapping_mul(K) ^ 3;
                for v in vals.drain(start..) {
                    h = (h.rotate_left(5) ^ v).wrapping_mul(K);
                }
                vals.push(h);
            }
        }
    }
    vals.pop().unwrap()
}

/// Spike 2's fold of per-result fingerprints, so checks compare with spike 2's harness.
pub fn fold_fps(st: &Heap, ts: &[Term]) -> u64 {
    ts.iter().fold(0, |h, &t| combine(h, fingerprint(st, t)))
}

/// Spike 2's workload generator (xorshift64*), so generated workloads are identical to spike 2's.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(seed.max(1))
    }
    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    pub fn below(&mut self, n: u64) -> u64 {
        self.next_u64() % n
    }
    pub fn pct(&mut self, p: u64) -> bool {
        self.below(100) < p
    }
}
