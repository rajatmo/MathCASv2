# mc-special: special functions

Status: done. At completion: full
suite 855 passed, 0 failed, 6 ignored; clippy clean. References: the crate's own ball path and published constants
(mpmath was not available); double-double constants computed with Python's `decimal`.

Functions (real arguments): `gamma`, `log_gamma`, `digamma`, `polygamma(n, x)`, `beta(a, b)`, `zeta`,
`bessel_k(nu, x)`, `bessel_i(nu, x)`, each at four levels: exact values, machine precision, balls at any precision,
derivatives in `d`.

## Modules

| module | contents |
|---|---|
| `dd` | double-double arithmetic (two-sum, Dekker/Veltkamp products with a safe split above 2^995, no FMA): ops, `exp`, `exp_scaled`, `log`, `sqrt`, `sin_cos_pi` with exact reduction; `Scaled` carries its own exponent so only the final rounding over- or underflows |
| `bernoulli` | exact B_2n from tangent numbers (Brent-Harvey), cached process-wide, grown by doubling, never cached after a stop; index cap 8000 |
| `machine::{gamma, zeta, bessel}` | machine paths (double-double inside, one rounding) |
| `ball::{gamma, zeta, bessel}` | rigorous enclosures; `ball::refine` is the retry loop |
| `exact` | closed forms |
| `functions` | `Special` (names, arities, machine and ball dispatch); implements `mc_numeric::library::LibraryFn` |
| `natives` | list threading, contagion, dispatch; `failure_tags::POLE` |

## Algorithms

Machine: gamma below 20 as x = n + z, |z| <= 1/2, Γ(1+z) = 1/R(z) (Taylor series of 1/Γ(1+z); the z+k factors exact
in double-double, so relative accuracy holds near poles), Stirling from 20, reflection below -200; log_gamma near 1 and
2 by log1p forms (no cancellation at the zeros), Stirling from 20; digamma by shifting to y >= 12 then the asymptotic
series, reflection for x < 0; polygamma by shifting to y >= n/2 + 20 then the asymptotic series, upward recurrence for
x < 0 (up to 2·10^6 steps); beta as exp(lnΓa + lnΓb - lnΓ(a+b)); zeta by Euler-Maclaurin (M = 16, N so the bound is
below 2^-80) for s >= 0 and the functional equation for s < 0; bessel_k/bessel_i by Temme (series for x <= 2,
Steed's CF2 with q-rescaling for x > 2, forward recurrence in the order; I by CF1, downward recurrence and the
Wronskian; I_-nu = I_nu + (2/π) sin(νπ) K_nu).

Balls (truncation bounded and added to the radius): log gamma by Stirling at a shift y >= p/2 + 10 (enveloping for
real y > 0: remainder at most the first omitted term), reflection with exact sin(πx) reduction; digamma/polygamma by
the asymptotic series plus recurrence; zeta by Euler-Maclaurin for s > -1 with Johansson's bound
|R| <= 4|(s)_2M|/(2π)^2M · N^(1-σ-2M)/(σ+2M-1) (N, M by a cost search), functional equation for s <= -1; I by its
power series (tail at most twice the last term once the term ratio is <= 1/2); K_n (integer) by A&S 9.6.11; K_nu
(non-integer) as π/2 (I_-nu - I_nu)/sin(νπ) with extra bits near integers; large x by the asymptotic expansion
(remainder at most the first omitted term once k >= |ν| - 1/2, DLMF 10.40.10) when its smallest term reaches 2^-p.
`refine` starts at p + guard and doubles up to max(16p, p + 4096), stopping early when the radius stops shrinking.

## Decisions

- **SF.1** Exact poles give `failure(pole, call)` (gamma family at 0, -1, ...; zeta(1); bessel_k(ν, 0);
  bessel_i(ν, 0) for negative non-integer ν; beta at poles of Γ(a) or Γ(b)); a ball touching a pole stays.
- **SF.2** Closed forms (below) with size caps; other exact arguments stay unevaluated for `n`.
- **SF.3** Machine poles follow the C library (gamma(±0) = ±inf, NaN at negative integers, zeta(1.0) NaN,
  digamma(±0) = ∓inf); complex-valued machine cases stay unevaluated.
- **SF.5** Machine tables (48 Taylor coefficients of 1/Γ(1+z) at 256 bits, Bernoulli quotients) are computed at
  first use from the exact and ball paths; nothing copied by hand.
- **SF.6** log_gamma(x < 0, non-integer) is ln|Γ(x)| - iπ·ceil(-x) (the limit from the upper half plane).
- **SF.7** Complex arguments stay unevaluated.
- **SF.8** K_nu with wide ball arguments uses monotonicity (hull of two corner values).
- **SF.9** `n` sees the functions through a registry in `mc-numeric` (`library.rs`: `LibraryFn`, `register`,
  `lookup`; `numeval` builds `Node::Lib` for unknown heads with numeric arguments and evaluates them on balls,
  complex balls and machine numbers; exact arguments are passed exactly, so they are refined, not rounded once).

## Exact values

gamma(n) = (n-1)! (n <= 2^20), Γ(k+1/2), Γ(1/2-k) with √π; log_gamma of those (with -iπ for negative half-integers);
digamma(n) = H_{n-1} - γ, ψ(k+1/2); polygamma(0, x) = digamma(x), polygamma at positive integers and half-integers via
zeta(n+1); zeta(0) = -1/2, zeta(2n) via B_2n and π^2n (2n <= 500), zeta(-n) = (-1)^n B_{n+1}/(n+1); beta through
exact gammas; bessel_k(n + 1/2, x) = √(π/2x) e^-x Σ (n+k)!/(k!(n-k)!) (2x)^-k (|ν| <= 200.5, symbolic x too);
bessel_i(ν, 0), I_{±1/2}(x).

## Edits outside the crate

`mc-numeric` (`library.rs`, `Node::Lib` in `numeval.rs`); `mc-calculus` (derivative rows for gamma, log_gamma,
digamma, polygamma, beta, bessel_k, bessel_i; `rules::OPAQUE` and `rules::resolvable` so partials in an order
argument, `derivative(1,0)(f)(...)`, are not resolved forever); `mc-engine`, `mc-lsp` registration.

## Tests (25)

Accuracy (`tests/accuracy.rs`): machine values against 160-bit ball references at the exact binary arguments, max
0.50 ulp, mean 0.23-0.25, over gamma and log_gamma (1507 points, [-20, 170] plus near poles and zeros), digamma (1500,
[-20, 100]), polygamma (1500, n = 1..6), zeta (808, [-20, 60] plus 1±1e-8), bessel_k and bessel_i (540 each,
ν in {0, 1/3, 1/2, 1, 2, 2.5, 5, 10, 30}, x in [1e-6, 700]). Enclosures (`tests/enclosure.rs`): 319 random at 50
digits and 64 at 300 digits contain the 2p+64-bit result with >= p-4 correct bits; ball arguments contain the values
at their ends. Session (`tests/session.rs`): exact values, symbolic arguments, poles, machine NaN/inf; `n` against
published values to 40-50 digits (Γ(1/3), Γ(1/4), ζ(3), ζ(5), K₀(1), K₁(1), I₀(1), ψ(1/3)); nested `n`;
comparisons; identities at 50 digits (recurrences, reflection, Wronskian, functional equation); derivatives checked
by central differences to 1e-20; `n_integrate`, `find_root(digamma)`, compiled `gamma(x)*zeta(x)`; 1 vs 4 threads;
interrupts at 200000 digits stop in about 120 ms.

## Measurements (release, busy machine)

| function | machine | 15 digits | 50 | 300 | 1000 |
|---|---|---|---|---|---|
| gamma | 1.7 µs | 0.08 ms | 0.23 ms | 1.3 ms | 45 ms* |
| log_gamma | 1.4 µs | 0.08 | 0.27 | 1.5 | 8.4 |
| digamma | 0.9 µs | 0.06 | 0.18 | 1.2 | 10 |
| polygamma | 17.5 µs | 0.07 | 0.17 | 1.4 | 16 |
| beta | 4.3 µs | 0.27 | 0.88 | 4.2 | 359* |
| zeta | 12.8 µs | 0.50 | 2.8 | 56 | 656 |
| bessel_k (x = 2.5) | 37 µs | 0.12 | 0.19 | 1.1 | 8.9 |
| bessel_i | 38 µs | 0.14 | 0.37 | 1.9 | 55 |

\* includes the first Bernoulli cache growth.

## Known gaps

1. Complex arguments stay unevaluated (also K and non-integer I at x < 0, non-integer polygamma orders).
2. Machine Bessel near x ≈ 2 is about 37 µs (CF2 needs about 250 double-double iterations); no plain-f64 fast path.
3. The ball gamma family is capped by the Bernoulli index (8000): about 6000 digits.
4. Machine polygamma at x < -2·10^6 gives NaN; machine beta for large arguments loses about |log B|·1e-16.
5. Compiled code declines these calls; the interpreter's machine path runs them.
6. Inside `n`, a library function with a complex-valued argument gives NaN (machine) or no value (ball).
7. Exact caps: gamma n <= 2^20; sums 10^4 terms; polygamma order <= 1000; log_gamma at integers n <= 1000; zeta
   2n <= 500; half-integer Bessel order <= 200.5.
8. The library registry is process-wide.

## Phase 3C: more special functions

41 `FUNCTIONS` entries, each with exact special values, machine values (measured within 0.5 ulp on every grid
except one documented terminating alternating 1F1 case), rigorous balls at any precision (truncation bounded, precision
doubled by `refine`) and derivatives in `mc-calculus`:

| group | functions |
|---|---|
| error functions | `erf`, `erfc`, `erfi`, `inverse_erf`, `inverse_erfc` |
| incomplete gamma and beta | `gamma(a, x)` (upper), `gamma_regularized` (Q), `beta(z, a, b)` (B_z), `beta_regularized` (I_z) |
| exponential integrals | `exp_integral_ei`, `exp_integral_e(nu, x)`, `log_integral`, `sin_integral`, `cos_integral`, `sinh_integral`, `cosh_integral` |
| polylog | `polylog(s, z)` |
| hypergeometric | `hypergeometric_2f1`, `hypergeometric_1f1`, `hypergeometric_0f1`, `hypergeometric_pfq([as], [bs], z)` |
| Bessel and Airy | `bessel_j`, `bessel_y`, `airy_ai`, `airy_bi`, `airy_ai_prime`, `airy_bi_prime` |
| elliptic | `elliptic_k(m)`, `elliptic_e(m)`, `elliptic_e(phi, m)`, `elliptic_f(phi, m)`, `elliptic_pi(n, m)`, `elliptic_pi(n, phi, m)` (m = k^2) |
| Jacobi | `jacobi_sn`, `jacobi_cn`, `jacobi_dn`, `inverse_jacobi_sn` |

Rust API for other crates (all `f64`, public): `machine::erf::{erf, erfc, erfi, inverse_erf, inverse_erfc,
erf_complex}`, `machine::incomplete::{gamma_p, gamma_q, gamma_upper, gamma_lower, beta_reg, beta_inc,
inverse_gamma_p, inverse_gamma_q, inverse_beta_reg}`, `machine::elliptic::{carlson_rf, carlson_rd, carlson_rj,
carlson_rc, amplitude}`; balls `ball::erf::*`, `ball::incomplete::*`, `ball::elliptic::carlson_*`,
`ball::hyper::pfq_sum`.

Decisions: **3C.1** one native per name with several arities (`gamma(x)`/`gamma(a, x)`, `beta`, `elliptic_e`,
`elliptic_pi`). **3C.2** exact closed forms only at special points (`exact_more.rs`); poles give `failure(pole, call)`.
**3C.3** a machine point with a complex or undefined value leaves the new functions unevaluated. **3C.4** complex
machine arguments for erf/erfc/erfi and integer-order polylog (on the cut z > 1 the limit from below).
**3C.5** `hypergeometric_pfq` is its own native (list parameters), reducing to the named functions.

Algorithms (each module documents its own and its error bounds): double-double machine paths (erf Taylor tables and
the Laplace continued fraction; Temme / series / Legendre fraction for incomplete gamma; the beta continued fraction;
E1/Ei series, fraction and asymptotics; polylog series, log expansion with zeta values, eta form, inversion; 2F1 by
the four transformations; Kummer and asymptotics for 1F1; Steed's method and Hankel for J/Y; Carlson duplication and
AGM; descending Landen for Jacobi); ball paths with proven remainders (DLMF 7.12.1, 8.11(ii), 6.12, 10.17(iii),
10.40.10, a derived Ei bound) and monotonicity-checked Newton for inverses.

Tests: `tests/session_3c.rs` (exact values and poles, machine and complex values, `n` to 20-40 digits against
published values, 33 derivatives against central differences at 60 digits, 1 vs 4 threads, interrupts) and seven
`accuracy_*` files (grids against 160-bit references, random enclosures at 50 and 300 digits: 1,963 in all).
Measurements: erf 0.1 us, incomplete gamma 3-5 us, 2F1 12-20 us, elliptic K/E 0.35 us, J/Y 3-26 us (machine);
50-digit balls 0.03-8 ms.

