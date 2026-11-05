# mc-linalg: linear algebra (phase 3A)

Exact, symbolic, ball and machine linear algebra. Dependencies: `mc-api`, `mc-arith`, `mc-core`, `mc-eval`, `mc-poly`
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
- **LA.11 Exact eigenvectors** for irreducible factors of degree >= 2: null space over Q[t]/(f) (`algebraic.rs`),
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
number field, FUNCTIONS vs natives); `exact.rs` (reference determinants and inverses of random 3-6 integer matrices,
6x6 Hilbert det 1/186313420339200000, 24 random rounds of identities: det(ab) = det a det b, inverse, solve,
rank + nullity, null space, rref idempotent, pseudo-inverse conditions; symbolic inverses through `together`);
`eigen.rs` (characteristic polynomials in every domain, exact eigensystems incl. radicals, complex, defective, root
objects, parameters; reconstruction; machine eigenpairs up to 60x60); `machine.rs` (reference values, decomposition
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
