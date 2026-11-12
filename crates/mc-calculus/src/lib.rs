//! MathCAS v2 calculus (roadmap item 2A): symbolic differentiation, partial fractions, the properties of
//! functions and optimisation; the helpers and the derivative rules every calculus crate uses.
//!
//! - [`term`]: canonical construction of derivative terms through `mc-arith`;
//! - [`rules`]: the table of derivative rules of the known functions, one row per function (the place to extend);
//! - [`diff`]: the derivative of a term in one variable (an iterative walk over the term's DAG, memoised);
//! - the native of `d`: variable specifications, orders, gradients;
//! - [`apart`]: partial fractions (item 2C); [`props`]: properties of functions; [`optimize`]: global extrema;
//! - [`common`]: helpers of every calculus crate; [`transform_kind`]: the integral transforms by name.
//!
//! The other areas of calculus are crates of their own above this one (round 34): `mc-limit` (limits and
//! asymptotics), `mc-integrals` (`integrate`, definite and indefinite), `mc-transforms` (integral transforms) and
//! `mc-dsolve` (differential and recurrence equations).
//!
//! The derivative operator is the term `derivative(n1, ..., nk)(f)`, which the parser writes for `f'`, `f''`, ...
//! and the printer shows with prime marks; differentiating an unknown function gives it (chain rule), and a known
//! function inside it is resolved when it is differentiated (`d(sin'(x), x)` is `-sin(x)`).
//!
//! ```
//! use mc_api::{Engine, StopFlag};
//! let mut e = mc_engine::CoreEngine::new();
//! let stop = StopFlag::new();
//! assert_eq!(e.eval("d(x^3 + sin(x), x)", &stop).unwrap().printed, "cos(x) + 3*x^2");
//! assert_eq!(e.eval("d(f(g(x)), x)", &stop).unwrap().printed, "f'(g(x))*g'(x)");
//! ```
//!
//! Decisions of this crate are numbered 2A.1, 2A.2, ... in the comments where they are applied.
#![warn(missing_docs)]
pub mod apart;
pub mod common;
pub mod diff;
pub mod optimize;
pub mod props;
mod natives;
pub mod numdiff;
pub mod rules;
pub mod term;
pub mod transform_kind;

use common::wrap;
use mc_core::{Heap, Store};
use mc_eval::{Attributes, Kernel};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// The calculus library written in MathCAS itself (`lib/calculus.mc`: the calculus libraries of rounds 28, 29
/// and 31, merged in round 34 in their load order): evaluated by the engine after the language library, so
/// its functions are defined in every session. They are listed in [`FUNCTIONS`].
pub const LIB_SOURCE: &str = include_str!("../../../lib/calculus.mc");

/// The name of [`LIB_SOURCE`] in messages.
pub const LIB_ORIGIN: &str = "lib/calculus.mc";

