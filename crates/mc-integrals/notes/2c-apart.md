# Item 2C, item 1: `apart` (partial fractions)

Status: built on the 2C skeleton. Only `src/apart.rs`, `tests/apart.rs` and
this note were touched.

## What was built (`src/apart.rs`)

`apart(e, x)` works in Rust on `mc-poly`'s packed polynomials (no round trips through the evaluator):

1. `e` is read with `mc_poly::ratfun::read` (a quotient `N/D` of integer polynomials in the kernels of `e`, in
   lowest terms). Kernels other than `x` must be free of `x`; they are parameters, so the coefficient field is
   K = Q(parameters).
2. `D` is factored over Z (`mc_poly::factor::factor`). Factors free of `x` go into a scalar `C`; the others give
   `D = prod p_i^k_i`, each `p_i` with a positive leading coefficient in `x` (`x - a`, not `a - x`).
3. `N/C = Q*D + R` in K[x] (pseudo-division of the numerators, the scalar divided out).
4. For each `q_i = p_i^k_i` with cofactor `M_i = D/q_i`: `A_i = R * M_i^-1 mod q_i` (extended Euclid in K[x],
   remainders kept monic), so `R/D = sum A_i/q_i`.
5. `A_i` written in base `p_i` by repeated division gives `a_j / p_i^j` with `deg a_j < deg p_i`.

Result: `Q + sum a_ij / p_i^j`, built with the canonical arithmetic and handed back with `Reply::Again`.

Examples (as printed):

| input | result |
|---|---|
| `apart(1/(x^2 - 1), x)` | `(-1/2)/(1 + x) + (1/2)/(-1 + x)` |
| `apart((x^3 + 1)/(x^2 + x), x)` | `-1 + x + x^-1` |
| `apart(1/(x*(x^2 + 1)), x)` | `-1*x/(1 + x^2) + x^-1` |
| `apart(1/(x - 1)^3/(x + 2), x)` | `(-1/9)*(-1 + x)^-2 + (-1/27)/(2 + x) + (1/27)/(-1 + x) + (1/3)*(-1 + x)^-3` |
| `apart(1/(x^3 - 1), x)` | `(1/3)/(-1 + x) + (-2/3 + (-1/3)*x)/(1 + x + x^2)` |
| `apart(1/((x - a)*(x - b)), x)` | `(a - b)^-1/(x - a) + -1/(a - b)/(x - b)` |
| `apart(sin(y)/(x^2 - 1), x)` | `(-1/2)/(1 + x)*sin(y) + (1/2)/(-1 + x)*sin(y)` |

## Decisions

- **2C.A1** Coefficients in K = Q(parameters) are kept as `num/den` pairs of integer polynomials over the whole ring
  (`den` free of `x`), reduced by a multivariate gcd after every operation. Pure-rational inputs take the same path
  (`den` is then an integer and the gcds are cheap). Division is pseudo-division followed by dividing out the
  scalar, so no rational-function field type was needed in `mc-poly`.
- **2C.A2** Inexact and complex numbers in `e` decline (`1.5/(x^2 - 1)` stays unevaluated): they would be read as
  kernels (transcendental parameters), and factoring or inverting over them is not meaningful (`1im` would be
  treated as a free symbol).
- **2C.A3** `apart(e)` uses the only kernel of `e` when it is a symbol; with none or several (`1/(x*y + 1)`, `3`)
  the call stays as it is.
- **2C.A4** Inputs free of `x`, and inputs with no negative power of something containing `x` whose denominator is
  free of `x` (polynomials such as `x^2 + 1`), come back unchanged; when a denominator in `x` cancels completely
  (`(x^2 - 1)/(x - 1)`) the expanded quotient (`1 + x`) is returned. A single term already decomposed comes out
  equal to itself (`1/(x + 1)`, `(x + 1)^-2`).
- **2C.A5** Work is charged to the evaluator's fuel through a `Tick` bridge to `mc-poly`'s `Meter` (the same scheme
  as `mc-poly`'s natives, which is private there); an interrupt returns the evaluator's abort,
  `failure(interrupted)` when only `mc-arith`'s stop flag was seen, and `failure(limit, memory)` beyond the memory
  limit of the polynomial kernels.
- If the factor list is ever not pairwise coprime (it is, from `mc-poly`'s factorisation), the inverse does not
  exist and the call declines instead of returning something wrong.

## Tests (`tests/apart.rs`)

`cargo test --release -p mc-calculus --test apart`: 4 tests, all pass (about 0.2 s).

- `exact_outputs`: the printed results above, plus `apart(e)` with one variable and `1/(x*y + 1)` in `x`.
- `unchanged_and_declined`: polynomials and single terms, a cancelled denominator, `sin(x)/(x + 1)`,
  `x^(1/2)/(x + 1)`, `1.5/(x^2 - 1)`, `apart(1/(x*y + 1))`, `apart(3)` (unevaluated), a number as the variable
  (`failure(argument, apart, ...)` from the skeleton's native).
- `corpus_recombines`: 20 inputs (linear, repeated, irreducible quadratic and quartic factors, polynomial parts,
  parameters `a`, `b`, a parameter in both numerator and a repeated factor, an x-free call `sin(y)`): the result
  is never unevaluated, `together(apart(e, x) - e)` is `0`, and for every term the numerator's degree in `x` is
  below the degree of the irreducible factor of its denominator (computed in the language as
  `deg(den) - deg(gcd(den, d(den, x)))`).
- `interruption`: `1/((x - 1)^60*(x + 2)^60*(x^2 + 3)^40*(x^3 - 5)^30)` (about 6.5 s uninterrupted) stops within
  2 s of an interrupt raised after 0.2 s, and the session keeps working.

## Known gaps

- Factorisation is over Q only (no algebraic extensions): `1/(x^2 - 2)` stays one term, `1/(x^4 + 1)` too.
- With parameters, coefficients are printed as expanded quotients (`(a - b)^-1`, `-1/(a - b)`), not factored or
  combined; denominators in the parameters are not simplified further. Results are correct for generic parameter
  values only (as usual: `1/((x - a)*(x - b))` assumes `a != b`).
- Multivariate factorisation in `mc-poly` gives up beyond its Kronecker limits; the part is then kept as one
  (possibly reducible) factor — still a correct, coarser decomposition.
- Big repeated factors are slowish (the example above: 6.5 s) because every K[x] operation runs a gcd; a dedicated
  Q[x] path (QPoly with integer denominators, no polynomial gcds) or Hermite-style reduction would be faster.
- Lists and equations are not threaded (`apart([..], x)` stays unevaluated).

## Requests (files not owned)

- None required. Optional: `mc-poly`'s `Tick` bridge (`natives.rs`, private) could be made public so calculus does
  not duplicate it.
