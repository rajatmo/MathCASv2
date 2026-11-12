# mc-linalg: linear algebra (phase 3A)

Exact, symbolic, ball and machine linear algebra. Dependencies: `mc-api`, `mc-arith`, `mc-core`, `mc-eval`, `mc-poly`,
`mc-algebra` (number fields, since F1)
(dev: `mc-engine`, `mc-bench`); its own small complex type, no `mc-numeric`.

## API

`install(k, h, stop)` and `FUNCTIONS` (27): `det`, `inverse`, `pseudo_inverse`, `rank`, `null_space`, `row_reduce`,
`linear_solve`, `eigenvalues`, `eigenvectors`, `eigensystem`, `characteristic_polynomial`, `identity_matrix`,
`diagonal_matrix`, `dot`, `cross`, `kronecker_product`, `trace`, `norm`, `lu`, `qr`, `cholesky`, `svd`,
`singular_values`, `grad`, `div`, `curl`, `laplacian`. `transpose` and `outer` come from mc-lists. There is no infix
`.`: products are `dot(a, b, ...)`.

Result conventions:
- `lu(m)` = `[l, u, p]` with `m[p] == dot(l, u)` (p 1-based); `qr(m)` = `[q, r]`, `m == dot(q, r)`, diag(r) real
  and non-negative; `cholesky(m)` = upper u with `m == dot(conjugate(transpose(u)), u)`; `svd(m)` = thin
  `[u, s, v]` with `m == dot(u, s, conj transpose(v))`; `singular_values(m)` the nonzero ones, largest first.
- `characteristic_polynomial(m, x)` = `det(x I - m)` (monic).
- `null_space(m)`: one vector per free column with that entry 1, from the last free column; orthonormal for machine
  matrices. `linear_solve(m, b)`: b a vector or matrix; free unknowns 0 when the solution is not unique.
- `norm(v, p)` for p a positive integer or `inf`; matrices: `norm(m)` the 2-norm, `norm(m, 1)`, `norm(m, inf)`,
  `norm(m, "frobenius")`. `diagonal_matrix(v, k)` puts v on the k-th diagonal.
- `grad`, `div`, `curl`, `laplacian` return `d(...)` terms for the evaluator; `grad` of a list is the Jacobian; 2D
  `curl` is the scalar curl.

Failures: `failure(argument, f, k)`, `failure(not_square, f)`, `failure(shape_mismatch, f)`,
`failure(singular, inverse)`, `failure(no_solution, linear_solve)`, `failure(not_positive_definite, cholesky)`,
`failure(not_hermitian, cholesky)`, `failure(undecided, f)` (a ball pivot that cannot be decided),
`failure(not_converged, f)`, `failure(interrupted)`, `failure(limit, memory)`. A non-list argument stays unevaluated.

Outside the crate: registration in `mc-engine` (after `mc_special`) and `mc-lsp`; `lib/prelude.mc` lost `dot` and
`norm` (natives now). `norm` of a symbolic vector is `(abs(a)^2 + abs(b)^2)^(1/2)` (was `sqrt(a^2 + b^2)`).

## Decisions

- **LA.1 Domains.** Entries go into one domain: machine if any machine number (complex when needed); ball for balls
  plus exact numbers; exact complex for Gaussian rationals; rational; symbolic if any entry is not a number.
- **LA.2 Packed arrays end to end:** packed input is read as a slice, machine results built with
  `real_array`/`int_array`, no term per element.
- **LA.3 Fraction-free exact and symbolic elimination.** Rationals to `Z` by row lcm scaling; symbolic entries as
  integer polynomials in their kernels (`mc_poly::ratfun::read`). Bareiss forward elimination for `det`/`rank`; a
  Gauss-Jordan Bareiss (every entry stays a minor) for `inverse`, `row_reduce`, `null_space`, `linear_solve`;
  smallest nonzero pivot; one gcd per result entry; a constant denominator folded into the coefficients.
- **LA.4 Interacting kernels** (radicals): a pivot candidate is rejected when its term evaluates to 0; other zero
  tests are polynomial only (gap 4).
- **LA.5 Machine singularity:** an LU pivot at most `n eps max|a|`; then `inverse` fails and `linear_solve` falls
  back to elimination with tolerance `max(r, c) eps max|a|`.
- **LA.6 Balls:** pivot on the ball with the largest midpoint excluding zero; a column whose candidates all contain
  zero (not exactly zero) gives `failure(undecided, f)`. Results enclose the exact ones.
- **LA.7 Machine `rank`, `null_space`, `singular_values`, `pseudo_inverse`** by the SVD, tolerance
  `max(r, c) eps s_max`.
