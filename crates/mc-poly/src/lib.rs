//! MathCAS v2 polynomial algebra (library package K).
//!
//! Polynomials are held in a packed sparse form ([`Poly`]: exponent vectors and integer coefficients, rational
//! coefficients over a common denominator in [`QPoly`]) and converted from and to canonical terms in the kernels
//! of the input ([`convert`]); by semantics 11 the packed form is observably the expanded term. On top of that:
//! multiplication by Kronecker substitution, heap merging or hashing ([`mul`]), and the functions listed in
//! [`FUNCTIONS`].
//!
//! Decisions of this package are numbered K.1, K.2, ... in the comments where they are applied.
#![warn(missing_docs)]
pub mod convert;
pub mod factor;
mod functions;
pub mod gcd;
pub mod modp;
pub mod mul;
mod natives;
pub mod poly;
pub mod qpoly;
pub mod ratfun;
pub mod resultant;
mod series;
pub mod work;
pub mod z;

pub use poly::Poly;
pub use qpoly::QPoly;
pub use z::Z;

use mc_core::{Heap, Store};
use mc_eval::{Attributes, Kernel};
use natives::{PolyFn, PolyNative};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// Every function of this package with a one-line description.
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
    ("gcd", "gcd(a, b, ...): the gcd of exact numbers, or of polynomials"),
    ("polynomial_quotient", "polynomial_quotient(p, q, x): the quotient of p by q as polynomials in x"),
    ("polynomial_remainder", "polynomial_remainder(p, q, x): the remainder of p by q as polynomials in x"),
    ("factor", "factor(p): the factorisation over the integers"),
    ("factor_square_free", "factor_square_free(p): the square-free factorisation"),
    ("resultant", "resultant(p, q, x): the resultant of p and q with respect to x"),
    ("discriminant", "discriminant(p, x): the discriminant of p with respect to x"),
    ("together", "together(e): e as one fraction in lowest terms"),
    ("cancel", "cancel(e): each term of e in lowest terms"),
    ("series", "series(e, x, x0, n): the power series of e in x at x0 up to the power n"),
    ("normal", "normal(s): a series as its truncated sum (other terms unchanged)"),
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
    ("polynomial_quotient", PolyFn::PolynomialQuotient),
    ("polynomial_remainder", PolyFn::PolynomialRemainder),
    ("factor", PolyFn::Factor),
    ("factor_square_free", PolyFn::FactorSquareFree),
    ("resultant", PolyFn::Resultant),
    ("discriminant", PolyFn::Discriminant),
    ("together", PolyFn::Together),
    ("cancel", PolyFn::Cancel),
    ("series", PolyFn::Series),
    ("normal", PolyFn::Normal),
];

/// Register the natives of this package with `k`. `stop` is the flag `mc-arith`'s number kernels poll (the
/// engine's arithmetic stop flag), so that long big-integer products inside a polynomial kernel stop promptly
/// (decision K.1).
pub fn install(k: &mut Kernel, h: &Heap, stop: &Arc<AtomicBool>) {
    for &(name, f) in TABLE {
        k.register(h.sym(name), Attributes::default(), Box::new(PolyNative { f, name, stop: stop.clone() }));
    }
}
