//! Integration spike I1: spike 1b's generational collector under spike 4's concurrent store and work-stealing
//! evaluator.
//!
//! - [`heap`]: one heap with two regions. The **old region** is a dense array in creation order with an
//!   open-addressing interning table; between safe points it is immutable, so every thread reads it without locks.
//!   The **young region** is spike 4's concurrent store C (segmented arena with per-thread id and argument chunks, 128
//!   shards of lock-free-probed arrays, a lock only on a miss). A minor collection at a safe point promotes the young
//!   survivors into the old region; a major one compacts the old region when it has doubled (decision 1b.1).
//! - [`young`]: the young region (arena and table), reset wholesale at every minor collection.
//! - [`memo`]: spike 4's lossy seqlock memo (decision 4.8), split into an old and a young table, indexed by the
//!   structural hash so that renumbering never moves an entry.
//! - [`eval`]: spike 4's evaluator, ported to branded handles, with its session state ([`eval::Defs`]) as a root
//!   container.
//! - [`work`]: spike 4's M6a, M8, M10 and spike 1b's S3, S8 and long session as sessions of evaluations.
//!
//! Handles are branded with the lifetime of a shared borrow of the heap (spike 1b's API): [`T<'h>`] is a plain
//! 32-bit `Copy` value that any number of threads may hold while they share `&'h Heap`; the safe point needs
//! `&mut Heap`, so no branded handle can survive into it. Only [`Trace`] containers hold handles across safe points.
//!
//! # Rooting by types, with parallel tasks
//!
//! Handles built on several threads inside one evaluation are plain values, shared freely, and canonical:
//! ```
//! use int_gc::eval::Defs;
//! use int_gc::heap::{Config, Heap, Order};
//! use int_gc::{work::Session, Roots};
//! let mut s = Session::new(Config::gen(Order::Topo), 1);
//! let f = s.heap.sym("f");
//! s.step(|h: &Heap, _: &Defs, keep: &mut Roots| {
//!     let (a, b) = rayon::join(|| h.app(f.t(), &[h.int(1)]), || h.app(f.t(), &[h.int(2)]));
//!     let (x, y) = rayon::join(|| h.app(f.t(), &[a, b]), || h.app(f.t(), &[a, b]));
//!     assert_eq!(x, y);
//!     keep.push(x);
//! });
//! // after the safe point, the root was rewritten (promoted) and is still the same term
//! let h = &s.heap;
//! let x = s.keep.get(h, 0);
//! assert_eq!(x, h.app(f.t(), &[h.app(f.t(), &[h.int(1)]), h.app(f.t(), &[h.int(2)])]));
//! ```
//! A handle kept across a safe point does not compile (the safe point needs `&mut Heap`):
//! ```compile_fail
//! use int_gc::heap::{Config, Heap, Order};
//! let mut heap = Heap::new(Config::gen(Order::Topo));
//! let f = heap.sym("f");
//! let t = heap.app(f.t(), &[heap.int(1)]);
//! heap.safe_point(&mut [], false);
//! let _ = heap.view(t);
//! ```
//! Nor can an evaluation return one (its scope is higher-ranked, so the result type cannot name it):
//! ```compile_fail
//! use int_gc::eval::Defs;
//! use int_gc::heap::{Config, Heap, Order};
//! use int_gc::{work::Session, Roots};
//! let mut s = Session::new(Config::gen(Order::Topo), 1);
//! let f = s.heap.sym("f");
//! let t = s.step(|h: &Heap, _: &Defs, _: &mut Roots| h.app(f.t(), &[h.int(1)]));
//! ```
//! Nor can a detached task hold one past the evaluation (`rayon::spawn` needs a `'static` closure); scoped tasks
//! (`join`, parallel iterators) end before the evaluation returns, which makes the safe point a global barrier:
//! ```compile_fail
//! use int_gc::eval::Defs;
//! use int_gc::heap::{Config, Heap, Order};
//! use int_gc::{work::Session, Roots};
//! let mut s = Session::new(Config::gen(Order::Topo), 1);
//! let f = s.heap.sym("f");
//! s.step(|h: &Heap, _: &Defs, _: &mut Roots| {
//!     let t = h.app(f.t(), &[h.int(1)]);
//!     rayon::spawn(move || println!("{:?}", t));
//! });
//! ```
pub mod eval;
pub mod heap;
pub mod memo;
pub mod work;
pub mod young;

