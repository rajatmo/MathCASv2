//! Arena-collection spike (plan section 5, spike 1b; decision 1.4 of spike 1).
//!
//! The hash-consed arena of spike 1 never frees anything. This crate adds collection at *safe points* (between
//! evaluations) to a copy of that store and compares the schemes on identical workloads:
//! - [`hc::HcStore`]: spike 1's `hc+imm+stamps` store, unchanged, as the baseline (never collects);
//! - [`gc::GcStore`] in its modes: `none` (same layout, collection off), `A` mark-sweep with a free list and argument
//!   compaction, `A-fl` mark-sweep with argument free lists by length, `B` sliding compaction, `C`/`C0`
//!   generational (a young region promoted by compaction, the old region compacted rarely).
//!
//! The [`Store`] interface is spike 1's, plus [`Store::safe_point`] (the only place a collection can happen) and
//! [`Store::gc_log`] (pause and memory per collection). Every workload passes its live handles as roots and continues
//! with the (possibly renumbered) handles the collector writes back.
pub mod gc;
pub mod hc;
pub mod work;

use std::time::Duration;

/// A read-only view of one term.
pub enum View<'a, T, S> {
    Int(i64),
    Sym(&'a S),
    App(&'a S, &'a [T]),
}

/// One collection, as recorded by the store.
#[derive(Clone, Debug)]
pub struct GcEvent {
    /// `sweep` (A), `full` (B, or C's major), `minor` (C)
    pub kind: &'static str,
    pub pause: Duration,
    /// bytes reserved by the store before and after (capacities of its arrays and tables)
    pub before: usize,
    pub after: usize,
    /// nodes alive after the collection
    pub live: usize,
}

pub trait Store {
    /// A term handle.
    type T: Clone;
    /// A symbol handle (used as a head and to test heads).
    type S: Clone;

    fn name(&self) -> &'static str;
    fn sym(&mut self, name: &str) -> Self::S;
    fn sym_term(&mut self, s: &Self::S) -> Self::T;
    fn int(&mut self, v: i64) -> Self::T;
    fn app(&mut self, head: &Self::S, args: Vec<Self::T>) -> Self::T;
    fn view<'a>(&'a self, t: &'a Self::T) -> View<'a, Self::T, Self::S>;
    /// Structural identity (`===`).
    fn same(&self, a: &Self::T, b: &Self::T) -> bool;
    /// A hash consistent with `same`.
    fn hash_term(&self, t: &Self::T) -> u64;
    /// Is `s` this symbol?
    fn sym_is(&self, s: &Self::S, other: &Self::S) -> bool;
    fn sym_name(&self, s: &Self::S) -> String;

    // Normal-form stamps and memoisation (semantics 4.3, 4.4).
    fn has_stamps(&self) -> bool {
        false
    }
    /// The memoised normal form of `t`, if still valid.
    fn memo_get(&self, _t: &Self::T) -> Option<Self::T> {
        None
    }
    fn memo_put(&mut self, _t: &Self::T, _nf: &Self::T) {}
    /// A definition of `s` changed: invalidate what depends on it.
    fn redefine(&mut self, _s: &Self::S) {}

    /// A safe point between evaluations. The store may collect: every term handle the caller still needs must be in
    /// `roots`, and a moving collector rewrites those handles in place. Handles not passed are invalid afterwards
    /// (symbol handles and immediates always stay valid). `force` collects regardless of the trigger policy (for C:
    /// a minor collection, what the end of a large evaluation triggers; the major one still follows its policy).
    fn safe_point(&mut self, _roots: &mut [&mut [Self::T]], _force: bool) {}
    fn gc_log(&self) -> &[GcEvent] {
        &[]
    }

    fn head_is(&self, t: &Self::T, s: &Self::S) -> bool {
        match self.view(t) {
            View::App(h, _) => self.sym_is(h, s),
            _ => false,
        }
    }
    fn as_int(&self, t: &Self::T) -> Option<i64> {
        match self.view(t) {
            View::Int(v) => Some(v),
            _ => None,
        }
    }
}

/// A structural fingerprint independent of the store, used to check that all stores compute the same terms.
/// Iterative, so deep terms do not overflow the stack. (Copied from spike 1.)
pub fn fingerprint<St: Store>(st: &St, t: &St::T) -> u64 {
    const K: u64 = 0x9E37_79B9_7F4A_7C15;
    enum Frame<'a, T> {
        Enter(&'a T),
        Exit(u64, usize),
    }
    let mut work: Vec<Frame<St::T>> = vec![Frame::Enter(t)];
    let mut vals: Vec<u64> = vec![];
    while let Some(f) = work.pop() {
        match f {
            Frame::Enter(t) => match st.view(t) {
                View::Int(v) => vals.push((v as u64).wrapping_mul(K) ^ 1),
                View::Sym(s) => vals.push(str_hash(&st.sym_name(s)) ^ 2),
                View::App(h, args) => {
                    work.push(Frame::Exit(str_hash(&st.sym_name(h)), args.len()));
                    for a in args.iter().rev() {
                        work.push(Frame::Enter(a));
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
}
