//! MathCAS v2 finite fields (roadmap 15B): prime and extension fields F_(p^n) with Conway
//! defining polynomials, exact arithmetic on their elements, embeddings between them, norms, traces, minimal
//! polynomials, primitive elements, discrete logarithms, roots, and polynomials over finite fields.
//!
//! - [`fp`]: the prime field and polynomials over it (Rabin's irreducibility test);
//! - [`field`]: F_p[x]/(f), its elements, orders and Pohlig-Hellman discrete logarithms;
//! - [`conway`]: Conway polynomials (search and coset enumeration) with [`conway_table`], and the fallbacks;
//! - [`fqpoly`]: polynomials over F_q and their factorisation (Cantor and Zassenhaus);
//! - [`embed`]: embeddings F_(p^m) -> F_(p^n) and their projections;
//! - [`terms`]: reading and writing fields, elements and polynomials as terms of the language;
//! - `natives`: the functions of [`FUNCTIONS`], and wrappers that give `add`, `mul`, `pow`, `equal`, `unequal`,
//!   `element`, `minimal_polynomial`, `factor`, `factor_square_free`, `expand`, `polynomial_gcd`, `polynomial_lcm`,
//!   `polynomial_quotient` and `polynomial_remainder` their meaning on elements (every other call goes to the
//!   native they wrap).
//!
//! Literature: R. Lidl and H. Niederreiter, Finite Fields (2nd ed., Cambridge 1997); J. von zur Gathen and
//! J. Gerhard, Modern Computer Algebra (3rd ed., Cambridge 2013); L. Heath and N. Loehr, New algorithms for
//! generating Conway polynomials over finite fields, J. Symbolic Comput. 38 (2004). Details in NOTES.md.
#![warn(missing_docs)]
pub mod conway;
pub mod conway_table;
pub mod embed;
pub mod field;
pub mod fp;
pub mod fqpoly;
mod natives;
pub mod terms;

use mc_core::Heap;
use mc_eval::Kernel;

/// The functions [`install`] registers, with a one-line description each.
pub const FUNCTIONS: &[(&str, &str)] = &[
    ("finite_field", "finite_field(p, n), finite_field(q), finite_field(p, f, repr): the field with p^n elements (Conway defining polynomial) or F_p[x]/(f); F(k), F([c0, ...]) are its elements"),
    ("finite_field_element", "finite_field_element(F, k), finite_field_element(F, [c0, c1, ...]): the element of F with index k or with these coefficients; a(\"Index\"), a(\"Coefficients\"), ... its properties"),
    ("finite_field_embedding", "finite_field_embedding(K, F), finite_field_embedding(a -> b): the embedding of the field K into F; E(a) the image, E(\"Projection\")(b) the projection back"),
    ("finite_field_index", "finite_field_index(a): the index of the element a in its field's representation"),
    ("finite_field_element_norm", "finite_field_element_norm(a): the norm of a to the prime field, as an integer 0..p-1"),
    ("finite_field_element_trace", "finite_field_element_trace(a): the trace of a to the prime field, as an integer 0..p-1"),
    ("finite_field_element_minimal_polynomial", "finite_field_element_minimal_polynomial(a, x), (a, x, k): the minimal polynomial of a over the prime field (over the subfield with p^k elements)"),
    ("is_finite_field_element", "is_finite_field_element(a): true when a is an element of a finite field"),
    ("is_finite_field_element_primitive", "is_finite_field_element_primitive(a): true when a generates the multiplicative group of its field"),
    ("finite_field_primitive_element", "finite_field_primitive_element(F): the primitive element of F with the least index"),
    ("frobenius_automorphism", "frobenius_automorphism(a), frobenius_automorphism(a, k): a^p, a^(p^k)"),
    ("finite_field_log", "finite_field_log(a), finite_field_log(a, g): the discrete logarithm k with x^k = a (g^k = a), by Pohlig and Hellman"),
    ("finite_field_root", "finite_field_root(a, n): the list of the n-th roots of the element a in its field"),
    ("conway_polynomial", "conway_polynomial(p, n, x): the Conway polynomial of degree n over F_p in x"),
    ("is_irreducible_polynomial", "is_irreducible_polynomial(f), with modulus: p or extension: F: true when f is irreducible over the rationals, F_p or the field F"),
    ("is_primitive_polynomial", "is_primitive_polynomial(f, p): true when f is a primitive polynomial over F_p"),
];

/// Register the natives of [`FUNCTIONS`] with `k`, and wrap the arithmetic and polynomial natives (install after
/// the libraries that define them).
pub fn install(k: &mut Kernel, h: &Heap) {
    natives::install(k, h);
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_table_lists_every_native() {
        let names: Vec<&str> = super::natives::TABLE.iter().map(|e| e.0).collect();
        let listed: Vec<&str> = super::FUNCTIONS.iter().map(|e| e.0).collect();
        assert_eq!(names, listed);
    }
}
