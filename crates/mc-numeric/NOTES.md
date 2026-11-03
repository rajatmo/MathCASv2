# Package L: `mc-numeric`

Status: built, tested and committed (branch `lib/l-numeric`: 068095d, d3c85f8, 33039df). 23 tests pass in debug and
release; clippy and `cargo doc` clean with `#![warn(missing_docs)]`. Dependencies: `mc-api`, `mc-arith`, `mc-core`,
`mc-eval`, `rustc-hash`; dev: `mc-engine`, `mc-syntax`, `num-bigint`.

## What was built

- **`consts`**: `Const::{Pi, E, EulerGamma, Catalan}` with `name`, `from_name`, `machine()` and `ball(p)` (an
  enclosure). pi by Chudnovsky with binary splitting; e by the sum of 1/k!; catalan by
  G = π/8·log(2+√3) + 3/8·Σ 1/((2k+1)² C(2k,k)); euler_gamma by Brent–McMillan with the harmonic sums in a
  six-component binary splitting. Cached process-wide at the best precision computed so far.
- **`series`**: generic binary splitting of series with term ratio p(k)/(q(k)·2^shift), with rigorous tail bounds.
- **`elementary`**: ball exp, log, sin, cos, sin_cos, tan, atan, asin, acos, sinh, cosh, tanh, sqrt, `pow_int`, `pow`
  (real exponents). exp and sin/cos by the bit-burst method (chunks of 8, 8, 16, 32, … bits, each by binary
  splitting); exp uses e^⌊x⌋ for the integer part; sin/cos reduce by kπ/2. log and atan by Newton at doubling
  precisions with a rigorous last step (log x = y + log(1+t), |log(1+t) − t| ≤ t²; atan x = y + atan(z),
  |atan z − z| ≤ |z|³). The rest are ball expressions of these. The argument's radius is propagated through
  derivative bounds.
- **`machine`**: the one shared set of real machine functions (decision E.4: `REAL` table, `RealFn`) and `C64`
  principal complex values.
- **`fun::Fun`**: one enumeration of the functions (names, descriptions, parity), evaluated on balls and machine
  numbers (`Mach::Real` / `Mach::Complex`).
- **`exact`**: exact identities: sin, cos, tan at qπ for denominators 1, 2, 3, 4, 6 with radicals through `Arith`;
  inverse lookups for asin, acos, atan; parity (`sin(-x) = -sin(x)`); `exp(log x) = x`, `sin(asin x) = x`,
  `log(exp y) = y` for real y; `exp(0)`, `exp(1) = e`, `exp(qπi)`, `log(1)`, `log(e)`, `log(e^q)`, `log(-1) = iπ`,
  `sinh/cosh/tanh(0)`.
- **`numeval`**: a term is read into a `Graph` iteratively; each maximal numeric subterm (an island) is evaluated and
  the rest kept. `numericize` implements `n(e)` and `n(e, d)`; `compare`, `sign`, `is_real` decide by refinement.
  `GUARD_BITS = 16`, `CAP_FACTOR = 16`, `CAP_EXTRA = 4096`, `COMPARE_CAP = 2^13`.
- **Natives** (`install`, `install_with_stop`): the 12 functions (`sqrt(x)` is `Arith::pow(x, 1/2)`); `n`;
  `is_positive`, `is_negative`, `is_real`; conditioned rules on `less`, `less_equal`, `greater`, `greater_equal`,
  `equal`, `unequal` decided by refinement (the engine's comparison natives decline for non-numbers, so these rules see
  exactly the comparisons involving constants or functions). `FUNCTIONS` lists 20 entries;
  `failure_tags::{PRECISION_NOT_REACHED, INVALID_ARGUMENT}`.

## Tests (9 unit, 13 integration, 1 doctest)

- Unit: binary splitting (exact geometric sum, e); constants against 50 known digits and the nearest machine values;
  ball functions against known values and f64; domain limits; complex principal values; the comparison truth table.
- `tests/enclosure.rs`: all 12 functions on random arguments with zero, about one-unit or 2^-20 relative radius, at p
  from 53 to 2000 bits; references at both ends and the midpoint at 2p+64 bits must lie inside the result ball;
  tightness for exact arguments. A mutation (series cut short, tail bound dropped) fails 3 of the 4 tests. Identities
  at 10^4 bits; large arguments (sin(123456789·2^20), exp(±700)); independent algorithms (pi = 4·atan(1) and
  6·asin(1/2); e from its continued fraction and as exp(1/2)²; gamma and G at two precisions); 10^4 digits of pi
  against `bench/cases/N/ref/N1-pi-1e4.txt`.