Known gaps: complex arguments only for erf/erfc/erfi and integer-order polylog (no complex balls); non-integer
polylog off the real range near large negative z and for z > 1; pFq with p = q + 1 at |z| >= 1 only for 2F1 or
terminating; ball 1F1 for |z| > 2^20; incomplete gamma for very large a (cost ~sqrt(a), NaN beyond ~1e10, no Temme
uniform expansion); elliptic m > 1 only where real; machine J/Y for |nu| > 1e6; ball Ei range limits; no user-level
inverse regularized gamma/beta (Rust API exists); no derivative for `hypergeometric_pfq`; no symbolic symmetries
(erf(-x) = -erf(x)); `dd::split` recurses on +-inf (new code guards its inputs); compiled code declines these calls.
mc-dist still uses its private stopgaps (its DI.2) and can switch to the API above.

## Round 19

Values fixed or added (DLMF formulas cited in the code): `hypergeometric_u(-n, b, z)` as the polynomial of DLMF
13.2.7 (for any b, z; machine `a = 0.0, -1.0, ...` too, so `whittaker_w` at those values); complex machine U for
non-integer b; `exp_integral_e` at complex machine arguments (continued fraction / series); Y_n and K_n of integer
order at complex z (DLMF 10.8.1, 10.31.1: the Kelvin functions now evaluate); `fresnel_f/g` by the asymptotic series
for |x| >= 6 and from machine S, C below (was 15-22 s at 10.0); half-integer Struve functions in closed form;
`parabolic_cylinder_d` at integer orders (Hermite) and at 0, and its non-integer orders (a ball sqrt(2) was an exact
radical, so the value never became a number); `erf`, `erfi`, `sin_integral`, `sinh_integral`, `inverse_erf` odd;
`log_gamma(1.0)` is 0.0; `logistic_sigmoid` at exact multiples of i pi; harmonic numbers by binary splitting
(100000 in 0.4 s, was 2.8 s); machine integer degrees and orders of the polynomial families and spherical Bessel
functions (no gamma poles, real at negative z); `elliptic_nome_q` at negative m; complex machine values under `n` of
`dawson_f`, `fresnel_s/c`, the incomplete gamma and beta functions; `q_factorial(n, q)` now takes the count first
(its documentation page says so) and handles |q| > 1; the gamma family at `complex_infinity` and any
function at `indeterminate` give `indeterminate`. The `n_sum`/`n_product` examples of the round 19 bugs
are exact to 1e-15.

## Round 20 (the precision contract, roadmap 16B)

- `ball::cseries`: complex-ball power series. pFq with complex parameters and argument (ratio bound of the real
  series with magnitudes |k + a| <= k + |a|, |k + b| >= k - |b|; working bits raised by the largest term); Bessel
  J, I by (z/2)^nu / Gamma(nu + 1) 0F1 (DLMF 10.2.2, 10.25.2, principal powers), Y and K of non-integer order
  (10.2.3, 10.27.4); Airy Ai, Bi, Ai', Bi' by 0F1 with c1 = Ai(0), c2 = -Ai'(0) (DLMF 9.4.1-4), |z| <= 200;
  digamma and polygamma at complex arguments by shifting to Re w >= p/2 + 10 and the asymptotic series (DLMF
  5.11.2, 5.15.8) with the remainder bound times 2^(M + n + 1) (sec(arg w / 2)^2 <= 2 for Re w > 0).
- `ball_value`: J_nu(x), I_nu(x) for x < 0 and non-integer nu are e^(i pi nu) times the value at -x (the value
  above the cut, as the principal powers give).
- P19-05 (`n(sum(1/(k^3 + 1), [k, 1, inf]), 40)`), P19-07 (`n(bessel_j(1/3, -2), 30)`), P19-08
  (`n(airy_ai(1 + im), 30)`) reach their digits. `tests/precision_contract.rs`: 30 digits agree with 60 and the
  30-digit ball contains the 60-digit value, for 20 expressions; three values against an independent evaluation.
- `bessel_j(1/3, -2.)` is off at 8e-15 relative in v2's machine complex path (the ball path is exact to its digits);
  recorded, not changed.

## Round 22 (bug fixing)

- **engine B21-1** `machine::complex::polygamma`: the recurrence psi^(m)(z) = psi^(m)(z+1) - (-1)^m m!/z^(m+1) had the
  sign of the correction term reversed for Re z < 1.
- **engine B21-2** `hermite_h` at a non-integer degree and an inexact argument (`more.rs`, `hermite_high_precision`):
  the 1F1 form of DLMF 18.5.13 cancels like e^(x^2); the argument (a machine number or a ball midpoint) is made exact
  and the form is evaluated by `n` at 17 (or the ball's) digits plus 0.44 x^2 + 12 guard digits, then rounded to the
  kind of the argument. The radius of an input ball is not propagated (documented gap).
- **engine B21-3, B21-6, B21-7, B21-8, ui B21-4** the complex-ball layer `ball::cspecial` (new): 0F1, 1F1, 2F1 for any
  complex parameters and argument (six argument maps with the Pfaff and connection formulas, DLMF 15.8; the
  degenerate cases b - a or c - a - b integer by a fourth-order Richardson limit over parameter shifts, which is the
  one non-rigorous step, radius 2^-p of the value), Ei, li, Si, Ci, Shi, Chi (DLMF 6.6), E_n (integer n by E_1 and
  the recurrence, other n through the incomplete gamma function), Gamma(a, z) and its regularisation, B(a, b) with the
  finite limit at cancelling poles, the incomplete beta function, K(m) and E(m) from 2F1 (side Im m = -0 beyond
  m = 1, so `elliptic_k(12.)` is `0.4634 - 0.7691im` and `elliptic_e(4.429)` is `0.3847 + 1.4696im`), Y_n and K_n of
  integer order at complex z (DLMF 10.8.1, 10.31.1) and Li_s (series for |z| < 0.9, the Bernoulli-polynomial inversion
  for integer s and |z| > 1.1). `Special::ball_value` continues with the complex enclosure when the real one has no
  value, so `n(bessel_y(1, -1))` and `n(cos_integral(-1))` are the complex values. Machine arguments with no
  machine algorithm (NaN, a cut, a complex value, outside a series' range) are evaluated at 192 bits and rounded
  (`natives::machine_by_balls`; `mc_numeric::numeval` does the same for library functions under `n`).
  `n(cot(1 + im))`, `n(asinh(3 + 2 im), 30)` etc.: `mc_numeric::formula_lib` gained complex machine and complex-ball
  forms of the reciprocal and inverse circular and hyperbolic functions, `sinc` and the Gudermannian pair.
  `cseries::up` overflowed (a^2 + b^2) for values above 2^512: 1F1 at |z| ~ 570 stopped at 98 digits; now `hypot`.
- **engine B21-4** `beta(-m, k)` for a positive integer k is (k - 1)!/(-m)_k (exact, machine and complex ball).
- **engine B21-5** `kelvin_ker/kei` use the principal K_nu at x e^(+-i pi/4) (the DLMF 10.27.8 J/Y form was valid only for
  -pi < arg <= pi/2). Cuts where x e^(+-i pi/4) is a negative real number.
- **engine B21-9** `fresnel_s/c` at a ball argument: `sqrt(pi)` next to a ball never became a number, so `fresnel_f`
  tried and failed for 8 s; pi is now evaluated at the ball's precision.
- **R20-17** `mc_numeric::elementary::{TailSafe, tail_safe}`: inside the scope `exp` of a ball far below zero is
  `0 +- 2^-(2^24 + p)`; generic evaluation still gets `None`. The quadrature crate enters the scope per worker thread.
- Gaps: `hypergeometric_pfq` with p = q + 1 and |z| >= 1 (3F2 beyond the unit disc), `product_log`, `jacobi_sn/cn/dn`,
  `elliptic_pi`, `inverse_jacobi_sn`, `inverse_erf` at complex arguments and 30 digits; Li_s for 0.9 <= |z| <= 1.1
  and non-integer s with |z| > 1; the 2F1 limit above is not rigorous.

## Round 22: the Meijer G-function (`meijer.rs`, `meijer_num.rs`)

`meijer_g([[a_1..a_n], [a_(n+1)..a_p]], [[b_1..b_m], [b_(m+1)..b_q]], z)` and `meijer_g(as, bs, z, r)` =
G(z^(1/r)) (with log z^(1/r) = (log z)/r). One formula `Def` (not listable: the parameters are lists; a list z
threads, matrices too).

- **Numbers** (`meijer_num.rs`, complex balls): Slater's theorem (DLMF 16.17.2; Luke 1969, 5.2) on the side of
  |z| = 1 where it converges: the b-sum for p < q and for p = q, |z| < 1; for p > q and p = q, |z| > 1 the b-sum
  of G^{n,m}_{q,p}(1/z | 1 - b; 1 - a) (DLMF 16.19.1) with the powers taken from -log z, so the identity also holds
  on the negative real axis. Working precision doubled while the result has fewer than p bits (cancellation
  between the k-terms, e.g. 2 K_0(2 sqrt z)), up to 16 p.
- **Coincident poles** (b_k - b_l integers, k, l <= m, or a regularized series 1 + b_k - b_l in Z<=0, l > m):
  b_k -> b_k + (k + 1) h, h = 2^-H, H = p + 40, the working precision (m + 1)(p + 40) from the start; the sums at
  h and 2h, the radius widened by twice their difference (an estimate, not a bound: the one non-rigorous step,
  O(h) = 2^-(p+40) against the 2^-p asked). Arguments with radii: the sums at the midpoints and at the midpoints
  moved by the radii (the radii would otherwise be multiplied by the 1/h terms). Checked: the perturbed value of
  G^{3,1}_{1,3}(-1/2 | i/3; 1/2, 1, 3/2) agrees to 1e-28 with the plain Slater sum at b_3 = 3/2 + 2^-90 (300 bits).
- **pFq**: an own series with the ratio bound of `ball::cseries::pfq_sum` but term magnitudes from the exponents
  (`cseries::pfq_sum` compares against doubles: beyond about 1000 bits its stopping test never fires and it runs
  2^20 terms; see Requests).
- **Exact** (`meijer.rs`): order reduction (PBM 3, 8.2.2.7-8); z = 0; named functions valid for all parameters
  (G^{1,0}_{0,1} = z^b e^-z, G^{1,1}_{1,1}, G^{1,0}_{0,2} = Bessel J (sin, cos at order +-1/2), G^{2,0}_{0,2} =
  2 K, G^{2,1}_{1,2} = Tricomi U, G^{2,0}_{1,2} = U, and z^b2 gamma(b1 - b2, z) when a = b2 + 1); p > q through
  1/z; Slater's sum with simple poles (p < q, or p = q with the singular point at -1; with the singular point at
  +1 a `piecewise` on |z| < 1 and |z| > 1), each pFq named where possible (exp, power, Bessel J/I, sin/cos/sinh/
  cosh, erf/erfi for 1F1(1/2; 3/2), e^(w/2) I_0 for 1F1(1/2; 1), elliptic K and E). With numbers throughout only
  named results are returned (a sum of general pFq is no simpler and `n` evaluates G directly).
- Derivative (row in mc-calculus `rules.rs`, helper `transforms::meijer_dz`): z G' = G^{m,n+1}_{p+1,q+1}(z | 0, a;
  b, 1).

Tests: `tests/meijer.rs` (the precision contract for 12 values: p < q, p > q, p = q on both sides, coincident
poles, negative and complex arguments, complex parameters, the generalized function; named reductions; values
against Bessel K, powers and exponentials at 40 digits; machine values; threading; the derivative rule by a
central difference); unit tests in `meijer_num.rs`.

`meijer_g([[im/3], []], [[1/2, 1, 3/2], []], -0.5)` is 0.431448476688395 - 0.0137706734599109 i at machine
precision (the 30-digit value 0.4314484766883949807 - 0.0137706734599109370 i, two methods agreeing to 1e-28, see
above).

Gaps: p = q at |z| = 1; large |z| for p < q beyond the cancellation the precision cap absorbs (no asymptotic
expansion); where the two pole families meet (a_l - b_k a positive integer, l <= n, k <= m) the value is the
limit of the perturbed function, which exists when a zero 1/gamma factor cancels the coincidence, and
none when it diverges; confluent exact cases without a named form stay unevaluated (no logarithmic Slater sums);
`around` arguments go through the generic propagation (the radius is not tight); function_domain and similar
properties of the reduced forms.

## Round 22 fix-up

- `ball/complex.rs::log_gamma`: the shift sum of N = p/2 principal logarithms (one complex logarithm per term,
  seconds at 1000 digits) is one logarithm of the product plus 2 pi i k, k from the machine sum of the factors'
  principal arguments (used off the real axis, |Im z| > 1e-6, where no factor is near the cut; the sum of
  logarithms stays for the rest). `gamma` of a complex argument at 1200 digits 4.2 s -> 0.5 s, at 30 digits 4x
  faster; the loops poll the stop flag. Checked by the reflection formula and log_gamma(z + 1) - log_gamma(z) - log z
  = 0 to 790 digits at five points on and off the branch.
