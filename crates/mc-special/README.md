# mc-special

Special functions: gamma and related functions, zeta, error functions, exponential integrals, polylogarithms,
hypergeometric functions, Bessel and Airy functions, elliptic and Jacobi functions, Mathieu, Heun, Meijer G and others.

## Modules
- `exact`, `exact_more`, `limits`, `rathyp`: exact values at special arguments, values at directed infinities and
  signed poles, and closed forms of hypergeometric series at symbolic arguments.
- `machine`: machine-precision functions computed in double-double arithmetic (`dd`, `cdd`) and rounded once; `bernoulli`
  supplies cached Bernoulli numbers.
- `ball`: rigorous enclosures at any precision.
- `functions`, `natives`: the functions as one enumeration (names, arities, machine and ball evaluation), and the
  natives with list threading and argument kinds.
- `appell`, `heun`, `mathieu`, `meijer`, `meijer_num`, `qhyper`, `weierstrass`, `jshift`, `modular`, `zeros`: families
  of functions of several variables, parameters or zeros.
- `cover`, `more`, `fast29`, `fast30`, `fast33`, `series30`: further functions defined by formulas, machine fast paths and
  series of special functions.

## Design
- Each function works at four levels: exact values (a closed form where one exists, otherwise the call stays
  unevaluated), machine reals, balls at any precision, and registration with `mc-numeric` so that `n` reaches the
  function inside any term.
- Machine values are double-double results rounded once. Ball results are enclosures: truncation errors are bounded
  and added to the radius, and the working precision doubles while the result is too wide.
- Exact poles give `failure(pole, call)`.
- Tables (such as Taylor coefficients of 1/gamma) are computed once at first use from the exact and ball paths.
- Depends on `mc-numeric`, `mc-arith` and `mc-eval`.

## Tests
- Unit tests in `src`, accuracy tests per function family in `tests/`, tests of enclosures, extreme arguments and the
  precision contract, session tests on the assembled engine, and a benchmark program in `examples/`.
- Run them with `cargo test --release -p mc-special`.
