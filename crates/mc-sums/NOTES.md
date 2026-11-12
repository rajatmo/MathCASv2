# Item 13B: `mc-sums` (sums, products and recurrences)

Round 16. Roadmap phase 13, item 13B: Gosper's algorithm, Zeilberger's algorithm, Petkovšek's
Hyper and Abramov's rational solutions for recurrences with polynomial coefficients, `r_solve` beyond constant
coefficients, `product` in closed form. Dependencies `mc-api`, `mc-arith`, `mc-core`, `mc-eval`, `mc-poly` (integer
polynomials: gcd, factorisation, conversion), `mc-simplify` (the round-15 cost measure, read only), `rustc-hash`.

## What was built

- `rf`: rational functions over the integers in the kernels of one ring (`num / den`, coprime, multivariate `gcd` of
  `mc-poly`), shifts `x -> x + h`, composition, coefficients in one variable, nullspaces and solutions of linear
  systems over the field of rational functions in the other variables (Gauss-Jordan with the smallest pivot).
- `hyper`: the ratio `t(k + 1) / t(k)` of a hypergeometric term, factor by factor: rational functions, `c^(a k + b)`,
  `exp`, `factorial`, `gamma`, `pochhammer`, `binomial`, `factorial_power`, `catalan_number` with integer slopes, an
  indefinite `product(g, k)`.
- `gosper`: Gosper's algorithm (Gosper 1978; *A = B* ch. 5). The Gosper form uses the dispersion set read from the
  irreducible factors (`p(k)` and `q(k + h)` proportional only for the `h` given by their two leading coefficients)
  instead of a resultant in a new variable. The polynomial equation is solved under the degree bound of Abramov,
  Bronstein and Petkovšek (operator in powers of `Δ`: the degree of the right side minus the order, or a nonnegative
  root of the indicial polynomial), the same solver serving Zeilberger, Hyper and Abramov. The certificate
  `R(k + 1) r(k) - R(k) = 1` is checked as an identity of rational functions before use.
- `zeil`: Zeilberger's creative telescoping (Zeilberger 1991; *A = B* ch. 6): orders 1 to 4, the unknown
  coefficients kept in the polynomial part of Gosper's equation so that one nullspace gives the recurrence and the
  certificate; the identity `sum s_j R_j = Rc(k + 1) r_k - Rc(k)` checked exactly.
- `recur`: Petkovšek's Hyper (1992; *A = B* ch. 8) over the divisor pairs of `p_0(n)` and `p_d(n - d + 1)` (bounded),
  roots `Z` that are rational in the parameters, polynomial `C` by the shared solver, every ratio checked against the
  recurrence; Abramov's universal denominator (1995) and rational solutions (homogeneous and with a right side).
- `prod`: products of rational functions (`pochhammer` form, or the rational function when the gamma functions
  cancel), infinite products by `prod gamma(lo - b) / prod gamma(lo - a)` with the convergence conditions checked,
  bilateral products, Euler's reflection for pairs `gamma(u) gamma(s - u)` (`-8 <= s <= 16`), `sin(i pi y) = i sinh`,
  `csch` for negative powers of `sinh`.