/// Every function of calculus with a one-line description: this crate's and those of `mc-integrals`, `mc-limit`,
/// `mc-dsolve` and `mc-transforms` (one table for the area, in the order the catalogue and the help index list them).
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
    ("risch_integrate", "risch_integrate(f, x): the Risch decision procedure alone (one exponential or logarithm over the rational functions of x): the antiderivative, failure(nonelementary, risch_integrate, reason) when it proves that none is elementary, unevaluated outside the procedure"),
    ("apart", "apart(e, x): the partial fraction decomposition of a rational function in x; apart(e) when e has one variable"),
    (
        "limit",
        "limit(f, x -> a): the limit of f as x tends to a (a point, inf or -inf) along the reals; direction: from_above | from_below for one side; failure(no_limit, limit, f) when the sides differ; unevaluated when not decided",
    ),
    ("max_limit", "max_limit(f, x -> a): the limit superior of f as x tends to a (the limit when it exists; the largest value an oscillation keeps reaching); direction: as for limit"),
    ("min_limit", "min_limit(f, x -> a): the limit inferior of f as x tends to a; direction: as for limit"),
    ("discrete_limit", "discrete_limit(f, n -> inf): the limit of the sequence f(n) over the integers; failure(no_limit, discrete_limit, f) when the even and odd terms differ"),
    ("asymptotic", "asymptotic(f, x -> a): the leading term of the asymptotic expansion of f as x tends to a (a point, inf or -inf): asymptotic(gamma(x), x -> inf) is sqrt(2 pi) x^(x - 1/2) e^(-x)"),
    ("function_domain", "function_domain(f, x), function_domain(f, [x, y]), function_domain(f, x, complexes): the condition on the variables under which f is defined; [f, cond] restricts"),
    ("function_singularities", "function_singularities(f, x): the condition on x at which f is singular (not defined or not analytic): cos(x) == 0 for tan(x)"),
    ("function_analytic", "function_analytic(f, x): true when f is analytic for every x of the domain (or of [f, cond]), false otherwise"),
    ("function_sign", "function_sign(f, x): 1 when f >= 0 for every x of its domain, -1 when f <= 0, 0 when f == 0, indeterminate otherwise"),
    ("function_monotonicity", "function_monotonicity(f, x): 1 for a nondecreasing f, -1 for a nonincreasing one, 0 for a constant, indeterminate otherwise; integers for a sequence"),
    ("function_convexity", "function_convexity(f, x): 1 for a convex f, -1 for a concave one, 0 for a linear one, indeterminate otherwise"),
    ("function_injective", "function_injective(f, x): true when f takes no value twice on its domain (or [f, cond]), false when it does"),
    ("function_discontinuities", "function_discontinuities(f, x), function_discontinuities(f, [x, y], complexes): the condition on the variables at which f is not continuous (poles, branch cuts and the boundary of the real domain, jumps)"),
    ("function_continuous", "function_continuous(f, x), function_continuous([f, cond], [x, y], complexes): true when f is continuous wherever cond holds, false when not; conditional_expression(true, c) with parameters"),
    ("function_period", "function_period(f, x): the smallest positive period of f in x (0 when f is not periodic); function_period(f, n, integers) of a sequence; [p1, p2] for several variables; [p] over the complexes"),
    ("function_surjective", "function_surjective(f, x), function_surjective([f, cond, ycond], x, y), function_surjective([f1, f2], [x, y], complexes): true when every point of the codomain (where ycond holds) is a value of f on the domain (where cond holds)"),
    ("function_bijective", "function_bijective(f, x), function_bijective([f, cond, ycond], x, y): true when f is injective and surjective from the domain onto the codomain"),
    ("piecewise_expand", "piecewise_expand(f), piecewise_expand(f, cond): nested piecewise, if, max, min, clip, unit_step, boole and abs as one piecewise([[v1, c1], ...], d); floor, ceiling, round and prime_pi of one variable inside a bounded cond"),
    ("function_poles", "function_poles(f, z), function_poles([f, cond], z): the poles of a meromorphic f with their orders, [[p1, m1], ...]; a family of poles as conditional_expression(p(c(1)), element(c(1), integers))"),
    ("function_meromorphic", "function_meromorphic(f, z): true when f is meromorphic (analytic but for poles) in the complex variables, or where cond holds for [f, cond]; false when not"),
    ("minimize", "minimize(f, x), minimize([f, cond], x), minimize(f, [x, y]): the global minimum [value, [x -> point]] (one variable: critical points, ends of the domain, limits; several: convex quadratics)"),
    ("maximize", "maximize(f, x): the global maximum [value, [x -> point]], as minimize"),
    ("min_value", "min_value(f, x): the global minimum value"),
    ("max_value", "max_value(f, x): the global maximum value"),
    ("arg_min", "arg_min(f, x): a point where the global minimum is attained"),
    ("arg_max", "arg_max(f, x): a point where the global maximum is attained"),
    ("function_range", "function_range(f, x, y): the values of f as a condition on y (a <= y and y <= b, y >= a, ...), from the global minimum and maximum"),
    ("find_instance", "find_instance(cond, x), find_instance(cond, [x, y]): one point satisfying cond, [[x -> v]] ([] when there is none)"),
    ("d_solve", "d_solve(eqs, y, x): the solutions [[y -> fn([x], sol)]] of differential equations for y(x) (linear of any order, the classical first-order classes, Riccati, special-function and Liouvillian bases, linear systems, first-order linear and wave PDEs, delay equations), with conditions y(x0) == v; constants c(1), c(2), ...; every solution checked by substitution"),
    ("d_solve_value", "d_solve_value(eqs, y(x), x): the solution of d_solve as an expression (fn([x], sol) for y, any expression in y such as y(3) or y'(x))"),
    ("asymptotic_d_solve_value", "asymptotic_d_solve_value(eqs, y(x), [x, x0, n]): a series solution of degree n at x0 (Taylor at an ordinary point, Frobenius at a regular singular point); asymptotic_d_solve_value(eqs, y(x), x, [e, 0, n]) the regular perturbation series in e"),
    ("r_solve", "r_solve(eqs, a, n): the solutions [[a -> fn([n], sol)]] of a linear recurrence with constant coefficients, with initial values a(k) == v"),
    ("r_solve_value", "r_solve_value(eqs, a(n), n): the solution of r_solve as an expression"),
    ("laplace_transform", "laplace_transform(f, t, s): the Laplace transform integrate(f exp(-s t), [t, 0, inf]); laplace_transform(f, [t1, t2], [s1, s2]) in several variables; generate_conditions: true for the region of convergence"),
    ("inverse_laplace_transform", "inverse_laplace_transform(F, s, t): the inverse Laplace transform (rational functions by residues, delays, shifts, a table, the derivative rule for logarithms)"),
    ("bilateral_laplace_transform", "bilateral_laplace_transform(f, t, s): the two-sided Laplace transform integrate(f exp(-s t), [t, -inf, inf]), with its strip of convergence as a conditional_expression"),
    ("inverse_bilateral_laplace_transform", "inverse_bilateral_laplace_transform(F, s, t): the inverse two-sided Laplace transform; conditional_expression(F, a < re(s) < b) gives the strip"),
    ("mellin_transform", "mellin_transform(f, x, s): the Mellin transform integrate(x^(s - 1) f, [x, 0, inf])"),
    ("inverse_mellin_transform", "inverse_mellin_transform(F, s, x): the inverse Mellin transform (Mellin-Barnes residue series, a table); assumptions: a < re(s) < b selects the strip"),
    (
        "fourier_transform",
        "fourier_transform(f, t, w): the Fourier transform 1/sqrt(2 pi) integrate(f exp(im w t), [t, -inf, inf]), distributions included; fourier_parameters: [a, b] for other conventions",
    ),
    (
        "inverse_fourier_transform",
        "inverse_fourier_transform(g, w, t): the inverse transform 1/sqrt(2 pi) integrate(g exp(-im w t), [w, -inf, inf])",
    ),
    ("fourier_sin_transform", "fourier_sin_transform(f, t, w): the sine transform sqrt(2/pi) integrate(f sin(w t), [t, 0, inf])"),
    ("inverse_fourier_sin_transform", "inverse_fourier_sin_transform(g, w, t): the inverse sine transform (the same integral in w)"),
    ("fourier_cos_transform", "fourier_cos_transform(f, t, w): the cosine transform sqrt(2/pi) integrate(f cos(w t), [t, 0, inf])"),
    ("inverse_fourier_cos_transform", "inverse_fourier_cos_transform(g, w, t): the inverse cosine transform (the same integral in w)"),
    ("z_transform", "z_transform(f, n, z): the Z transform sum(f(n) z^-n, [n, 0, inf]); generate_conditions: true for abs(z) > r"),
    ("inverse_z_transform", "inverse_z_transform(F, z, n): the inverse Z transform of a rational function of z (residues and delta functions)"),
    ("discrete_delta", "discrete_delta(n1, ..., nk): 1 when every argument is 0, 0 when one is a nonzero number"),
    ("kronecker_delta", "kronecker_delta(n1, ..., nk): 1 when the arguments are equal, 0 when they are different numbers"),
    // round 28, written in MathCAS (lib/calculus.mc)
    ("triangle_wave", "triangle_wave(x), triangle_wave([a, b], x): the triangle wave of period 1 between -1 and 1 (a and b), 0 at x = 0"),
    ("square_wave", "square_wave(x), square_wave([a, b], x): the square wave of period 1, 1 (b) on the first half of each period, -1 (a) on the second"),
    ("sawtooth_wave", "sawtooth_wave(x), sawtooth_wave([a, b], x): the sawtooth wave x - floor(x) of period 1 (scaled to run from a to b)"),
    ("heaviside_lambda", "heaviside_lambda(x), heaviside_lambda(x, y, ...): the triangle 1 - |x| for |x| < 1, else 0 (a product over the arguments)"),
    ("heaviside_pi", "heaviside_pi(x), heaviside_pi(x, y, ...): 1 for |x| < 1/2, 0 for |x| > 1/2, undefined at |x| = 1/2 (a product)"),
    ("alternating_harmonic_number", "alternating_harmonic_number(m), (m, r), (m, r, s): the sum of (-1)^(k + 1) s^k/k^r for k = 1, ..., m"),
    ("alternating_factorial", "alternating_factorial(m): the sum of (-1)^(m - k) k! for k = 1, ..., m, continued to inexact m"),
    ("hyper_harmonic_number", "hyper_harmonic_number(r, m), (r, m, s): the hyperharmonic number, the r-fold running sum of 1/k (1/k^s) up to m"),
    ("multiple_harmonic_number", "multiple_harmonic_number(m, [r1, ...], [s1, ...]): the sum over m >= n1 > n2 > ... >= 1 of s1^n1/n1^r1 s2^n2/n2^r2 ..."),
    ("asymptotic_less_equal", "asymptotic_less_equal(f, g, x -> a): whether f = O(g) as x approaches a (limsup |f/g| finite); direction:, assumptions:"),
    ("asymptotic_equal", "asymptotic_equal(f, g, x -> a): whether f and g are of the same order as x approaches a (f = O(g) and g = O(f))"),
    ("asymptotic_greater_equal", "asymptotic_greater_equal(f, g, x -> a): whether g = O(f) as x approaches a"),
    ("asymptotic_equivalent", "asymptotic_equivalent(f, g, x -> a): whether f ~ g as x approaches a (f/g tends to 1)"),
    ("asymptotic_greater", "asymptotic_greater(f, g, x -> a): whether g = o(f) as x approaches a (g/f tends to 0)"),
    ("asymptotic_less", "asymptotic_less(f, g, x -> a): whether f = o(g) as x approaches a (f/g tends to 0)"),
    ("difference_quotient", "difference_quotient(f, [x, h]), (f, [x, k, h]), (f, spec1, spec2, ...): the (k-th) forward difference quotient of f in x with step h"),
    ("wronskian", "wronskian([f1, ..., fk], x): the Wronskian determinant; wronskian(eq, y, x): Abel's form for a linear differential equation"),
    ("casoratian", "casoratian([f1, ..., fk], m): the Casoratian determinant of the shifted values fi(m + j)"),
    ("hankel_transform", "hankel_transform(f, r, s), (f, r, s, nu): integrate(f bessel_j(nu, s r) r, [r, 0, inf]) for powers, exponentials, Gaussians, (r^2 + a^2)^-p and deltas"),
    ("inverse_hankel_transform", "inverse_hankel_transform(g, s, r), (g, s, r, nu): the inverse Hankel transform (the same integral in s)"),
    ("hilbert_transform", "hilbert_transform(f, t, s): (1/pi) PV integrate(f(t)/(s - t), [t, -inf, inf]) for sinusoids times rational functions, rational functions (real poles give deltas), deltas and piecewise polynomials of compact support; option assumptions:"),
    ("inverse_hilbert_transform", "inverse_hilbert_transform(g, s, t): the inverse Hilbert transform, -hilbert_transform(g, s, t)"),
    ("fourier_sequence_transform", "fourier_sequence_transform(f, k, w): the sum of f(k) exp(-im k w) over the integers (deltas and one-sided terms through z_transform)"),
    ("discrete_max_limit", "discrete_max_limit(f, k -> inf): the upper limit of the sequence f(k) over the integers (k -> -inf too)"),
    ("discrete_min_limit", "discrete_min_limit(f, k -> inf): the lower limit of the sequence f(k) over the integers"),
    ("discrete_asymptotic", "discrete_asymptotic(f, k -> inf): the leading asymptotic form of the sequence f(k) (Stirling's formula for factorials, then asymptotic)"),
    ("difference_root", "difference_root(f)(k): the k-th term of the sequence defined by f(y, n) = [recurrence, initial values]"),
    ("differential_root", "differential_root(f)(x0): the value at x0 of the solution of f(y, x) = [linear differential equation, initial values], numerically"),
    ("convolve", "convolve(f, g, x, y): the convolution integrate(f(x) g(y - x), [x, -inf, inf])"),
    ("dt", "dt(e, x): the total derivative, other symbols functions of x (dt(s, x)); dt(e): the total differential; dt(e, [x, k])"),
    ("inverse_function", "inverse_function(f): the inverse of a named elementary function or of a function solve can invert"),
    ("implicit_d", "implicit_d(f, eqs, ys, x), (..., [x, k]), (..., x1, x2): the derivative of f with ys defined implicitly by eqs; implicit_d(eq, y, x) dy/dx"),
    // round 31 (lib/calculus.mc)
    ("spheroidal_s1_prime", "spheroidal_s1_prime(n, m, gamma, z): the z-derivative of spheroidal_s1 (the row of d)"),
    // round 29 (lib/calculus.mc)
    ("between", "between(x, [a, b]): a <= x <= b; ranges as lists of pairs, intervals or centered intervals; between([a, b]) the operator form"),
    ("booleans", "the domain of the truth values true and false (element(x, booleans))"),
    ("o", "o(x), o(x, x0): the order term O(x - x0); o(x)^k + f truncates the series of f at order k"),
    ("n_roots", "n_roots(p == q, x): the numerical roots of a polynomial equation as x == r1 or x == r2 ..., with multiplicity"),
    ("lyapunov_solve", "lyapunov_solve(a, c): x with a.x + x.a^H == c; lyapunov_solve(a, b, c): x with a.x + x.b == c"),
    ("discrete_lyapunov_solve", "discrete_lyapunov_solve(a, c): x with a.x.a^H - x == c; (a, b, c): a.x.b - x == c; ([a, d], [b, e], c): a.x.b - d.x.e == c"),
    ("riccati_solve", "riccati_solve([a, b], [q, r]): the stabilizing x of a^H.x + x.a - x.b.r^-1.b^H.x + q == 0 (numerically)"),
    ("discrete_riccati_solve", "discrete_riccati_solve([a, b], [q, r]): the stabilizing x of a^H.x.a - x - a^H.x.b.(r + b^H.x.b)^-1.b^H.x.a + q == 0 (numerically)"),
    ("pade_approximant", "pade_approximant(f, [x, x0, [m, k]]): the Pade approximant with numerator degree m and denominator degree k at x0; [x, x0, n] the [n/n] one"),
    ("apart_square_free", "apart_square_free(e), (e, x): partial fractions over the square-free factors of the denominator"),
    ("partial_fraction_elements", "partial_fraction_elements(r, x), (r, x, key): the parts of the partial fraction decomposition (poles, multiplicities, coefficients, ...) as an association"),
    ("polynomial_reduction", "polynomial_reduction(p, [g1, ...], xs): the remainder of p on division by the gi"),
    ("symmetric_reduction", "symmetric_reduction(f, xs), (f, xs, ss): [p, q] with f = p + q, p a polynomial in the elementary symmetric polynomials (or the names ss), q the remainder"),
    ("find_generating_function", "find_generating_function(a, x): a rational generating function of the list a, from the shortest linear recurrence with constant coefficients"),
    ("polynomial_q", "polynomial_q(e, x), (e, [x1, ...]), (e): whether e is a polynomial in the variables, with coefficients free of their symbols"),
    ("solve_always", "solve_always(eqs, vars): the values of the other symbols for which the polynomial equations hold for every value of vars"),
    ("compose_series", "compose_series(f, g), (f, g, h, ...): the power series f(g(x)) of the series f and g (g at the expansion point of f)"),
    ("inverse_series", "inverse_series(s), (s, y): the series x(y) with s(x(y)) = y, a Puiseux series when s starts at a power above 1"),
    ("integrate_change_variables", "integrate_change_variables(inactive(integrate)(f, x), u, eq): the inactive integral in the new variable u related to x by eq (limits mapped); [u, v], [eq1, eq2] with the Jacobian"),
    ("radon_transform", "radon_transform(f, [x, y], [p, phi]): the integral of f over the line x cos(phi) + y sin(phi) = p"),
    ("asymptotic_integrate", "asymptotic_integrate(f, [t, a, b], [x, x0, k]): the expansion of the integral to order k in x at x0; x -> x0 the leading term"),
    ("asymptotic_sum", "asymptotic_sum(f, [k, a, b], [x, x0, m]): the expansion of the sum to order m in x at x0; x -> x0 the leading term"),
    ("asymptotic_product", "asymptotic_product(f, [k, a, b], [x, x0, m]): the expansion of the closed-form product to order m in x at x0"),
    ("asymptotic_solve", "asymptotic_solve(eqs, [y, y0], [x, x0, k]): the solution y(x) through y(x0) = y0 as a power series to order k (regular points; systems with [[y1, y2], [a1, a2]])"),
    ("d_stability_conditions", "d_stability_conditions(eqs, vs, t), (..., points): [equilibrium, condition for asymptotic stability] for autonomous differential equations"),
    ("r_stability_conditions", "r_stability_conditions(eqs, vs, n), (..., points): [equilibrium, condition for asymptotic stability] for autonomous recurrence equations"),
    ("difference_root_reduce", "difference_root_reduce(lhs == rhs, n): true when the identity of the sequences holds for n = 0, ..., 40, false when it fails there"),
    ("green_function", "green_function([L, bc1, bc2], u(x), [x, a, b], y): the Green's function of a second-order linear operator with two homogeneous conditions (boundary or initial)"),
];