- Bessel J, I, Y, K of a complex-ball order above 2^40 are not enclosed (`bessel_j(2^53 + 1, 0.5)` spent 9 s in
  guards and series and then gave up; now it gives up at once).
- Integer-order ball Bessel J and Y (`ball/bessel_j.rs`) decline orders above 2^18: the series builds n!, so
  `n(bessel_j(2^53 + 1, 0.5))` (and any sum holding it, as in the mc-jit library test) ran for minutes.

## Round 24 (bug fixing: hypergeometric U, Whittaker, Meijer G, huge parameters)

Literature: DLMF 13.2.42 (Tricomi's U from two 1F1), 13.2.9 (the logarithmic case, here by the mean over b = n +- eps),
15.4.6 (2F1(a, b; b; z) = (1 - z)^-a), 16.17 and 16.19 (Slater sums, inversion), Luke 5.2.

- **B23-2, X23-7, B23-23 (U part)** `cover.rs`. Three causes in `tricomi`/`hypergeometric_u`: (1) eps was added to b at the
  precision tag p (b + eps rounded back to b: the "limit" was the unperturbed formula at a pole), now eps = 2^-(p/2 + 32)
  carried at the working precision w (error eps^2, cancellation p/2 + 32 bits, covered by the extra 2 p0 + 16); (2)
  the `gamma_pole` test goes through f64, so a1 = a - b + 1 = -2 - 2^-58 counted as the pole -2 and 1/Gamma(a1) was set
  to 0: `exact_gamma_pole` tests the exact ball or rational; (3) arguments with a radius (a 133-bit z) multiply the
  radius by the 1/eps^2 terms: the formula is evaluated at the midpoints and at the midpoints moved by the radii and
  the value widened by twice the difference (an estimate, as for meijer_g). A result with fewer than p - 4 correct
  bits is not returned. The same code now serves complex arguments and negative real z (B23-18): principal power of z
  (the value above the cut), complex parameters, integer b by the same mean. Checked against the integral
  U(1, 3, 3) = 1/z + 1/z^2 = 4/9, U(2, 5, 3) = 1/3, the 30-digit values at 100 digits.
- **B23-1 / meijer_g exact** `meijer.rs::reduce`: p = q with (-1)^(p-m-n) = -1 and m + n < p (the singular point is -1, the
  sum for |z| < 1 is empty when m = 0) took the |z| < 1 Slater sum for every z; the function changes sides of the
  unit circle (contour closed to the right inside, to the left outside), so it now takes the two-sided path.
- **B23-6, D23-10 (meijer_num.rs)**: `accuracy` squared the f64 parts, so a ball of value 1e276 (noise of terms near e^|z|)
  overflowed to an infinite "accuracy" and was accepted (the -inf, inf and 2.5e276 results); it now works from the
  exponents. The precision loop stopped when the relative accuracy did not improve; a ball dominated by noise has
  a falling radius and constant relative accuracy, so progress is judged by the radius, the next precision is
  p + 8 - acc bits further when acc >= 0 and double otherwise. The starting precision includes the largest
  term of the Slater series (`cancellation_bits`, machine arithmetic, `cseries::max_term_log2`) and a value that
  needs more than the cap (p + 16384 bits, |z| up to 11000 for 1F1-like sums, 3e7 for 0F1-like) is declined
  at once; two misses in a row stop the doubling (each doubling repeated the complex gamma functions at larger
  precision: 22 s for z = 1e30). G(1000) now takes 0.1 s. Beyond the cap the call stays unevaluated (no
  asymptotic expansion: R22-05); machine inputs fall through to the named closed forms (exp(-z) underflows to 0.0)
  but an `indeterminate`, infinite or NaN result is not an answer and stays unevaluated.
- **B23-11, R22-01 (hangs)**: a non-terminating series with p <= q needs about |z|^(1/(q + 1 - p)) terms; beyond
  the term limit all three series (`machine::hyper::series`, `ball::hyper::pfq_sum`, `ball::cseries::pfq_sum`,
  `meijer_num::pfq_sum`) answer None at once. A lower parameter with non-negative real part gives |k + b| >= k + b_lo from
  k = 0, so the ratio bound holds from any k (`ratio_bound` takes the lower bounds): a lower parameter of 1e308
  settles after a few terms (the old bound needed k > max b: never). `up_f` clamps to f64::MAX (1.8e308 * (1 + 1e-12)
  overflowed). R22-01 (the complex series not stopping above about 1000 bits) is not reproducible any more:
  `n(hypergeometric_1f1(1 + im, 2, 1/3), 4000)` runs in 0.5 s (the stopping test uses Mag exponents); test added.
- **B23-12, D23-03**: 2F1(a, b; b; z) = (1 - z)^-a in the exact rules, the machine and the ball 2F1; 2F1(1, 1; 2; z) =
  -log(1 - z)/z; 0F1(; 1/2; w) and 0F1(; 3/2; w) for symbolic w as cosh/sinh of 2 sqrt(w), with the even square
  root c u^2 -> sqrt(c) u (cos(x), sin(x)/x for w = -x^2/4); `meijer_g` G^{1,0}_{0,2} at nu = -1/2 likewise.
- **B23-28**: 1F1(0; b; z) = 1 also for b = 0 (before a = b gave exp(z) at 30 digits and 1 at machine precision).
- Not done: D23-02 (3F2 at z = -1 needs a convergence acceleration with a rigorous tail: deferred), D23-07
  (`bessel_j(a, sqrt(x^2))` needs an assumption on x), D23-28 (not a bug: a_1 - b_1 = 1 pinches the contour; the
  Slater coefficient Gamma(0) is the divergent limit, the two straight-line contours differ by the residue at s = 0).
- Gap: general pFq with complex parameters (`hypergeometric_pfq([1 + im, 1/2], [2, 3], z)`) is not dispatched to
  `cseries::hyp_pfq` (only 0F1, 1F1, 2F1 are).

## Round 24

- **Machine orthogonal polynomials (X23-1..3, X23-5, D23-24)** `more.rs`: at a machine argument and degree >= 9 the
  classical three-term recurrences replace Horner on the exact coefficients (Jacobi DLMF 18.9.2 and its special cases,
  Laguerre 18.9.13, Hermite 18.9.3, Legendre/Chebyshev 18.9.1; associated Legendre by the derivative recurrence with
  (1-x^2)^(m/2) divided out, DLMF 14.10.3; Q_n forward on |x| < 1). Gautschi, Orthogonal Polynomials (2004) 1.3 for
  stability. Hermite beyond the machine range returns a signed infinity; degree or order above 1e9 stays unevaluated.
  `spherical_harmonic_y` uses the same derivative recurrence.
- **Kelvin (B23-3, D23-26)** `cover.rs`: ber/bei of non-integer order are the series DLMF 10.61.5, i.e.
  J_nu(z e^(3 pi i/4)) times e^(2 pi i nu k) with k fixed by arg z (the principal power of the rotated argument is off
  by e^(-2 pi i nu) beyond arg z = pi/4); results are evaluated numerically at the precision of the arguments
  (no more `re(... pi ...)`); ker/kei of a machine x > 1100 underflow to 0.
- **Machine complex Bessel K/Y, polygamma (B23-4, B23-5, B23-22)** `machine/complex.rs`: the series forms return
  `None` when more than 2 to 5 digits cancel; the natives then fall back to the 192-bit enclosure (which now honours
  the stop flag, and is not tried for arguments above 1e12).
- **Hankel (B23-4, B23-23)**: J +- iY at machine arguments when less than 4 digits cancel, else (and for balls) in
  balls at p + extra bits, midpoint evaluation with the radius carried by |H'| r (DLMF 10.6.2); doubled until the
  result has its digits. Not a proof (first-order radius), recorded as such.
- **Polylog (B23-10, B23-20)**: integer s <= 0 exact (Eulerian polynomial, any z) and in balls; non-integer real s on
  complex balls for |log z| <= 4.5 by Li_s(e^mu) = Gamma(1-s)(-mu)^(s-1) + sum zeta(s-k) mu^k/k! (DLMF 25.12.12,
  rigorous tail as the real case); real z > 1 is the limit from below. Gaps: |log z| > 4.5 (Hurwitz inversion),
  integer s in 0.9 <= |z| <= 1.1.
- **product_log (D23-11, B23-14)**: branch index may be an integer-valued float; start value for k = 0 near z = -1;
  ball precision by Halley iteration (cubic) in balls at p + 32 bits with the square of the last correction added
  (an enclosure, not a proof).
- **Jacobi (B23-25, D22/D23-22, D23-24, B23-23)**: complete K and E of a real ball below 1 use R_F (the 2F1 connection
  formula at c - a - b = 0 grew the radius of a ball argument with the working precision: `n(jacobi_sn(10+im, 7/10),
  30)` 7.5 s -> 80 ms); u reduced modulo 2K with K at the bits consumed (real and complex balls); machine |u| > 1e12
  goes to the 192-bit real ball function; ratios (cd, sc, ...) at the midpoints of ball arguments with relative radius
  2^-(p-8).

## Round 25

- **B23-19 (`meijer.rs::coefficient`)**: a Slater coefficient with an exact non-positive integer in a *denominator*
  gamma (a_j - b_k = -2 in G^{3,0}_{3,3}) is 0, not `failure(pole)`; with a numerator gamma at a pole too the quotient
  is a limit and stays as it was. The three examples of the bug now give 0.32763384708414415, 0.1845955120716484
  and 31.703967617780588 (independent evaluations: ...434, ...804, ...061).
- **D23-26 (`cover.rs::kelvin`)**: `kelvin_ber(nu, 0)` = 0 for exact nu > 0, `kelvin_ker(0, 0)` = inf, `kelvin_kei(0, 0)` =
  -pi/4 (DLMF 10.61.5, 10.65.9); a machine argument whose value overflows (`kelvin_ber(1e5)`, e^70710, sign
  unknown) stays unevaluated instead of `indeterminate`. The derivatives in x are in mc-calculus (`rules.rs`).
- **B23-23, X23-6**: both were already fixed by round 24 (the two ignored tests were stale); un-ignored. The comparison
  in the B23-23 test uses `re` and `im_part` of the difference: `abs` of a complex ball that straddles 0 stays
  `ball(...)^(1/2)` because mc-numtheory's native `abs` squares the parts with ordinary arithmetic.
- **`hypergeometric_u` at large z (`u_asymptotic`)**: `hermite_h(2.5, 100.)` = 2^2.5 U(-1.25, 1/2, 1e4) did not return
  (the connection formula needs e^z: 14000 guard bits) and gave `indeterminate` on the machine path. For z in the right
  half plane with |z| >= 0.7 p + 4(|a| + |b|) the series `z^-a sum (a)_k (a-b+1)_k/k! (-z)^-k` (DLMF 13.7.3) is summed until
  a term is below 2^-(p+6), the first omitted term doubled is the radius (estimate).
- Not done: D23-02 (3F2 at z = -1: a convergence acceleration with a rigorous tail), D23-07 (`bessel_j(a, sqrt(x^2))`
  needs an assumption on x), R22-05, R22-11 (as in round 24), B21-8 (3F2 beyond the unit disc).


## Round 27: wrong values of round 26, special functions and numerics

Literature: DLMF 18.9.2, 18.9.7, 18.9.13 (three-term recurrences), 8.11.2 and 10.40.2 (large-argument expansions of
Gamma(a, z) and K_nu), 11.6.1 (Struve minus Bessel Y), 12.9.1 (parabolic cylinder D), 13.7.3 (U), 10.67.1 (Kelvin), 10.11.2
(Y on the negative axis), 15.8.2 (2F1 beyond the disc), 25.2.9 and 25.2.12 (Euler-Maclaurin for zeta); Holmes and
Featherstone, J. Geodesy 76 (2002) 279 (normalised associated Legendre recurrence); Gautschi, Orthogonal Polynomials (2004).

- **B26-14, B26-15, B26-16, X26-7 (`more.rs`)**: with exact rational arguments `laguerre_l`, `gegenbauer_c` and `jacobi_p` now
  run the three-term recurrence in *rational arithmetic* up to degree 3000 (`exact_recurrence`): no cancellation, so
  `n` of the exact value is correctly rounded at any precision (the 30-digit value was a ball of radius 1e179 and the
  machine value 0.0 or 1e125). Machine arguments (and an integer-valued machine degree, which is what `n` makes of an
  exact degree) use the recurrence in doubles with the pair rescaled near the range limit (`machine_recurrence`, shared
  by Jacobi, generalised Laguerre and Gegenbauer). The gamma-ratio form of the 2F1 overflowed from degree 250. The
  machine value carries the rounding of the argument (1e-14 at degree 200 against the exact value).
- **X26-8**: `spherical_harmonic_y` at a machine angle uses the fully normalised recurrence (`normalized_legendre_machine`),
  every quantity O(1); (2m - 1)!! overflowed from m = 150, the start is carried with a power of 1e200 when sin^m underflows.
- **B26-19 (gegenbauer part)**: `gegenbauer_c(n, -1/2 - m, x)` for non-integer n: gamma(2a) and the lower parameter a + 1/2 = -m are
  both poles; the limit is -2 (-1)^m (2m + 1)!/m! times the regularized 2F1. Deferred: the `meijer_g` part (a_1 - b_1 = 2 pinches
  the contour; G of the parameters moved by +-h is +-2.5e8 i at h = 1e-8, so there is no limit, and no convention for the
  value is fixed yet). Test `b26_19_meijer_pinched_contour` stays ignored.
- **X26-6 (`meijer.rs`, mc-numeric `formula.rs`)**: G^{3,0}_{0,3}(z | 0, 1/3, 2/3) at z >= 1e4 summed terms of e^(3 z^(1/3)) that cancel to
  e^(-3 z^(1/3)), with the exact parameters turned into balls (or machine numbers: `n` of the lists) of radius 2^-117. Now the
  parameter lists of exact numbers stay exact (`numeric_arguments`, and in `numericize` for `meijer_g`) and `numeric()` builds
  the parameter balls at p + 128 + twice the largest term (`cancellation_bits`). z = 1e6 gives 1.8675604580722739e-130 in 1 ms.
- **B26-20 (`machine/complex.rs`)**: `gamma_upper_large`, the series of DLMF 8.11.2 for |z| >= 40 off the negative axis, with
  e^-z and z^(a-1) formed separately (the phase of e^-z is exact in doubles). Also `exp_integral_e` beyond |z| = 700 (continued
  fraction). The old formula through 1F1 lost everything at 3e4 i (at 1e4 i the old 1F1 route agrees with the new one to 14
  digits).
- **B26-21**: machine zeta for Re s >= -1/2 and |s| >= 30 is Euler-Maclaurin with the phases t log k in double-double
  (`zeta_em_large`, |s| terms); in plain doubles the phase of 1e4 i log k is wrong in its eleventh digit (checked against
  25 digits at exact binary arguments: 1e-15). The complex ball zeta used 1.5 tau extra bits per term (150000 bits at tau = 1e5):
  now 2 log2(tau) + 40, and k^-s is multiplicative (one exponential per prime): `n(zeta(1/2 + 100000 im), 30)` 1.9 s.
- **B26-22** (the seven inputs of the test): `bessel_k` complex Hankel expansion (`bessel_k_large`), `bessel_y` of a negative real
  argument through DLMF 10.11.2, `struve_h` at x >= 40 + nu^2 as Y_nu plus the asymptotic series, `parabolic_cylinder_d`
  at x >= 30 by DLMF 12.9.1 (the exponential underflows to 0.0 as the value does), `hypergeometric_u` at Re z < -0.7 (p + 10) by
  13.7.3 (the e^z terms are below 2^-p there), `kelvin_ber` and `kelvin_bei` of a machine x > 1100 overflow to an infinity with
  the sign of cos or sin of alpha = x/sqrt 2 + (nu/2 - 1/8) pi in double-double (unevaluated when the sign is within 1e-3 of 0),
  `meijer_g` complex series: see the next item.
- **`meijer_num::pfq_sum`**: a complex ball multiplied k times by a ratio of modulus 0.84 has a radius growing like 1.19^k (the
  rectangle bound of a complex product is up to sqrt 2 too wide), so a series at |w| = 0.84 exhausted its 2^20 terms (6.6 s for
  G at |z| = 1.19). Terms now carry the radius |t| rho with rho the sum of the relative radii of the factors and 2^(2 - wp) per
  step (first order, one bit of margin); 6.6 s -> 0.1 s. Not changed: `ball::cseries::pfq_sum` (real ratios there).
- **B26-17 (`cover.rs::kelvin`)**: a real Kelvin function computed as a complex ball has the imaginary part 0 +- 2^-p, which
  `n` cannot certify; the failure's best value is accepted and its real part returned.
- **B26-18 (`ball/cspecial.rs`)**: three causes. (1) `dist_to_int` required an exact integer, so the difference of two equal
  exact parameters (balls of radius 2^-116) was not seen as degenerate and Gamma(b - a) was a pole ball; it now tests whether
  the ball contains an integer. (2) Degenerate parameters that are balls are evaluated at their midpoints and widened by 16
  times the sum of the radii times the modulus (an estimate, like the Richardson limit). (3) The power (-z)^-a of a real negative
  base took the cut: the limit from below, arg = +pi (`pw`); the Richardson step widened by 2^-p of the modulus, which left a
  small imaginary part 8 bits short and made `refine` double to its cap (18 s): now 2^-(p + 8) of each part.
- **B26-26 (`more.rs`)**: `chebyshev_t` and `chebyshev_u` at a real x > 1 and a non-integer degree: cosh(n acosh x) and
  sinh((n + 1) acosh x)/sinh(acosh x) (real balls) instead of cos(n acos x) with an imaginary 0 +- 2^-p.
- **X26-16 (mc-arith `text.rs`)**: not the sum: `1 - 10.0~25^-10^8` is the ball 1 +- 3 10^-(10^8), correct to 10^8 digits, and
  printing asked for every digit (the exact m - r with a gap of 3.3e8 bits, 2.8 GB). `ball_digits` takes the radius no smaller than
  2^(top - p - 64). The time limit was honoured by the evaluation; the printing ran after it.
- Gaps: `n(f(exact), d)` of the polynomial families above degree 3000 or with non-rational parameters still goes through machine or
  2F1 balls (the ball recurrence with a spread estimate for the argument radius was not written); `zeta` at Re s < -1/2 and
  |s| > 1e3 still uses the functional equation in plain doubles for the phases (about 1e-11 at 1e4 i); `hypergeometric_u`
  near the imaginary axis at large |z| still uses the connection formula.

## Round 28: coverage functions

Modules `cover28.rs` (Scorer, modified Lommel, Carlson R_F/R_K, Coulomb) and `weierstrass.rs` (p, p', zeta, sigma, inverse p),
on the working form of `cover.rs` (its helpers are now `pub(crate)`). Literature in the module docs (DLMF 9.12, 11.9, 19.36,
33.2, 23.2, 23.6, 20.2; Carlson 1995 for the duplication algorithm). Tests: `tests/round28.rs` (values, special values, and the
precision contract for every function: 30 digits agree with 60 and the 30-digit ball contains the 60-digit value; the machine
value agrees with `n(exact, 40)` to 4e-15).

- **Scorer**: the Maclaurin series of Hi and Hi' with the three-term step a_(k+3) = a_k z^3/((k+2)(k+3)), Gi = Bi - Hi. The
  working precision adds (4/3)|z|^(3/2) log2 e bits (Bi, Hi ~ exp(2/3 z^(3/2)) while Gi ~ 1/(pi z)).
- **lommel_t1/t2**: t is the Lommel s series with +z^2; T was derived from DLMF 11.9.5 and the connection of K with I_(+-nu):
  T = -t + 2^(mu-1) Gamma((mu-nu+1)/2) Gamma((mu+nu+1)/2) (I_nu + (sin mu pi + sin nu pi)/pi K_nu), checked against the
  terminating expansion at mu = 2, nu = 5. At a gamma pole with a terminating expansion
  (mu = 2, nu = 5) the finite sum z^(mu-1) sum prod((mu-2i+1)^2 - nu^2) z^(-2k) is returned (the limit of the formula).
- **carlson_rf/rk**: duplication until the spread is below 2^(-w/8), then the 7th-order series of DLMF 19.36.1. A zero mean (R_F of
  the Weierstrass roots at p = 0) skips the convergence test for one step. R_K = (2/pi) R_F(0, x, y).
- **Coulomb**: F from 1F1 directly (so F ~ 1e-7 at eta = 7 keeps its digits, which F = (H+ - H-)/2i would lose), H+ from U, G =
  H+ - iF, H- = H+ - 2iF. 2l an integer keeps l exact so U takes its integer-b branch. Ball inputs: `n(ball, d)` with d above the
  ball's precision is a failure, so closed forms are numericized only when they are not already numbers (`nw`).
- **Weierstrass**: roots by Durand-Kerner then Newton on midpoints (last step on the balls so the radii of g2, g3 propagate); real
  invariants make the real roots exactly real (a complex ball straddling the axis cannot take sqrt off the cut). Half-periods
  K(m)/s and iK(1-m)/s from the labelling with the largest Im tau; K by an AGM loop here, because `elliptic_k` of a complex ball
  hung (`n(elliptic_k(1 - m), 30)` at m = 1.19i/(1.35 + 0.60i): gap, not investigated). The AGM stops one step after |a - b| <
  2^(-w/2)|a| (ball differences stop at their radii). theta_1 and three derivatives from one series; zeta and sigma carried back
  by quasi-periodicity (DLMF 23.2.20).
- **inverse_weierstrass_p**: u = +-R_F(p - e1, p - e2, p - e3), the sign with p'(u) = sqrt(4p^3 - g2 p - g3) (principal), real
  arguments: a negative real u plus the real period. Off the real axis the result is correct up to a period; reduction into one
  fixed period parallelogram is not done (gap).
- Machine values against 40 or more digits of the exact argument: scorer_gi(5.0) is 0.06491978409385311 (the value
  0.064919784093853112); lommel_t1(7/3 + i, 2 i, 4.5 - i) has real part 20.986265403374414 (20.98626540337441336 at 40
  digits); weierstrass_p(12.0, [3, 2]) is 77.23115876119488 (77.231158761194871 at 60 digits); weierstrass_p(1.5, [i, 2 - i])
  has real part 0.8267070837386243 (0.82670708373862431 at 30 digits).
- **elliptic_log/elliptic_exp** (`weierstrass.rs`): the curve y^2 = x^3 + a x^2 + b x shifted by a/3 is the Weierstrass curve
  of g2 = 4a^2/3 - 4b, g3 = 4ab/3 - 8a^3/27 with y = p'/2, so elliptic_log = inverse_weierstrass_p([x + a/3, 2y], g) (the
  sign from p'(u) = 2y, no period shift) and elliptic_exp = [p(u) - a/3, p'(u)/2]. The result is one representative modulo
  the lattice (x = 0.3 on y^2 = x^3 - 5x^2 + x: another choice differs by the real period).
- **Heun functions** (`heun.rs`): one engine for A y'' + B y' + C y = 0 with polynomial coefficients (the five Heun equations
  written polynomially; the heun_d form has epsilon z^2 + delta z + gamma),
  Frobenius series at a regular singular start, Taylor re-expansion along the segment (steps at most half the distance to
  the nearest singularity and 1/2), detours above singularities within 0.3 of the segment. Ball continuation suffers the
  wrapping effect (radius 3e189 at 30 digits for heun_d near its irregular singularity 0), so the continuation runs on
  midpoints; the radius is twice the difference between w and w + 48 bits plus first-order effects of argument radii (one
  perturbed run per argument with a radius). An estimate, not a proof (like `refined_ratio`). On the cut z > 1 of heun_c the
  convention is open (the one-sided limits at z = 4 are 471.1 +- 213.7i): gap.
