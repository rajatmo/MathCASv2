//! Root containers: everything that keeps terms across safe points (1b.2, I1.6).
//!
//! Branded handles [`T<'h>`] cannot outlive the heap borrow they came from, so they cannot survive a safe point
//! (which needs `&mut Heap`). A term kept across evaluations is held as a [`Raw`] handle inside a container that
//! implements [`Trace`]; at a safe point the collector visits every such container (possibly more than once) and
//! rewrites the handles of the terms it moves.
//!
//! Every structure that keeps terms across evaluations must implement `Trace`: session history, rule tables and
//! their indexes (A.2), number tables, front-end pins. Only symbols (immediates) may be held unrooted.
use crate::Heap;
use mc_api::T;

/// An unbranded handle, for root containers. A `Raw` that is not reachable from a [`Trace`] container passed to a
/// safe point goes stale at that safe point: it may then name a collected term or, after a major collection,
/// another term.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[repr(transparent)]
pub struct Raw(pub(crate) u32);

impl Raw {
    /// Unbrand a handle, to keep it in a root container.
    #[inline]
    pub fn new(t: T<'_>) -> Raw {
        Raw(t.raw())
    }

    /// The handle, branded for the scope of `h`.
    #[inline]
    pub fn get<'h>(self, _h: &'h Heap) -> T<'h> {
        T::from_raw(self.0)
    }

    /// The handle's bits, to keep it in an atomic slot (for example an own value read without locks). A container
    /// holding bits must turn them back into a `Raw` with [`Raw::from_bits`] and visit it at every safe point.
    #[inline]
    pub fn to_bits(self) -> u32 {
        self.0
    }

    /// The `Raw` of bits from [`Raw::to_bits`] (or [`HOLE`](crate::HOLE), which names no term).
    #[inline]
    pub fn from_bits(bits: u32) -> Raw {
        Raw(bits)
    }
}

/// Holders of terms that outlive an evaluation. At a safe point the collector calls `trace`, more than once, and may
/// rewrite every handle; `visit` must reach every `Raw` the container holds.
pub trait Trace {
    /// Call `visit` on every handle held.
    fn trace(&mut self, visit: &mut dyn FnMut(&mut Raw));
}

/// A growable root container: session history and handles a caller keeps between evaluations.
#[derive(Default, Debug)]
pub struct Roots(Vec<Raw>);

impl Roots {
    /// An empty container.
    pub fn new() -> Roots {
        Roots(vec![])
    }
    /// Keep `t`.
    pub fn push(&mut self, t: T<'_>) {
        self.0.push(Raw::new(t));
    }
    /// Replace the handle at `i`.
    pub fn set(&mut self, i: usize, t: T<'_>) {
        self.0[i] = Raw::new(t);
    }
    /// The handle at `i`, branded for the scope of `h`.
    pub fn get<'h>(&self, h: &'h Heap, i: usize) -> T<'h> {
        self.0[i].get(h)
    }
    /// Number of handles kept.
    pub fn len(&self) -> usize {
        self.0.len()
    }
    /// No handle kept?
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    /// Keep only the first `n` handles.
    pub fn truncate(&mut self, n: usize) {
        self.0.truncate(n)
    }
    /// Keep nothing.
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

impl Trace for Raw {
    fn trace(&mut self, visit: &mut dyn FnMut(&mut Raw)) {
        visit(self);
    }
}

impl<X: Trace> Trace for Vec<X> {
    fn trace(&mut self, visit: &mut dyn FnMut(&mut Raw)) {
        for x in self.iter_mut() {
            x.trace(visit);
        }
    }
}

impl<X: Trace> Trace for Option<X> {
    fn trace(&mut self, visit: &mut dyn FnMut(&mut Raw)) {
        if let Some(x) = self {
            x.trace(visit);
        }
    }
}
