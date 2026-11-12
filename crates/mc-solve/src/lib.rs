//! MathCAS v2 equation solving (roadmap item 3B).
//!
//! - [`solve`](FUNCTIONS): polynomial equations by radicals up to degree four and root objects beyond, rational
//!   equations, radical equations, simple transcendental inversions, linear systems by fraction-free elimination,
//!   polynomial systems by lexicographic Groebner bases (`mc_poly::groebner`) and back substitution, all with
//!   symbolic parameters (generic solutions);
//! - `roots`, `n_solve`, `eliminate`, `groebner_basis`;
//! - `reduce`: real polynomial and rational inequalities in one variable by exact real root isolation;
//! - `root(x => p(x), k)`: root objects, evaluated numerically by `n` (Aberth iteration, exact real roots, complex
//!   balls).
//!
//! ```
//! use mc_api::{Engine, StopFlag};
//! let mut e = mc_engine::CoreEngine::new();
//! let stop = StopFlag::new();
//! assert_eq!(e.eval("solve(x^2 == 1, x)", &stop).unwrap().printed, "[[x -> -1], [x -> 1]]");
//! ```
//!
//! Decisions of this crate are numbered 3B.1, 3B.2, ... in the comments where they are applied.
#![warn(missing_docs)]
mod chart;
mod bounds;
mod cx;
mod domain;
mod front;
mod interval;
mod linear;
mod modular;
mod natives;
mod nsolve;
mod numroots;
mod order;
mod plog;
pub mod realroots;
mod reduce;
mod reduce_cx;
mod reduce_fn;
mod rootobj;
mod solve;
mod system;
mod univariate;

use mc_core::{Heap, Store};
use mc_eval::{Attributes, Kernel};
use natives::{SolveFn, SolveNative};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// Every function of this crate with a one-line description.
pub const FUNCTIONS: &[(&str, &str)] = &[
    (
        "solve",
        "solve(eqs, xs): the solutions as a list of rule lists [[x -> a, ...], ...]: polynomial equations by radicals (degree <= 4) or root objects, rational, radical and simple transcendental equations (principal values), linear and polynomial systems (Groebner bases), with parameters (generic solutions)",
    ),
    (
        "solve_values",
        "solve_values(eqs, xs): the values of the solutions of solve(eqs, xs): [a, b, ...] for one unknown, [[a1, b1], ...] for a list of unknowns",
    ),
    (
        "n_solve_values",
        "n_solve_values(eqs, xs): the values of the numeric solutions of n_solve(eqs, xs): [a, b, ...] for one unknown, [[a1, b1], ...] for a list of unknowns",
    ),
    ("roots", "roots(p == q, x): the roots of a polynomial equation in x as x == r1 or x == r2 ..., repeated by multiplicity"),
    (
        "n_solve",
        "n_solve(eqs, xs): numeric solutions; all roots of a polynomial in one unknown (with multiplicity) at machine precision, or to d digits with options(digits -> d)",
    ),
    ("eliminate", "eliminate(eqs, xs): the equations p == 0 implied by eqs that are free of the unknowns xs"),
    ("groebner_basis", "groebner_basis(polys, xs), (polys, xs, elims): the reduced Groebner basis, xs first and the other symbols as variables after them (elims eliminated); monomial_order: lexicographic, degree_lexicographic, degree_reverse_lexicographic, elimination_order or a weight matrix; modulus: p"),
    (
        "reduce",
        "reduce(cond, x, reals): a condition of polynomial or rational equations and inequalities in one real variable as intervals, x < a or b < x and x <= c",
    ),
    (
        "root",
        "root(x => p(x), k): the k-th root of the polynomial p (real roots ascending, then complex by real and imaginary part); numeric under n",
    ),
];

/// The table of natives: (name, function).
const TABLE: &[(&str, SolveFn)] = &[
    ("solve", SolveFn::Solve),
    ("solve_values", SolveFn::SolveValues),
    ("n_solve_values", SolveFn::NSolveValues),
    ("roots", SolveFn::Roots),
    ("n_solve", SolveFn::NSolve),
    ("eliminate", SolveFn::Eliminate),
    ("groebner_basis", SolveFn::GroebnerBasis),
    ("reduce", SolveFn::Reduce),
    ("root", SolveFn::Root),
];

/// Register the natives of this crate with `k`. `stop` is the flag `mc-arith`'s number kernels poll (the
/// engine's arithmetic stop flag), so that long computations stop promptly (decision 3B.1).
pub fn install(k: &mut Kernel, h: &Heap, stop: &Arc<AtomicBool>) {
    for &(name, f) in TABLE {
        // solve reads the assumption context: facts about the unknowns filter its solutions (decision 3B.20)
        let a = Attributes { uses_assumptions: matches!(f, SolveFn::Solve), ..Attributes::default() };
        let native = SolveNative { f, name, stop: stop.clone() };
        if matches!(f, SolveFn::Solve | SolveFn::Reduce) {
            // the assumptions: option (decision IOSYM.4)
            k.register(h.sym(name), a, Box::new(mc_poly::opts::WithAssumptions { name, inner: native }));
        } else {
            k.register(h.sym(name), a, Box::new(native));
        }
    }
}