- Not found: `plot([weierstrass_p(x, [1, 2]), 2], [x, -6, 6])` overflows the stack (exit 0xC0000409) while the same plot
  with one curve, and the values at the sample points, evaluate (a crash in the plotting path; to investigate in mc-plot).
- **Appell F1-F4** (`appell.rs`): rows of one variable times a Gauss function of the other (the smaller modulus outside), so F1
  and F3 work for one variable beyond the unit disk. The Gauss functions take the midpoint of y: `hypergeometric_2f1(2, 2,
  5, y)` at a ball y beyond the unit disk (c - a - b an integer) has a real part whose radius grows with the precision of y
  (2^-66 at 139 bits, 2^102 at 811 bits): a bug of the ball 2F1 found here, not fixed (gap).
  appell_f4(3, 2, 1, 2, 1/5, 1/7) is 15.5438068320793 (the direct double sum in rationals summed to 120 x 120
  terms: 15.543806832079271).
- **Zeros** (`zeros.rs`): sign changes counted in machine arithmetic (step 0.1 for the Bessel functions, 0.5/sqrt(1 + |x|)
  for Ai, a sixth of the mean zero spacing for Z), bisection, Newton at the working precision on midpoints with a last ball
  step. Non-integer k follows the phase form: bessel_j_zero(nu, m - t) is the m-th zero of J cos(pi t) - Y sin(pi t);
  for bessel_y_zero the first zero of Y cos(pi t) + J sin(pi t) enters the origin at t = 1/2, so t >= 1/2 counts from the
  second (checked at k = 2/3, 0.9, 1.1, 4/3, 5/3; k = 1/3 has no zero).
  bessel_y_zero(0, 1) is 0.89357696627916752158 at 20 digits, and the machine value agrees.
  Gap: zeta zeros are counted by sign changes of Z from the start height, not by Turing's method (a close pair would be
  missed; k = 6709 needs about 40000 evaluations of Z).
