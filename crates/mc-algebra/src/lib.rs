//! MathCAS v2 algebraic numbers (roadmap phase F, item F1): one number-field type for the whole system.
//!
//! - [`field`]: number fields `Q[t]/(f)` and their elements in the power basis: arithmetic, inverses, traces,
//!   norms, characteristic and minimal polynomials; [`field::Alg`], an element with its field;
//! - [`embed`]: embeddings, which complex root of `f` the generator is, as a certified isolating disk refined on
//!   demand with ball arithmetic;
//! - [`algnum`]: algebraic numbers (an element of a field with an embedding), their exact signs, and primitive
//!   elements of towers `Q(θ, β) = Q(γ)`;
//! - [`kpoly`]: polynomials over a number field: modular gcd with a Euclidean fallback, norms, Trager's
//!   factorisation checked by expansion;
//! - [`qpoly`]: the dense rational polynomials underneath;
//! - [`realroot`]: real roots of polynomials over real number fields, real algebraic numbers with exact comparison
//!   and signs, sample points (item F2);
//! - the natives `minimal_polynomial`, `to_number_field`, `root_sum`, `to_radicals` and the `extension:` option of
//!   `factor` (module `natives`), with the conversion between terms and algebraic numbers (module `convert`).
//!
//! ```
//! use mc_api::{Engine, StopFlag};
//! let mut e = mc_engine::CoreEngine::new();
//! let stop = StopFlag::new();
//! assert_eq!(e.eval("minimal_polynomial(sqrt(2) + sqrt(3), x)", &stop).unwrap().printed, "1 + -10*x^2 + x^4");
//! ```
//!
//! Decisions of this crate are numbered F1.1, F1.2, ... in the comments where they are applied.
#![warn(missing_docs)]
pub mod algnum;
mod convert;
pub mod countroots;
mod cx;
pub mod embed;
pub mod field;
pub mod kpoly;
mod minpoly;
mod natives;
pub mod qpoly;
pub mod realroot;
mod rootsum;

pub use algnum::AlgNum;
pub use field::{Alg, Elem, NumberField};

use mc_core::{Heap, Store};
use mc_eval::{Attributes, Kernel};
use natives::{AlgFn, AlgNative, AlgebraicArithmetic, ExtOp, ExtensionOption, NormalRootSum};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// Every function of this crate with a one-line description.
pub const FUNCTIONS: &[(&str, &str)] = &[
    (
        "minimal_polynomial",
        "minimal_polynomial(a, x): the minimal polynomial of the algebraic number a in x (primitive, integer coefficients)",
    ),
    (
        "to_number_field",
        "to_number_field(a, theta): the coefficients [c0, c1, ...] with a = c0 + c1 theta + ...; to_number_field(a) with a as generator; to_number_field([a, b, ...]) a primitive element and every number in it",
    ),
    (
        "root_sum",
        "root_sum(x => p(x), x => g(x)): the sum of g over the roots of the polynomial p, with multiplicity; roots of linear and quadratic factors become radicals",
    ),
    ("to_radicals", "to_radicals(e): the sums over roots in e of degree 3 and 4 written with radicals"),
    (
        "root_reduce",
        "root_reduce(a): the algebraic number a as a rational, a quadratic radical or one root object root(x => p(x), k) with p its minimal polynomial",
    ),
    ("algebraic_number_norm", "algebraic_number_norm(a): the norm of the algebraic number a over the rationals, in the field Q(a)"),
    ("algebraic_number_trace", "algebraic_number_trace(a): the trace of the algebraic number a over the rationals, in the field Q(a)"),
    (
        "count_roots",
        "count_roots(p, x): the number of real roots of the polynomial p with multiplicity; count_roots(p, [x, a, b]) in the closed interval [a, b], or in the closed rectangle with corners a and b when one is complex",
    ),
    // round 28, written in MathCAS (lib/calculus.mc)
    ("algebraics", "algebraics: the domain of the algebraic numbers for element(x, algebraics), decided by the Lindemann-Weierstrass and Gelfond-Schneider theorems"),
    ("add_sides", "add_sides(rel, c): c added to every side of an equation or inequality; add_sides(a == b, rel) side by side"),
    ("subtract_sides", "subtract_sides(rel, c): c subtracted from every side; subtract_sides(rel): rel(lhs - rhs, 0)"),
    ("multiply_sides", "multiply_sides(rel, c): every side multiplied by c, an inequality reversed for negative c (a piecewise when the sign is open)"),
    ("divide_sides", "divide_sides(rel, c): every side divided by c (a piecewise when c may be 0 or of either sign); divide_sides(rel) by the right side"),
];

/// The table of natives: (name, function).
const TABLE: &[(&str, AlgFn)] = &[
    ("minimal_polynomial", AlgFn::MinimalPolynomial),
    ("to_number_field", AlgFn::ToNumberField),
    ("root_reduce", AlgFn::RootReduce),
    ("algebraic_number_norm", AlgFn::AlgebraicNumberNorm),
    ("algebraic_number_trace", AlgFn::AlgebraicNumberTrace),
    ("root_sum", AlgFn::RootSum),
    ("to_radicals", AlgFn::ToRadicals),
    ("count_roots", AlgFn::CountRoots),
];

/// Register the natives of this crate with `k`, and the `extension:` option of `factor` (call after `mc-poly`
/// is installed). `stop` is the engine's arithmetic stop flag.
pub fn install(k: &mut Kernel, h: &Heap, stop: &Arc<AtomicBool>) {
    for &(name, f) in TABLE {
        // functions of one number thread over lists (minimal_polynomial([a, b], x)); to_number_field takes a list
        let listable = matches!(f, AlgFn::MinimalPolynomial | AlgFn::RootReduce | AlgFn::AlgebraicNumberNorm | AlgFn::AlgebraicNumberTrace);
        let a = Attributes { listable, ..Attributes::default() };
        k.register(h.sym(name), a, Box::new(AlgNative { f, name, stop: stop.clone() }));
    }
    // the extension: option of mc-poly's functions (decision F1.9)
    for (name, op) in [("factor", ExtOp::Factor), ("factor_square_free", ExtOp::SquareFree), ("polynomial_gcd", ExtOp::Gcd)] {
        let st = stop.clone();
        k.wrap_native(h.sym(name), move |inner| Box::new(ExtensionOption { inner, stop: st, op, name }));
    }
    // sums, products and integer powers of algebraic_number terms of one field stay in that field
    for name in ["add", "mul", "pow"] {
        let st = stop.clone();
        k.wrap_native(h.sym(name), move |inner| Box::new(AlgebraicArithmetic { inner, stop: st }));
    }
    // normal expands sums over roots into sums over root objects (decision F1.12)
    let st = stop.clone();
    k.wrap_native(h.sym("normal"), move |inner| Box::new(NormalRootSum { inner, stop: st }));
}
