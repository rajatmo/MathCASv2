//! MathCAS v2 equation solving (roadmap package 3B).
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
//! use mc_bench::{Engine, StopFlag};
//! let mut e = mc_engine::CoreEngine::new();
//! let stop = StopFlag::new();
//! assert_eq!(e.eval("solve(x^2 == 1, x)", &stop).unwrap().printed, "[[x -> -1], [x -> 1]]");
//! ```
//!
//! Decisions of this package are numbered 3B.1, 3B.2, ... in the comments where they are applied.
#![warn(missing_docs)]
mod cx;
mod domain;
mod linear;
mod natives;
mod numroots;
mod realroots;
mod reduce;
mod rootobj;
mod solve;
mod system;
mod univariate;

use mc_core::{Heap, Store};
use mc_eval::{Attributes, Kernel};
use natives::{SolveFn, SolveNative};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// Every function of this package with a one-line description.
pub const FUNCTIONS: &[(&str, &str)] = &[
    (
        "solve",
        "solve(eqs, xs): the solutions as a list of rule lists [[x -> a, ...], ...]: polynomial equations by radicals (degree <= 4) or root objects, rational, radical and simple transcendental equations (principal values), linear and polynomial systems (Groebner bases), with parameters (generic solutions)",
    ),
    ("roots", "roots(p == q, x): the roots of a polynomial equation in x as x == r1 or x == r2 ..., repeated by multiplicity"),
    (
        "n_solve",
        "n_solve(eqs, xs): numeric solutions; all roots of a polynomial in one unknown (with multiplicity) at machine precision, or to d digits with options(digits -> d)",
    ),
    ("eliminate", "eliminate(eqs, xs): the equations p == 0 implied by eqs that are free of the unknowns xs"),
    ("groebner_basis", "groebner_basis(polys, xs): the reduced lexicographic Groebner basis in xs (other symbols are parameters)"),
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
    ("roots", SolveFn::Roots),
    ("n_solve", SolveFn::NSolve),
    ("eliminate", SolveFn::Eliminate),
    ("groebner_basis", SolveFn::GroebnerBasis),
    ("reduce", SolveFn::Reduce),
    ("root", SolveFn::Root),
];

/// Register the natives of this package with `k`. `stop` is the flag `mc-arith`'s number kernels poll (the
/// engine's arithmetic stop flag), so that long computations stop promptly (decision 3B.1).
pub fn install(k: &mut Kernel, h: &Heap, stop: &Arc<AtomicBool>) {
    for &(name, f) in TABLE {
        // solve reads the assumption context: facts about the unknowns filter its solutions (decision 3B.20)
        let a = Attributes { uses_assumptions: matches!(f, SolveFn::Solve), ..Attributes::default() };
        k.register(h.sym(name), a, Box::new(SolveNative { f, name, stop: stop.clone() }));
    }
}
