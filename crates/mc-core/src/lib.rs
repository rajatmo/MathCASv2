//! The MathCAS v2 core store (docs/design/05-assembly.md, package A): a concurrent hash-consed arena with a
//! generational collector and number payloads, implementing [`mc_api::Store`].
//!
//! - **Two regions** (I1.1). The young region is spike 4's concurrent store: nodes in per-thread id chunks, argument
//!   slots and small payloads in per-thread slabs, one shared lock-free interning table. The old region is dense and
//!   immutable between safe points, read by every thread with plain loads.
//! - **Numbers** (I3). Symbols and 30-bit integers are immediates. Nodes are 32 bytes: compounds, one-limb integers
//!   (`Word`), rationals over two integer terms, machine reals, and payload nodes (big integers, balls, strings)
//!   whose words live in a limb arena when short and in a box when long. One canonicalising constructor
//!   ([`Store::big_int`]) chooses the integer tier. Payloads are hashed in full and interned by value.
//! - **Collection** at safe points only ([`Heap::safe_point`], `&mut`): minor collections promote young survivors in
//!   per-thread chunk order, major ones slide the old region (see the `gc` module docs).
//! - **The memo** ([`Heap::memo_get`], [`Heap::memo_put`]): dense by term id per region, weak, lossy, stamped with
//!   start epoch and read set (A.1).
//! - **Sessions** ([`Session`]): evaluations in a higher-ranked scope on a thread pool, a safe point after each.
//! - **Stress verification** ([`Config::stress`], [`Heap::verify`]) and session shapes for tests and measurements
//!   ([`shapes`]).
//!
//! # Rooting by types, with parallel tasks
//!
//! Handles built on several threads inside one evaluation are plain values, shared freely, and canonical:
//! ```
//! use mc_core::{Config, Heap, Roots, Session, Store};
//! let mut s = Session::new(Config::default(), 2, Roots::new());
//! let f = s.heap.sym("f");
//! s.step(|h: &Heap, keep: &mut Roots| {
//!     let (a, b) = rayon::join(|| h.call(f, &[h.int(1)]), || h.call(f, &[h.int(2)]));
//!     let (x, y) = rayon::join(|| h.call(f, &[a, b]), || h.call(f, &[a, b]));
//!     assert_eq!(x, y);
//!     keep.push(x);
//! });
//! s.collect_full();
//! // the root was rewritten (promoted) and is still the same term
//! let h = &s.heap;
//! let x = s.state.get(h, 0);
//! assert_eq!(x, h.call(f, &[h.call(f, &[h.int(1)]), h.call(f, &[h.int(2)])]));
//! ```
//! A handle kept across a safe point does not compile (the safe point needs `&mut Heap`):
//! ```compile_fail
//! use mc_core::{Config, Heap, Store};
//! let mut heap = Heap::new(Config::default());
//! let f = heap.sym("f");
//! let t = heap.call(f, &[heap.int(1)]);
//! heap.safe_point(&mut []);
//! let _ = heap.view(t);
//! ```
//! Nor can an evaluation return one (its scope is higher-ranked, so the result type cannot name it):
//! ```compile_fail
//! use mc_core::{Config, Heap, Roots, Session, Store};
//! let mut s = Session::new(Config::default(), 1, Roots::new());
//! let f = s.heap.sym("f");
//! let t = s.step(|h: &Heap, _: &mut Roots| h.call(f, &[h.int(1)]));
//! ```
//! Nor can a detached task hold one past the evaluation (`rayon::spawn` needs a `'static` closure):
//! ```compile_fail
//! use mc_core::{Config, Heap, Roots, Session, Store};
//! let mut s = Session::new(Config::default(), 1, Roots::new());
//! let f = s.heap.sym("f");
//! s.step(|h: &Heap, _: &mut Roots| {
//!     let t = h.call(f, &[h.int(1)]);
//!     rayon::spawn(move || println!("{:?}", t));
//! });
//! ```
#![warn(missing_docs)]
mod gc;
mod handle;
mod hash;
mod heap;
mod memo;
mod node;
mod old;
mod roots;
mod scratch;
mod segs;
mod session;
pub mod shapes;
mod symbols;
mod unpack;
mod verify;
mod young;

pub use gc::{GcEvent, GcKind, GcStats};
pub use handle::{HOLE, SMALL_MAX, SMALL_MIN};
pub use heap::{Config, Heap, MemoHit};
pub use mc_api::{Packed, Store, Sym, View, PACK_MIN, T};
pub use roots::{Raw, Roots, Trace};
pub use scratch::{detached, scratch_active, Scratch, ScratchStats, SLOTS};
pub use session::Session;