- **LA.8 Eigenproblems.** Symmetric: Householder tridiagonalisation and implicit QL (cyclic Jacobi for n <= 16).
  General: Householder Hessenberg, complex shifted QR (Givens, Wilkinson shift, exceptional shift every 10 steps,
  at most 60n steps) to complex Schur form, eigenvectors by back substitution. Real/Hermitian input: imaginary parts
  below `64 n eps ||A||_F` dropped.
- **LA.9 Interrupts:** every input-dependent loop charges a `mc_poly::work::Meter` bridged to `NativeCx::tick` and
  the arithmetic stop flag.
- **LA.10 Exact eigenvalues:** `det(lambda I - m)` by fraction-free elimination, `mc_poly::factor`; linear factors
  give rational (function) roots, quadratics radicals, irreducible degree >= 3 give `root(x => p(x), k)` (real roots
  ascending first, then the others by real then imaginary part; ordered from companion-matrix eigenvalues). Results
  are re-evaluated, so a `root` definition from mc-solve applies.
- **LA.11 Exact eigenvectors** for irreducible factors of degree >= 2: null space over Q[t]/(f) (`algebraic.rs`: the
  field type of `mc-algebra` since item F1),
  then t replaced by each root; quadratics printed as `p + q sqrt(D)`.
- **LA.12 Order:** eigenvalues by decreasing modulus, then real part, then imaginary part, with multiplicity;
  defective eigenvalues give zero vectors for the missing eigenvectors; machine eigenvectors have unit length with
  their largest component real and positive.
- **LA.13 Exact decompositions:** `lu` by exact elimination; `qr` by unnormalised Gram-Schmidt then one radical per
  column; `cholesky` from L D L^H, U = sqrt(D) L^H; `singular_values` and the 2-norm by square roots of the nonzero
  exact eigenvalues of the smaller Gram matrix; characteristic polynomials of machine, ball and Gaussian matrices by
  Berkowitz.

## Tests (32 in 7 files)

Unit tests (complex arithmetic, LU/QR/Cholesky, Jacobi/Schur/SVD, tridiagonal QL vs Jacobi at 40x40, elimination,
number field, FUNCTIONS vs natives); `exact.rs` (expected determinants and inverses of random 3-6 integer matrices,
6x6 Hilbert det 1/186313420339200000, 24 random rounds of identities: det(ab) = det a det b, inverse, solve,
rank + nullity, null space, rref idempotent, pseudo-inverse conditions; symbolic inverses through `together`);
`eigen.rs` (characteristic polynomials in every domain, exact eigensystems incl. radicals, complex, defective, root
objects, parameters; reconstruction; machine eigenpairs up to 60x60); `machine.rs` (expected values, decomposition
reconstruction at several shapes and complex, 200x200 inverse/solve, Hilbert 8x8 conditioning, numerical rank);
`ops.rs` (products, constructors, norms with overflow-safe scaling, vector calculus identities); `balls.rs`
(enclosures at 40 and 200 digits, undecided cases); `session.rs` (1 vs 4 threads and stress collector, interrupts
of 220x220 30-digit det/inverse and a 40x40 symbolic det within 3 s, 500x500 timing).

## Measurements (release, busy machine)

500x500 machine `inverse` + `det` + `linear_solve` 76 ms (det 8 ms, inverse 24 ms); symmetric `eigenvalues` 500 in
0.18 s (eigenvectors 0.15 s, residual ~1e-15); general `eigenvalues` 200 in 57 ms; `qr` 300 in 9 ms;
`singular_values` 500 in 3.1 s (one-sided Jacobi).

## Known gaps

1. Machine SVD is one-sided Jacobi, not Golub-Kahan: accurate but slow for large sizes.
2. Exact eigenvectors for quadratic or higher factors with parameter coefficients stay unevaluated (eigenvalues work).
3. Eigen-functions on exact complex and ball matrices, symbolic `lu`/`qr`/`cholesky`/`pseudo_inverse`, exact `svd`,
   and `qr`/`cholesky` on balls stay unevaluated.
4. Symbolic elimination with interacting kernels (sqrt(2) entries) zero-tests only pivot candidates through their
   terms; inconsistent-row detection in `linear_solve` and some ranks could be misjudged. Exact complex numbers in
   symbolic matrices are kernels.
5. `root(x => p, k)` is provisional until aligned with mc-solve's `root`; machine eigenvalues of real nonsymmetric
   matrices are not forced into exact conjugate pairs.
6. No error bounds for symmetric ball eigenvalues.
7. Noted while testing: `expand` can leave `(-1/4)*(sum)` after a radical squares to a sum (tests use
   `expand(expand(...))`); `cancel` and `together` do not thread over lists.

## Round 18

`identity_matrix([m, n])` and `working_precision: d`; `diagonal_matrix(v, k, n | [m, n])`; `target_structure`
`"Dense"`, `"Sparse"`, `automatic` accepted (the dense form). Decision la-9d.1: exact eigenvectors with rational
entries are scaled to coprime integers. Kept: `characteristic_polynomial` is det(x I - m), monic (det(m - x I)
differs by a sign for odd n).

