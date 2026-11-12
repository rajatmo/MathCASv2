//! MathCAS v2 linear algebra (roadmap phase 3, item 3A): determinants, inverses, elimination, linear systems,
//! eigensystems, decompositions, products and vector calculus, as natives of the evaluator.
//!
//! Every function reads its vector and matrix arguments into one arithmetic **domain** ([`tensor::Domain`],
//! decision LA.1) and computes there:
//!
//! - **machine** (machine reals or complex numbers anywhere): dense row-major `f64` or complex arrays read from and
//!   written to packed arrays without a term per element ([`dense`]): LU with partial pivoting, Householder QR,
//!   Cholesky, cyclic Jacobi for symmetric eigenproblems, Hessenberg reduction and shifted QR for general ones,
//!   one-sided Jacobi for the SVD;
//! - **exact numbers** that are not all rational (Gaussian rationals) and **balls**: elimination with pivoting on
//!   `mc-arith` numbers ([`elim`]);
//! - **rational and symbolic**: fraction-free elimination over integers or integer polynomials in the entries'
//!   kernels ([`exact`]); exact eigenvalues through the factored characteristic polynomial and eigenvectors over
//!   number fields ([`eigen`], [`algebraic`]).
//!
//! ```
//! use mc_api::{Engine, StopFlag};
//! let mut e = mc_engine::CoreEngine::new();
//! let stop = StopFlag::new();
//! assert_eq!(e.eval("det([[1, 2], [3, 4]])", &stop).unwrap().printed, "-2");
//! assert_eq!(e.eval("inverse([[2, 0], [0, 4]])", &stop).unwrap().printed, "[[1/2, 0], [0, 1/4]]");
//! ```
//!
//! Decisions of this crate are numbered LA.1, LA.2, ... in the comments where they are applied.
#![warn(missing_docs)]
pub mod algebraic;
mod decomp;
pub mod dense;
mod apeigen;
mod eigen;
pub mod elim;
pub mod exact;
pub mod more;
mod natives;
mod ops;
pub mod scalar;
mod systems;
pub mod tensor;

use mc_core::{Heap, Store};
use mc_eval::{Attributes, Kernel};
use natives::{Fun, LinalgNative};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// Every function of this crate with a one-line description.
pub const FUNCTIONS: &[(&str, &str)] = &[
    ("det", "det(m): the determinant of a square matrix (exact, symbolic, ball or machine)"),
    ("inverse", "inverse(m): the inverse of a square matrix; failure(singular, inverse) when it has none"),
    ("pseudo_inverse", "pseudo_inverse(m): the Moore-Penrose pseudo-inverse of a matrix of numbers"),
    ("rank", "rank(m): the rank of a matrix (machine matrices by singular values above max(r, c) eps s_max)"),
    ("null_space", "null_space(m): a basis of the vectors x with dot(m, x) == 0 (orthonormal for machine matrices)"),
    ("row_reduce", "row_reduce(m): the reduced row echelon form"),
    ("linear_solve", "linear_solve(m, b): a solution x of dot(m, x) == b for a vector or matrix b; failure(no_solution, linear_solve) when inconsistent"),
    ("eigenvalues", "eigenvalues(m): the eigenvalues with multiplicity, by decreasing modulus (exact through the factored characteristic polynomial)"),
    ("eigenvectors", "eigenvectors(m): one eigenvector per eigenvalue in the order of eigenvalues(m); zero vectors for a defective eigenvalue"),
    ("eigensystem", "eigensystem(m): [eigenvalues(m), eigenvectors(m)]"),
    ("characteristic_polynomial", "characteristic_polynomial(m, x): det(x I - m), monic in x"),
    ("identity_matrix", "identity_matrix(n): the n x n identity matrix; identity_matrix([m, n]) rectangular; working_precision: d"),
    ("diagonal_matrix", "diagonal_matrix(v), diagonal_matrix(v, k): the square matrix with v on the k-th diagonal and zeros elsewhere; diagonal_matrix(v, k, n) or (v, k, [m, n]) of that size"),
    ("dot", "dot(a, b, ...): the product of vectors and matrices (scalar product, matrix-vector, matrix-matrix), from the left"),
    ("cross", "cross(u, v): the cross product of two vectors of length 3"),
    ("kronecker_product", "kronecker_product(a, b, ...): the Kronecker product of matrices (or of vectors)"),
    ("trace", "trace(m): the sum of the diagonal of a square matrix"),
    ("norm", "norm(v), norm(v, p), norm(m), norm(m, 1 | inf | \"frobenius\"): vector p-norms (p a positive integer or inf) and matrix norms (default the 2-norm)"),
    ("lu", "lu(m): [l, u, p] with l unit lower triangular, u upper triangular and m[p] == dot(l, u)"),
    ("qr", "qr(m): [q, r] with m == dot(q, r), q with orthonormal columns, r upper triangular"),
    ("cholesky", "cholesky(m): the upper triangular u with m == dot(conjugate_transpose(u), u) for a positive definite m"),
    ("svd", "svd(m): [u, s, v] with m == dot(u, s, conjugate_transpose(v)), s diagonal with decreasing singular values (machine matrices)"),
    ("singular_values", "singular_values(m): the nonzero singular values in decreasing order"),
    ("grad", "grad(f, [x1, ..., xn]): the gradient [d(f, x1), ...]; for a list of functions the Jacobian matrix"),
    ("div", "div(v, [x1, ..., xn]): the divergence d(v1, x1) + ... + d(vn, xn)"),
    ("curl", "curl(v, [x, y, z]): the curl of a vector field; curl([v1, v2], [x, y]) the scalar curl"),
    ("laplacian", "laplacian(f, [x1, ..., xn]): the sum of the second derivatives d(d(f, xi), xi)"),
    ("matrix_function", "matrix_function(f, m): the matrix function f(m) (diagonal, triangular, eigenvector or interpolation forms)"),
    ("matrix_exp", "matrix_exp(m): the matrix exponential; matrix_exp(m, v) its product with v"),
    ("matrix_log", "matrix_log(m): the principal matrix logarithm"),
    ("matrix_power", "matrix_power(m, k): the k-th matrix power (integer k by repeated squaring, negative k through the inverse)"),
    ("conjugate_transpose", "conjugate_transpose(m): the conjugate of the transpose of m"),
    ("hilbert_matrix", "hilbert_matrix(n): the n x n Hilbert matrix 1/(i + j - 1); hilbert_matrix([m, n]) rectangular"),
    ("rotation_matrix", "rotation_matrix(theta): the 2D rotation by theta; rotation_matrix(theta, w): the 3D rotation about the axis w"),
    ("sparse_array", "sparse_array(rules, dims, default): the array with the values of position -> value rules (literal or pattern positions), built dense; sparse_array(automatic, dims, default, data): the compressed row form"),
];

