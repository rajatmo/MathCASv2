# Package 2C, item 2: integration of rational functions

Branch `2c-rational`, on the 2C skeleton (051f448). Files: `src/integrate/rational/` (`mod.rs`, `upoly.rs`,
`hermite.rs`, `logpart.rs`, `params.rs`), `tests/integrate_rational.rs`, this note. Nothing outside these changed.

## What it does

`rational::integrate(cx, b, f, x, depth)` (the dispatcher calls it after linearity and the base table) gives an
antiderivative of any rational function of `x` whose residues need at most square roots (see the gaps), and `None` for
anything else, so the call stays `integrate(..)`.

1. **Reading.** `mc-poly`'s rational converter (`Conv::rational`, `ratfun::read`) gives `n/d` in lowest terms over the
   kernels of `f`. Every kernel other than `x` must be free of `x` and exact (not an inexact or complex number), or the
   integrand is not rational in `x`: `None`. With `x` the only kernel the coefficients are in Q; otherwise they are
   in Q(parameters) (`params.rs`).
2. **Polynomial part.** `n = p d + a`, `∫ p` term by term.
3. **Rational part.** Yun's square-free factorisation of `d`, then Bronstein's quadratic `HermiteReduce`:
   `∫ a/d = Σ b/v^j + ∫ a*/d*`, `d*` square-free. Each `b/v^j` is written with `v` a primitive integer polynomial when
   it can be (`-1/(2 + 3x + x^2)`, `-1/(2 (x - 1)^2)`).
4. **Logarithmic part** (`logpart.rs`). Rothstein–Trager: `R(t) = res_x(d*, a* - t d*')`, computed as
   `∏_{d*(α)=0} (a*(α) - t d*'(α))` at t = 0..deg d* (Euclid over the field) and Newton interpolation, then factored
   (`mc-poly` univariate over Q, multivariate over Q(parameters)). For each irreducible factor r and root c,
   `v_c = gcd(d*, a* - c d*')` (Trager: the gcd is taken over K[θ]/(r)), and:
   - deg r = 1: `c log(v_c)`;
   - deg r = 2, r splits over K: two such terms;
   - deg r = 2, real roots: `c± log(V0 + c± V1)` with `c± = (-r1 ± √δ)/2`;
   - deg r = 2, complex roots `α ± iβ`: with `v_c = P + iQ`, the pair is `α log(P^2 + Q^2) + β LogToAtan(P, Q)`,
     Rioboo's `LogToAtan` turning `i log((P + iQ)/(P - iQ))` into a sum of `2 atan(polynomial)` (real and without
     the jumps of `atan` of a quotient). When β is not in K, `LogToAtan` runs over K(√m) (β² = m written `s²·m'`);
   - deg r = 4 and even (`t^4 + p t^2 + q`): split into two quadratics over a real `K(√w)`, either
     `(t² + θt + β)(t² - θt + β)` (β² = q, θ² = 2β - p) or `(t² - s1)(t² - s2)` (s = (-p ± √Δ)/2), then each as
     above over that field. This covers `1/(x^4 + 1)`, `x^2/(x^4 + 1)`, `1/(x^4 - 2)`, `1/(x^4 + a^4)`.
   - anything else (cubic residue fields, as for `1/(x^3 - 2)`): `None`, decided after factoring R and before any
     term is built. As a check, the degrees of the `v_c` must add up to deg d*, or the integral is given up.

Exact results (printed):

| integrand | result |
|---|---|
| `1/(x^2 + 1)` | `atan(x)` |
| `1/(x^2 + 2*x + 5)` | `(1/2)*atan((1/2)*(1 + x))` |
| `x/(x^2 + 1)` | `(1/2)*log(1 + x^2)` |
| `1/(x^2 - 1)` | `(-1/2)*log(1 + x) + (1/2)*log(-1 + x)` |
| `(x^3 + 1)/(x^2 - 4)` | `(1/2)*x^2 + (7/4)*log(2 + x) + (9/4)*log(-2 + x)` |
| `1/(x^3 + 1)` | `(-1/6)*log(1 - x + x^2) + (1/3)*log(1 + x) + (1/3)*atan((1/3)*(-1 + 2*x)*3^(1/2))*3^(1/2)` |
| `1/(x^4 + 1)` | `(-1/8)*log(1 + -1*x*2^(1/2) + x^2)*2^(1/2) + (1/8)*log(1 + x*2^(1/2) + x^2)*2^(1/2) + (1/4)*atan(-1 + x*2^(1/2))*2^(1/2) + (1/4)*atan(1 + x*2^(1/2))*2^(1/2)` |
| `(2*x + 3)/(x^2 + 3*x + 2)^2` | `-(2 + 3*x + x^2)^-1` |
| `1/(x - 1)^3` | `(-1/2)*(-1 + x)^-2` |
| `x^5/(x^2 + 1)^2` | `-log(1 + x^2) + (-1/2)/(1 + x^2) + (1/2)*x^2` |
| `3*x^2/(x^3 - 2)` | `log(-2 + x^3)` |
| `1/(x^2 + a^2)` | `atan(x/a)/a` |
| `1/(x^2 + a)` | `atan(x*a^(-1/2))*a^(-1/2)` |
| `1/((x - a)*(x - b))` | `log(x - a)/(a - b) + -1*log(x - b)/(a - b)` |

(`1 + -1*x*2^(1/2) + x^2` is how the printer shows `1 - √2 x + x^2`.)

## Decisions