use std::marker::PhantomData;

/// Handle bits. Bit 31: immediate. Immediates: bit 30 set = symbol (index in the low 30 bits), clear = 30-bit
/// integer (decision 1b.3). Stored terms: bit 30 set = young (index into the young arena), clear = old (index into
/// the old region).
pub const IMM: u32 = 1 << 31;
pub const SYMB: u32 = 1 << 30;
pub const YOUNG: u32 = 1 << 30;
pub const PAYLOAD: u32 = (1 << 30) - 1;
pub const IMM_MIN: i64 = -(1 << 29);
pub const IMM_MAX: i64 = (1 << 29) - 1;

/// A term handle branded with the lifetime of the heap borrow that produced it. It is valid while that borrow
/// lives, and the borrow checker ends it before any safe point (which takes `&mut Heap`).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[repr(transparent)]
pub struct T<'h>(u32, PhantomData<&'h ()>);

impl<'h> T<'h> {
    #[inline]
    pub(crate) const fn from_bits(b: u32) -> T<'h> {
        T(b, PhantomData)
    }
    #[inline]
    pub fn bits(self) -> u32 {
        self.0
    }
    #[inline]
    pub fn is_imm(self) -> bool {
        self.0 & IMM != 0
    }
    #[inline]
    pub(crate) fn raw(self) -> Raw {
        Raw(self.0)
    }
}

/// A symbol: an immediate, never moved or collected, so it may be held anywhere (rule tables, workload structs).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Sym(pub(crate) u32);

impl Sym {
    /// A symbol is valid in every scope.
    #[inline]
    pub fn t<'h>(self) -> T<'h> {
        T::from_bits(IMM | SYMB | self.0)
    }
    pub fn index(self) -> u32 {
        self.0
    }
}

/// An unbranded handle. Only root containers hold one; the collector visits and rewrites them at safe points.
/// Constructed only inside this crate (from a branded handle by a root container).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Raw(pub(crate) u32);

/// Holders of terms that outlive an evaluation. At a safe point the collector calls `trace` (more than once) and may
/// rewrite every handle.
pub trait Trace {
    fn trace(&mut self, visit: &mut dyn FnMut(&mut Raw));
}

/// A growable root container: the session history and every handle a workload keeps between evaluations.
#[derive(Default)]
pub struct Roots(Vec<Raw>);

impl Roots {
    pub fn new() -> Roots {
        Roots(vec![])
    }
    pub fn push(&mut self, t: T<'_>) {
        self.0.push(t.raw());
    }
    pub fn set(&mut self, i: usize, t: T<'_>) {
        self.0[i] = t.raw();
    }
    /// The handle at `i`, branded for the scope of `h`.
    pub fn get<'h>(&self, _h: &'h heap::Heap, i: usize) -> T<'h> {
        T::from_bits(self.0[i].0)
    }
    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub fn clear(&mut self) {
        self.0.clear()
    }
}

impl Trace for Roots {
    fn trace(&mut self, visit: &mut dyn FnMut(&mut Raw)) {
        for r in self.0.iter_mut() {
            visit(r);
        }
    }
}

pub const K: u64 = 0x9E37_79B9_7F4A_7C15;

/// SplitMix64's finaliser (spike 4).
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

pub fn str_hash(s: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.bytes() {
        h = (h ^ b as u64).wrapping_mul(0x100_0000_01b3);
    }
    h
}

/// Deterministic pseudo-random numbers for building workloads (xorshift64*, as spikes 1 and 1b).
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

/// A splittable generator (spike 4's SplitMix64, decision 4.7).
#[derive(Clone, Copy, Debug)]
pub struct SplitMix(u64);

impl SplitMix {
    pub fn new(seed: u64) -> SplitMix {
        SplitMix(seed)
    }
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(K);
        finalize(self.0)
    }
    pub fn split(seed: u64, i: u64) -> SplitMix {
        SplitMix(finalize(seed ^ finalize(i.wrapping_add(K))))
    }
}