/// The natives in the order of [`FUNCTIONS`].
const NATIVES: &[(&str, Fun)] = &[
    ("det", Fun::Det),
    ("inverse", Fun::Inverse),
    ("pseudo_inverse", Fun::PseudoInverse),
    ("rank", Fun::Rank),
    ("null_space", Fun::NullSpace),
    ("row_reduce", Fun::RowReduce),
    ("linear_solve", Fun::LinearSolve),
    ("eigenvalues", Fun::Eigenvalues),
    ("eigenvectors", Fun::Eigenvectors),
    ("eigensystem", Fun::Eigensystem),
    ("characteristic_polynomial", Fun::CharacteristicPolynomial),
    ("identity_matrix", Fun::IdentityMatrix),
    ("diagonal_matrix", Fun::DiagonalMatrix),
    ("dot", Fun::Dot),
    ("cross", Fun::Cross),
    ("kronecker_product", Fun::KroneckerProduct),
    ("trace", Fun::Trace),
    ("norm", Fun::Norm),
    ("lu", Fun::Lu),
    ("qr", Fun::Qr),
    ("cholesky", Fun::Cholesky),
    ("svd", Fun::Svd),
    ("singular_values", Fun::SingularValues),
    ("grad", Fun::Grad),
    ("div", Fun::Div),
    ("curl", Fun::Curl),
    ("laplacian", Fun::Laplacian),
];

/// Register the natives of this crate with `k`. `stop` is the flag `mc-arith`'s number kernels poll (the
/// engine's arithmetic stop flag).
pub fn install(k: &mut Kernel, h: &Heap, stop: &Arc<AtomicBool>) {
    for &(name, f) in NATIVES {
        k.register(h.sym(name), Attributes::default(), Box::new(LinalgNative { f, name, stop: stop.clone() }));
    }
    mc_numeric::formula::install(k, h, more::DEFS);
    // normal(numeric_array(xs, type)) is the list xs; every other call goes to the earlier native
    k.wrap_native(h.sym("normal"), |inner| Box::new(natives::NormalNumericArray { inner }));
}

#[cfg(test)]
mod tests {
    #[test]
    fn every_function_has_a_native() {
        assert_eq!(super::FUNCTIONS.len(), super::NATIVES.len() + super::more::DEFS.len());
        for (a, b) in super::FUNCTIONS.iter().zip(super::NATIVES) {
            assert_eq!(a.0, b.0);
        }
    }
}
