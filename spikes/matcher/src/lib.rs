//! Matcher spike (plan section 5, spike 2).
//!
//! - [`store`]: spike 1's hash-consed arena with immediate small integers, extended with machine reals, the canonical
//!   order (semantics 8) and canonicalisation of associative-commutative heads (flatten and sort, semantics 7.3).
//! - [`pat`]: patterns (semantics 6.1 subset), their compilation, and the instance-of check used for rule order
//!   (semantics 6.3).
//! - [`matcher`]: matching with backtracking in continuation-passing style, modulo associativity and commutativity,
//!   with a naive and a pruning AC strategy that enumerate matches in the same order (semantics 6.2).
//! - [`index`]: rule sets with linear or indexed dispatch (a discrimination tree, plus a key-set filter for AC heads),
//!   and definition-time ordering by the instance-of relation.
//! - [`eval`]: a mini innermost evaluator with own values, argument-head rules (semantics 6.5), normal-form stamps and
//!   read-set tracking (semantics 4.3, decision 13.1).
//! - [`work`]: the benchmark workloads (R4, R5, R13, R14, S7 and definition-time cost).
pub mod eval;
pub mod index;
pub mod matcher;
pub mod pat;
pub mod store;
pub mod work;

#[cfg(test)]
mod tests;

use std::cell::Cell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use store::{Store, Term, View};

/// The external interrupt (semantics 10). Nothing in the harness sets it; every bounded loop polls it through
/// [`Limit::tick`], so it is wired where a front end would set it.
pub static INTERRUPT: AtomicBool = AtomicBool::new(false);

/// A time limit plus the interrupt, polled by every loop whose trip count depends on the input (semantics 10).
/// Reading the clock on every step would dominate small steps, so it is read every 1024 ticks.
pub struct Limit {
    deadline: Option<Instant>,
    ticks: Cell<u32>,
    hit: Cell<bool>,
}

impl Limit {
    pub fn new(d: Option<Duration>) -> Limit {
        Limit { deadline: d.map(|d| Instant::now() + d), ticks: Cell::new(0), hit: Cell::new(false) }
    }
    pub fn none() -> Limit {
        Limit::new(None)
    }
    /// One step of work; true when the computation must stop.
    #[inline]
    pub fn tick(&self) -> bool {
        if self.hit.get() {
            return true;
        }
        let t = self.ticks.get().wrapping_add(1);
        self.ticks.set(t);
        if t & 1023 == 0 {
            let late = self.deadline.is_some_and(|d| Instant::now() > d);
            if late || INTERRUPT.load(Ordering::Relaxed) {
                self.hit.set(true);
                return true;
            }
        }
        false
    }
    pub fn hit(&self) -> bool {
        self.hit.get()
    }
}

/// Returned when a limit or the interrupt stopped a computation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stop;

/// A structural fingerprint built from symbol names and values, not from term ids, so that different stores (and
/// differently ordered constructions) of the same term agree. Iterative, so deep terms do not overflow the stack.
pub fn fingerprint(st: &Store, t: Term) -> u64 {
    const K: u64 = 0x9E37_79B9_7F4A_7C15;
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

/// Combine fingerprints in sequence (order matters).
pub fn combine(h: u64, v: u64) -> u64 {
    (h.rotate_left(7) ^ v).wrapping_mul(0x9E37_79B9_7F4A_7C15)
}

pub fn str_hash(s: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.bytes() {
        h = (h ^ b as u64).wrapping_mul(0x100_0000_01b3);
    }
    h
}

/// Deterministic pseudo-random numbers for building workloads (xorshift64*).
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
    /// True with probability `p` percent.
    pub fn pct(&mut self, p: u64) -> bool {
        self.below(100) < p
    }
}
