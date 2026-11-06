//! MathCAS v2 numerical ordinary differential equations (numerics roadmap, package N3): `n_dsolve`,
//! `n_dsolve_value` and the `interpolating_function` results, as natives of the evaluator.
//!
//! - `system`: the equations reduced to a first-order system `Y' = F(t, Y)` with initial values;
//! - `model`: `F`, its Jacobian and `dF/dt` at machine reals, through `mc_numfn::compile_expr` (compiled by
//!   `mc-jit`, the interpreter as the fallback);
//! - `dopri`, `rodas`, `step`, `driver`: Dormand–Prince 5(4), the Rosenbrock method Rodas4 for stiff systems,
//!   the automatic switch between them, step size control and the integration loop;
//! - `dense`, `events`: dense output and zero crossings of event functions;
//! - `taylor`: the Taylor series method on balls under `digits:`;
//! - `solution`: solutions kept by the session and the `interpolating_function` terms naming them;
//! - `parametric`: `parametric_n_dsolve_value`, the system reduced once into a `parametric_function` term;
//! - `natives`: argument forms, results and failures.
//!
//! ```
//! use mc_bench::{Engine, StopFlag};
//! let mut e = mc_engine::CoreEngine::new();
//! let stop = StopFlag::new();
//! let v = e.eval("n_dsolve_value([y'(t) == -y(t), y(0) == 1], y(1), [t, 0, 1])", &stop).unwrap().printed;
//! assert!((v.parse::<f64>().unwrap() - (-1.0f64).exp()).abs() < 1e-7);
//! ```
//!
//! Decisions of this package are numbered N3.1, N3.2, ... in the comments where they are applied.
#![warn(missing_docs)]
mod dense;
mod dopri;
mod driver;
mod events;
mod lu;
mod model;
mod natives;
mod options;
mod parametric;
mod rodas;
mod solution;
mod step;
mod system;
mod taylor;
mod util;

use mc_core::{Heap, Store};
use mc_eval::{Attributes, Kernel};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// Every function of this package with a one-line description.
pub const FUNCTIONS: &[(&str, &str)] = &[
    (
        "n_dsolve",
        "n_dsolve(eqs, y, [t, t0, t1]): the numerical solution of ordinary differential equations with initial conditions as [y -> interpolating_function(...)]; [y, z] for systems; method: auto | rk45 | stiff | taylor, accuracy_goal, precision_goal, max_steps, max_step_size, max_step_fraction, events: [g == 0], event_action: stop | record, compile: auto | true | false; digits: d for balls (Taylor series; error estimated, not guaranteed)",
    ),
    (
        "n_dsolve_value",
        "n_dsolve_value(eqs, expr, [t, t0, t1]): expr (in y(t), y'(t), ...) along the numerical solution: a closure of t, or a number for expr = y(t1); the options of n_dsolve",
    ),
    (
        "parametric_n_dsolve_value",
        "parametric_n_dsolve_value(eqs, expr, [t, t0, t1], [p1, ..., pm]): a parametric_function f of the parameters, the equations reduced once; f(v1, ..., vm) is n_dsolve_value with the parameters set to the values (initial values and options may use them); the options of n_dsolve",
    ),
    (
        "parametric_function",
        "parametric_function(params, expr, data): the result of parametric_n_dsolve_value; f(v1, ..., vm) integrates with the parameters set to the numbers v1, ..., vm (a symbolic value leaves the call as it is)",
    ),
    (
        "interpolating_function",
        "interpolating_function(\"y\", [lo, hi], steps, key): a solution of n_dsolve; f(t) is its value at t in [lo, hi] (a list of times gives a list; exact times such as pi/2 through n), failure(out_of_range, t, [lo, hi]) outside; \"y'\" names the derivative, which d(f(u), u) gives",
    ),
    ("domain", "domain(f): the interval [lo, hi] of an interpolating function (or of the closure of n_dsolve_value) where it has values"),
];

/// Register the natives of this package with `k`. `stop` is the flag `mc-arith`'s number kernels poll (the
/// engine's arithmetic stop flag). The solutions of the session live in one store shared by the natives.
pub fn install(k: &mut Kernel, h: &Heap, stop: &Arc<AtomicBool>) {
    let a = Attributes::default();
    let store = Arc::new(solution::Store::default());
    let solve = |kind| natives::Solve { stop: stop.clone(), store: store.clone(), kind };
    k.register(h.sym("n_dsolve"), a, Box::new(solve(natives::Kind::Rules)));
    k.register(h.sym("n_dsolve_value"), a, Box::new(solve(natives::Kind::Value)));
    k.register(h.sym("parametric_n_dsolve_value"), a, Box::new(solve(natives::Kind::Parametric)));
    k.register(
        h.sym("parametric_function"),
        a,
        Box::new(natives::ParametricCall { solve: solve(natives::Kind::Parametric) }),
    );
    k.register(
        h.sym("interpolating_function"),
        a,
        Box::new(natives::Interpolating { stop: stop.clone(), store: store.clone() }),
    );
    k.register(h.sym("domain"), a, Box::new(natives::DomainOf));
}
