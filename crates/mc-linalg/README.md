# mc-linalg

Linear algebra as natives of the evaluator: determinants, inverses, elimination, linear systems, eigensystems,
decompositions, products and vector calculus.

## Modules
- `tensor`: reading vectors and matrices (packed or not) into a shape and an arithmetic domain.
- `dense`, `scalar`: dense row-major kernels on `f64` and complex numbers (products, LU, QR, Cholesky, Jacobi, QR
  iteration, SVD), generic over a `Scalar` trait.
- `elim`: Gaussian elimination with pivoting over exact numbers and balls.
- `exact`: fraction-free elimination over integers and integer polynomials in the entries' kernels.
- `systems`, `decomp`, `ops`, `more`: determinants, inverses, ranks, null spaces and solves; `lu`, `qr`, `cholesky`,
  `svd`; products and constructors; matrix functions defined by formulas.
- `eigen`, `algebraic`, `paramfield`, `apeigen`: characteristic polynomials, exact eigenvalues, eigenvectors over
  number fields `Q[t]/(f)` or over parameters, and the symmetric eigenproblem at arbitrary precision.
- `natives`: one native per function name, bridging the kernels' work budget to the evaluator's fuel.

## Design
- Every function reads its arguments into one domain (machine, exact non-rational numbers or balls, rational or
  symbolic) and computes only there.
- Machine arrays are read from and written to packed arrays without building a term per element.
- Eigenvalues come from the factored characteristic polynomial (`mc-poly`); number fields come from `mc-algebra`;
  formula-defined functions use `mc-numeric`.
- Kernels poll a work budget so that long computations can be stopped.

## Tests
- Unit tests in `src`, a documentation example in the crate docs, and integration tests in `tests/` for machine,
  exact, ball, eigen and operation cases, run on the assembled engine (`mc-engine`, a dev-dependency).
- Run them with `cargo test --release -p mc-linalg`.
