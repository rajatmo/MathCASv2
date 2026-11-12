//! MathCAS v2 polynomial algebra (library item K).
//!
//! Polynomials are held in a packed sparse form ([`Poly`]: exponent vectors and integer coefficients, rational
//! coefficients over a common denominator in [`QPoly`]) and converted from and to canonical terms in the kernels
//! of the input ([`convert`]); by semantics 11 the packed form is observably the expanded term. On top of that:
//! multiplication by Kronecker substitution, heap merging or hashing ([`mul`]), and the functions listed in
//! [`FUNCTIONS`].
//!
//! Decisions of this crate are numbered K.1, K.2, ... in the comments where they are applied.
#![warn(missing_docs)]
pub mod convert;
pub mod decompose;
pub mod descartes;
pub mod factor;
pub mod fglm;
mod functions;
pub mod gcd;
mod mfactor;
pub mod groebner;
pub mod modp;
pub mod mul;
mod natives;
pub mod opts;
pub mod poly;
pub mod qpoly;
pub mod ratfun;
pub mod resultant;
mod ringfns;
pub mod ring;
mod series;
pub mod work;
pub mod unipoly;
pub mod z;

pub use poly::Poly;
pub use qpoly::QPoly;
pub use z::Z;
pub use natives::thread_relation;
pub use series::{any_series, expansion, series_arith, series_data_parts, series_derivative, series_integral, Expansion};

use mc_core::{Heap, Store};
use mc_eval::{Attributes, Kernel};
use natives::{PolyFn, PolyNative};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// Every function of this crate with a one-line description.
pub const FUNCTIONS: &[(&str, &str)] = &[
    ("expand", "expand(e): multiply out sums, products and positive integer powers"),
    ("coefficient", "coefficient(p, x, k): the coefficient of x^k in p (k = 1 by default)"),
    (
        "coefficient_list",
        "coefficient_list(p, x): the coefficients of x^0 .. x^degree (nested for a list of variables)",
    ),
    ("degree", "degree(p, x): the largest power of x in p (-inf for 0)"),
    ("variables", "variables(p): the variables (kernels) of p"),
    ("polynomial_gcd", "polynomial_gcd(p, q, ...): the greatest common divisor of polynomials"),
    ("polynomial_lcm", "polynomial_lcm(p, q, ...): the least common multiple of polynomials"),
    ("gcd", "gcd(a, b, ...): the gcd of exact numbers (gcd() = 0), or of polynomials"),
    ("lcm", "lcm(a, b, ...): the least common multiple of exact numbers (lcm() = 1), or of polynomials"),
    ("polynomial_quotient", "polynomial_quotient(p, q, x): the quotient of p by q as polynomials in x"),
    ("polynomial_remainder", "polynomial_remainder(p, q, x): the remainder of p by q as polynomials in x"),
    ("factor", "factor(p): the factorisation over the integers"),
    ("factor_square_free", "factor_square_free(p): the square-free factorisation"),
    ("resultant", "resultant(p, q, x): the resultant of p and q with respect to x"),
    ("discriminant", "discriminant(p, x): the discriminant of p with respect to x"),
    (
        "subresultants",
        "subresultants(p, q, x): the principal subresultant coefficients psc_0, ..., psc_k of p and q in x (k the smaller degree)",
    ),
    (
        "subresultant_polynomials",
        "subresultant_polynomials(p, q, x): the subresultant polynomials S_0, ..., S_k of p and q in x (k the smaller degree)",
    ),
    (
        "subresultant_polynomial_remainders",
        "subresultant_polynomial_remainders(p, q, x): p, q and their subresultant polynomial remainder sequence in x",
    ),
    (
        "decompose",
        "decompose(p, x): polynomials [g1, g2, ...] with p = g1(g2(...)) in x, each inner one monic without constant term, verified by composition",
    ),
    ("together", "together(e): e as one fraction in lowest terms"),
    ("cancel", "cancel(e): each term of e in lowest terms"),
    ("series", "series(e, x, x0, n): the power series of e in x at x0 up to the power n"),
    ("normal", "normal(s): a series as its truncated sum (other terms unchanged)"),
    ("O", "O(x^n): the order term of a printed series; series(... + O(x^n), x, 0, k) reads it back"),
    // round 28, written in MathCAS (lib/calculus.mc)
    ("series_coefficient", "series_coefficient(s, k): the coefficient of order k of a series; series_coefficient(f, [x, x0, k], ...) of the expansion of f"),
    ("factor_list", "factor_list(p): [[c, 1], [f1, e1], ...], the numeric factor and the irreducible factors with multiplicities (negative for a denominator)"),
    ("factor_terms", "factor_terms(p), (p, x), (p, [x1, ...]): the numerical content of p pulled out, and with variables the factors free of them"),
    ("exponent", "exponent(e, x), (e, x, h): the largest power of x in e (h applied to all the powers, such as min or list)"),
    ("collect", "collect(e, x), (e, x, h), (e, [x1, ...]): the terms of e gathered by powers of x, with h applied to each coefficient"),
    ("cyclotomic", "cyclotomic(k, x): the k-th cyclotomic polynomial in x"),
    ("polynomial_reduce", "polynomial_reduce(p, [g1, ...], [x1, ...]): [[q1, ...], r] with p = q1 g1 + ... + r, by multivariate division in lexicographic order"),
    ("polynomial_mod", "polynomial_mod(p, m): p with coefficients reduced modulo the integer m, or its remainder modulo the polynomial m; a list of moduli in turn"),
    ("partial_fractions", "partial_fractions(f, x): the partial fraction decomposition of f over the complex numbers; extension: reals for real factors"),
    ("polynomial_expression_q", "polynomial_expression_q(e, xs), (e, xs, test): whether e is a polynomial expression in the variables xs (coefficients passing test)"),
    ("possible_zero_q", "possible_zero_q(e): whether e is probably zero (by simplification, then by high-precision values at sample points)"),
];

