//! MathCAS v2 calculus (roadmap package 2A): symbolic differentiation.
//!
//! - [`term`]: canonical construction of derivative terms through `mc-arith`;
//! - [`rules`]: the table of derivative rules of the known functions, one row per function (the place to extend);
//! - [`diff`]: the derivative of a term in one variable (an iterative walk over the term's DAG, memoised);
//! - the native of `d`: variable specifications, orders, gradients;
//! - [`integrate`] and [`apart`]: antiderivatives, definite integrals, partial fractions (package 2C);
//! - [`limit`]: one- and two-sided limits at points and infinities (`limit(f, x -> a)`).
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
pub mod apart;
pub mod diff;
pub mod dsolve;
pub mod integrate;
pub mod limit;
pub mod optimize;
pub mod props;
mod natives;
pub mod rules;
pub mod term;
pub mod transforms;

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
    (
        "integrate",
        "integrate(f, x): an antiderivative in x (no constant added); integrate(f, [x, a, b]) the definite integral from a to b",
    ),
    ("apart", "apart(e, x): the partial fraction decomposition of a rational function in x; apart(e) when e has one variable"),
    (
        "limit",
        "limit(f, x -> a): the limit of f as x tends to a (a point, inf or -inf) along the reals; direction: from_above | from_below for one side; failure(no_limit, limit, f) when the sides differ; unevaluated when not decided",
    ),
    ("function_domain", "function_domain(f, x), function_domain(f, [x, y]), function_domain(f, x, complexes): the condition on the variables under which f is defined; [f, cond] restricts"),
    ("function_singularities", "function_singularities(f, x): the condition on x at which f is singular (not defined or not analytic): cos(x) == 0 for tan(x)"),
    ("function_analytic", "function_analytic(f, x): true when f is analytic for every x of the domain (or of [f, cond]), false otherwise"),
    ("function_sign", "function_sign(f, x): 1 when f >= 0 for every x of its domain, -1 when f <= 0, 0 when f == 0, indeterminate otherwise"),
    ("function_monotonicity", "function_monotonicity(f, x): 1 for a nondecreasing f, -1 for a nonincreasing one, 0 for a constant, indeterminate otherwise; integers for a sequence"),
    ("function_convexity", "function_convexity(f, x): 1 for a convex f, -1 for a concave one, 0 for a linear one, indeterminate otherwise"),
    ("function_injective", "function_injective(f, x): true when f takes no value twice on its domain (or [f, cond]), false when it does"),
    ("minimize", "minimize(f, x), minimize([f, cond], x), minimize(f, [x, y]): the global minimum [value, [x -> point]] (one variable: critical points, ends of the domain, limits; several: convex quadratics)"),
    ("maximize", "maximize(f, x): the global maximum [value, [x -> point]], as minimize"),
    ("min_value", "min_value(f, x): the global minimum value"),
    ("max_value", "max_value(f, x): the global maximum value"),
    ("arg_min", "arg_min(f, x): a point where the global minimum is attained"),
    ("arg_max", "arg_max(f, x): a point where the global maximum is attained"),
    ("find_instance", "find_instance(cond, x), find_instance(cond, [x, y]): one point satisfying cond, [[x -> v]] ([] when there is none)"),
    ("d_solve", "d_solve(eqs, y, x): the solutions [[y -> fn([x], sol)]] of an ordinary differential equation for y(x) (constant-coefficient linear, Euler, first-order linear, separable), with conditions y(x0) == v; constants c(1), c(2), ..."),
    ("d_solve_value", "d_solve_value(eqs, y(x), x): the solution of d_solve as an expression"),
    ("r_solve", "r_solve(eqs, a, n): the solutions [[a -> fn([n], sol)]] of a linear recurrence with constant coefficients, with initial values a(k) == v"),
    ("r_solve_value", "r_solve_value(eqs, a(n), n): the solution of r_solve as an expression"),
    (
        "fourier_transform",
        "fourier_transform(f, t, w): the Fourier transform 1/sqrt(2 pi) integrate(f exp(im w t), [t, -inf, inf]) (unit steps in f as bounds, Gaussians in closed form)",
    ),
    (
        "inverse_fourier_transform",
        "inverse_fourier_transform(g, w, t): the inverse transform 1/sqrt(2 pi) integrate(g exp(-im w t), [w, -inf, inf])",
    ),
];

/// Register the natives of this package with `k`. `stop` is the flag `mc-arith`'s number kernels poll (the
/// engine's arithmetic stop flag).
pub fn install(k: &mut Kernel, h: &Heap, stop: &Arc<AtomicBool>) {
    k.register(h.sym("d"), Attributes::default(), Box::new(natives::D { stop: stop.clone() }));
    k.register(h.sym("derivative"), Attributes::default(), Box::new(natives::DerivativeAt { stop: stop.clone() }));
    // integrate reads the assumption context: symbolic bounds and parameters decided by its facts (decision 2C4.5)
    let reads = Attributes { uses_assumptions: true, ..Attributes::default() };
    k.register(h.sym("integrate"), reads, Box::new(natives::Integrate { stop: stop.clone() }));
    k.register(h.sym("apart"), Attributes::default(), Box::new(natives::Apart { stop: stop.clone() }));
    for (name, p) in [
        ("function_domain", props::Prop::Domain),
        ("function_singularities", props::Prop::Singularities),
        ("function_analytic", props::Prop::Analytic),
        ("function_sign", props::Prop::Sign),
        ("function_monotonicity", props::Prop::Monotonicity),
        ("function_convexity", props::Prop::Convexity),
        ("function_injective", props::Prop::Injective),
    ] {
        k.register(h.sym(name), Attributes::default(), Box::new(props::PropNative { p, stop: stop.clone() }));
    }
    for (name, kind) in [
        ("minimize", optimize::Kind::Minimize),
        ("maximize", optimize::Kind::Maximize),
        ("min_value", optimize::Kind::MinValue),
        ("max_value", optimize::Kind::MaxValue),
        ("arg_min", optimize::Kind::ArgMin),
        ("arg_max", optimize::Kind::ArgMax),
        ("find_instance", optimize::Kind::FindInstance),
    ] {
        k.register(h.sym(name), Attributes::default(), Box::new(optimize::OptNative { kind, stop: stop.clone() }));
    }
    for (name, kind) in [
        ("d_solve", dsolve::Kind::DSolve),
        ("d_solve_value", dsolve::Kind::DSolveValue),
        ("r_solve", dsolve::Kind::RSolve),
        ("r_solve_value", dsolve::Kind::RSolveValue),
    ] {
        k.register(h.sym(name), Attributes::default(), Box::new(dsolve::DSolveNative { kind, stop: stop.clone() }));
    }
    k.register(h.sym("fourier_transform"), Attributes::default(), Box::new(transforms::Fourier { inverse: false, stop: stop.clone() }));
    k.register(h.sym("inverse_fourier_transform"), Attributes::default(), Box::new(transforms::Fourier { inverse: true, stop: stop.clone() }));
    k.register(h.sym("limit"), Attributes::default(), Box::new(limit::LimitFn { stop: stop.clone() }));
}
