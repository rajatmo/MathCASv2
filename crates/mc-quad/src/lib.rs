//! MathCAS v2 numerical integration (numerics roadmap, package N2): `n_integrate` as a native of the evaluator.
//!
//! - `rules`: Gauss–Kronrod rules (7-15 and 30-61 points), computed once per process;
//! - `machine`, `gk`, `de`, `nested`: machine precision: adaptive Gauss–Kronrod with a global error heap, the
//!   double-exponential rules, infinite ranges by substitution, multiple integrals;
//! - `hp`: the double-exponential rules on balls under `digits:`;
//! - `rigorous`: guaranteed enclosures by Clenshaw–Curtis with Bernstein-ellipse error bounds (`rigorous: true`);
//! - `domain`, `multi`, `cubature`, `mc`, `qmc`, `qmc_tables`: multiple integrals on the unit cube with the
//!   integrand evaluated in parallel blocks: Genz–Malik adaptive cubature, VEGAS+ and plain Monte Carlo, randomised
//!   lattice rules and scrambled Sobol points (package quad-1, decisions quad-1.1, quad-1.2, ...);
//! - `options`, `natives`: options and goals; argument forms, method choice, results and failures.
//!
//! ```
//! use mc_bench::{Engine, StopFlag};
//! let mut e = mc_engine::CoreEngine::new();
//! let stop = StopFlag::new();
//! assert_eq!(e.eval("n_integrate(exp(-x^2), [x, -inf, inf])", &stop).unwrap().printed, "1.7724538509055163");
//! ```
//!
//! Decisions of this package are numbered N2.1, N2.2, ... in the comments where they are applied.
#![warn(missing_docs)]
mod cubature;
mod de;
mod domain;
mod gk;
mod hp;
mod machine;
mod mc;
mod multi;
mod natives;
mod nested;
mod options;
mod qmc;
mod qmc_tables;
mod rigorous;
mod rules;

use mc_core::{Heap, Store};
use mc_eval::{Attributes, Kernel};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// Every function of this package with a one-line description.
pub const FUNCTIONS: &[(&str, &str)] = &[(
    "n_integrate",
    "n_integrate(f, [x, a, b]): the integral of f from a to b (a, b may be -inf, inf); [x, a, s1, ..., b] splits at s1, ...; more ranges [y, c(x), d(x)] for multiple integrals (adaptive cubature for 2 to 7 variables, lattice quasi-Monte Carlo or VEGAS above); method: gauss_kronrod | double_exponential | nested | cubature | vegas | lattice | sobol | monte_carlo; precision_goal, accuracy_goal, max_points, points, iterations, seed; sampling methods give around(value, error); digits: d for a ball whose radius is an error estimate; rigorous: true for a ball guaranteed to contain the integral",
)];

/// Register the natives of this package with `k`. `stop` is the flag `mc-arith`'s number kernels poll (the
/// engine's arithmetic stop flag).
pub fn install(k: &mut Kernel, h: &Heap, stop: &Arc<AtomicBool>) {
    k.register(h.sym("n_integrate"), Attributes::default(), Box::new(natives::NIntegrate { stop: stop.clone() }));
}
