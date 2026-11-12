//! MathCAS v2 sums and recurrences (roadmap item 13B).
//!
//! The algorithms work on rational functions over the integers in the kernels of the input ([`rf`]):
//!
//! - [`hyper`]: recognising hypergeometric terms and their ratio `t(k + 1) / t(k)`;
//! - [`gosper`]: Gosper's algorithm with the polynomial solutions of linear recurrences under the
//!   Abramov-Bronstein-Petkovšek degree bound;
//! - [`zeil`]: Zeilberger's creative telescoping;
//! - [`recur`]: Abramov's rational solutions and Petkovšek's Hyper for recurrences with polynomial coefficients;
//! - [`prod`]: products of rational functions through `pochhammer` and `gamma`, Euler's reflection formula;
//! - [`sumfn`]: the closed forms of definite, indefinite and infinite sums; [`verify`]: their checks;
//! - [`numsum`]: numeric sums and products;
//! - [`rsolve`]: `r_solve` and `r_solve_value` for linear recurrences with polynomial coefficients.
//!
//! The crate wraps the list library's `sum` and `product` and the calculus crate's `r_solve` (installed
//! after them): the inner natives keep their fast paths, and the methods here run when they decline.
//!
//! ```
//! use mc_api::{Engine, StopFlag};
//! let mut e = mc_engine::CoreEngine::new();
//! let stop = StopFlag::new();
//! assert_eq!(e.eval("sum(binomial(n, k), [k, 0, n])", &stop).unwrap().printed, "2^n");
//! ```
#![warn(missing_docs)]
pub mod conv;
pub mod cx;
pub mod gf;
pub mod gosper;
pub mod hyper;
pub mod numsum;
pub mod prod;
pub mod recur;
pub mod rf;
pub mod rsolve;
pub mod series;
pub mod shift;
pub mod sumfn;
pub mod verify;
pub mod zeil;
mod natives;

use mc_core::{Heap, Store};
use mc_eval::{Attributes, Kernel};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// Every function of this crate with a one-line description.
pub const FUNCTIONS: &[(&str, &str)] = &[
    ("n_sum", "n_sum(f, [k, a, b]): the sum of f for k from a to b (b may be inf) in machine precision, with convergence acceleration"),
    ("n_product", "n_product(f, [k, a, b]): the product of f for k from a to b (b may be inf) in machine precision"),
    ("difference_delta", "difference_delta(f, k), difference_delta(f, [k, m]), difference_delta(f, [k, m, h]): the forward difference f(k + h) - f(k), m times"),
    ("discrete_shift", "discrete_shift(f, k), discrete_shift(f, [k, m]), discrete_shift(f, [k, m, h]): f with k replaced by k + m h"),
    ("discrete_ratio", "discrete_ratio(f, k), discrete_ratio(f, [k, m]), discrete_ratio(f, [k, m, h]): the ratio f(k + h) / f(k), m times"),
    ("generating_function", "generating_function(f, n, z): the sum of f z^n over n >= 0 in closed form; generating_function(f, [n, m], [x, y]) for several variables"),
    ("sum_convergence", "sum_convergence(f, n), sum_convergence(f, [n, a, inf]): true when the sum of f converges, false when it diverges, or a condition on the parameters"),
    ("recurrence_table", "recurrence_table(eqs, a, [n, nmin, nmax]): the values a(nmin), ..., a(nmax) of the sequence defined by a recurrence and its initial values; [n, nmax] starts at 1, [n, nmin, nmax, step] steps"),
    // round 28, written in MathCAS (lib/calculus.mc)
    ("find_repeat", "find_repeat(xs), (xs, k): the shortest block the list, string or matrix xs repeats at least twice (k times)"),
    ("find_transient_repeat", "find_transient_repeat(xs, k): [transient, block], the shortest transient after which xs repeats a block at least k times"),
    ("truncate_sum", "truncate_sum(e, k): every sum(f, [i, a, b]) in e cut to its first k terms"),
];

/// Register the natives of this crate (after `mc-lists` and `mc-calculus`, whose `sum`, `product` and `r_solve`
/// it wraps). `stop` is the engine's arithmetic stop flag.
pub fn install(k: &mut Kernel, h: &Heap, stop: &Arc<AtomicBool>) {
    for name in ["sum", "product"] {
        let st = stop.clone();
        let product = name == "product";
        k.wrap_native(h.sym(name), move |inner| Box::new(natives::SumWrap { inner, stop: st, product }));
    }
    for (name, value) in [("r_solve", false), ("r_solve_value", true)] {
        let st = stop.clone();
        k.wrap_native(h.sym(name), move |inner| Box::new(natives::RSolveWrap { inner, stop: st, value }));
    }
    for (name, f) in [
        ("n_sum", natives::Fun::NSum),
        ("n_product", natives::Fun::NProduct),
        ("difference_delta", natives::Fun::Delta),
        ("discrete_shift", natives::Fun::Shift),
        ("discrete_ratio", natives::Fun::Ratio),
        ("generating_function", natives::Fun::Gf),
        ("sum_convergence", natives::Fun::Conv),
        ("recurrence_table", natives::Fun::Table),
    ] {
        k.register(h.sym(name), Attributes::default(), Box::new(natives::Native { f, stop: stop.clone() }));
    }
}
