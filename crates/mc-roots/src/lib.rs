//! MathCAS v2 numerical root finding and minimisation (roadmap phase 4A, package R): `find_root`, `find_minimum`
//! and `find_maximum` as natives of the evaluator.
//!
//! - `func`: the user's expression evaluated at numeric points, behind the `NumFn` trait;
//! - `newton`: damped Newton's method (real or complex, any number of equations) with Broyden's fallback and
//!   optional bounds;
//! - `bracket`: the secant method and Brent's method in one real variable;
//! - `minimize`: Brent's minimisation, BFGS;
//! - `refine`: the refinement with balls under `digits:` and the interval Newton verification;
//! - `linalg`, `scalar`, `options`: dense linear algebra, machine scalars, the options and goals;
//! - `natives`: argument forms, method choice, results and failures.
//!
//! Results are lists of rules, so they substitute with `rewrite`:
//!
//! ```
//! use mc_bench::{Engine, StopFlag};
//! let mut e = mc_engine::CoreEngine::new();
//! let stop = StopFlag::new();
//! assert_eq!(e.eval("find_root(x^2 - 2, [x, 1])", &stop).unwrap().printed, "[x -> 1.4142135623730951]");
//! ```
//!
//! Decisions of this package are numbered R.1, R.2, ... in the comments where they are applied.
#![warn(missing_docs)]
mod bracket;
mod func;
mod linalg;
mod minimize;
mod natives;
mod newton;
mod options;
mod refine;
mod scalar;

use mc_core::{Heap, Store};
use mc_eval::{Attributes, Kernel};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// Every function of this package with a one-line description.
pub const FUNCTIONS: &[(&str, &str)] = &[
    (
        "find_root",
        "find_root(f, [x, x0]): a root of f (or of lhs == rhs) near x0 as [x -> root]; [x, x0, x1] secant, [x, x0, a, b] in a bracket, [[x, x0], [y, y0]] systems; digits: d for balls",
    ),
    (
        "find_minimum",
        "find_minimum(f, [x, x0]): a local minimum near x0 as [f_min, [x -> x_min]]; [[x, x0], [y, y0]] for several variables (BFGS); digits: d for balls",
    ),
    (
        "find_maximum",
        "find_maximum(f, [x, x0]): a local maximum near x0 as [f_max, [x -> x_max]]; [[x, x0], [y, y0]] for several variables",
    ),
];

/// The result of a machine iteration.
pub(crate) enum Outcome<S> {
    /// The iteration ended at `x` with the error estimate `err`; `converged` when the estimate meets the goals and
    /// the residual test passes.
    Done {
        /// the last iterate
        x: Vec<S>,
        /// the error estimate of `x`
        err: f64,
        /// the goals were met
        converged: bool,
        /// iterations used
        iterations: usize,
    },
    /// A value at this point is not a number.
    NotNumeric(Vec<S>),
}

/// Register the natives of this package with `k`. `stop` is the flag `mc-arith`'s number kernels poll (the
/// engine's arithmetic stop flag).
pub fn install(k: &mut Kernel, h: &Heap, stop: &Arc<AtomicBool>) {
    let a = Attributes::default();
    k.register(h.sym("find_root"), a, Box::new(natives::FindRoot { stop: stop.clone() }));
    k.register(h.sym("find_minimum"), a, Box::new(natives::FindExtremum { stop: stop.clone(), maximum: false }));
    k.register(h.sym("find_maximum"), a, Box::new(natives::FindExtremum { stop: stop.clone(), maximum: true }));
}
