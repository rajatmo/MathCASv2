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
        "n_dsolve(eqs, y, [t, t0, t1]): the numerical solution of ordinary differential equations with initial conditions as [y -> interpolating_function(...)]; [y, z] for systems; method: auto | rk45 | stiff | taylor, accuracy_goal, precision_goal, max_steps, max_step_size, events: [g == 0], event_action: stop | record, compile: auto | true | false; digits: d for balls (Taylor series; error estimated, not guaranteed)",
    ),
    (
        "n_dsolve_value",
        "n_dsolve_value(eqs, expr, [t, t0, t1]): expr (in y(t), y'(t), ...) along the numerical solution: a closure of t, or a number for expr = y(t1); the options of n_dsolve",
    ),
    (
        "interpolating_function",
        "interpolating_function(\"y\", [lo, hi], steps, key): a solution of n_dsolve; f(t) is its value at t in [lo, hi] (a list of times gives a list), failure(out_of_range, t, [lo, hi]) outside; \"y'\" names the derivative",
    ),
];

/// Register the natives of this package with `k`. `stop` is the flag `mc-arith`'s number kernels poll (the
/// engine's arithmetic stop flag). The solutions of the session live in one store shared by the natives.
pub fn install(k: &mut Kernel, h: &Heap, stop: &Arc<AtomicBool>) {
    let a = Attributes::default();
    let store = Arc::new(solution::Store::default());
    let solve = |value| Box::new(natives::Solve { stop: stop.clone(), store: store.clone(), value });
    k.register(h.sym("n_dsolve"), a, solve(false));
    k.register(h.sym("n_dsolve_value"), a, solve(true));
    k.register(
        h.sym("interpolating_function"),
        a,
        Box::new(natives::Interpolating { stop: stop.clone(), store: store.clone() }),
    );
}