- **q_hypergeometric_pfq / bilateral_hypergeometric_pfq** (`qhyper.rs`): the basic series by its term ratio; the bilateral
  series as two pFq, the one outside its disk (p = q + 1, |z| > 1) by the connection formula DLMF 16.8.8 (written here, as
  `hypergeometric_pfq` does not continue 3F2 beyond |z| = 1: a gap of that function). Tested at z = 0.1, 0.3,
  0.5, 5 and 5.4. Gap: p != q (one series of zero radius, e.g. ([1/2, 1/3], [1/4, 1/5, 1/6], 1.0)) stays unevaluated.
- **Mathieu** (`mathieu.rs`): Taylor stepping with cos(2 z0 + 2t) expanded at each point; the even/odd functions are the
  Floquet solutions normalized by sum c_k^2 = 1 (the values are ce(0) w_I and se'(0) w_II with the IVP solutions
  w_I, w_II), the exponent branch +-nu + 2m nearest sqrt(a) (it fixes the sign of the odd
  function). Characteristic values: Sturm bisection of the truncated tridiagonal matrices, then secant at 64, 128, ... bits
  on y'(pi/2) or y(pi/2); non-integer r by the Floquet condition. mathieu_c(2, 1, 37/10) at 50
  digits is -0.70674734130376727046760504036431438649802416523000045 (agreeing with the 500-digit value);
  mathieu_c(2, 1, 3.2) at machine precision is -0.8995311634414612 (= 60 digits). Gaps: the normalization branch
  for complex a or q (complex nu) is not settled; mathieu_characteristic_a of complex q stays unevaluated; each value costs about
  0.1-0.5 s at machine precision (plots time out).

## Round 29: ball 2F1 fixes, machine fast paths, new functions

Tests: `tests/round29.rs` (the 2F1 radius and elliptic_k hang, the Appell retry, the precision contract of every fast path
against `n(exact, 40)`, speed of 400-point tables, lattice conventions, values of the new functions).

- **Ball 2F1 (item 1)**. Three bugs. (a) `appell` retried while its value lacked the goal's digits, but tested after its
  own fixed widening 2^-(p - 8): whenever that fell below the goal the loop went to 818 bits (6 s for
  `n(appell_f1(1, 1, 3/5, 3, 2/3, 2/5), 22)`, 0.16 s now); the sum's own digits decide. (b) The degenerate connection
  formulas (c - a - b or b - a an integer) shift a parameter by e = 2^-(p/4 + 8); the shifted functions are of size 1/e and
  cancel only at the midpoint, so the radius of a ball z came back times 2^(p/4 + 8) and grew with the precision asked
  for. A ball z is now evaluated at its midpoint and widened by 2 |F'(z)| rad(z), F' = (ab/c) 2F1(a+1, b+1; c+1; z) (first
  order, an estimate like the parameter case beside it). (c) `pfq_sum` multiplied rectangular complex balls term by
  term: each product grows the radii by (|Re q| + |Im q|)/|q| (the wrapping effect), 1.06 per term at w = 0.33 + 0.74 i,
  so the tail test never passed, the series ran to its 2^20-term limit and `refine` doubled: `elliptic_k` of a complex ball
  hung (0.15 s now). The term is kept as an exact midpoint with a rotation-invariant bound of its relative error
  (`polar_step`: rel(t q) <= rel(t) + rel(q) + the rounding, in log2 form).
- **Machine fast paths (item 2)**, `fast29.rs` on `cdd.rs` (complex double-double, ~32 digits; sqrt and log by one
  Newton step, sin/cos by the exact reduction of `dd`). When every argument is a machine or exact number and one is a
  machine number, the kernel runs natively and rounds once; each estimates its rounding error (largest term or step
  value times 2^-102 times a margin) and declines above 2^-56 of the value, outside its ranges or near ill-conditioned
  points, so the ball path keeps every value it had. Exact rationals enter as double-doubles (1/3 rounded to a double moved
  F2 by an ulp). Kernels: Scorer (the Maclaurin series of w'' = z w -+ 1/pi for |z| <= 9.5, the asymptotic series DLMF
  9.12.27-30 on the real axis where the smallest term is below 2^-56, i.e. |x| >= 15.2; Gi(-x) = Bi(-x) - Hi(-x));
  Weierstrass p, p', zeta, sigma (roots, AGM, theta series as in `weierstrass.rs`); inverse_weierstrass_p and Carlson
  R_F/R_K (duplication to a spread of 1e-5 and the 7th-order series); Mathieu (the Taylor stepping and the Floquet
  normalization, kept per (a, q) for plots in z; characteristic values by the secant method in double-double);
  Appell F1-F4 (rows times a double-double 2F1: the series or Pfaff's transformation, whichever argument is smaller;
  for F2 the outer variable is the one with the smaller |x|/|1 - y|, since its rows grow like (1 - y)^-m); Heun G, C, B,
  D, T (the recurrence and path of `heun.rs`); lommel_t1 (its series). Speed: 400 values of scorer_gi, weierstrass_p,
  mathieu_c, appell_f4, heun_b take 0.01-0.1 s (each value took 0.05-0.5 s). Not reached: Coulomb (needs U of complex
  parameters in double-double) and lommel_t2 (its Bessel part cancels against t; a machine I, K is not enough).
- **Weierstrass lattice functions** (`weierstrass.rs`): `weierstrass_half_periods`, `_half_period_w1/w2/w3`,
  `weierstrass_e1/e2/e3`, `weierstrass_eta1/eta2/eta3`, `weierstrass_invariants`, `_invariant_g2/g3`,
  `elliptic_exp_prime`. The half-periods are the reduced basis of the lattice (tau = omega3/omega1 with Re tau in
  (-1/2, 1/2], |tau| >= 1, omega1 with Re > 0 or Re = 0 and Im < 0; on |tau| = 1 the pair whose omega1 has the larger
  real part, then Re tau > 0, then the smaller Im omega1); real invariants with g3 < 0 take -i times the basis of
  [g2, -g3] (p(u; g2, -g3) = -p(iu; g2, g3)), reduced again in the turned frame. Tests fix these rules at 15
  invariants, including the boundary cases [2, 0], [-2, 0], [-3, 1], [0, -1], [1, 2i]. e_k = p(omega_k), eta_k = zeta(omega_k) with omega2 = -omega1 - omega3. Invariants from the half-periods by
  theta2^4, theta4^4 (DLMF 23.6.2-4). Closed forms: [a, 0] and [0, b] (a, b > 0, scaled lemniscatic and equianharmonic
  cases), the degenerate real lattice g2^3 = 27 g3^2, g3 > 0 (omega1 = pi/(2 sqrt(3c)), eta1 = (pi/2) sqrt(c/3), c =
  sqrt(g2/12)), half-periods with tau = i or e^(i pi/3). elliptic_exp_prime = [2y, 3p^2 - g2/4] for the point of
  elliptic_exp. weierstrass_invariants([1., 2. i]) is [8.124218443053017, 4.44305204494595]
  at machine precision, [8.12421844305301709, 4.44305204494594999] at 30 digits.
- **airy_bi_zero** (`zeros.rs`, as airy_ai_zero). airy_bi_zero(3) is -4.830737841662016 at machine precision
  and -4.83073784166201593 at 20 digits.
- **stieltjes_gamma** (`round29.rs`): Euler-Maclaurin summation of ln(x + a)^n/(x + a) in real balls, the derivatives as
  exact polynomials in ln t (P_(r+1) = -(1 + r) P_r + P_r'), N = (w + 64)/8 + 2n + 10 terms, the cancellation (n + 1)
  log2 ln N bits added; omitted corrections bounded by the last one. Real a > 0 only (complex a: gap). Checked:
  gamma_1, gamma_3, gamma_10, gamma_100 = -4.2534e17 and gamma_2(1/2) at 50 digits agree with the expected values.
- **siegel_theta** (`round29.rs`): the lattice sum around the largest term -Y^-1 Im s, over the box where the quadratic
  form reaches 2^-(w+16) (the smallest eigenvalue of Y by inverse iteration), the omitted terms bounded. Non-symmetric
  omega stays unevaluated. siegel_theta([[0.5 + i]], [2]) is 1.0000069746847124 +
  0.08642783652859559 i and elliptic_theta(3, 2 pi, e^(i pi (0.5 + i))) is ...59562 i (the two routes agree);
  siegel_theta([[i, 0], [0, i]], [1, 2]) is 1.1803405990160962 (= theta3(e^-pi)^2). Gap: about 6 ms a
  value (no double-double kernel), plots of it are slow.
- **spheroidal_eigenvalue, spheroidal_ps** (`round29.rs`): DLMF 30 conventions (equation 30.2.1, Legendre expansion
  30.8.1 with the recurrence 30.8.2-4, normalization 30.4.4). The eigenvalue is the root of Bouwkamp's continued
  fraction at k = 0 (downwards to k0 = -floor((n - m)/2), upwards to K = w/6 + 2|gamma| + n/4 + 12), started from the
  machine eigenvalue of the symmetrized tridiagonal matrix (real gamma^2) and refined by the secant method at 64, 128,
  ... bits on the midpoint of gamma^2. Ps from the coefficients and the Ferrers functions by their recurrence. Agrees
  with the expected values to 50 digits at (117, 5, 1), (11, 1, 21) and Ps(7, 5, 4, 0). Gaps: non-integer n or m and complex m
  stay unevaluated; Ps at |x| >= 1; about 5 ms a value. Not reached: spheroidal_s1, spheroidal_qs, fox_h, fox_h_reduce,
  meijer_g_reduce.

## Round 30: series of special functions, symbolic identities

### Series of special functions (`series30.rs`, `lib/special.mc`)

- **D30.1** `series` stays `mc-poly`'s native; this crate wraps it (`Kernel::wrap_native`, installed after
  `mc-poly`). When the native declines, or answers with unevaluated `derivative(...)` coefficients (its Taylor table
  differentiates through the evaluator and some derivatives do not evaluate at the point, e.g. theta functions at
  q = 0), the wrapper evaluates the library helper `ds_series(e, x, x0, n)`. The helper answers `ds_none` when it
  has nothing, and the call stays as before.
- **D30.2** The helper replaces every special function f(..., u, ...) of x (one argument depending on x, u0 its
  value at the point, or inf when u -> inf) by its truncated expansion with n + 3 terms and runs `series` again, so
  products, quotients and compositions with elementary functions come from the series kernels. Expansions at
  infinity with an exponential or oscillating factor (Airy, Bessel, gamma, factorial) come back as pieces
  `[[p1, s1], ...]`; they are used only when the expression is a constant multiple of the one function, and the
  result is `c (p1 series(s1) + ...)`, the factor kept outside.