- `sumfn`: the closed forms of `sum` (see the module doc): telescoping, Gosper, partial fractions (harmonic numbers,
  `digamma`, Hurwitz `zeta`; even rational functions and bilateral sums through `pi cot`), Zeilberger with the
  recurrence solved and fitted to the first values (values normalised through `function_expand` and shift
  reduction), polynomial times geometric series by `theta = w d/dw`, hypergeometric series `t(lo) pFq` with Gauss's
  theorem and the elementary `0F0`, `1F0`, `0F1` (Bessel `I`), `1F1(1; 2)`, `2F1(1, 1; 2)`; closed forms tidied by the
  cost measure, binomial recognition (`gamma(x) / (gamma(y) gamma(z))` with `x = y + z - 1`, Legendre's duplication).
- `verify`: checks of definite sums at integer points with rational parameters (exact equality, else 30-digit balls,
  a value that is exactly zero recognised by its precision failure).
- `numsum`: numeric sums and products: direct summation, Wynn's epsilon for alternating series checked against
  Richardson on even partial sums, Euler-Maclaurin with `n_integrate` for slowly converging series, tails for long
  finite ranges, steps and nested iterators.
- `rsolve`: `r_solve` / `r_solve_value` (see the module doc), `recurrence_table`, iteration for point targets.
- `shift`, `conv`, `gf`: shifted values of known functions (for `discrete_shift`, `difference_delta`,
  `discrete_ratio`), `sum_convergence`, `generating_function`.
- `natives`: wrappers of the list library's `sum` and `product` and the calculus crate's `r_solve`,
  `r_solve_value` (installed after them with `Kernel::wrap_native`: the inner natives keep their fast paths, this
  crate's methods run when they decline; `r_solve` tries this crate first) and the eight new functions.

## Decisions

- B13.1 Wrapping, not replacing: `mc-lists` keeps its exact finite sums, polynomial and geometric closed forms;
  `mc-sums` is consulted only when they decline.
- B13.2 The indefinite sum of a non-polynomial is Gosper's antidifference as it comes (no constant fixed); of a
  polynomial the inner native's `S(0) = 0` form. The indefinite product is `prod_{j=1}^{k-1}`; factorials under
  symbolic powers are written as `gamma`.
- B13.3 First-order recurrences use `h(n) = prod_{k=1}^{n-1} r(k)`, so `c(1) = a(1)` (`c(1) 3^(n - 1)` for
  `a(n + 1) = 3 a(n)`); the test of `mc-calculus` was updated.
- B13.4 Closed forms of sums with symbolic bounds are checked from bound value 1: identities are generic
  (`sum((-1)^k binomial(n, k))` is 0 for `n >= 1`).
- B13.5 Infinite series whose convergence is undecided (a symbolic ratio) get the generic answer (`(1 - a)^-1`);
  decided divergence leaves the sum unevaluated.
- B13.6 Budgets: 20 s per call for the polynomial kernels (time and the interrupt), at most 4 Zeilberger orders, 1024
  Hyper pairs, degree 60 for polynomial solutions, reflection pairs with `|s| <= 16`, 100 000 numeric terms.

## Checks and accuracy

- Every Gosper and Zeilberger certificate is checked exactly; every definite closed form with symbolic data is
  compared with the terms at several points; every `r_solve` solution is substituted back (at runtime and in the
  tests). A wrong closed form is never returned: the sum stays unevaluated instead.
- `n_product(1 + (-1)^i/i^2, [i, 100, 10^4])` gives `1.0000503370333946` (an independent float64
  log-sum gives `1.0000503370333864`).
- `n_sum(1/((k - 20)^2 + 1), [k, 0, inf])` gives `3.10461587694`.

## Tests

`crates/mc-sums/tests/sums.rs`: Gosper, Zeilberger, infinite sums, products, numeric sums, recurrences checked by
substitution (14 recurrences, residual below 1e-9 at n = 2..7 with the constants given values), `r_solve` forms,
differences and ratios, generating functions and convergence, interruption.

## Known gaps

- Systems of recurrences, partial recurrences (`a(m, n)`), recurrences with non-rational coefficients (`2^n`),
  algebraic roots `Z` in Hyper, non-hypergeometric solutions (Legendre, Bessel, digamma), nonlinear `r_solve`.
- Multiple sums whose inner sum is not hypergeometric (Euler sums), `q`-products, Abel regularisation,
  `generate_conditions` (the generic answer is given).
- The forms of `r_solve` general solutions with polynomial coefficients are not normalised to a fixed basis.
- The engine does not simplify `binomial(m, 0)`, `gamma(m + 1)/gamma(m)` or `2 2^(1 + n) - 2^(2 + n)`; this crate
  works around the first two locally (`sumfn::normalize_values`); the third keeps `simplify(eqs /. sol)` from giving
  `true` on two `r_solve` examples (a request for `mc-simplify`).

## Round 20

- `n_sum`/`n_product` with `working_precision: d` (or `digits: d`), and `sum` with ball summands or bounds (what
  `n(sum(...), d)` leaves of a sum without a closed form), at d digits (`numsum::numeric_digits`): finite ranges
  term by term; terms falling geometrically summed until below 10^-(d+15); alternating decreasing terms by
  Cohen, Rodriguez Villegas and Zagier's Algorithm 1 (exact weights, checked by a second run with 12 more terms);
  otherwise the tail from N by the Abel-Plana formula, the kernel integral over unit pieces of [0, T] with expm1,
  computed for N = lo + 16 and lo + 24, which must agree to d digits (this rejects non-analytic summands such as
  `mod`, which fall back to machine precision). Products as exp of the sum of logarithms (the logarithm's tail
  integral does not converge at high precision yet: products fall back to machine precision; open).
- Q19-07: indefinite sums of summands periodic in k (every k inside `mod(p(k), m)` with an integer polynomial p):
  F(k) = C floor(k/P) + sum_r c_r floor((mod(k, P) + P - 1 - r)/P), verified by differencing at 3P + 2 integers.
- Q19-26 deferred: `sum` evaluates its body before it sees it (it does not hold), so `sow` inside runs once; making
  `sum` hold its arguments changes evaluation order across mc-lists, mc-sums and mc-decide's wrappers.
- Machine `n_sum`: zero terms before the first nonzero one (up to 1000, e.g. `unit_step(n - 3)`) no longer count as
  convergence (`n_sum((n - 3) unit_step(n - 3) 5^-n, [n, 0, inf])` was 0.0, now 1/400; reported by n14e).
- The Abel-Plana tail integral is tried in closed form first (2 s), each numeric integral under a 10 s (tail) or 20 s (kernel) limit; a
  summand that defeats them falls back to machine precision instead of timing out.
- `n_sum(1/n^(51/50), [n, 1, inf], working_precision: 40)` gives
  50.578670041015603217613302535407415749695 (= zeta(51/50) to 44 digits, checked against `n(zeta(51/50), 45)`).
  It takes 14 s; the `n_product` of the same kind falls back to machine precision after the time limits.

## Round 22: divergence, oscillating series, precision speed

- **Verdict before the number** (`series::classify`; ui B21-1, D21-01, D21-04): after 64 terms of an infinite machine
  sum the size of the terms is sampled in windows of 24 at `lo + 2^10`, `2^13`, `2^16` (a machine real index first,
  an exact integer index when that gives no number), and the exponent `p` of `|f(k)| ~ k^-p` fitted to the window
  maxima. Terms that overflow, do not tend to zero (`p <= 0.02`), or are of one sign with `p <= 1.003` diverge; an
  exponent between 1 and 1.1 that drifts towards 1 (`1/(k log k)`) is "too slowly convergent". Both print a message
  (`n_sum: the sum does not converge ...`) and leave the call unevaluated, as do products whose logarithms do not
  converge. A numeric `n_sum` of a symbolic summand says so (D21-26). Options are read (`method:`, `rigorous:`;
  unknown names print a message; D21-25): `euler_maclaurin` forces the Abel-Plana tail, `alternating_signs` the CVZ
  acceleration, `direct` the term-by-term sum.
- **Alternating series** (B21-31): Cohen, Rodriguez Villegas and Zagier, Algorithm 1, in machine arithmetic from 30
  terms, accepted when 24 and 30 terms agree to 1e-11 (`(-1)^k/sqrt(k)` was 1% off: the old fallback assumed an
  error expansion in 1/N, but the partial sums of `k^-1/2` have one in `N^-1/2`).
- **Other convergent series** (D21-02, B21-25): the Abel-Plana formula in machine arithmetic: the tail integral by
  `integrate` (2 s) or `n_integrate` with `accuracy_goal: 17`, the second integral by 3 x 32-point Gauss-Legendre
  panels on `[0, 7.5]`; the splits `lo + 16` and `lo + 24` must agree to 1e-11 (else the old Euler-Maclaurin path).
  `sin(k)/k` is now `(pi - 1)/2` to 3e-16 (was 1.9e-8). Needed a fix in mc-quad: see its notes (oscillatory tails).
- **Finite products** are multiplied (`n_product(k, [k, 1, 10])` is 3628800.0, D21-03); logarithms only on
  overflow. A finite range over 20000 terms whose tails diverge is summed term by term (compensated), up to 3e6.
- **Precision path speed** (D21-08, R20-14, R21-G4, B21-26): the symbolic `sum`/`product` of an exact summand is tried
  first under 2 s and `n` of its closed form taken (`n_sum(1/k^2, ..., working_precision: 30)` 2.6 s -> 0.16 s; the
  60-digit `1 - 1/(k+1)^2` product 20 s -> 0.09 s; the R20-17 test passes through this). Products of exponentials use
  the exponent sum; the classification runs before any integral. Summands that vanish in machine arithmetic
  (`exp(-k^5)`) are summed directly. Open: summands without a closed form at 40+ digits still go through
  `n_integrate` on complex pieces (`log(k)/k^3` at 40 digits takes 12 s, then machine precision); alternating
  products with non-monotone logarithms (`1 + (-1)^k/k^2`) at working precision fall back to machine precision.
- B21-24: `sum(f, [k, a, b])` for a small integer `a` with no closed form is the sum from 1 or 0 less the first
  terms (`sum((-1)^k/k, [k, 2, inf])` = `1 - log(2)`, `sum(binomial(n, k), [k, 1, n])` = `-1 + 2^n`).
- R20-10 (sums with side effects) belongs to the evaluator (`sum` evaluates its body before it sees it): not done.

## Round 24

- **Series near p = 1 (D23-08, D23-09, B23-30).** The verdict of `series::classify` for one-signed terms whose windowed
  exponent is within 0.1 of 1 comes from `boundary`: the local exponent `p(x) = -log2(|f(2x)|/|f(x)|)` at
  `x = 1e8, 1e16, ..., 1e96` and `d = p - 1`. `d` constant and positive is a power law (convergent: `k^-1.0001`
  gives 10000.5772229, `k^-1.01` 100.57794334, both to 15 digits); `d ln x` constant is `1/(k ln^q k)` (Cauchy's
  condensation test: convergent for `q > 1.05`, divergent below 0.95, otherwise a message); `d` negative, or falling to
  0 faster than `1/ln x` (`1/k + 1/k^1.5`), diverges; a drifting exponent (`1/(k ln k ln ln k)`) is a message. The
  old windows rule called p <= 1.003 divergent (1.0001 included).
- **Tail integrals** (`series::tail_integral`, used by Abel-Plana and Euler-Maclaurin): `integrate` of the integrand in
  `u = ln k` (`log(k)` replaced by `u`, `exp(u)` powers combined by `power_expand`) first, then in `k` for an exact
  summand, then `n_integrate` in `u` (explicit `precision_goal: 6`, which accepts a usable estimate without the warning),
  then in `k`. In `k` the double-exponential rule stops at 1e308 and loses `X^-0.01 / 0.01` (0.09 of 97.27 for
  `k^-1.01`): the wrong 100.4897 of D23-08 (the cause was in `n_integrate`, see mc-quad NOTES). In `u` a slow power is an
  exponential (`e^(-0.01 u)`) and `1/(k ln^q k)` is `u^-q`, which the exponential-sinh rule integrates to 15 digits.
  Oscillating integrands keep the `k` form first (the oscillatory method needs it). The Richardson fallback of `slow`
  is accepted only when it does not move by 1e-6 when the first mark is dropped (a tail like `N^-0.01` gave numbers 10% off).
- **Alternating series with a rising start (B23-24).** `alternating_sum` (machine) and `numeric_digits` (digits) start the
  Cohen-Rodriguez Villegas-Zagier acceleration at the first of `lo + 0, 16, 32, 64, 128` at which 30 terms alternate with
  falling magnitudes, the leading terms added directly. `(-1)^k log(k+1)/sqrt(k)` from 3 gave -0.348 in machine
  precision (the true value is -0.3977565, the mean of two consecutive partial sums of 10^5 terms) and a silent
  machine-precision value at 20 digits; both are now -0.39775645674135 at 17 and 20+ digits. Alternating terms whose
  pair sums do not cancel (`(-1)^k/sqrt(k) - 1/(2k)`, the logarithm of `1 + (-1)^k/sqrt(k)`) are told by the exponent
  of the pair sums at 2^13 and 2^16 (divergent for <= 1.003), when the pair sums are above 1e-9 of the terms (below
  that they are rounding noise: the logarithms of `1 + (-1)^k/k` pair to 1/k^4 and that product converges to 1).
- **Long finite ranges (B23-8, B23-27).** A finite range is summed or multiplied with the summand compiled
  (`mc-numfn`, policy `Compiled`; about 20 ns a term): up to 2e7 terms directly, to 4e8 when the tails diverge. The old
  switch took the difference of two infinite tails, which assumes convergence (`n_product(1 + (-1)^k/sqrt(k), [k, 2,
  10^6])` was 0.0212, true 0.0016792387; 10^6 terms of `1 - 1/(2k)` took over 10 s, now 0.04 s). Products by the
  product itself, the logarithms (parity of the negative factors kept) when it overflows.
- **Bounds.** `n_sum` and `n_product` accept non-integer bounds (the terms are `a, a + 1, ...` up to `b`, the summand
  shifted to an integer index) and `-inf` as the lower bound (`n_sum(exp(-k^2), [k, -inf, inf])`, D23-27); an exact
  non-integer bound (3/2) reaches no closed form of the integer-bounded kind (D23-18). A product with a zero factor is 0.
- **Divergent exact sums (B23-7).** `2F1(1, 1; 2; z) = -log(1 - z)/z` only for `abs(z) <= 1`: `sum(2^k/k, [k, 1, inf])`
  is left unevaluated instead of `-im*pi`.
- **Messages.** `Sx::subst` replaces the index structurally (no evaluation of the summand first) and several
  iterators keep the inner sum unevaluated until the outer index has a value: no message, instead of one per
  outer value (D23-19). `n_sum(..., working_precision: d)` with `d > 16` that only a machine value could satisfy
  returns the call and a message instead of a 16-digit value printed as `d` digits.
- Gaps: `n_sum(cos(k)/k, ..., working_precision: 30)` takes 15 s and then declines (an oscillating tail through
  Abel-Plana on complex pieces at 45 digits; a Dirichlet-type acceleration would do it); `log(k)/k^3` at 40 digits
  takes 9 s (R22-10: right to 40 digits now, no longer machine precision); `1/(k ln(k + 1)^1.01)` and
  `1/(k ln k ln ln k)` are messages, not numbers (the tails need the integral of a function of `ln ln k`).

## Round 25

- **R24-03, Euler-Maclaurin tail (`numsum::euler_maclaurin`).** `n_sum(cos(k)/k, ..., working_precision: 30)` took 15 s
  and declined; `log(k)/k^3` at 40 digits 9 s. For an infinite range the first method is now the tail
  `int_N^inf f + f(N)/2 - sum_j B_(2j)/(2j)! f^(2j-1)(N)` (Olver 1974, ch. 8 section 1; DLMF 2.10.1) from
  N = lo + 1.2 w + 16. The derivatives come from the Taylor coefficients of f at N, which are the trapezoid sums of
  f on the circle of radius N/4 (aliasing error 4^-M, M = (w + 12)/0.6 + 8 points, half of them for a real f by
  conjugate symmetry), so the correction is `(1/M) sum_j f(N + r z_j) W(z_j^-1/r)` with the polynomial
  `W(q) = sum_(m odd) B_(m+1)/(m+1) q^m`, whose coefficients come from DLMF 24.12.2 (zeta(2n)); only M values of f
  are needed whatever the order. The tail is computed with J and J - 6 terms (J = 0.65 w + 12), which must agree to
  10^-(w + 2), and the sum from two values of N (8 apart) must agree to d digits; any miss falls back to the
  Abel-Plana route as before. The integral `int_N^inf f` is the closed form when `integrate` finds one in 2 s
  (`cos(k)/k` is `-cos_integral(N)` since round 25, see mc-calculus). Times: `cos(k)/k` 30 digits 0.3 s, `sin(k)/k`
  0.3 s, `log(k)/k^3` 40 digits 0.4 s, `log(k)/k^2` 40 digits 0.7 s.
- The method needs (theta/2 pi)^(2J) below 10^-w for an oscillation of angular frequency theta, so `cos(3 k)/k` and
  `cos(2 k)/k^2` at 30 digits still go to Abel-Plana and decline after about 5 s; `cos(k)/k` at 45 digits (the closed-form
  integral at 160 digits exceeds its 2 s limit) as well.

## Round 27

- `n_product` that overflows a machine real gives `inf` (the exponential of the log sum was `inf * sin(0) = nan`); `number`
  returns the symbol `inf` for an infinite real (B26-13).

## Round 32

- `generating_function` of `discrete_delta(n - k)` (z^k, 0 for k < 0) and `unit_step(n - k)` (z^k/(1 - z)).

## Round 36

- `recurrence_table(eqs, a, [n, nmax])` starts at the first initial index (1 when there is none), as the other form of the
  range does; `working_precision: d` rounds the exactly iterated values to d digits (`machine_precision`: doubles).
- `generating_function(..., generate_conditions: true)` attaches the ratio-test condition |z| < 1/r, r the limit of
  |a(n+1)/a(n)| (none when r is 0 or the limit is not found). Literature: Wilf, generatingfunctionology, ch. 2.

## Round 36, follow-up

- `sum(.., [k, a, inf], generate_conditions: true)`: the ratio-test condition r < 1 when r depends on the parameters.
- The reflection formula is not applied to a gamma pair at an integer (gamma(1) gamma(3) of (k^2 - 1)/(k^2 + 2) divided by sin(pi) = 0).

## Round 38: convergence on the scale of iterated logarithms

- The comparison of `sum_convergence` with `n^p log(n)^q log(log(n))^r log(log(log(n)))^s` (Bertrand's series)
  goes one level deeper (four levels), and each level's limit `lim g / log^(k + 1)(n)` that `limit` does not find
  directly is taken after the change of variable `n = exp^(k + 1)(v)` with `power_expand`, where the iterated
  logarithms are powers of v (`scaled_limit`). `1/(n log n log log n)` diverges, `1/(n log n (log log n)^2)`
  converges.
- Gap: summands with `harmonic_number` and other discrete functions (no asymptotic equivalent is substituted).
