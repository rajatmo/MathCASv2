# mc-special: special functions

Status: merged (branch `lib/special`, commits 54aa423..0428823 from main 037eee3; merge 9940533). On the branch: full
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
reference values, 33 derivatives against central differences at 60 digits, 1 vs 4 threads, interrupts) and seven
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
