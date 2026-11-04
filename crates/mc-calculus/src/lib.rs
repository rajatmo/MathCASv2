//! MathCAS v2 calculus (roadmap package 2A): symbolic differentiation.
//!
//! - [`term`]: canonical construction of derivative terms through `mc-arith`;
//! - [`rules`]: the table of derivative rules of the known functions, one row per function (the place to extend);
//! - [`diff`]: the derivative of a term in one variable (an iterative walk over the term's DAG, memoised);
//! - the native of `d`: variable specifications, orders, gradients.
//!
//! The derivative operator is the term `derivative(n1, ..., nk)(f)`, which the parser writes for `f'`, `f''`, ...
//! and the printer shows with prime marks; differentiating an unknown function gives it (chain rule), and a known
//! function inside it is resolved when it is differentiated (`d(sin'(x), x)` is `-sin(x)`).
//!
//! ```
//! use mc_bench::{Engine, StopFlag};
//! let mut e = mc_engine::CoreEngine::new();
//! let stop = StopFlag::new();
//! assert_eq!(e.eval("d(x^3 + sin(x), x)", &stop).unwrap().printed, "cos(x) + 3*x^2");
//! assert_eq!(e.eval("d(f(g(x)), x)", &stop).unwrap().printed, "f'(g(x))*g'(x)");
//! ```
//!
//! Decisions of this package are numbered 2A.1, 2A.2, ... in the comments where they are applied.
#![warn(missing_docs)]
pub mod diff;
mod natives;
pub mod rules;
pub mod term;

use mc_core::{Heap, Store};
use mc_eval::{Attributes, Kernel};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// Every function of this package with a one-line description.
pub const FUNCTIONS: &[(&str, &str)] = &[
    (
        "d",
        "d(f, x): the derivative in x; d(f, [x, n]) the n-th; d(f, x, y) mixed; d(f, [x, y]) the gradient (a list)",
    ),
    (
        "derivative",
        "derivative(n)(f): the n-th derivative of the function f, written f', f'', ...; derivative(n1, ..., nk)(f) for k arguments",
    ),
];

/// Register the natives of this package with `k`. `stop` is the flag `mc-arith`'s number kernels poll (the
/// engine's arithmetic stop flag).
pub fn install(k: &mut Kernel, h: &Heap, stop: &Arc<AtomicBool>) {
    k.register(h.sym("d"), Attributes::default(), Box::new(natives::D { stop: stop.clone() }));
}
