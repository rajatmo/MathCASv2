//! MathCAS v2 decisions under assumptions (roadmap item F3).
//!
//! - [`Decider`]: one service answering sign, zero, nonzero, interval-membership, realness and integrality
//!   questions about an expression under the facts of an assumption context, in tiers from the cheapest
//!   ([`decide`]): structure and facts with ball refinement of closed parts ([`facts`]), the Groebner normal form
//!   modulo the equational facts ([`basis`]), ball enclosures over the bounds of the variables and exact quadratic
//!   forms ([`quad`]), exact real root isolation in one variable ([`sturm`]), and `reduce` for one real variable
//!   (the [`Reals`] hook, for several variables when phase 11B lands). `refine`, `simplify`'s assumption path,
//!   `limit`'s and `integrate`'s sign questions and `solve`'s conditions all go through it.
//! - [`cond`]: conditional results, `conditional_expression(value, condition)`, with their arithmetic, printing,
//!   `simplify`, `refine` and `normal`; and [`Conditions`], the per-call accumulator in which methods record the
//!   conditions they need, simplified once at the end and attached as one conditional result.
//!
//! ```
//! use mc_api::{Engine, StopFlag};
//! let mut e = mc_engine::CoreEngine::new();
//! let stop = StopFlag::new();
//! assert_eq!(e.eval("refine(sign(x^2 - x*y + y^2 + 1), element(x | y, reals))", &stop).unwrap().printed, "1");
//! ```
//!
//! Decisions of this crate are numbered F3.1, F3.2, ... in `NOTES.md` and in the comments where they apply.
#![warn(missing_docs)]
pub mod basis;
pub mod build;
pub mod cond;
pub mod decide;
pub mod facts;
pub mod quad;
pub mod sturm;

pub use build::Bld;
pub use cond::{Conditions, CONDITIONAL};
pub use decide::{register_reals, sign_in_context, Decider, OneVariable, Reals, RealsFactory, Sign, Stats};
pub use facts::{Bound, Info, Kind, Range, Sgn, Syms};

use mc_core::{Heap, Store};
use mc_eval::{Attributes, Kernel};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// Every function of this crate with a one-line description.
pub const FUNCTIONS: &[(&str, &str)] = &[
    (
        "conditional_expression",
        "conditional_expression(value, condition): value where condition holds (true gives value, false undefined); arithmetic, comparisons and functions take the condition outward, simplify and refine work under it, normal gives the value",
    ),
    ("nonnegative_rationals", "the domain of the rational numbers >= 0"),
    ("nonpositive_rationals", "the domain of the rational numbers <= 0"),
];

/// Register the natives of this crate with `k` (after `mc-simplify` and `mc-solve`, whose functions the
/// deciders call). `stop` is the flag `mc-arith`'s number kernels poll.
pub fn install(k: &mut Kernel, h: &Heap, stop: &Arc<AtomicBool>) {
    k.register(h.sym(basis::BASIS), Attributes::default(), Box::new(basis::BasisNative { stop: stop.clone() }));
    cond::install(k, h, stop);
}