## Round 19 fixes

- **f19.1** `svd` of an exact or ball matrix is computed in machine arithmetic (it stayed unevaluated; Q19-27).
- **f19.2** `rotation_matrix([u, v])`: the rotation taking the direction of u to that of v (2 or 3 entries; in
  space about `u x v` by Rodrigues' formula; parallel vectors give the identity, opposite ones stay). It was read
  as an angle (`cos([u, v])`).

## Round 20

- Symbolic quadratic eigenvalues: with free symbols in the coefficients the minus root (-b - sqrt(d))/(2a) comes
  first; numeric coefficients (pi, im) keep the plus root first.

## Round 29: sparse_array bands

`sparse_array` accepts `band(start)`, `band(start, end)`, `band(start, end, step)` as rule positions (`expand_bands` in
`more.rs`): the band runs over `start + k step` within `end` (automatic: the array's dimensions) and the array's
bounds; a scalar value fills every position, a list gives one value per position from the start, a matrix (rank of the
array) is one block and a list of such matrices consecutive blocks (the corner advances by the block size times the
step). A rule `[_, ..., _] -> v` stands for all other positions (the default value; an explicit third argument wins),
so `sparse_array(array_rules(a))` is `a`.

## Round 32

- **Eigenvectors of exact matrices with radical or complex entries stay exact** (`eigen.rs`): a quadratic factor of the
  characteristic polynomial over the radicals of the entries (no free symbols) is solved like a rational root, by the
  exact null space of `m - root I`; exact complex matrices (`[[im, 1], [1, 0]]`) go through the same exact path instead of
  declining. `eigensystem([[sqrt(2), 1], [1, 0]])` is `[[(sqrt(2) +- sqrt(6))/2], ...]` with vectors `[(sqrt(2) +- sqrt(6))/2, 1]`
  (residuals checked at machine precision in `wrong_values.rs`).
- **`matrix_exp(m, v)` of a numeric strictly triangular `m`**: the Taylor series `sum m^k v / k!` stops at the first zero
  vector (at most n terms), one sparse product per term, exact for exact entries (`exp_nilpotent_action`; Higham 2008,
  ch. 10). A 1000 x 1000 band matrix ended in a hang before.
- **Sparse arrays**: negative positions count from the end (`[-1] -> v`); `band(start, end) -> list` repeats the values
  (and a block or list of blocks) cyclically along a band with an explicit end, a band without an end runs once; a negative
  start or end counts from the end of its dimension.
- Gaps: `interpolation_order` of `find_peaks` (spline peaks), `svd` with `"Thin"`.

## Round 33

- `svd(m, spec)`: `"Thin"` (the default, kept: decision above), `"Compact"` (nonzero singular values), `"Full"`
  (unitary u and v, s of the size of m; the columns beyond the rank by Gram-Schmidt against the standard basis), a
  positive integer or `up_to(q)` (the q largest) and a negative integer (the smallest).
- `eigenvalues`, `eigenvectors`, `eigensystem` of ball matrices (numbers of a given precision, `n(x, 20)` entries)
  are computed in machine arithmetic, like `svd` (f19.1): the digits beyond 16 are lost; `up_to(q)` is accepted as a
  count.
- `trace(a, f, n)`: `f` (default `add`) of the entries a[i][i]..[i] down the first n levels; vectors and non-square
  matrices are allowed (`trace([[1, 2]])` is 1 now, it was `failure(not_square)`).
- `hilbert_matrix`, `rotation_matrix`, `cholesky` accept `target_structure: "Dense"`/`"Sparse"`; `hilbert_matrix`
  `working_precision:` (a number of digits or `machine_precision`).
- `sparse_array` with pattern positions: the conditions `when abs(i - j) == 1` over integer positions are evaluated by
  a small integer evaluator (add, mul, abs, mod, power, comparisons, and/or/not) and only other conditions go to the
  evaluator: 999 x 999 in 1 s (12 s before).
- Not done: `"Cauchy"`/`"Hankel"`/`"Structured"` target structures, exact `svd`, `eigensystem` of a symbolic pencil
  `[a, b]`.

## Round 36

No Rust change. Conventions recorded with property tests (`mc-engine/tests/r36_w36_linalg.rs`): `qr` returns `[q, r]` with `m == q.r`; `svd` and `eigensystem` vector signs are free; `null_space` and `range_space` give any basis. Gaps: eigen/svd of high-precision matrices run in machine arithmetic; quantity arrays.
## Round 36

- `Tick::charge`: a raised stop flag sets the abort (see mc-cad NOTES).

## Round 36, later fixes

- Real symmetric matrices of balls are solved at their own precision (`apeigen.rs`): Householder tridiagonalisation
  and implicit QR with the Wilkinson shift (Golub and Van Loan 4th ed., algorithms 8.3.1 and 8.3.3) in binary
  floats of p + 64 + 4 log2(n) bits; eigenvalue radii by Weyl (n max|e_ij| plus the method's rounding), eigenvector
  radii that over the gap (Davis-Kahan). The smallest eigenvalue of hilbert_matrix(100, working_precision: 200) is
  1.0e-150 (the machine solver gave 1e-19), in 0.55 s; eigensystem of an 18-digit matrix gives 18-digit values.
  Eigenvectors keep the convention of the machine solver (unit length, largest entry positive; now in the
  eigenvectors docs). Gap: nonsymmetric ball matrices and svd stay in machine arithmetic.
- `curl` of an array of rank k >= 2 in n variables: the rank n - k - 1 array (1/k!) eps d_i A (the Hodge dual of
  the exterior derivative); curl(grad(F)) of a vector field is 0.
- Matrices of quantities with a unit per column (M diag(u)): row_reduce, null_space, rank, least_squares and
  matrix_minimal_polynomial (one unit throughout) work on the magnitudes and put the units back (lib/lists.mc); the
  natives of row_reduce, null_space, pseudo_inverse and rank decline on quantity entries (their zero tests did not
  see quantity(0, u), so a singular matrix reduced to the identity).
- rotation_matrix of complex vectors: Gram-Schmidt with the Hermitian product (unitary results), and
  rotation_matrix([u, v]) for complex u, v the unitary map I + E (U - I) E^H.

## Round 36, later fixes

- `matrix_function` of machine numbers mixed with exact numeric constants (`4 pi`, `e`): the matrix at machine precision first (was a symbolic result with root objects).
- `matrix_exp` of a numeric matrix with inexact entries (n <= 16): scaling and squaring with the Taylor series (Moler and Van Loan 2003, method 3) with 25 guard digits, rounded to the input's precision (machine numbers back as machine numbers, balls at their least precision; a 40-digit matrix gave machine numbers before). For [[1. + i, 2, 3 - 2i], [0, 4 pi, 5i], [e, 0, 6]] the entry (1, 1) is -1927.4173267518713 + 10417.597324917992 i against the 40-digit Taylor sum -1927.41732675187202 + 10417.59732491799609 i; the eigenvector method used before (-1927.417326751816) was several hundred times further off.

## Round 37: pencils

- **r37.5** `characteristic_polynomial([a, b], x)`, `eigenvalues([a, b])`, `eigenvectors([a, b])`, `eigensystem([a, b])`
  of a pencil (a list of two square matrices of one size; a matrix of two rows is not one): the generalized
  eigenproblem a v = lambda b v (Golub and Van Loan, Matrix Computations, 7.7). The characteristic polynomial is
  det(x b - a), the form that is `characteristic_polynomial(a, x)` = det(x I - a) for b = I (round 18 'Kept'); det(a - x b)
  differs by (-1)^n. Exact entries: the roots of det(x b - a) by the factoring of the
  matrix case, and `inf` once for each degree the polynomial lacks (first: the largest modulus); eigenvectors are
  the null spaces of a - lambda b (number fields for irreducible factors of degree 3 and more), those of `inf` the
  null space of b. A singular pencil (det identically 0) declines. Machine entries: the eigen functions of b^-1 a
  (same eigenvalues and eigenvectors) when b is nonsingular to working precision (the LU test of `det`); a
  singular b declines, as do balls (no ball QZ algorithm: the general method for a singular b, the QZ iteration of
  Moler and Stewart, is not implemented). Symbolic pencils whose quadratic factor has parameters give eigenvalues
  but no eigenvectors (as for matrices).
- `norm(v, p)` of a vector for any real p >= 1 (machine, rational) or a symbolic p: (sum |v_k|^p)^(1/p); an order
  below 1 is not a norm (failure); a matrix with such a p stays unevaluated (only 1, 2, inf and Frobenius are
  matrix norms here). The Frobenius norm is named in any letter case ("Frobenius").
- `rank(m, modulus: p)` and `row_reduce(m, modulus: p)` for a prime p (deterministic Miller-Rabin): elimination over
  GF(p) through the generic field elimination (`ModP`, inverses by Fermat's little theorem); a rational entry a/b
  is a b^-1, a denominator divisible by p or a composite modulus declines. `rank(m, tolerance: t)`: the singular
  values of a machine matrix above t times the largest; `row_reduce(m, tolerance: t)`: pivots at most t times the
  largest entry are zero; exact matrices ignore the tolerance.
- `hankel_matrix`, `toeplitz_matrix`, `upper_triangularize`, `lower_triangularize` take `target_structure:
  "Dense" | "Sparse" | automatic` (the dense list form, as the constructors' options here); other structures stay.
