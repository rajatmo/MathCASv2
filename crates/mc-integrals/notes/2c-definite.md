# 2C item 4: definite integrals

`integrate(f, [x, a, b])` by the fundamental theorem with discontinuity checks. Code:
`src/integrate/definite/` (`mod.rs`: driver; `points.rs`: critical points, polynomial roots, Sturm counts;
`limit.rs`: a private one-sided limit). Tests: `tests/integrate_definite.rs`.

## Algorithm

1. **Bounds.** A bound is *real* when it is `inf`, `-inf` (`-1*inf`) or a term whose `n(...)` is a finite
   machine real (`0`, `pi`, `2^(1/2)`, `0.5`). Otherwise it is *symbolic* (symbols, complex numbers).
2. **Real bounds.** Order them (`sign(a - b)`; undecided gives up). Equal bounds give 0, reversed bounds the
   negated integral.
   - **Rational poles first** (2C4.4): when `f` is a rational function of `x` (by `cancel(together(f))`, a
     denominator with real constant coefficients), a real root of its denominator in the closed interval makes
     the integral divergent. This is decided before any antiderivative is looked for, for any degree (exact roots
     for degree 1, 2 and binomials, a Sturm sequence otherwise): `1/(x^5 - x - 1)` on `[0, 2]` is divergent
     even without an antiderivative.
   - **Antiderivative** `F` from `antiderivative(cx, b, f, x, 0)`; none leaves the call unevaluated.
   - **Critical points** of `f` and `F` in `[a, b]` (`points::collect`): the bases of negative, fractional or
     symbolic powers, the arguments of `log`, `u - 1` and `u + 1` for `asin(u)`, `acos(u)`. Poles of arguments
     (`atan(1/x)` jumps at 0) are found by the walk into the arguments. Known functions of `x`: `+`, `*`, `^`,
     `exp`, `log`, `sin`, `cos`, `sinh`, `cosh`, `tanh`, `atan`, `asin`, `acos`; any other function of `x` in `f`
     or `F` gives up (2C4.6). Every collected expression goes through `cancel(together(.))`; numerator and
     denominator must be polynomials in `x`; they are factored (`factor`) and the real roots of each factor in the
     interval located: exactly for degree 1, 2 and binomials `c x^n + d`; for other factors a Sturm sequence
     decides whether there is a root inside, and if there is one the call stays (the root is not located).
   - **Pieces.** Between consecutive critical points `F` is a continuous antiderivative, so the integral is
     `Σ F(right⁻) - F(left⁺)`. At a non-critical finite point the value is `rewrite(F, [x -> c])`; at a critical
     point or `±inf` it is a one-sided limit. An infinite one-sided value means divergence (2C4.2); an unknown
     limit leaves the call (unless another piece diverges: then the integral diverges anyway).
   - **Real logarithms** (2C4.5): on a piece where the argument `u` of an *additive* `log(u)` in `F` (a term of
     a sum, or a constant multiple of one; not inside another function or a product with another factor in `x`)
     is negative, `log(u)` is replaced by `log(-u)` (same derivative, `F` changes by a constant). So
     `integrate(1/x, [x, -2, -1])` is `-log(2)` instead of `-log(-2) + 1im*pi`. `x*log(x)` is not touched:
     there the replacement would change `F` by `iπx`.
3. **Symbolic bounds** (2C4.3): `F(b) - F(a)` only when `f` and `F` are continuous for every real `x`: built
   from polynomials by `exp`, `sin`, `cos`, `sinh`, `cosh`, `tanh`, `atan` and non-negative integer powers, or,
   failing that syntactic test, when the critical-point search finds no critical point on the whole real line
   (`1/(1 + x^2)` with `F = atan(x)`). A symbolic bound together with an infinite one gives up. The other
   symbols are taken as real (`atan(a*x)` is continuous).

## The limit helper (2C4.7)

`limit(g, x, point, side)` moves the point to `t → 0⁺` (`x = c ± t`, `x = ±1/t`) by `rewrite` and walks the
term. A limit is `Fin(v, s)` (evaluated finite value; when `v = 0`, `s` is the side it is approached from: ±1 or
0 unknown) or `Inf(±1)`. Rules: constants; sums (`∞ - ∞` by the series, else by the dominant term
`a·(1 + Σ others/a)`); products (`0·∞` by dominance: `exp(u)` with algebraic `u → ±∞` beats algebraic and
`log`-of-algebraic factors; an algebraic factor tending to 0 beats logarithms; else the series); constant powers
(sign and parity rules; the side of an approach to 0 from the series when the walk does not know it); `u^v` as
`exp(v log u)`; `exp`, `log` (0⁺ only), `atan` (±π/2 at ±∞), `sin`, `cos` (finite only), `sinh`, `cosh`,
`tanh`, `asin`, `acos` (inside [-1, 1]). The fallback is the leading term `c t^k` of `series(g, t, 0, n)` for
n = 4, 8, 16; it requires integer k and a coefficient whose sign is decided (k ≤ 0). Anything else is `None`.

## Decisions