/// The table of natives: (name, function).
const TABLE: &[(&str, PolyFn)] = &[
    ("expand", PolyFn::Expand),
    ("coefficient", PolyFn::Coefficient),
    ("coefficient_list", PolyFn::CoefficientList),
    ("degree", PolyFn::Degree),
    ("variables", PolyFn::Variables),
    ("polynomial_gcd", PolyFn::PolynomialGcd),
    ("polynomial_lcm", PolyFn::PolynomialLcm),
    ("gcd", PolyFn::Gcd),
    ("lcm", PolyFn::Lcm),
    ("polynomial_quotient", PolyFn::PolynomialQuotient),
    ("polynomial_remainder", PolyFn::PolynomialRemainder),
    ("factor", PolyFn::Factor),
    ("factor_square_free", PolyFn::FactorSquareFree),
    ("resultant", PolyFn::Resultant),
    ("discriminant", PolyFn::Discriminant),
    ("subresultants", PolyFn::Subresultants),
    ("subresultant_polynomials", PolyFn::SubresultantPolynomials),
    ("subresultant_polynomial_remainders", PolyFn::SubresultantPolynomialRemainders),
    ("decompose", PolyFn::Decompose),
    ("together", PolyFn::Together),
    ("cancel", PolyFn::Cancel),
    ("series", PolyFn::Series),
    ("normal", PolyFn::Normal),
];

/// Register the natives of this crate with `k`. `stop` is the flag `mc-arith`'s number kernels poll (the
/// engine's arithmetic stop flag), so that long big-integer products inside a polynomial kernel stop promptly
/// (decision K.1).
pub fn install(k: &mut Kernel, h: &Heap, stop: &Arc<AtomicBool>) {
    for &(name, f) in TABLE {
        let native = PolyNative { f, name, stop: stop.clone() };
        if f == PolyFn::Series {
            k.register(h.sym(name), Attributes::default(), Box::new(opts::WithAssumptions { name, inner: native }));
        } else {
            k.register(h.sym(name), Attributes::default(), Box::new(native));
        }
    }
}
