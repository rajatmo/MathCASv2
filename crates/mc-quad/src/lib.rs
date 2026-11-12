//! MathCAS v2 numerical integration (numerics roadmap, item N2): `n_integrate` as a native of the evaluator.
//!
//! - `rules`: Gauss–Kronrod rules (7-15 and 30-61 points), computed once per process;
//! - `machine`, `gk`, `de`, `nested`: machine precision: adaptive Gauss–Kronrod with a global error heap, the
//!   double-exponential rules, infinite ranges by substitution, multiple integrals;
//! - `hp`: the double-exponential rules on balls under `digits:`;
//! - `rigorous`: guaranteed enclosures by Clenshaw–Curtis with Bernstein-ellipse error bounds (`rigorous: true`);
//! - `domain`, `multi`, `cubature`, `mc`, `qmc`, `qmc_tables`: multiple integrals on the unit cube with the
//!   integrand evaluated in parallel blocks: Genz–Malik adaptive cubature, VEGAS+ and plain Monte Carlo, randomised
//!   lattice rules and scrambled Sobol points (decisions quad-1.1, quad-1.2, ...);
//! - `options`, `natives`: options and goals; argument forms, method choice, results and failures.
//!
//! ```
//! use mc_api::{Engine, StopFlag};
//! let mut e = mc_engine::CoreEngine::new();
//! let stop = StopFlag::new();
//! assert_eq!(e.eval("n_integrate(exp(-x^2), [x, -inf, inf])", &stop).unwrap().printed, "1.7724538509055159");
//! ```
//!
//! Decisions of this crate are numbered N2.1, N2.2, ... in the comments where they are applied.
#![warn(missing_docs)]
mod cubature;
mod de;
mod domain;
mod fft;
mod gk;
mod hp;
mod machine;
mod mc;
mod multi;
mod natives;
mod nested;
mod osc;
mod oscb;
mod options;
mod qmc;
mod qmc_tables;
mod regions;
mod rigorous;
mod rules;

use mc_core::{Heap, Store};
use mc_eval::{Attributes, Kernel};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// Every function of this crate with a one-line description.
pub const FUNCTIONS: &[(&str, &str)] = &[(
    "n_integrate",
    "n_integrate(f, [x, a, b]): the integral of f from a to b (a, b may be -inf, inf); [x, a, s1, ..., b] splits at s1, ...; more ranges [y, c(x), d(x)] for multiple integrals (adaptive cubature for 2 to 7 variables, lattice quasi-Monte Carlo or VEGAS above); method: gauss_kronrod | double_exponential | nested | cubature | vegas | lattice | sobol | monte_carlo; precision_goal, accuracy_goal, max_points, points, iterations, seed; sampling methods give around(value, error); digits: d for a ball whose radius is an error estimate; rigorous: true for a ball guaranteed to contain the integral",
)];

/// The numerics library (`lib/numerics.mc`): functions written in the language itself,
/// evaluated by the engine after the language library so that they are defined in every session.
pub const LIBRARY_SOURCE: &str = include_str!("../../../lib/numerics.mc");

/// The name of [`LIBRARY_SOURCE`] in messages.
pub const LIBRARY_ORIGIN: &str = "lib/numerics.mc";

