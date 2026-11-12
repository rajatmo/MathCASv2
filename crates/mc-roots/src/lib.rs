//! MathCAS v2 numerical root finding and minimisation (roadmap phase 4A, item R): `find_root`, `find_minimum`
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
//! use mc_api::{Engine, StopFlag};
//! let mut e = mc_engine::CoreEngine::new();
//! let stop = StopFlag::new();
//! assert_eq!(e.eval("find_root(x^2 - 2, [x, 1])", &stop).unwrap().printed, "[x -> 1.4142135623730951]");
//! ```
//!
//! Decisions of this crate are numbered R.1, R.2, ... in the comments where they are applied.
#![warn(missing_docs)]
mod bracket;
mod func;
mod linalg;
mod lp;
mod minimize;
mod natives;
mod newton;
mod nopt;
mod optim;
mod options;
mod pslq;
mod refine;
mod scalar;

use mc_core::{Heap, Store};
use mc_eval::{Attributes, Kernel};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// Every function of this crate with a one-line description.
pub const FUNCTIONS: &[(&str, &str)] = &[
    (
        "find_root",
        "find_root(f, [x, x0]): a root of f (or of lhs == rhs) near x0 as [x -> root]; [x, x0, x1] secant, [x, x0, a, b] in a bracket, [[x, x0], [y, y0]] systems; digits: d for balls; compile: auto | true | false",
    ),
    (
        "find_minimum",
        "find_minimum(f, [x, x0]): a local minimum near x0 as [f_min, [x -> x_min]]; [[x, x0], [y, y0]] for several variables (BFGS); digits: d for balls; compile: auto | true | false",
    ),
    (
        "find_maximum",
        "find_maximum(f, [x, x0]): a local maximum near x0 as [f_max, [x -> x_max]]; [[x, x0], [y, y0]] for several variables",
    ),
    ("find_min_value", "find_min_value(f, vars): the value of a local minimum (the arguments of find_minimum)"),
    ("find_max_value", "find_max_value(f, vars): the value of a local maximum (the arguments of find_maximum)"),
    ("find_arg_min", "find_arg_min(f, vars): the position of a local minimum (the arguments of find_minimum)"),
    ("find_arg_max", "find_arg_max(f, vars): the position of a local maximum (the arguments of find_maximum)"),
    (
        "n_minimize",
        "n_minimize(f, vars), n_minimize([f, cons...], vars): the global minimum as [f_min, [x -> ...]]; linear programs exactly (simplex, branch and bound for integers), others by multistart augmented Lagrangian with a KKT Newton polish; constraints <=, >=, ==, and, or, element(x, integers), element([x, y], disk()) and other regions",
    ),
    ("n_maximize", "n_maximize(f, vars): the global maximum as [f_max, [x -> ...]] (the arguments of n_minimize)"),
    ("n_min_value", "n_min_value(f, vars): the global minimum value (the arguments of n_minimize)"),
    ("n_max_value", "n_max_value(f, vars): the global maximum value (the arguments of n_minimize)"),
    ("n_arg_min", "n_arg_min(f, vars): the position of the global minimum (the arguments of n_minimize)"),
    ("n_arg_max", "n_arg_max(f, vars): the position of the global maximum (the arguments of n_minimize)"),
    (
        "find_integer_null_vector",
        "find_integer_null_vector([x1, ..., xn]): integers c with c . x = 0 to the precision of x (PSLQ), or a failure with the norm bound; [xs, d] with |c| <= d",
    ),
    (
        "root_approximant",
        "root_approximant(x), root_approximant(x, n): the algebraic number of degree at most n (8) and small height closest to x (PSLQ on its powers)",
    ),
    ("vector_greater_equal", "vector_greater_equal([a, b]): a >= b componentwise (a constraint of n_minimize; [a, b], \"NormCone\" for |a - b| in the norm cone)"),
    ("vector_greater", "vector_greater([a, b]): a > b componentwise"),
    ("vector_less_equal", "vector_less_equal([a, b]): a <= b componentwise"),
    ("vector_less", "vector_less([a, b]): a < b componentwise"),
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

/// Register the natives of this crate with `k`. `stop` is the flag `mc-arith`'s number kernels poll (the
/// engine's arithmetic stop flag).
pub fn install(k: &mut Kernel, h: &Heap, stop: &Arc<AtomicBool>) {
    let a = Attributes::default();
    k.register(h.sym("find_root"), a, Box::new(natives::FindRoot { stop: stop.clone() }));
    k.register(h.sym("find_minimum"), a, Box::new(natives::FindExtremum { stop: stop.clone(), maximum: false }));
    k.register(h.sym("find_maximum"), a, Box::new(natives::FindExtremum { stop: stop.clone(), maximum: true }));
    use nopt::{Optimize, Out, VectorCompare};
    let forms: [(&'static str, bool, bool, Out); 10] = [
        ("n_minimize", false, true, Out::Full),
        ("n_maximize", true, true, Out::Full),
        ("n_min_value", false, true, Out::Value),
        ("n_max_value", true, true, Out::Value),
        ("n_arg_min", false, true, Out::Arg),
        ("n_arg_max", true, true, Out::Arg),
        ("find_min_value", false, false, Out::Value),
        ("find_max_value", true, false, Out::Value),
        ("find_arg_min", false, false, Out::Arg),
        ("find_arg_max", true, false, Out::Arg),
    ];
    let held = Attributes { hold: mc_eval::Hold::All, ..a };
    for (name, maximum, global, out) in forms {
        k.register(h.sym(name), if global { held } else { a }, Box::new(Optimize { stop: stop.clone(), maximum, global, out, name }));
    }
    k.register(h.sym("find_integer_null_vector"), a, Box::new(pslq::FindIntegerNullVector));
    k.register(h.sym("root_approximant"), a, Box::new(pslq::RootApproximant));
    for (name, ge, strict) in [
        ("vector_greater_equal", true, false),
        ("vector_greater", true, true),
        ("vector_less_equal", false, false),
        ("vector_less", false, true),
    ] {
        k.register(h.sym(name), a, Box::new(VectorCompare { ge, strict }));
    }
}