- **D30.3** `log(x)` at infinity and `log(-x)` at 0 are passed to the kernels as a placeholder constant and put
  back afterwards: the kernels would print them as `-log(x^-1)` and `log(x) + i pi` (the latter is wrong for x < 0,
  which is where Ei's expansion uses it).
- **D30.4** The library's sums are folds over lambdas, never `sum(..., [j, ...])`: a global `j = 5` must not reach
  them (it did in the first version: `sum(0, [5, 0, 6])`).
- Expansions and sources: Si, Shi, Ci, Chi, Ei, E_n near 0 (A&S 5.1.10-12, 5.2.14-16; DLMF 6.6, 8.19.8); li near 1
  as Ei(log u); J_n, I_n, Y_n, K_n of integer order near 0 (DLMF 10.2.2, 10.8.1, 10.25.2, 10.31.1); ker, kei
  (DLMF 10.65.1-3); H_n, L_n (DLMF 11.2.1-2); K(m), E(m) (DLMF 19.5.1-2); the nome through Jacobi's series in
  e = (1 - (1 - m)^(1/4))/(2 (1 + (1 - m)^(1/4))) with the coefficients 1, 2, 15, 150, 1707, ... (DLMF 19.5.5,
  OEIS A002103); theta functions in q (DLMF 20.2.1-4); log_gamma, digamma, polygamma at 0, -1, ... by the
  recurrence (DLMF 5.5.2, 5.15.5); Li_s, W near 0 (DLMF 25.12.10, 4.13.5); Weierstrass p, p', zeta, sigma from the
  Laurent coefficients c_k (DLMF 23.9.2-3); reciprocal Jacobi functions as quotients of sn, cn, dn; the regular
  Coulomb function near 0 (DLMF 33.6.1); at infinity Stirling's series (DLMF 5.11.1-2), digamma/polygamma
  (DLMF 5.15.8), harmonic numbers, U(a, b, x) (DLMF 13.7.3), Airy functions and derivatives (DLMF 9.7.5-8), Hankel's
  expansions of J, Y, I, K (DLMF 10.17.3-4, 10.40.1-2), McMahon's expansions of Bessel zeros (DLMF 10.21.19) and
  the Airy zeros (DLMF 9.9.6, 9.9.18).
- Checked: each expansion against the function's value near the point (tests/round30.rs, `chk`), e.g. Weierstrass
  p at 0.1 to 1e-10, the nome at 0.1 to 1e-11, log_gamma at 10 to 1e-9, McMahon at k = 10 to 1e-12.
- Gaps: Lommel functions near 0 (the kernels absorb a symbolic power x^(m+1) into the coefficients and print a
  wrong order term; left unevaluated), Barnes G, hyperfactorial, log_integral at 0 and infinity (expansions in
  1/log x), Mathieu characteristic values at infinity, inverse_erfc at 0, the bilateral pFq at infinity.
- Found in `mc-poly` (reported): `series(x^(-1/4), x, inf, 0)` is `O(x^-1)` (a fractional leading power between
  the orders is dropped; the wrapper asks for at least order 1 in the pieces); `series(log(-x), x, 0, n)` is
  `log(x) + i pi` also under `x < 0`; a Taylor table entry with unevaluated derivatives is returned as a value.

### Symbolic identities and special values (`lib/special.mc`, second part)

- **D30.5** Library rules on the special-function natives (tried only when a native declines): zeros of Airy,
  Bessel and zeta at their own zero functions; Anger/Weber at 0; Whittaker M, W at 0 (by the leading powers, DLMF
  13.14.14-21, W_{k,1/2}(0) = 1/gamma(1 - k) and 0 when 1 - k is a pole); parity of integer-order Bessel and Kelvin
  functions, of F, E, Pi in phi and of am, epsilon, Z and the odd inverse Jacobi functions; inverse Jacobi
  functions at m = 0, 1, inf (circular, hyperbolic, 0); the K and 2 i K' shifts of Z and epsilon (DLMF 22.16);
  Neville thetas at m = 1; theta_1(n pi) = 0; lambda(i) = 1/2, q(1/2) = e^-pi and its inverse; Weierstrass p, zeta,
  sigma at the half periods and the quasi-periods; elliptic_exp on the degenerate curve [0, 0]; Chebyshev,
  Gegenbauer, Hermite, Legendre at 0 and 1 and for fractional degree; Lerch at s = 0, a = 0, 1, 1/2; Li_s(-1),
  Li_3(1/2); Heun G and C cases that are 2F1 and 1F1; 2F1 at 1 (Gauss), with c a non-positive integer (complex
  infinity), K(z), atanh; 1F1 by Kummer's transformation when a - b is a positive integer, I_0, erf; U(1, b, x),
  U(1/2, 1, x); terminating pFq with an upper parameter -N (N <= 60).
- **D30.6** Complex coefficients in patterns are written as literals (`2im*elliptic_k(1 - m_)`, `1im`): `im` in a
  pattern is the symbol, which never matches the number in an evaluated term.
- Checked: every identity with a numerical counterpart against the function's value (tests/round30.rs,
  `identities_agree_with_values`, all within 1e-12); the epsilon shift by i K' against the expected numerical
  value (3.4928761545259 + 0.6511397147591 i at u = 0.3, m = 0.4).
- n(e3([11, 7])) gives -1.0, exact: e3 = -1 since 4t^3 - 11t - 7 = (t + 1)(4t^2 - 4t - 7).

### Machine values the natives left (`lib/special.mc`, third part; `cover.rs`)

- **D30.8** jacobi_amplitude: K(m) is now taken numerically (an exact m such as 2/3 left `elliptic_k(2/3)` and the
  call unevaluated: `jacobi_amplitude(4., 2/3)`); complex arguments by am = -i log(cn + i sn) + 2 pi round(re(u/4K)).
- **D30.9** Library rules for machine arguments: Ferrers Q off the integers from P (DLMF 14.9.10); the two-endpoint
  inverse_erf, inverse_gamma_regularized, inverse_beta_regularized by solving for the second endpoint;
  inverse_jacobi_sn at complex arguments by v R_F(1 - v^2, 1 - m v^2, 1) (DLMF 19.25.5); factorial_power through
  gamma; pFq and regularized pFq (lower parameters at poles: 1/gamma = 0, the sum starts past them) summed term by
  term when convergent (p <= q, or p = q + 1 with |z| < 0.9), at most 600 terms. All agree with the expected digits
  to 1e-13 relative (tests/round30.rs).
- Gaps: elliptic_pi at complex m (carlson_rj has no complex machine path), polygamma and polylog of complex order
  outside |z| < 1, lerch_phi with |z| > 1, legendre_q with complex degree and order (legendre_p has none),
  mittag_leffler_e with negative alpha, zernike_r of negative degree.

### Coulomb machine fast path (`fast30.rs`)

- **D30.10** F, G, H+, H- at machine reals l >= 0, eta, rho > 0 by Steed's method (Barnett et al., CPC 8 (1974)
  377; DLMF 33.8) in double-double: CF1 (modified Lentz) for F'/F, the complex CF2 for H+'/H+, the Wronskian for
  the normalization, the sign of F from a downward recurrence. Inside the turning point q ~ 1/G^2 comes from a
  cancellation; past |G| = 1e6 (and when a fraction does not converge in 2e5 / 1e5 steps) the ball path is used.
  Against the 30-digit ball path at 11 points (inside and outside the turning point): within 4e-16 relative.
  200 values of coulomb_g(0, 0.3, r) now take about 2 ms of evaluation (were about 6 ms each).
- Not reached: complex rho (complex plots of coulomb_f, coulomb_h1/h2 still use the ball path), Lommel T2,
  Siegel theta and spheroidal fast paths.

## Round 31

- `spherical_bessel_j(k, x)` at a machine real with 0 < k <= 60 and |x| < k: the power series
  x^k/(2k+1)!! sum (-x^2/2)^m / (m! (2k+3)...(2k+2m+1)) (Abramowitz and Stegun 10.1.2) instead of the upward
  recurrence, which grows like (k/x)^k in error (`spherical_bessel_j(20, 0.0703)` was -1.49e30, now 6.6e-49).
  Checked against the exact-argument form at 40 digits (tests/round31.rs). Larger |x| keeps the recurrence (stable).
- `activate(inactive(meijer_g)(..., c u^2-type argument, 1/2))` for the J case G^{1,0}_{0,2}: the root of the argument
  is taken by `even_sqrt`, extended to products of even powers (a^2 x^2/4 gives a x/2), so the result is
  `bessel_j(1, a*x)` (the x > 0 convention of the Mellin representations) and not `bessel_j(1, (a^2 x^2)^(1/2))`.
- Not reached: Coulomb functions at complex rho on a machine fast path (needs a complex double-double
  gamma and Tricomi U kernel; complex plots of coulomb_f and coulomb_h1/h2 still use the ball path); the library `hypergeometric_pfq`
  complex-contour plots.

## Round 32

- **Crashes of the round-31 run.** `series(kelvin_ber(x), x, 0, 15)` and `kelvin_bei`: the derivative table of the
  native grows like 2^order (each derivative of a Kelvin function is two terms of the neighbouring orders), so the
  wrapper in `series30.rs` answers a ber or bei of the series variable at 0 from the closed series first
  (`lib/special.mc`, DLMF 10.61.1-2; 4 terms of x^4 at 15 in 0.4 s, were a 5 s hang). `kelvin_ber(-n, 0)` and
  `kelvin_bei(-n, 0)` are 0 (J_(-n) = (-1)^n J_n). `mathieu_characteristic_a`: beyond |q| = 1e5 the Taylor solver and
  the ball path spiralled (allocation of 3e16 bytes at q = 1e30, a tridiagonal matrix of 4 sqrt|q| rows); machine
  arguments at an integer order take the large-parameter expansion (DLMF 28.8.1 through h^-5, h = sqrt|q|, s = 2r + 1,
  s = 2r - 1 for odd r at q < 0; valid for h > 32 s^2, checked against the tridiagonal eigenvalue to 1e-12), also as
  the fallback when the double-double secant method finds no root; all other large-|q| calls stay unevaluated.
  `struve_h/l`, `anger_j`, `weber_e`, `lommel_s1` sum a Maclaurin series of 2|z| terms at 1.5|z| extra bits: beyond
  |z| = 2500 they stay unevaluated (the machine asymptotic form of `struve_h` at x >= 40 + nu^2 is unchanged).
  `spheroidal_eigenvalue`: |gamma| > 1.5e4 stays unevaluated (2|gamma| continued-fraction terms per secant step).
- `find_root(spheroidal_eigenvalue(2, 0, c) == spheroidal_eigenvalue(0, 0, c), [c, 2 + 3 im], working_precision: 30)`
  still allocates 2^58 bytes: the failure is in `mc-roots` after the function returns, at a root where the residual is
  an exact zero ball (the same crash: `find_root(sqrt(c - 1 - i) - sqrt(c - 1 - i), [c, 2 + im], working_precision: 30)`).
  To fix in `mc-roots`.
- Spheroidal eigenvalue: the last solution is kept (key: n, m, gamma^2 as printed, w): a plot of `spheroidal_ps` over x
  is 4 times faster (1.2 s -> 0.27 s for 99 points).
- Coulomb F at a complex machine rho (`fast30::coulomb_f_complex`): the Maclaurin series
  F = C_l rho^(l+1) sum a_j rho^j, j (j + 2l + 1) a_j = 2 eta a_(j-1) - a_(j-2) (from the equation itself; DLMF 33.6.5) in
  complex double-double, C_l from |Gamma(l + 1 + i eta)|^2 in closed form at integer and half-integer l (DLMF 5.4.3-4),
  |rho| <= 12, cancellation below 14 digits, else the ball path. 61 x 61 grid points in 70 ms (were 30 ms each); values
  identical to the 40-digit path to 2e-16. G and H+- at complex rho still use the ball path.
- `inverse_jacobi_*` at complex arguments: the real-case formulas (DLMF 22.15) may land on a wrong sheet
  (`inverse_jacobi_cd(0.2 + i, 0.1 - 0.3 i)` returned u with cd(u) = -x); the result is kept when the forward function
  maps it back, else the images under u -> -u, 2K - u and the shifts by 2K, 2iK' are tried (2K - u first).
- `zeta_zero(k, t0)`: the zeros 1/2 +- i t_j ordered by imaginary part (k > 0 upwards from t0, k < 0 downwards), so
  `zeta_zero(2, -20)` is the zero 14.13i above, `zeta_zero(-3, 20)` is -21.02i (was -30.42i); one-argument form unchanged.
- Checked and correct: the Lommel integrals (within 1.2e-13 of the exact termwise integral 0.09811887551149243),
  coulomb_h1/h2 at (1/2, 7, 1) (agrees with the 1F1 form to 13 digits), dedekind_eta at a machine argument 1e22 pi + i
  (1e22 pi is an integer N and eta(N + i) = e^(pi i N/12) eta(i), exact), elliptic_log and inverse_weierstrass_p
  (values fixed up to a lattice period; they invert), inverse_jacobi_ds/nc/ns (the answers invert), heun_c/d/g at real
  z beyond the singular point (the continuation along the upper half plane, checked by integrating the equation), and
  an `n` value whose exact value is 9.0e-26.
- Not reached: `mathieu_c/s/s_prime` at complex a with q < 0 (normalization convention undetermined),
  `differential_root(y' = y)(0.5)` is 6e-10 off exp(0.5) (mc-calculus / mc-ode).

## Round 33

- **Jacobi functions at complex machine arguments** (`fast33.rs`): sn, cn, dn (and so the nine ratios, which now take
  the double-double kernel through `jacobi_ratio`) at a complex u and a real m, in complex double-double: the real
  parameter reductions of A&S 16.10 (m < 0) and 16.11 (m > 1), the imaginary transformation sn(u | m) = -i sc(iu | 1 - m)
  etc. (DLMF 22.6.9-11) for 1/2 < m < 1, and for m <= 1/2 the theta quotients (DLMF 22.2.4-6) with q = exp(-pi K'/K) <= e^-pi
  after reducing u by the periods 2K and 2iK' (the q-series to 1e-34, the powers e^(ikv) by multiplication). 6 to 35 ms
  per value on the ball path, 5-30 us now; `complex_plot3d` of the Jacobi functions (12 to 67 s) takes 0.1-0.3 s. Values agree with the
  40-digit value of the exact argument to 3e-15. `elliptic_k(m)` for a machine m > 1 (on the cut from below):
  (K(1/m) - i K(1 - 1/m))/sqrt(m) with the double-double AGM (23 ms -> 5 us); `elliptic_f(phi, m)` for m > 1 beyond
  the branch point asin(1/sqrt m) (was unevaluated): sin(phi) R_F(cos^2, 1 - m sin^2 + i0, 1) with 2 k K(m) per half
  period (DLMF 19.2.10, 19.25.5); `inverse_jacobi_*` computed K' = K(1 - m) eagerly (complex, 20 ms, for m < 0 or
  m > 1): now only when a shift by 2iK' is tried.