/// Register the natives of this crate with `k`. `stop` is the flag `mc-arith`'s number kernels poll (the
/// engine's arithmetic stop flag).
pub fn install(k: &mut Kernel, h: &Heap, stop: &Arc<AtomicBool>) {
    k.register(h.sym("d"), Attributes::default(), Box::new(natives::D { stop: stop.clone() }));
    k.register(h.sym("derivative"), Attributes::default(), Box::new(natives::DerivativeAt { stop: stop.clone() }));
    // the properties read the assumption context (decision 2C4.5)
    let reads = Attributes { uses_assumptions: true, ..Attributes::default() };
    k.register(h.sym("apart"), Attributes::default(), Box::new(natives::Apart { stop: stop.clone() }));
    for (name, p) in [
        ("function_domain", props::Prop::Domain),
        ("function_singularities", props::Prop::Singularities),
        ("function_analytic", props::Prop::Analytic),
        ("function_sign", props::Prop::Sign),
        ("function_monotonicity", props::Prop::Monotonicity),
        ("function_convexity", props::Prop::Convexity),
        ("function_injective", props::Prop::Injective),
        ("function_discontinuities", props::Prop::Discontinuities),
        ("function_continuous", props::Prop::Continuous),
        ("function_meromorphic", props::Prop::Meromorphic),
        ("function_period", props::Prop::Period),
        ("function_poles", props::Prop::Poles),
        ("function_surjective", props::Prop::Surjective),
        ("function_bijective", props::Prop::Bijective),
        ("piecewise_expand", props::Prop::PiecewiseExpand),
        ("function_range", props::Prop::Range),
    ] {
        k.register(h.sym(name), reads, wrap(name, props::PropNative { p, stop: stop.clone() }));
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
        k.register(h.sym(name), Attributes::default(), wrap(name, optimize::OptNative { kind, stop: stop.clone() }));
    }
}