/// Every public function of the numerics library with a one-line description.
pub const LIBRARY_FUNCTIONS: &[(&str, &str)] = &[
    ("ratios", "ratios(xs): the successive ratios xs[i + 1]/xs[i]; ratios(xs, k): k times; ratios(xs, [k1, k2, ...]): k1 times at level 1, k2 at level 2, ..."),
    ("linear_recurrence", "linear_recurrence(ker, init, n): the first n terms of a[k] = ker[1] a[k - 1] + ... + ker[d] a[k - d] from init; (ker, init, [m]), (ker, init, [m1, m2]): term m, terms m1 to m2 (m1 <= 0 runs backwards)"),
    ("threshold", "threshold(data), (data, delta), (data, [\"Hard\" | \"Soft\" | \"Firm\" | \"PiecewiseGarrote\" | \"SmoothGarrote\" | \"Hyperbola\", delta, ...]), (data, [\"LargestValues\", k]): data with the small values set to zero"),
    ("cantor_staircase", "cantor_staircase(x): the Cantor function, exact at rational x, numeric at inexact x; 0 for x <= 0 and 1 for x >= 1"),
    ("dirac_comb", "dirac_comb(x1, ..., xk): the Dirac comb, a sum of delta functions at the integers (a product over the arguments): 0 at numbers that are not integers"),
    ("mandelbrot_set_member_q", "mandelbrot_set_member_q(c): whether the orbit of 0 under z^2 + c stays bounded for max_iterations steps (default 1000); lists elementwise"),
    ("interpolating_polynomial", "interpolating_polynomial(data, x): the polynomial of least degree through values [f1, ...] at 1, 2, ..., points [[x1, f1], ...] or Hermite data [[x1, f1, d1, ...], ...], in Newton form"),
    ("fourier", "fourier(list), fourier(array): the discrete Fourier transform 1/n^((1 - a)/2) sum u[r] exp(2 pi i b (r - 1)(s - 1)/n) along every level, fourier_parameters [a, b] (default [0, 1]), at the precision of the data"),
    ("inverse_fourier", "inverse_fourier(list), inverse_fourier(array): the inverse discrete Fourier transform, with the same fourier_parameters as fourier"),
    ("d_fixed_points", "d_fixed_points(eqs, vars, t): the fixed points of ordinary differential equations: the constant solutions, as states [y, y', ...] (derivatives 0), one list per point"),
    ("r_fixed_points", "r_fixed_points(eqs, vars, n): the fixed points of difference equations: the constant solutions, as states [y, ..., y] (one entry per step of the order), one list per point"),
    ("parametric_nd_solve", "parametric_nd_solve(eqs, y, [t, t0, t1], [p1, ...]): [y -> f] with f the parametric function parametric_n_dsolve_value(eqs, y, ...) of the parameters"),
    ("parametric_nd_solve_value", "parametric_nd_solve_value(eqs, expr, [t, t0, t1], [p1, ...]): another name of parametric_n_dsolve_value"),
    ("fourier_coefficient", "fourier_coefficient(f, t, n): the n-th exponential Fourier coefficient (|b|/(2 pi))^((1 + a)/2) times the integral of f exp(-i b n t) over a period [-pi/|b|, pi/|b|]; symbolic n with its exceptions; fourier_parameters [a, b]"),
    ("fourier_cos_coefficient", "fourier_cos_coefficient(f, t, n): the n-th cosine coefficient |b|/pi times the integral of f cos(b n t) over [-pi/|b|, pi/|b|]"),
    ("fourier_sin_coefficient", "fourier_sin_coefficient(f, t, n): the n-th sine coefficient |b|/pi times the integral of f sin(b n t) over [-pi/|b|, pi/|b|]"),
    ("fourier_series", "fourier_series(f, t, k): the exponential Fourier series of f to order k, the sum of the coefficients times exp(i b n t) for n from -k to k"),
    ("fourier_trig_series", "fourier_trig_series(f, t, k): the trigonometric Fourier series of f to order k, a0/2 plus a_n cos(b n t) + b_n sin(b n t) for n from 1 to k"),
    ("fourier_cos_series", "fourier_cos_series(f, t, k): the cosine series of f to order k, a0/2 plus a_n cos(b n t) for n from 1 to k"),
    ("fourier_sin_series", "fourier_sin_series(f, t, k): the sine series of f to order k, the sum of b_n sin(b n t) for n from 1 to k"),
    ("unilateral_convolve", "unilateral_convolve(f, g, tau, t): the convolution on [0, inf), the integral of f(tau) g(t - tau) for tau from 0 to t; lists of variables for several dimensions"),
    ("mellin_convolve", "mellin_convolve(f, g, x, y): the Mellin convolution, the integral of f(x) g(y/x)/x for x from 0 to inf; lists of variables for several dimensions"),
    ("list_interpolation", "list_interpolation(array), (array, [[xmin, xmax], ...]), (array, [[x1, x2, ...], ...]): a function interpolating the values on a grid by piecewise Lagrange polynomials of interpolation_order (default 3)"),
    ("function_interpolation", "function_interpolation(f, [x, a, b], ...): a function interpolating the expression f sampled on a grid over the ranges, as list_interpolation"),
    ("mean_around", "mean_around(xs): the mean of numbers with its standard error as around(m, s/sqrt(n)); of around values the weighted mean; of vectors vector_around(mean, covariance/n)"),
    ("random_polygon", "random_polygon(), (k), (type), ([type, k]), ([type, dist, k]), (spec, m), (spec, [m1, ...]), (d -> spec): a random polygon in the unit square (\"Simple\", \"Convex\", \"StarShaped\", \"ConvexHull\"), m of them, or one in a random plane of dimension d"),
    ("find_peaks", "find_peaks(data), (data, sigma), (data, sigma, s), (data, sigma, s, t): the peaks [position, value] of a list: local maxima (plateaus at their middle) after Gaussian blurring by sigma, sharpness above s, value above t"),
];

/// Register the natives of this crate with `k`. `stop` is the flag `mc-arith`'s number kernels poll (the
/// engine's arithmetic stop flag).
pub fn install(k: &mut Kernel, h: &Heap, stop: &Arc<AtomicBool>) {
    k.register(h.sym("n_integrate"), Attributes::default(), Box::new(natives::NIntegrate { stop: stop.clone() }));
    k.register(h.sym(natives::TAIL_EXP), Attributes::default(), Box::new(natives::TailExp));
    k.register(h.sym(fft::NAME), Attributes::default(), Box::new(fft::Fft));
}