- **2C4.1** Bounds are classified by `n(.)`: a real bound is a finite machine real numerically; comparisons use
  `sign(a - b)` on the exact terms (semantics 2.5: undecided comparisons give up, never guess).
- **2C4.2** A divergent integral is `failure(divergent, integrate, f)` (semantics 10: errors are values; `f`
  rather than the whole call so that re-evaluating the failure does not integrate again). This includes a
  definite infinite value (`integrate(1/x, [x, 1, inf])`): no `inf` is returned, as `inf` has no arithmetic in
  the system (`inf - inf` is 0 by cancellation). No principal values.
- **2C4.3** Symbolic bounds: only antiderivatives continuous on the whole real line (above).
- **2C4.4** Rational poles in the closed interval are decided before the antiderivative.
- **2C4.5** Additive logarithms of negative arguments are made real on each piece.
- **2C4.6** Unknown functions of `x` in `f` or `F`, polynomials with non-constant or undecidable coefficients,
  and roots of degree ≥ 3 non-binomial factors inside the interval give up (`Ok(None)`).
- **2C4.7** The limit helper is conservative (above); `None` is preferred to any guess.
- Generic parameters are treated as the indefinite integral treats them: `integrate(x^n, [x, 1, 2])` is
  `(2^(n+1) - 1)/(n+1)` (wrong for `n = -1`, as `integrate(x^n, x)` is).
- Powers of negative numbers follow the system's principal branch: `integrate(x^(1/2), [x, -1, 0])` is
  `complex(0, 2/3)`; `integrate(x^(-1/3), [x, -1, 1])` is `3/2 - (3/2)*(-1)^(2/3)` in some printed form.

## Checked with richer antiderivatives

The base table of the skeleton only has `x^n`, `1/x`, `exp(x)`, `sin(x)`, `cos(x)`. With a temporary
(uncommitted) extension of the table these were checked and gave the right values:
`log(x)` on `[0, 1]` → `-1` (limit of `x log x - x`); `log(x)` on `[-1, 0]` → `-1 + iπ`; `log(x)` on
`[1, inf]` divergent (`∞ - ∞` by the dominant term); `1/(1+x^2)` on `[-inf, inf]` → `pi`; `1/(4+4x^2)` with
`F = -(1/4)*atan(1/x)` on `[-1, 1]` → `pi/8` and on `[-inf, inf]` → `pi/4` (jump at 0 handled by splitting);
`exp(-x)` and `x*exp(-x)` on `[0, inf]` → `1`; `1/(x^2 - 1)` with `F = (1/2)(log(x-1) - log(x+1))` on
`[2, 3]` → `(1/2) log(3/2)` and on `[-1/2, 1/2]` → `-log(3)` (real by 2C4.5); `(1 - x^2)^(-1/2)` on `[-1, 1]`
→ `pi` (asin limits at ±1); `log(x)/x` on `[0, 1]` divergent, on `[1, e]` → `1/2`; `x*log(x)` on `[0, 1]` →
`-1/4`. These should become tests once the other 2C parts are in (they are not in the test file because
the antiderivatives do not exist here). `exp(-x)` on `[0, inf]` is not a test: the base table does not cover
argument `-x`.

## Known gaps

- Results are not simplified beyond the evaluator: `(1/2)*(log(2) - log(4)) + (1/2)*log(3)`.
- `series` does not expand fractional powers or logarithms at 0, so limits that need them beyond the walk's
  rules give up (e.g. `t^(1/2) - t` tending to 0 under a negative power).
- Roots of irreducible factors of degree ≥ 3 (non-binomial) inside the interval are not located: the call stays
  unless `f` is rational (then it is divergent).
- No periodic singularities (`tan`, `sec`, `log(sin(x))`), no `abs`, `sign`, piecewise functions, no
  oscillation at infinity (`sin(x)` on `[0, inf]` stays).
- Parameters in coefficients of critical polynomials (`1/(x - a)` on `[0, 1]`) give up.
- Complex bounds go through the symbolic path (only for antiderivatives continuous on the real line, which for
  `atan` of a polynomial is not the same as analytic in the plane).

## Changes needed elsewhere

None required. Suggestions for later work:
- When `limit` (a later item) exists, `definite/limit.rs` can be replaced by it.
- Functions added to antiderivatives by the other 2C parts that are not in the known lists of
  `points::collect` and `limit::lim` (e.g. `tan`, `acot`, `asinh`, `abs`, `erf`) make definite integrals give up
  until they are added there (one match arm each: the expressions whose zeros are critical, and the limit rule).

## Since round 17 (item 14D)

The driver is described in `NOTES.md` (section "Round 17 item 14D", decisions 14D.1-14D.10). Of the gaps
above: periodic singularities of `tan`, `sec`, `cot`, `csc` and `log(sin(u))` with linear arguments are located,
oscillation at infinity is decided by Dirichlet's test (`sin(x)` on `[0, inf]` is divergent), limits use Gruntz's
algorithm after the walk, values are tidied (expanded, combined over a denominator, logarithms of rationals
merged).