- `tests/session.rs` (through the evaluator with `mc-engine`'s arith install plus this package): exact values, machine
  values, balls, refined comparisons including N12.
- `tests/limits.rs`: `n(pi, 0)` → `invalid_argument`; `n(pi, 10^12)` → `failure(limit, memory, …)`;
  `n(sin(1)^2 + cos(1)^2 - 1, 10)` → `precision_not_reached`; a fixed-accuracy ball input → `precision_not_reached`;
  `n` through lists and other heads, machine contagion, complex values; N1 at 10^4 (reference file) and 10^5 (against
  a 10^5+100-digit run, release); N12 at 1 and 4 threads; the stop flag raised before and during `n(pi, 2·10^7)` gives
  `failure(interrupted, …)` within seconds, with nothing half-computed cached.

## Measurements (busy machine, release, `examples/bench.rs` under the guard: 10 s, peak 48 MB)

| case | evaluate | print |
|---|---|---|
| N1 `n(pi, 10^4)` | 0.002 s | 0.007 s |
| N1 `n(pi, 10^5)` | 0.053 s | 0.087 s |
| N1 `n(pi, 10^6)` | 0.85 s | 1.77 s |
| N12 (three comparisons) | 0.3 ms | – |

| function | 10^3 digits | 10^4 | 10^5 |
|---|---|---|---|
| exp | 0.5 ms | 9 ms | 0.31 s |
| log | 0.8 ms | 13 ms | 0.42 s |
| sin, cos, tan | 0.5 ms | 20 ms | 0.60 s |
| atan | 1 ms | 29 ms | 0.97 s |
| asin, acos | 0.8 ms | 27 ms | 0.88 s |
| sinh, cosh, tanh | 0.5 ms | 9 ms | 0.30 s |
| sqrt | <0.1 ms | 0.7 ms | 18 ms |

## Decisions

- **L.1** `install(k, h)` and `install_with_stop(k, h, stop)` take the heap (the kernel holds none, and symbols belong
  to a heap); this deviates from the brief's `install(k)`.
- **L.2** `exp(x)` stays an `exp` call (not `e^x`), except `exp(0) = 1`, `exp(1) = e`, `exp(log x) = x`, `exp(qπi)`
  at special angles; `log(0)` stays unevaluated; `sqrt(x)` becomes `x^(1/2)`; other exact arguments stay unevaluated.
- **L.3** Constants are cached process-wide (mutex) at the best precision so far and rounded for lower requests;
  nothing is cached after a stop.
- **L.4** The algorithms and bounds above.
- **L.5** exp has ball values for |x| < 2^40, sin/cos for |x| < 2^(2^22); beyond, `precision_not_reached`.
- **L.6** `n` evaluates islands and keeps the rest (maps over lists and through any head); a machine real inside an
  island makes it a machine computation; exact complex numbers become complex balls; (-1)^q units become complex
  values.
- **L.7** `n(e, d)` starts at ⌈d·log2 10⌉+16 bits and doubles up to max(16·p0, p0+4096) bits within the size limit;
  the result is rounded to p0 (independent of the number of rounds); a d that is not a positive integer gives
  `failure(invalid_argument, call)`.
- **L.8** Comparisons refine from 64 bits up to 2^13 bits (or 4× a ball operand's precision); machine reals mean a
  machine comparison; `==` is decided only `false`, or `true` for two coinciding exact points; undecided stays
  unevaluated. A thread-local slot hands the condition's decision to the right side.
- **L.9** A machine argument outside a function's real domain gives the principal complex value.
- **L.10** The elementary functions map over a list argument (no `listable` attribute yet).
- **L.11** Integer square roots run with the stop flag off.
- **L.12** No ball functions of complex balls (such calls stay unevaluated).

## Bugs and requests for other crates

1. **`mc-arith`:** `Nat::isqrt` panics ("isqrt correction did not converge") when the stop flag is raised during it,
   because a stopped multiplication returns garbage; `Float::div` and `Nat::div` panic on a garbage zero divisor.
   Worked around here (uninterruptible square roots, zero checks); `mc-arith`'s own ball sqrt is still exposed through
   `Arith::pow` of balls to half-integer powers.
2. **`install` signature** needs the heap (L.1); the other library packages will hit the same.
3. **Real powers of balls:** the engine's `pow` native returns `Done`, so rules on `pow` never run and `pow(ball,
   1.7~)` stays unevaluated (B.13); the term-level `pow` could call `mc_numeric::elementary::pow` when `Arith` leaves a
   ball power unevaluated.

## Known gaps

- No complex ball functions (`n((-8)^(1/3), d)` works only for the (-1)^q unit form).
- No log1p/expm1: log, sinh, tanh lose relative accuracy near 1 / 0 (the doubling loop compensates at extra cost).
- `is_real(log(-2))` stays undecided rather than `false`.
- `n(log(0))` and exp beyond 2^40 report `precision_not_reached` although they are range or domain problems.
- Square roots at 10^7 digits are uninterruptible for a few seconds (L.11).
- P12 needs `series`, `normal`, `coefficient` (package K).
- Printed ball digits drop trailing zeros (`n(e^(-10^5), 10)` prints 9 digits); the `digits d` check could flag such
  cases.

## What integration needs

- In `mc-engine`, after `arith::install`, call `mc_numeric::install_with_stop(&mut k, h, &arith_stop)` with the
  engine's `mc-arith` stop flag.
- Optionally wire request 3 (real ball powers) into the engine's `pow`.
- N1 at 10^5 and 10^6 has no reference file.