- **Jacobi functions at the quarter periods** (`jshift.rs`): `jacobi_pq(x + a K(m) + b i K(1 - m), m)` for integers a,
  b (and a half-integer a at x = 0, with K/2 values from the duplication formulas) is a monomial in
  k = sqrt(m), k' = sqrt(1 - m), i times another Jacobi function of x. The four functions (s, c, d, 1) form a vector whose
  shift by K is (c, -k' s, k', d) and by iK' is (1, -i d, -i k c, k s) (DLMF Table 22.4.3 multiplied through by the common
  factor), so composing the two maps a and b times (mod 4) gives all 12 functions at all shifts without a table (checked
  at 11 x 7 shifts of the 12 functions against the numeric values: 5e-15). `jacobi_cd(x, m) == jacobi_cd(x + 4 K(m), m)`
  is `true`, `jacobi_ns(K(m)/2, m)` is `(1 + (1 - m)^(1/2))^(1/2)`. At m = 0 and m = 1 each ratio is in its own name
  (`cot`, `csch`, ...: DLMF 22.5.1-2); `jacobi_cn(z, 1)` is `sech(z)`. `jacobi_amplitude(u, 1)` is `gudermannian(u)`;
  amplitude, zeta and epsilon at K and K/2 and the inverses at v = 0, +-1 (`inverse_jacobi_cd(0, m)` is K(m)).
  `elliptic_f(pi/2, m)` is K(m), `elliptic_e(pi/2, m)` is E(m), `elliptic_pi(n, pi/2, m)` is `elliptic_pi(n, m)`,
  `elliptic_pi(n, 0, m)` is 0 and `elliptic_pi(n, 0)` is pi/(2 sqrt(1 - n)).
- **Elliptic Pi at complex machine arguments**: Carlson R_C (duplication, series of order 7 in s), R_J (the 1995
  algorithm: duplication with R_C, series of order 6) and the Pi formulas of DLMF 19.25.14 in double-double
  (`elliptic_pi_complete`, `elliptic_pi_inc` with Pi(n; phi + k pi | m) = Pi(n; phi | m) + 2 k Pi(n | m)); on the cuts
  n >= 1, m >= 1 they decline. `elliptic_pi(-1, 2.5 + i, 1/3 - i)` = 1.4237821081510118 + 0.10216920379605779 i, the
  straight-path integral of the integrand.
- **Modular functions near the real axis** (`klein_invariant_j`, `modular_lambda` at a machine tau): reduction to the
  fundamental domain with tau -> tau - n and -1/tau in double-double arithmetic (the input is an exact dyadic
  number), then the theta constants at |q| <= e^(-pi sqrt(3)/2); lambda is mapped back through the recorded
  involutions lambda -> lambda/(lambda - 1), 1 - lambda. The nome series at tau = 0.3 + 0.001 i lost 11 digits (and took
  10 ms); now full precision, 30 us. Values beyond 1e308 (Im tau < 1e-8) decline.
- `mathieu_s(a, 0, z)` is sin(sqrt(a) z) and `mathieu_s_prime(a, 0, z)` is sqrt(a) cos(sqrt(a) z) (DLMF 28.12: se_nu(z, 0) =
  sin(nu z)); the symbolic form had a factor 1/sqrt(a), the numeric form was right.
- `appell_f1(a, b1, b2, a, x, y)` = (1 - x)^-b1 (1 - y)^-b2 (the double series factorizes at c = a); the plots with
  c = a and x = 4 took 6 s per point on the continuation, now exact.
- Prelude rules (`lib/special.mc`): `hypergeometric_0f1(b, x)` at half-integer b (cosh/sinh of 2 sqrt x and the
  contiguous relation in both directions), `zeta(2, 1/4)`, `zeta(2, 3/4)`, `lerch_phi(z, -n, a)` as the rational function
  (a + z d/dz)^n 1/(1 - z), `fresnel_s/c(+-i inf)`, `klein_invariant_j(rho) = 0`, `log_barnes_g(-n) = -inf`.
- **Checked, not bugs**: mathieu_c/s/s_prime at a complex point (the values satisfy the equation to 1e-8), kelvin_ker
  at an argument where -1.2 e^(i pi/4) puts K_2 on its cut, sinh_integral and struve_l at points where the value is
  tiny, scorer_hi_prime, airy_ai_zero (the two roots of an interval: order and exact form belong to mc-solve),
  binomial (assumptions).
- **Gaps**: the sign of the Floquet normalization of `mathieu_c/s/s_prime` at complex a jumps with q (`mathieu_c(i, -q, 0)`
  changes sign between q = 1.5 and 1.8: the root sqrt(sum c_k^2) is the principal one of a quantity that crosses its
  cut); a continuous choice needs tracking along q. `n(elliptic_f(12/5, 3), 20)` (a complex value on the ball path)
  and `n(...)` of exact arguments with a complex outcome at machine precision still give `indeterminate`/`precision_not_reached`.
  `hypergeometric_2f1` (and so `appell_f1` rows) beyond |z| > 1 outside the Pfaff half plane goes through the ball path
  (2-10 ms): 1/z and 1 - z transformations in double-double need the complex gamma function in double-double.
  `complex_contour_plot(re(klein_invariant_j(z)), [z, -1 - 0.1 i, 1 + 3 i])` still times out in mc-plot (adaptive
  refinement of a function that varies like e^(2 pi/y) near the real axis; 30 us per value).

## Round 34

- The special-function library file is `lib/special.mc` (it had the round number in its name); helper prefix `ds_` kept. No
  behaviour change.

## Round 36