- **2C-R.1 Own dense arithmetic.** Polynomials in x are dense coefficient vectors over a field given by a small trait
  `Fld` (`upoly.rs`): Q (`mc-arith`'s `Rat`), Q(parameters) (`params.rs`: reduced quotients of `mc-poly` `Poly`s, with
  `mc-poly`'s gcd), and quadratic extensions `Quad<K>` of any of them (towers for the quartic case). One generic
  implementation of division, gcd, extended Euclid, Hermite, resultant and `LogToAtan` serves all of them. `mc-poly`
  is used for reading, for factoring R, and inside the parameter field. Nothing goes through the evaluator (no
  `apply`), and `apart` is not used.
- **2C-R.2 Rothstein–Trager with Trager's gcds** rather than Lazard–Rioboo–Trager: the residue fields that are output
  have degree at most 2 (4 for even quartics), so a gcd over K[t]/(r) per factor is cheap and avoids subresultants in
  Q[t][x]. R(t) is computed by evaluation/interpolation (d* monic, so `∏ g(α)` is the resultant at every point).
- **2C-R.3 Real output.** Complex pairs give `log` of a real polynomial plus `atan` of polynomials (Rioboo); log
  arguments are primitive integer polynomials when rational (constant factors dropped), atan arguments keep their
  constant outside (`(1/2)*(1 + x)`). No absolute values: `log(-1 + x)` is the antiderivative of `1/(x - 1)` for
  this CAS (as in the base table's `log(x)`).
- **2C-R.4 Parameters** (kernels free of x: symbols, `sqrt(2)`, `pi`, `exp(a)`) are independent transcendentals.
  Signs are known only for constants; a pair of residues whose discriminant δ has unknown sign takes the complex
  (`atan`) form when δ's leading numeric coefficient is negative and the real (`log`) form otherwise. Both forms are
  formal identities (β² = m in the field), so the derivative is the integrand either way, for every value of the
  parameters where it is defined (`1/(x^2 + a)` gives `atan(x/√a)/√a`, complex for a < 0 but still correct).
  Square roots of parameter expressions are written `s √m` with m from the square-free factorisation.
- **2C-R.5 Interruption.** Every coefficient loop counts on a `Ticker`: one unit of the evaluator's fuel per 1024
  coefficient operations (`cx.tick`), and `mc-arith`'s stop flag is polled. `mc-poly` kernels get a `Work` budget
  over the same ticker. An interrupt or fuel exhaustion propagates as the evaluator's `Abort`; the stop flag, a
  zero divisor or a failed `mc-poly` kernel give `None`. Nothing panics on user input (indexing is guarded by degree
  checks; no `unwrap` on arithmetic results). Field operations of the parameter field cannot reach the fuel; they
  poll the stop flag only.
- **2C-R.6** Inexact numbers (`0.5`) and complex constants as coefficients: `None` (left to a numeric method).

## Tests (`tests/integrate_rational.rs`, 5 tests, all pass)

`cargo test --release -p mc-calculus --test integrate_rational` (and `--test integrate_base`, 2 tests, still passes).

- `antiderivatives_check_by_differentiation`: 26 integrands through `Sys::check_antiderivative` at 3/7, 13/5, -29/11
  (all the examples of the brief, `1/(x^4 + 1)`, `1/(x^4 + 1)^2`, `1/(x^6 - 1)`, `1/(4 x^2 + 4 x + 5)^3`, mixed
  repeated and irreducible factors, polynomial parts, rational-only results).
- `exact_results`: 11 printed results (table above) and the shape of `1/(x^3 + 1)`.
- `parameters`: 11 integrands with parameters, checked by differentiation after substituting values
  (`1/(x^2 + a^2)`, `1/(x^2 - a^2)`, `1/((x - a)(x - b))`, `1/(x^2 + a)`, `x/(x^2 + a x + b)`, `1/(x + a)^2`,
  `1/(x^2 + sqrt(2))`, `1/(x^4 + a^4)`, `(x^2 + 1)/(a x^2 + b)`, `1/(x^2 + y^2 + 1)`, `x^3/(x^2 + a^2)^2`).
- `declines`: `sin(x)/(x^2 + 1)`, `1/(x^2 + sqrt(x))`, `1/(x^3 - 2)` (cubic residues), `1/(x^2 + 0.5)` stay
  unevaluated; the session goes on.
- `interrupt`: `integrate(1/(x^200 + 7 x + 3), x)` (about 5 s to decline uninterrupted) stops within 2 s of an
  interrupt raised at 0.3 s, and the session keeps working.

## Known gaps

- Residue fields of degree 3, or 4 when R's factor is not an even quartic (and higher): no closed form is attempted
  (`1/(x^3 - 2)`, `1/(x^5 + x + 1)`-type pieces), the whole integral stays unevaluated. A RootSum-style result
  (`Σ_{r(c)=0} c log(v_c)`) would cover them if a representation is wanted later.
- Speed for large denominators: R(t) costs deg d resultants by Euclid over Q (degree 120: about 1 s; degree 200:
  about 5 s, interruptible). A modular resultant (`mc-poly`'s, bivariate) or Lazard–Rioboo–Trager subresultants would
  be faster; the dense arithmetic is quadratic in the degree.
- With parameters, results can be bulky when the discriminant has unknown sign (`x/(x^2 + a x + b)` gives the real
  log form with `√(a^2 - 4b)`), and the sign choice is a heuristic (2C-R.4). No `simplify` is applied to the result.
- Parameter-field operations (polynomial gcds in the parameters) poll only the stop flag, not the fuel.
- Inexact coefficients are not integrated.

## Requests (files I do not own)

None needed. Possibly useful later: `mc-poly`'s `RatFun::{add, mul, inv}` are private; making them public would let
`params.rs` reuse them instead of its own reduced quotients.