- **Derivative rules** (`mc-calculus/src/rules.rs`, rows only): the parameter derivatives of the Jacobi functions that were
  operators. d/dm am(u|m) = (m sn cn - dn (E - (1 - m) u))/(2 m (1 - m)) (Byrd and Friedman 710.00), E = `jacobi_epsilon`;
  sn_m = cn am_m, cn_m = -sn am_m, dn_m = -(sn^2 + 2 m sn cn am_m)/(2 dn), the nine quotients by the quotient rule,
  epsilon_m = (E - u)/(2m) + dn am_m, zeta_m from E(m)/K(m). The twelve inverse functions by implicit differentiation (d/dv =
  1/pq', d/dm = -pq_m/pq_u at u = the inverse). `inverse_gamma_regularized` (s: -+Gamma(a) e^x x^(1-a)), `inverse_beta_regularized`
  (s: B(a, b) x^(1-a) (1-x)^(1-b)), `inverse_elliptic_nome_q` (4 m (1-m) K^2/(pi^2 q)), `modular_lambda` (4 i l (1-l) K(l)^2/pi),
  `klein_invariant_j` (chain rule through lambda), the Bessel zeros in the index (pi^2 x f(x)^2/2 from the modulus-phase form,
  checked against central differences of the continuous zero), spherical Hankel in z, `barnes_g` (G'/G). All rows checked by
  central differences in `crates/mc-engine/tests/r36_w36_special.rs`.
- **Series** (`lib/special.mc`): Jacobi sn, cn, dn, amplitude, epsilon, zeta and the nine quotients in m at 0 (perturbation of
  phi' = sqrt(1 - m sin^2 phi); the derivative at m = 0 is a 0/0, so the Taylor path cannot be used); the inverse functions to
  first order; Heun G, C, B, D, T from the recurrence of their equations (`ds_ode`); Mathieu c at 0; the characteristic value a_r(q)
  (DLMF 28.6); the spheroidal eigenvalue to g^2; Mittag-Leffler, Whittaker M/W (integer 1/2 + mu - k), q-hypergeometric,
  q-Pochhammer (Euler), regularized pFq, inverse nome, theta prime, Anger and Weber (integral representation differentiated),
  zeta (functional equation and the Stieltjes constants), double factorial, Barnes G and the hyperfactorial through gamma.
  `normal` removes the `O(...)` terms inside a product or a sum of products (series at infinity with an exponential factor).
- **Numerics**: `neville_theta_*` with an exact m and an inexact z (or the reverse) were half symbolic: all arguments are now
  balls at the working precision. `elliptic_theta` accepts an integer-valued real index (`n` maps over every argument) and has
  exact zeros at multiples of pi/2 (and for the primes). `elliptic_theta(a, q)` is `elliptic_theta(a, 0, q)`. Heun functions in
  the lower half plane are reached without crossing the real axis (the detour around a singularity follows the side of z; the
  value for Im z < 0 was on another sheet; checked by an ODE integration outside the system). Regularized hypergeometric
  functions accept an inexact lower parameter 0., -1., ... (the same limit as the exact integer); `beta_regularized(0, 0, 0)` and
  `struve_h/l(nu, complex_infinity)` are `indeterminate`; `kelvin_ber/bei` at a negative real x of integer order are real;
  machine values of a closed form go through 20 digits (n(e^9) was 2 ulp off); an imaginary part 1e-40 below the real part of a
  machine value of 2F1 on the cut is dropped. `hypergeometric_2f1` at exact rational arguments terminates (17 for
  2F1(3/2, -4; 1/2; 2)) and uses Euler's transformation when c - a is a non-positive integer (used by appell_f3/f4).
  `multinomial` with a symbolic argument is the quotient of factorials.
- **Not wrong (recorded)**: wider enclosures, the Heun continuation beyond the singular points, Weierstrass and Siegel
  values (30-digit checks), lattice periods, conventions at a = 0 of the Lerch transcendent.
- **Left**: difference/differential roots, bilateral pFq, `find_root` options, `function_analytic` at pole
  parameters, the `around` radius of Meijer G, a few derivative rows for lists.
- Not done: the `mathieu_c` documentation example of 1 s under load (precision 30 needs 0.5 s for a_r alone: the 64-bit stage of the secant method).

## Round 36, follow-up

- **Regressions of the round 36 changes.** `neville_theta_*` with machine arguments stay machine numbers (the complex ball path
  took 30 ms and failed at complex m: a complex ball times `pi` stayed a product, now joined as for real balls in
  `mc-engine/src/arith.rs`); an inexact m = 1 gives (sinh, 1, 1, cosh) and `elliptic_nome_q` of an inexact 1 is 1 (it
  looped). Jacobi sn, cn, dn at a machine m = 0 or 1 with a complex u are circular or hyperbolic (DLMF 22.5.3-4; the other
  paths divide by an infinite K' and never returned) and the inverse functions there are acos, acosh, asinh, ... (dn, nd at
  0 and cd, dc at 1 are constant and stay unevaluated). `modular_lambda`/`klein_invariant_j` decline machine points below
  Im tau = 1 that the reduction cannot take (Im tau < 1e-8, or values beyond the machine range) instead of the nome series
  at |q| near 1 (noise); the contour plot takes 0.14 s. 2F1(a, b; a; z) = (1 - z)^-b (DLMF 15.4.6) in machine and ball
  arithmetic (integer b by products: no imaginary noise on the cut); a machine value with an exactly zero imaginary part is
  real; regularized pFq at an exact pole parameter with machine arguments is evaluated in balls from the exact machine
  values and rounded once. `around` through a function with parameter lists (meijer_g) uses the first-order slope
  (`mc-numeric/src/uncertain.rs`). Docs: the 30-digit Mathieu s and s' examples use 20 digits (under 1 s).
- **Fixes.** The index derivative of the Bessel zeros is pi^2 j (J^2 + Y^2)/2 (DLMF 10.18.8 with 10.21(iii)); J(j_(nu,k)) = 0
  only at an integer k. `function_analytic` is false for 0F1 at b = 0, -1, ... and for j_n at a negative integer n. d/dx
  elliptic_log([x, y], [a, b]) = 1/(2y). The z-derivative of the bilateral series by the contiguous relations
  z H' = a1 (H(a1 + 1) - H) or (b1 - 1)(H(b1 - 1) - H); no power series at z = 0 (the value 0 at z = 0 is kept). `difference_root` below its
  initial values runs the reflected recurrence (w(t) = y(-t)); without a value it stays unevaluated (it returned the
  equations). Jacobi's imaginary transformation (DLMF 20.7.30-33) for theta_k at a real nome in (e^-pi, 1): theta_3(pi/2,
  e^(-pi/1000)) = 5.09e-340 to 50 digits (was a ball around 0). Series of Riemann-Siegel Z at 0 from Z = e^(i theta)
  zeta(1/2 + it), symmetrized (Z even). `find_root` (mc-roots): a machine Newton run that ends on a residual rounded to 0
  with its last step beyond the goals (a multiple root) continues in 30-digit balls (sin(x - 10) - x + 10: 4e-10 from the
  root); without convergence there it answers not_converged (exp(1/(x^2 - 1)) has no zero); the ball
  refinement no longer stops at the first step whose exponent did not shrink (a triple root halves it every other step),
  stops at a residual ball around 0 under linear convergence, and estimates the error as 4 times the last step there.
- **Recorded** (82 examples): equal forms checked at three points with 30-digit balls (the inverse Jacobi derivatives,
  matrix functions of erf), the third derivative of inverse_jacobi_sc (checked by difference quotients), design
  (`d(f, [x, n])` with a symbol is the gradient), declines and unevaluated examples (Carlson R_K derivatives, Heun
  primes, difference/differential roots, function_range).
## Round 36, follow-up (algebra)

- `multinomial` up to 32 arguments.

## Round 36, numerics follow-up

- `Work::out`: an imaginary part that is a ball around 0 (or 1e-40 below the real part) makes the machine result real
  only when every argument is real; a complex argument keeps a complex value with imaginary part 0.0
  (`elliptic_theta(2, 0, 0.2 + 0.0 im)` had become real in the follow-up, and the half-period page's
  `e1([12., 7.])` lost its 0.0 im).

## Round 37: inputs with no answer

- Complex machine values that went to the 192-bit ball path (1-5 ms a point; the contour plots of 80 x 80 points
  timed out): 2F1 beyond |z| < 0.9 and the Pfaff disk by the connection formulas to 1/z, 1/(1 - z), 1 - z (DLMF
  15.8.2-15.8.4), each a sum of two series, declined when the parameter difference it divides by is an integer, when
  the two terms cancel to fewer than 9 digits, or near exp(+-i pi/3) where no transformation reaches |w| < 0.9; a real
  z > 1 is the limit from below as on the ball path (the powers of -z and 1 - z take arg +pi). Theta_a and theta_a'
  by the series of DLMF 20.2.1-4 with the principal q^(1/4) (the ball path's convention), |q| < 0.99. pFq for p <= q
  (and p = q + 1 inside |z| < 0.9) by its series. Each declines when its largest term exceeds the result a
  thousandfold. Measured against the ball path at 30 digits: relative errors below 2.4e-14 (2F1), 6e-15 (theta),
  6e-16 (pFq).
- Double-double kernels (`fast29`, the unit U = 2e-31 with the cancellation bound of `lommel_t1`): Lommel's s (the
  alternating series), the q-product (x; q)_inf (the sum of |x q^k|/|1 - x q^k| bounds the relative error in units
  of U) and the basic hypergeometric series; the Weierstrass lattice quantities (e_k, eta_k, half-periods) from the
  kernel's lattice, the half-periods reduced by `reduce_basis` on machine numbers and then recomputed in double-double
  numbers as the integer combinations of the kernel's basis they are (a 1-ulp difference in eta2 otherwise).
- pFq on complex balls (`ball::cspecial::hyp_pfq`): the series with its rigorous tail (`pfq_sum`) and `refine`; complex
  ball arguments of pFq with p, q not 0/1, 1/1 or 2/1 were unevaluated.
- New function `mathieu_c_prime(a, q, z)`: the derivative of `mathieu_c`, y' of the same integration (DLMF 28.2), the
  even normalisation; q = 0 gives -sqrt(a) sin(sqrt(a) z); odd in z. With the derivative rows of mc-calculus
  `d(mathieu_c(3, 2, x), x)` and its higher derivatives are closed forms, so the plots of the derivatives no longer
  need numerical differentiation (which at 0.5 s per 34-digit Mathieu value would time out).
- Degenerate Weierstrass lattices: exact invariants with g2 != 0 and g2^3 = 27 g3^2 (g2 = 12 c^2, g3 = -8 c^3, c = -3
  g3/(2 g2)): p = c + k^2 csch(k u)^2, p' = -2 k^3 csch(k u)^2 coth(k u), zeta = -c u + k coth(k u), sigma = sinh(k u)/k
  exp(-c u^2/2), k = sqrt(3c) (Abramowitz and Stegun 18.12). Checked by p'^2 = 4p^3 - g2 p - g3, zeta' = -p, sigma'/sigma
  = zeta and against the lattice algorithm at g3 moved by 10^-7.
- Lommel closed forms at symbolic arguments: mu = nu (s = c H_nu, S = c (H_nu - Y_nu), t = c L_nu, T = c (I_(-nu) -
  L_nu), c = pi (1/2)_nu 2^(nu - 1), DLMF 11.9.7 and 11.9.5; t at mu = -nu too), s and S at mu = 0 and -1 through the
  Anger and Weber functions (Watson 10.71), S when mu +- nu is an odd positive integer (the terminating expansion,
  DLMF 11.9.9). Each checked numerically against the series and connection paths at a real and a complex point.
- Mittag-Leffler: E_(m,b) for integers m >= 1 and b as (1/m) z^((1-b)/m) sum_k w^(k(1-b)) exp(w^k z^(1/m)), w = e^(2 pi
  i/m) (independent of the branch of z^(1/m)); E_(1,b) = e^z z^(1-b) (Q(b-1, 0) - Q(b-1, z)) (Gorenflo, Kilbas,
  Mainardi, Rogosin 2014, 4.1-4.2). Only at symbolic z or exact non-real z: real exact values keep the real series.
- `weierstrass_p`, `_p_prime`, `_zeta`, `_sigma` of `around(2, 0.01)` at [12, -8] give the first-order spread
  |f'(2)| 0.01 (0.000408878, 0.00142195, 0.0101178, 0.00329872, from the closed forms and checked by
  p'' = 6 p^2 - g2/2, zeta' = -p, sigma' = sigma zeta). Treating the occurrences of u in a closed form as independent
  would be wrong: for zeta = -u + sqrt(3) coth(sqrt(3) u) it gives sqrt(1 + (3 csch(2 sqrt 3)^2)^2) 0.01 = 0.0100007
  instead of |1 + 3 csch(2 sqrt 3)^2| 0.01 = |p(2)| 0.01.
- Still declining (no general method here): Mathieu values at 30+ digits (0.5 s each, a term-level Taylor
  integration), so numerical derivatives in the Mathieu parameters time out; T_(mu,nu) at machine z (the connection
  formula cancels like e^|z|, no double-double Bessel I and K); hypergeometric_u at large |z| (the M-combination cancels,
  the asymptotic series is too short at |z| ~ 10); Mittag-Leffler E_(1/4) on the negative axis (the series needs
  hundreds of digits; an integral representation is the method); Lerch Phi beyond |z| = 1; interval and
  centered-interval arguments of most special functions (an enclosure needs a rigorous range, not samples).

## Round 37, follow-up: wrong values and hangs

- **Half-integer Struve forms** (`struve_half_integer`): the prefactor is sqrt(2/pi) z^(-1/2), not (2/(pi z))^(1/2);
  the two differ in sign on the negative axis, where the principal branch H_nu(z) = (z/2)^(nu + 1) (even series) takes
  z^(-1/2) like the terms c_nu of the recurrence (DLMF 11.4.5-8, 11.4.23-25). H_(5/2)(-17/10) was -2.18i, the series
  gives -0.0944i. The term c_nu = (z/2)^nu/(sqrt(pi) Gamma(nu + 3/2)) is 0 at nu = -3/2, -5/2, ... (1/Gamma at a pole):
  orders below -3/2 failed with `pole`. Lommel s, S, t, T with mu = nu reuse these forms. Checked: the defining series
  at 30 digits, the machine kernel; Bessel J, Y, I, K, spherical Bessel, Anger and Weber at half-integer orders and
  negative or complex arguments agree with their numerical paths.
- **Kelvin ker, kei at 0, integer order** (`kelvin_k_at_zero`): from K_n(w) = (1/2) sum_(k<n) ((n-k-1)!/k!)
  (w/2)^(2k-n) (-1)^k + O(w^n log w) (DLMF 10.31.1) with w = x e^(pi i/4) and the factor e^(-n pi i/2): the term k
  carries e^(i pi (6k - 3n)/4); the first one with a nonzero real (ker) or imaginary (kei) part gives the infinity of
  its sign, else the term k = n/2 gives ker_n(0) = 1/n (only n = +-2 is finite: ker_2(0) = 1/2), kei 0; negative
  orders by (-1)^n. Inexact zeros give the value in their kind. ker_2(0.) was -inf, kei_0(0.) was 0.
- **Kelvin machine values near 0**: the parts p, m (Bessel values of the rotated argument) are machine numbers with an
  error relative to their size; the value can be far below them (ker_2(1e-6): |p| ~ 2e12, value 0.5) and lost 12
  digits (0.49988 for 0.4999999999999018; ber_2(1e-6) 0.2 % off). The measured loss log2(|p|/|value|) now asks for a
  ball evaluation of the closed form at the arguments' binary values, at (53 + loss)/3.3 + 8 digits, then twice and four
  times that, until 57 correct bits; otherwise no value. Values with a loss of at most 4 bits keep the machine path.
  Gap: at large x (bei(0, 500.)) the machine path is ~1e-14 off because the rotated argument x e^(3 pi i/4) is
  rounded (relative error x ulp); the ball path is right but bessel_k of a large complex ball needs ~600 digits there.
- **Hyperfactorial** (`hyperfactorial_numeric`): H(z) = K(z + 1) = exp(zeta'(-1, z + 1) - zeta'(-1)), the definition
  behind the property table (real domain z >= -1, cut z <= -1). For Re(z + 1) > 0, log K(a) = (a - 1) log Gamma(a) -
  log G(a); elsewhere the defining sum shifts a by N with principal logarithms of the terms, the terms on the cut
  carrying pi i, so H(z) = |H(z)| e^(pi i P) (P the sum of |a + j| over the negative terms; P an integer gives the exact
  sign). Gamma(z + 1)^z/G(z + 1) gave the real -0.98686 at z = -2.2; the value is 0.98686 e^(1.4 pi i). Exact negative
  integers: H(-n) = (-1)^(n (n - 1)/2) H(n - 1) (H(-2) = -1). Evaluated at working precision (log H ~ |z|^2 log|z|/2
  bits more). Checked by H(z) = H(z - 1) z^z and by the Maclaurin series of log G (DLMF 5.17.3).
  H(3.4) = 745.9769577319412 is the correctly rounded value at the double 3.4 (the 30-digit value at its binary is
  745.97695773194115905).
- **Barnes G at real x < 0**: |G(x)| = exp(Re log G(x)) and the sign (-1)^(c (c + 1)/2), c = ceil(-x), from G(x) = G(x
  + c)/prod Gamma(x + k): real, without the rounding-size imaginary part of exp(log G). Machine integers <= 0 are zeros
  (0.0). log G is taken unrounded at the working precision of log_barnes_g (40 guard bits, enough for |log G| up to
  2^30) before exp: exp of log G rounded to the target lost log2 |log G| bits (barnes_g(-12.7) was 11 ulp off). The cost per value is
  O(|x|) log-gamma evaluations (the shift of log G), 0.4 s at x = -1000: a reflection by the Clausen function would make
  it O(1) (not done).
- **Scorer Hi at large x > 0** (`fast29::scorer`): Hi = Bi - Gi (DLMF 9.12.12) with Gi by its asymptotic series and the
  overflow of Bi as the overflow of Hi (log Bi from DLMF 9.7.5); the ball path declines at once when the Maclaurin
  series would need more than the 20 000 terms a series may take (2 |z|^(3/2) terms): scorer_hi(1e6) set up constants
  at 6 10^8 digits and never returned; scorer_hi(1e6 i) now declines in milliseconds.
- **U(a, b, 0)** for Re b < 1 (a not a non-positive integer): Gamma(1 - b)/Gamma(a - b + 1) (DLMF 13.2.20-22), 0 at a
  pole of the denominator; the rule U(1, b, x) = e^x x^(1 - b) Gamma(b - 1, x) is not applied at x = 0, where it gave 0
  (U(1, 1/2, 0) = 2). Re b >= 1 has no finite value and stays unevaluated.
- **inverse_elliptic_nome_q(1) = 1** (the end of the real domain -1 < q <= 1; m(q) -> 1 as q -> 1-); a ball with a
  radius at 1 stays unevaluated (it reaches outside the domain).
- **Inverse Jacobi functions for real m > 1**: the reciprocal-modulus transformation (DLMF 22.17.2-4, k = sqrt(m)): each
  inverse at m is an inverse at 1/m divided by k (dn <- cn, cd <- dc, nd <- nc, dc <- cd, ds <- cs(v/k), cn <- dn, sd <-
  sc(k v), nc <- nd, sc <- sd(k v), ns <- ns(v/k), cs <- ds(v/k)). The real range of the function at m maps to the real
  range at 1/m, where the principal inverse is real; the DLMF 22.15 formulas used |v| only and gave complex values on
  half the range (inverse_jacobi_dn(-0.3, 3.) was 0.787 + 2.343i for the real 1.2149). Agrees with the documented
  inverse_jacobi_nd(-1, m) = 2 K(1/m)/sqrt(m) for m > 1. Every value is checked by the forward function at 30 digits
  in the tests. Outside the real range the value is another preimage than before (both invert).
- **alternating_factorial** (`lib/calculus.mc`, outside the crate): exact m by a(k) = k! - a(k - 1) with a running
  factorial (each factorial afresh took 3.7 s at m = 10^4); Re m < -1 by Gamma(m + 2) Gamma(-m - 1) = pi/sin(pi m), i.e.
  e pi Q(-m - 1, 1)/sin(pi m), where the product of an underflow and an overflow gave `indeterminate` (m = -1000.5).
  Not reproduced: "1.5 s a value" at machine arguments (0.15 s here); a 5000-digit m takes 30 s in gamma (Stirling
  at 5000 digits).
