# Item L: `mc-numeric`

Status: built and tested. 23 tests pass in debug and
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
- `tests/session.rs` (through the evaluator with `mc-engine`'s arith install plus this crate): exact values, machine
  values, balls, refined comparisons including N12.
- `tests/limits.rs`: `n(pi, 0)` → `invalid_argument`; `n(pi, 10^12)` → `failure(limit, memory, …)`;
  `n(sin(1)^2 + cos(1)^2 - 1, 10)` → `precision_not_reached`; a fixed-accuracy ball input → `precision_not_reached`;
  `n` through lists and other heads, machine contagion, complex values; N1 at 10^4 (expected-value file) and 10^5 (against
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
  to a heap); this deviates from the plan's `install(k)`.
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
2. **`install` signature** needs the heap (L.1); the other library crates will hit the same.
3. **Real powers of balls:** the engine's `pow` native returns `Done`, so rules on `pow` never run and `pow(ball,
   1.7~)` stays unevaluated (B.13); the term-level `pow` could call `mc_numeric::elementary::pow` when `Arith` leaves a
   ball power unevaluated.

## Known gaps

- No complex ball functions (`n((-8)^(1/3), d)` works only for the (-1)^q unit form).
- No log1p/expm1: log, sinh, tanh lose relative accuracy near 1 / 0 (the doubling loop compensates at extra cost).
- exp beyond 2^40 reports `precision_not_reached` although it is a range problem.
- Square roots at 10^7 digits are uninterruptible for a few seconds (L.11).
- P12 needs `series`, `normal`, `coefficient` (item K).
- Printed ball digits drop trailing zeros (`n(e^(-10^5), 10)` prints 9 digits); the `digits d` check could flag such
  cases.

## What integration needs

- In `mc-engine`, after `arith::install`, call `mc_numeric::install_with_stop(&mut k, h, &arith_stop)` with the
  engine's `mc-arith` stop flag.
- Optionally wire request 3 (real ball powers) into the engine's `pow`.
- N1 at 10^5 and 10^6 has no expected-value file.

## Item 1C (roadmap phase 1: numbers)

All 1C number targets built, plus the two needs found in 2A (one part open, gap 1). At completion: workspace 549 passed;
clippy and `cargo doc` clean. A `lang.rs` fix made here duplicated the one already in `mc-eval`, which was kept.

### What was built

- `value` (public): exact values behind inexact numbers: the rational of a machine real or a ball midpoint; the
  interval a number stands for (half an ulp each side for machine reals, the ball for balls); log10 of magnitudes of
  any exponent; ball precision and accuracy in decimal digits; `simplest(lo, hi)`, the rational with the smallest
  denominator in a closed interval (continued fractions, fuel per partial quotient).
- `numbers` (private): shared symbols and stop flag, the native table, the iterative walks `map_leaves`, `for_numbers`.
- `parts`: `re`, `im_part`, `conjugate`, `arg`, and the rules on `abs` and `sign`. `approx`: `precision`, `accuracy`,
  `set_precision`, `rationalize`, `chop`. `piecewise`: `clip`, `rescale`, `unit_step`. `natives`: `n(x, digits: d)`.
- `numeval` (2A's requests): `abs` is a node of numerical evaluation (`Node::Abs`, ball rule {|m|, r}, valid when the
  ball contains 0), refined with the rest of an island; an exact exponent of `pow` outside an island stays exact
  (`n(1/(1 + abs(v)), 25)` gives `(1.0000000000000000000000000000 + abs(v))^-1`; `n(1/(1 + abs(1 - pi)), 25)` reaches
  25 digits).
- Listability: `re`, `im_part`, `conjugate`, `arg`, `unit_step` are `listable`; `chop`, `rationalize`, `set_precision`
  reach every number inside any term; `clip`, `rescale` map over a list first argument. 12 new `FUNCTIONS` entries;
  the `n` entry mentions `digits:`.

### Decisions

- **1C.1** Parts of numbers are exact operations on the parts: `im_part` of a real is exact 0; `arg` of a real is exact
  0 or pi; `arg` of an exact complex is an exact form evaluated again (`arg(1 + im)` is `pi/4`, `arg(-3 - 4im)` is
  `atan(4/3) - pi`); inexact complex: machine atan2 (IEEE signed zeros) or a ball argument by quadrants (a ball across
  the branch cut or the origin stays unevaluated).
- **1C.2** Parts of terms: a term splits into summands c (-1)^q r1..., with at most one complex unit and known-real r's
  (closed numeric terms with real values, or calls of `re`, `im_part`, `abs`, `arg`); `re`, `im_part`, `conjugate`
  distribute over that form; identities for any x (`re(re(x)) = re(x)`, `im_part(abs(x)) = 0`,
  `conjugate(conjugate(x)) = x`, `re(conjugate(x)) = re(x)`, `im_part(conjugate(x)) = -im_part(x)`,
  `arg(abs(x)) = 0`); anything else unevaluated.
- **1C.3** `chop(x[, delta])`: delta default 10^-10, any positive real; an inexact real chopped to exact 0 when
  |x| < delta, a ball only when |m| + r < delta; complex parts separately; exact numbers never chopped.
- **1C.4** `precision`/`accuracy` in decimal digits as machine reals (`inf` for exact): machine reals precision
  53 log10 2, accuracy that minus log10|x| (1075 log10 2 for zero); balls accuracy min(p log10 2 - log10|m|, -log10 r),
  precision accuracy + log10|m| (at least 0); over a term the least over its numbers.
- **1C.5** `set_precision(x, d)` is the deliberate exit from enclosure: every number becomes a ball of ceil(d log2 10)
  bits whose radius is only the new rounding (old radius dropped; machine reals at their binary value); closed
  subterms evaluated to d digits; `d = inf` gives exact binary values; bad d is `invalid_argument`, too large
  `failure(limit, memory, ...)`.
- **1C.6** `rationalize(x)`: the simplest rational in an inexact number's interval, kept if within 10^-4/q^2 (else
  unchanged; exact numbers and closed terms unchanged). `rationalize(x, dx)`: the simplest rational within dx widened
  by the number's interval, for every kind of real; closed numeric terms decided exactly by refinement
  (`rationalize(pi, 1/100) = 22/7`). Deliberate difference: an exact x is rationalized too
  (`rationalize(355/113, 1/100) = 22/7`).
- **1C.7** `abs` and `sign` of closed non-number terms are rules from `mc-numeric` (number natives stay in
  `mc-numtheory`): a closed real term of decided sign gives +-t distributed over sums (`abs(1 - pi) = -1 + pi`); the
  single-summand form of 1C.2 gives |c|(+-r...) or (c/|c|)(-1)^q(+-1) for `sign`; `abs(conjugate(x)) = abs(x)`.
- **1C.8** `clip` returns its bounds as given and `unit_step` exact 0 or 1 for every kind of real; comparisons through
  refinement (undecided: unevaluated); `clip` with lo > hi is `invalid_argument`; decided factors 1 are dropped from a
  remaining `unit_step`.
- **1C.9** `n(e, digits: d)` equals `n(e, d)`; both allowed only if equal; other options `failure(unknown_option, name)`.
- **1C.10** `rescale(xs)` uses the list's extremes (equal values give zeros); `rescale(x, [a, a])` is
  `division_by_zero`.

Exact semantics for derivatives (2A, real x): `re(x) = x`, `im_part(x) = 0`, `conjugate(x) = x`,
`arg(x) = pi unit_step(-x)` (x != 0), `abs' = sign`, `sign' = 0` and `unit_step' = 0` away from 0, `clip' = 1` on
(lo, hi) and 0 outside, `rescale` slope (d - c)/(b - a); `chop`, `rationalize`, `set_precision` have none.

### Tests

Unit 17 (8 new: simplest rationals against brute force on 2000 intervals; exact values and intervals of machine
reals; digits; the closeness rule; chop thresholds including an exponent of -10^6; ball `arg` in every quadrant and on
the cut; real signs). `tests/numbers.rs` 14: parts of numbers and terms, `arg`/`abs`/`sign` of closed terms,
precision and accuracy, `set_precision`, `rationalize`, `chop`, clip/rescale/unit_step, `digits:`, `n` through `abs`
and powers, for exact, rational, ball and machine inputs, lists, symbols and failures; random properties (60 each:
`re + im*im_part = z`, `z conjugate(z) = abs(z)^2`, `n(arg(z))` against atan2, `rationalize(x, dx)` minimal by brute
force); iterative walks over 10^4-element lists and 10^4-deep nests.

### Known gaps

1. `pow(ball, ball)` stays unevaluated in general (belongs in `mc-arith` or the engine's `pow` native: an exactly
   integral ball exponent to `pow_int`, others to `elementary::pow`); inside `n` exact exponents stay exact.
2. `rationalize` of a 10^6-digit ball is one continued-fraction step per partial quotient (interruptible, slow).
3. `arg` of closed complex terms (`sin(1) + im*cos(1)`) stays unevaluated (no complex-ball elementary functions).
4. `precision`/`accuracy` of a term is the least over its numbers, not error propagation.
5. `arg(complex(-1.0, -0.0)) = -pi` (IEEE signed zeros).

## Item N0C (roadmap 07, N0 item 4: elementary functions on complex balls)

### What was built

- **`cball` (public):** `CBall { re: Ball, im: Ball }` (a rectangle) with exact `neg`, `conj`, `mul_i`, `div_i`, and
  `add`, `sub`, `mul`, `mul_real`, `square`, `div`, `recip`, `round`, `has_digits`, `is_real`, `contains_zero`;
  functions `exp`, `log`, `sqrt`, `sin`, `cos`, `sin_cos`, `tan`, `sinh`, `cosh`, `sinh_cosh_c`, `tanh`, `atan`,
  `asin`, `acos`, `pow_int`, `pow_rat`, `pow` (complex exponent), `abs`, `arg` (moved here from `parts`). Each is a
  ball expression of the real ball functions of the parts, as in Arb's `acb`:

  | function | formula |
  |---|---|
  | exp | e^a (cos b + i sin b) |
  | log | log(a^2 + b^2)/2 + i arg z (log\|b\| on the imaginary axis) |
  | arg | atan(b/a), atan(b/a) ± pi, ±pi/2 - atan(a/b) by quadrant; exact 0 / pi on the real axis |
  | sqrt | t = sqrt((\|z\| + a)/2), t + i b/(2t) for a >= 0; t = sqrt((\|z\| - a)/2), \|b\|/(2t) + i sign(b) t for a < 0 (no cancellation) |
  | sin, cos, sinh, cosh | products of the real sin/cos and sinh/cosh (one exp) of the parts |
  | tan | (sin 2a + i sinh 2b)/(cos 2a + cosh 2b); tanh z = -i tan(iz) |
  | atan | (log(1 + iz) - log(1 - iz))/(2i), with -log2\|z\| guard bits |
  | asin | -i log(iz + sqrt(1 - z^2)) after reducing to Re z >= 0 >= Im z (asin(-z) = -asin z, asin(conj z) = conj asin z), where the sum does not cancel; acos = pi/2 - asin |
  | z^n | binary powering with log2 n guard bits; negative n by a reciprocal |
  | z^q, z^w | half-integers through sqrt; otherwise exp(w log z) with \|w log z\| guard bits; 0^w = 0 for Re w > 0 |
- **`Fun::cball`**: the dispatch over the twelve functions.
- **`numeval`**: complex numbers (`Node::CNum`) and (-1)^q units are numeric nodes; islands evaluate with
  `Graph::eval_complex` (refinement and the digits criterion as for real balls); `Graph::build_with_vars` +
  `Graph::eval_complex_at` evaluate an *expression in given variables* on complex balls without building number
  terms (for N2); `ball_power(&Num, &Num)` gives principal ball powers; `cball_term`. The (-1)^q leaf path of
  `numericize` is gone (those units are islands now).
- **Natives**: the elementary functions evaluate complex-ball arguments (`sqrt` included); a real ball outside a real
  domain (`log(-2.5~20)`, `asin(2.0~20)`) gives the principal complex ball.
- **`mc-engine` (outside this crate, 11 lines in `src/arith.rs`, native `Pow`):** when `Arith::pow` leaves a power
  of numbers with a ball unevaluated (B.13), it calls `mc_numeric::numeval::ball_power` under the stop flag's scope.
  So `z^(1/2)`, `z^(1/3)`, `z^z` for a complex ball z, `2.0~30^(1/3)`, `2^(1.5~30)`, `(-2.0~30)^(1/3)` evaluate
  (closes item L's request 3 and 1C's gap 1).

Examples (release REPL):

    n(exp(1 + 2im), 30)     complex(-1.1312043837568136384312552555107947, 2.471726672004818927616930893551665)
    n(sqrt(1 + 1im), 30)    complex(1.0986841134678099660398011952406784, 0.45508986056222734130435775782246857)
    n(log(-2 + 1im), 30)    complex(0.8047189562170501873003796666130938, 2.6779450445889871222483871518182885)
    n(atan(1 + 1im), 30)    complex(1.0172219678978513677227889615504829, 0.4023594781085250936501898333065469)
    n((1 + 2im)^(1/3), 30)  complex(1.219616507971757622571496581351432, 0.4717112677893890047819389461677654)
    n(log(-2), 20)          complex(0.693147180559945309417232, 3.14159265358979323846264)
    n(asin(2), 20)          complex(1.570796326794896619231322, -1.316957896924816708625046)
    z = n(1 + 2im, 30); z^z complex(-0.22251715680177260839710476114667904, 0.10070913113607538371230775446157579)
    n(exp(1 + pi*im), 30)   complex(-2.71828182845904523536028747135266, ball(-2.6036779231074204528e-35, 6.55e-35))
    exp(im*pi)              -1   (exact identities run first, unchanged)

### Decisions

- **N0C.1** Complex ball functions are ball expressions of the real ball functions of the parts (table above), so
  every radius is propagated by construction (rigorous, not tight: the parts are treated independently). Functions
  take p bits, work with 8-16 guard bits (more for small arguments of atan/asin and large |w log z| in powers) and
  round to p. Natives evaluate complex-ball arguments at the larger part precision.
- **N0C.2** Branches are the principal ones of `machine::C64` (same formulas): log, sqrt and non-integer powers cut
  along the negative real axis, atan along the imaginary axis beyond ±i, asin/acos along the real axis beyond ±1.
  Balls have no signed zero: an exact zero imaginary part is the upper side (`log(-2) = log 2 + i pi`, as the machine
  functions for +0.0; `asin(2) = pi/2 - 1.317i` as `C64::asin`). A region that touches a cut with an inexact part
  (imaginary part 0 ± r on the negative axis), contains a pole (tan) or a singular point (log 0, atan(±i)) has **no
  value** (`None`; the call stays unevaluated; `n` refines and may report `precision_not_reached`). This is Arb's
  analytic mode, what rigorous integration needs (the function is not analytic there). A covering ball (imaginary
  part [-pi, pi]) was rejected: it encloses the principal values, but would let integration bounds treat a
  non-analytic integrand as analytic.
- **N0C.3** A real argument (exact zero imaginary part) inside the real domain uses the real ball function, so real
  results keep an exact zero imaginary part and print as real balls; exact zero parts stay exact through the
  arithmetic.
- **N0C.4** `n` evaluates complex islands; a real island whose real evaluation fails where the complex one succeeds
  (an argument provably outside a real domain: `log(-2)`, `asin(2)`, `sqrt(1 - pi)`) continues with complex balls,
  consistent with L.9 for machine reals (was `precision_not_reached`); the natives do the same for a real ball
  (was: the call stays; `tests/limits.rs` updated). `is_real` decides `false` once the imaginary part excludes 0
  (`is_real(log(-2))`, `is_real(exp(im))`; was undecided); comparisons and signs ignore complex islands (as before);
  `rationalize` skips them.
- **N0C.5** A complex ball has d correct digits when each part has, or when its radius is below 10^-d times a lower
  bound of the other part (which has d digits): a tiny part next to a large one is accepted relative to |z|, and
  prints as `ball(m, r)` (`n(exp(1 + pi*im), 30)` above).
- **N0C.6** Powers: integer exponents by binary powering, half-integers through sqrt, other rationals and complex
  exponents as exp(w log z); `e^w = exp(w)`; `0^w` is 0 for Re w > 0 and has no value otherwise; through the
  engine's `pow` for numbers (`ball_power`), inside `n` for islands.
- **N0C.7** `abs` of a region that may contain 0 is the enclosure [0 ± (|a| + |b|)] (always a value); elsewhere
  sqrt(a^2 + b^2).
- **N0C.8** `Graph::build_with_vars(st, ar, nm, t, &[x, ...])` turns given symbols into variable nodes; such islands
  have no real-ball value and NaN machine values, and are meant for `eval_complex_at`.

### Tests (`tests/complex_balls.rs`, 8 tests; 2 unit tests in `cball`)

- Containment: 200 random arguments per precision (40 in debug) at 30 and 100 digits, parts (2u - 1) 10^v with v in
  [-1.5, 1] plus 64 random low bits, one in eight on an axis, radius zero, one unit at p, or relative 2^-20; for each
  of 18 operations (12 functions, z^7, z^(1/3), z^(-5/2), z^(0.75 - 1.25i), abs, arg) the check value at **3p bits**
  at the midpoint and the four corners lies inside the result (for an exact result part, the check value contains
  it). The check values are independent where possible: exp, sin, cos, tan, sinh, cosh, tanh from a Taylor-series
  exp written in the test (complex ball arithmetic, its own tail bound); log, sqrt, atan, asin, acos, z^(1/3) are
  checked at 3p to invert those check values and to lie in the principal range. Exact arguments must give more than
  p/2 correct bits. At most 20% misses per operation (poles, cuts, wide regions). A mutation shrinking exp's real
  radius by 8 is caught at once.
- Machine agreement: 1000 random points in [-3, 3]^2 and the cut points (-2, 2, -3, -0.5, 1.5 on the real axis, ±2i)
  for all 12 functions, against `Fun::machine` at 15 digits (relative 1e-12, 1e-13 on the cuts).
- Identities at 30 and 100 digits on 150 random arguments: exp(log z) contains z, sqrt(z)^2 contains z,
  sin^2 + cos^2 and cosh^2 - sinh^2 contain 1, tan overlaps sin/cos (overlap for regions).
- Branch cuts at the API: on the exact axis from above; regions across the negative axis (log, sqrt, arg, z^(1/3)),
  across the asin/acos and atan cuts give `None`; just above/below the cut gives the two sides; points (log 0,
  atan(±i), tan(pi/2), 0^w); abs near 0.
- Through the evaluator: the examples above, exact values (`exp(im*pi) = -1`, `exp(im*pi/2) = im`,
  `log(-1) = im*pi`, `sqrt(-4) = 2im`), complex balls as arguments (`exp(z)`, `sqrt(z)`, `z^(1/2)`, `z^z`,
  `exp(log(z))`, `n(abs(z))`, `arg(z)`), real balls outside a domain, machine values unchanged, `is_real`, a cut with
  an inexact imaginary part (`log(-2 + y*im)` with y = 0 ± r stays), the tiny-part criterion, 1000 digits.
- Interruption: a raised flag before `n(f(3 + 5im), 10^4)` for exp, log, sin, atan, asin gives
  `failure(interrupted, ...)` at once; raised 50 ms into `n(atan(1 + 2im), 10^6)`, `n(log(-2 + 1im), 10^6)`,
  `n(exp(1 + 2im)^(1/3), 10^6)` (with the evaluator's interrupt) each stops well within the 5 s bound (the whole
  test takes 0.2 s); afterwards `n(exp(1 + 2im), 20)` is correct, and 10^4 digits complete.
- Expressions in variables: exp(x) sin(x) + x^(1/3) + abs(x) pi through `build_with_vars`/`eval_complex_at` at
  exact points and a region (corners checked), 50 digits; no argument or a region across the cut gives `None`.

### Measurements (busy 4-core machine, release, `examples/cball_bench.rs`; z = 1.3 + 2.7i with 117-bit parts; in
parentheses the real ball function of the real part at the same precision)

| function | 30 digits | 100 digits | 1000 digits | 10^4 digits |
|---|---|---|---|---|
| exp | 53 µs (24 µs) | 99 µs (46 µs) | 1.1 ms (0.46 ms) | 39 ms (9.4 ms) |
| log | 116 µs (44 µs) | 280 µs (132 µs) | 2.9 ms (1.4 ms) | 81 ms (32 ms) |
| sin | 86 µs (28 µs) | 108 µs (51 µs) | 1.2 ms (0.62 ms) | 44 ms (30 ms) |
| cos, tan | 59 µs | 110 µs | 1.2 ms | 42-44 ms |
| sqrt | 3.3 µs (0.6 µs) | 4.8 µs (1.6 µs) | 46 µs (17 µs) | 4.0 ms (1.5 ms) |
| atan | 341 µs (66 µs) | 586 µs (136 µs) | 5.0 ms (1.3 ms) | 134 ms (45 ms) |
| asin, acos | 235 µs | 360-570 µs | 2.6 ms | 72 ms |
| sinh, cosh, tanh | 53-58 µs | 105-152 µs | 1.1-1.3 ms | 40-51 ms |

pow(z, w) and z^(1/3) at 100 digits: 0.5-0.6 ms. Through the evaluator (first call, printing excluded):
`n(exp(1 + 2im), d)` 0.24 / 0.13 / 0.91 / 32 ms at d = 30 / 100 / 1000 / 10^4; `n(log(1 + 2im), d)`
0.24 / 0.50 / 3.1 / 70 ms; `n(sin(1 + 2im), d)` 0.11 / 0.13 / 0.84 / 37 ms. A complex exp costs a real exp plus a
real sin_cos; log a real log plus a real atan (arg); atan two complex logs.

### Known gaps

1. Radii are rigorous but not optimal: parts are treated independently (no midpoint-radius split of the complex
   argument as in Arb's `acb_exp`), so wide input regions get radii a small factor larger than necessary.
2. Relative accuracy is lost near zeros of the result: log z for |z| near 1, sinh/sin imaginary parts for tiny
   imaginary arguments (sinh from one exp), acos near 1; `n`'s refinement compensates, the Rust API gives correct but
   wider balls.
3. `abs` of number terms is `mc-numtheory`'s native (it works in the engine; this crate's tests have only
   `n(abs(z))`); `arg`/`abs` of closed complex terms without `n` stay unevaluated (1C gap 3).
4. Large imaginary parts: tan/tanh, sin/cos with |Im z| beyond 2^40 have no value (exp's range, L.5).
5. No covering mode across cuts (N0C.2); a caller needing both sides must split the region.
6. Found in `mc-eval` (not fixed here): a term evaluated while the stop flag is raised keeps its
   `failure(interrupted, ...)` result in the session: after `n(exp(1 + 2im), d)` evaluated with the flag raised,
   `1 + 2im` itself evaluates to `failure(interrupted, 2im + 1)` for the rest of the session (the test uses other
   literals).

### What N2 can call

- Points: `mc_numeric::cball::{exp, log, sqrt, sin, cos, tan, sinh, cosh, tanh, atan, asin, acos}(z: &CBall, p: u64)
  -> Option<CBall>`, `sin_cos`, `sinh_cosh_c` (pairs), `pow_int(z, &Int, p)`, `pow_rat(z, &Rat, p)`,
  `pow(z, w: &CBall, p)`, `abs(z, p) -> Option<Ball>`, `arg(z, p) -> Option<Ball>`, `Fun::cball(f, z, p)`.
- Expressions: `let g = Graph::build_with_vars(st, &ar, &Names::new(st), expr, &[x]); let is =
  g.island(g.node(expr)?);` then `g.eval_complex_at(&is, p, &[z]) -> Option<CBall>` per point (an enclosure of f
  over the region z; `None` where f has no analytic value on the region: a cut, pole or singular point).

## Round 12: values that flow between functions

What changed (across the crates involved; the decisions are numbered io-num.N):

- **Quantities** (`quantity.rs`): functions of the magnitude keep the unit (`abs`, `re`, `im_part`, `conjugate`,
  `rationalize`, `floor`, `ceiling`, `round`, `integer_part`, `fractional_part`; a quantity step such as
  `round(q, quantity(1, "Meters"))` is converted to q's unit first); `sqrt` and rational powers multiply the
  exponents of the unit names (`sqrt(quantity(16, "Meters"^2))` is `quantity(4, "Meters")`); `mod` keeps the unit,
  `quotient` and `sign` give numbers; `quantity(list, u)` is the list of quantities (io-num.1).
- **Comparisons** (`natives.rs` `decide`): quantities compare by their magnitudes in a common unit (a dimensionless
  unit against a number too), so `<`, `>`, `==` of quantities are decided; `around(x, d)` and
  `centered_interval(c, r)` compare as the interval of their spread, like `interval` (io-num.2). Two plain numbers
  after the unit conversion use the canonical comparison (`numeval::compare` refuses plain numbers).
- **Propagation** (`uncertain.rs`): a function whose derivative has no closed form (or no machine value at the
  centre) propagates `around` / `centered_interval` by the five-point difference quotient with h = 10^-3 max(1, |x|)
  (io-num.3); step functions (`floor`, `ceiling`, `round`, `integer_part`, `sign`, `unit_step`, `real_sign`,
  `unit_box`, `heaviside_theta`, `kronecker_delta`, `boole`) give their exact value when it is the same at both ends
  of every spread, else `indeterminate` (io-num.4); `around(quantity, quantity)` and `around(x, scaled(s))` (s |x|)
  give `quantity(around(m, dm), unit)` (io-num.5). `re`, `im_part`, `conjugate`, `arg`, `clip`, `unit_step` and
  `rationalize` now route these values through `uncertain::apply`.
- `atan(x, y)` for real numbers is `arg(x + i y)` (it was unevaluated); `log(1/n)` is `-log(n)` (io-num.6).
- mc-numtheory: `factorial(x)` of an inexact number is `gamma(1 + x)`; `binomial` of inexact numbers by the falling
  factorial (natural k) or by gamma functions (io-num.7).
- mc-special: the incomplete beta `beta(z, a, b)` and `beta_regularized` for z > 1 with a natural b (the polynomial
  integral continued past 1).
- mc-dist: data statistics of quantities are those of the magnitudes in the first element's unit, in that unit to
  the statistic's power (variance: unit^2, skewness: none) (io-num.8); lists with `around`/interval elements are
  summed through `add` (`apply(add, xs)`), so their uncertainty is carried (gap G53 asks mc-lists' `sum` for the
  same); statistics of a dataset are the association of the column statistics (io-num.9); support conditions are
  written `x > 0` / `k >= 0` and the Poisson distribution function is `gamma_regularized(1 + floor(k), m)`;
  `moment(d, r)` for a symbolic order: chi-square, gamma, exponential, beta, log-normal and uniform closed forms.
- mc-linalg: `sparse_array(automatic, dims, default, [1, [offsets, columns], values])` (the compressed row form)
  is read, built dense like the rule form; `sparse_array(rules, n)` with list positions is n x ... x n (io-num.10);
  results whose entries hold `around`, intervals or quantities are evaluated again so their arithmetic is carried
  out (io-num.11); `norm(x)` of a scalar is `abs(x)`; `matrix_function(f, m, method: ...)` accepts the option and
  ignores it (io-num.12); `eigenvalues(m, k)`, `eigenvectors(m, k)`, `eigensystem(m, k)` take the first k (k < 0
  the last).

- Interpolation (gap G68): `interpolation(data, interpolation_order: k)` is the local polynomial of degree k
  through the k + 1 nodes nearest the interval holding x (for k = 3 the cubic through two nodes on each side,
  moved inside the table at its ends; order 1 is `method: linear`), as `interpolated(xs, ys, k)`; a fourth
  argument of `interpolated` is a derivative order, and mc-calculus turns `derivative(n)(interpolated(...))` into
  it, for the spline (exact derivatives of its cubic pieces) and the local scheme (`spline::local`, Newton form)
  (io-num.13). Compiled code takes only the plain spline; the others stay with the interpreter.
- Saving values (gap G75): `put(value, file)` writes the value's input form after a line `#! mathcas 2 value`,
  `get(file)` reads it back with `to_expression`; an `interpolating_function` (data in the solving session) is
  written as the `interpolation` through 16 values per solution step, at least 2049 (io-num.14). Chosen over a
  native `export_data` format: a value file is text the user can read, and `put`/`get` say what they do.

Tests: `tests/interop.rs` in mc-numeric, mc-dist, mc-linalg and mc-io.

## Round 16: the interrupt in the constants

- **C9B2.num1 The length search of the constants' series is linear.** `log2_factorial_lower(n)` summed
  `floor(log2 k)` over `k <= n`, and `e` (and the exponential and sine series of `elementary`) search the least `n`
  with a large enough bound by calling it for `n = 2, 3, ...`: quadratic, about 10^11 steps for 10^6 digits of `e`,
  with no stop poll, so `n(e, 10^6)` ignored the interrupt for minutes. It is now a closed form over bit lengths
  (64 steps). Test: `consts::tests::interrupt_stops_constants` raises the stop flag 0.3 s into each constant at
  10^6 and 10^7 digits and checks each returns within 5 s; `series::tests::log2_factorial_closed_form` checks the
  closed form against the sum. (`n(pi, 10^6)` itself finishes in about 2 s and is interrupted promptly at 10^7.)

## Round 19 fixes

- **f19.1** `n(e, d)` of an exact zero (Q19-15): when the ball at the precision cap contains 0 and lies within
  10^-d of it, the value is the zero ball of that radius (`ball(0, 1.9e-1116)` for `(1 + sqrt(6))^2 - 2 (1 +
  sqrt(6)) - 5`), a zero with an absolute accuracy; it was
  `failure(precision_not_reached, ...)`, which broke every residual check `n(lhs - rhs, d)`. The callers that read
  that failure as a zero (mc-sums `verify`, mc-calculus `portfolio`) accept the ball as well.

## Round 20

- `exp` of a ball entirely below -2^40 is [0 +- 2^-(2^24 + p)] (the exact exponent is not representable): far tails of
  decaying integrands stay numeric, so tanh-sinh ranges are no longer cut short (`1/k^3 + exp(-k)` on [17, inf)).
- `expm1` and `log1p` raise the working precision by -log2 |x| (they used the ball's relative radius, which is
  infinite for exact balls, and lost up to -log2 |x| bits).
- `acosh` on (-1, 1) (and `asech` through it) is i acos(x): the real part a zero ball without radius.
- `is_positive`/`is_negative` of a non-real number are false (the plain native; mc-simplify's assumption wrapper
  still answers first).
- `log1p`/`expm1` of |x| < 2^-(p/2 + 10): x -+ x^2/2 with the remainder |x|^3 in the radius (no working precision
  of -log2 |x| bits, which made tiny arguments, as in the polylog accuracy tests, very slow).

## Round 22

- `formula_lib::OneArg` implements `machine_complex` and `ball_complex` (the reciprocal and inverse circular and
  hyperbolic functions, `sinc`, `gudermannian`, `inverse_gudermannian`, `log10`, `log2`): `n(cot(1 + im))` was NaN
  (engine B21-3) and `n(cot(3 + 2 im), 30)` failed (engine B21-6). acosh and atanh keep the logarithm forms, asinh is
  asin(iz)/i; branch cuts as before.
- `numeval::eval_machine`: a library function whose machine value is NaN is evaluated from the exact machine
  arguments at 192 bits and rounded when 14 digits are certain.
- `elementary::{TailSafe, tail_safe}` (R20-17): see the mc-special notes; only for quadrature.

## Round 24

- `numeval::island_value` (B23-29): `n(e)` of an exact closed form that contains a sum is also evaluated in balls to 17
  digits; where the machine evaluation differs by more than 4e-15 relative the correctly rounded value is used
  (`n(polygamma(5, 981))` was 0.0).
- `elementary::sin_cos_small` (R22-09, part): |r| < 1/8 by the Maclaurin series, so sin of a tiny ball keeps its
  relative radius. `x/(exp(x) - 1)` near 0 (the rest of R22-09) needs an expm1 form: not done.


## Round 27

- `formula.rs::numeric_arguments`: a list of exact numbers (the parameter lists of `meijer_g`) stays exact when an argument is
  inexact, instead of becoming a list of machine numbers or balls at the precision of z (`meijer_g(..., 1.0e5)` summed terms of
  e^140 with parameters rounded to 53 bits: X26-6). It returns `None` when nothing was converted.
- `numeval.rs::numericize`: the parameter lists of `meijer_g` whose entries are all exact numbers are not numericized.

## Round 29: constants through the lists library

`n` (natives.rs `NumericN`) sends `khinchin` and `champernowne_number(...)` to `b29_n_constant(x, digits)` of
`lib/lists.mc` (digits 0 = machine precision); everything else is unchanged. They are not `Const`s: inside a larger
expression `n` does not know them yet.

## Round 31

- `fourier`/`inverse_fourier` of a machine matrix with integer b: F_rows . U . F_cols with DFT matrices built from the
  n twiddle factors (`nl_fourier_matrix` in lib/numerics.mc), native matrix products instead of list arithmetic through
  the mixed-radix recursion (O(len^2) interpreted list operations at prime lengths): a 51 x 101 transform 4.2 s -> 0.16 s.
  Arbitrary-precision data keeps the recursion.

## Round 32

- `sin`, `cos`, `tan` of `asin`, `acos`, `atan` of an exact argument are algebraic (`exact.rs`, `trig_of_inverse`):
  `cos(atan(4/3))` is 3/5, `sin(atan(x))` is `x/sqrt(1 + x^2)`, `tan(acos(x))` is `sqrt(1 - x^2)/x`; the poles (y^2 = -1,
  y^2 = 1) and inexact arguments are left to the evaluator. `from_polar_coordinates(abs_arg(3 + 4 im))` is `[3, 4]`.

## Round 36

- `centered_interval(c, r)` of a function of one uncertain argument is now the tight image of [c - r, c + r]: the
  midpoint and half-width of the end values when the derivative keeps its sign on a 9-point grid, else the interval
  image (`image`); sums and products of several centered intervals use the exact ball rules (the first-order estimate
  was not an enclosure for products). `centered_interval(x)` of a ball or number is its midpoint and radius.
- `rationalize(x, dx)`: among the fractions with the smallest denominator in [x - dx, x + dx], the one nearest x (0 when
  0 is inside).
- `surd` compiles (odd k: sign(a) |a|^(1/k); even k: a^(1/k)) and accepts the machine index `n` produces.
- An indeterminate argument makes an uncertain-number result indeterminate. `normal` of a centered interval or an
  `around` is its midpoint (hook in mc-algebra's `normal` wrapper).
- mc-dist: `median` of closed numeric constants (compared by `n`) and of `weighted_data`; the variance and standard
  deviation of complex data use |x - mean|^2.
- mc-roots: global `n_*` searches with at most three variables run a second, dense search (10^5 Halton points, 64
  starts) and keep its answer only when clearly better than the first.
- By design: optimizer answers agree with the closed form to machine precision; machine overflow stays `inf` (semantics 2); `n(e, d)` keeps guard bits so `precision` reports more than d.
- Gaps: machine arithmetic in `julia_set_iteration_count` loses the 1e-309 growth of an orbit near |z| = 1.
## Round 36

Unit `Micrograms` (`ug`) added to the quantity table. Gap: `centered_interval` powers of two intervals are a first-order estimate, not an enclosure.

## Round 36, later fixes

- `centered_interval` sums, products and powers with a complex centre or radius, with an `interval` mixed in, or with
  an uncertain exponent go through rectangular box arithmetic (`uncertain_boxes.rs`): a complex radius rx + i ry is
  the box of the two parts, products by interval arithmetic on the parts, z^w = exp(w log z) with log over a box away
  from 0 and the branch cut, every end widened outwards by a few ulps (Moore, Kearfott and Cloud, "Introduction to
  Interval Analysis", 2009). centered_interval(2, 1/100)^centered_interval(3, 1/100) was 8 +- 0.17704 (upper end
  8.17704 below 2.01^3.01 = 8.17749); it is now the corner hull [1.99^2.99, 2.01^3.01].
  interval([2, 3]) centered_interval(4, 1/10) is the tight [7.8, 12.3] (10.05 +- 2.25, where a ball product gives
  10 +- 2.3); the complex products and powers are also tighter (sampled values all inside, test).
- `interval` with an interval exponent (or a number base): the corners of x^y (monotone in each argument for x > 0);
  machine ends of computed pieces move outwards by one ulp (pi^3. gave 31.006276680299816 < pi^3).
- The mantissa form `centered_interval([[re part, im part], prec])` reads complex balls.
- Circular functions of an angle quantity (`cos(quantity(45.8, "AngularDegrees"))`) are of the value in radians;
  physical constants as units: SpeedOfLight, GravitationalConstant, StandardAccelerationOfGravity,
  ElementaryCharge, BoltzmannConstant, AvogadroConstant (CODATA 2018).
- Gap: centered intervals of functions of several uncertain arguments other than add/mul/pow still use the first
  order slope estimate.
## Round 36, algebra fixes

- log(complex_infinity) = inf; log2/log10 of an infinity is inf.

## Round 36, later fixes

- Functions of several `centered_interval`s other than sums and products no longer use the first-order slope
  estimate (not an enclosure where the slopes vary over the spreads). `centered_corners`: when every partial
  derivative has a closed form and keeps its sign on a grid over the box (5 points a side for two spreads, 3 for three
  or four), the function is monotone in each argument and its range is spanned by two corners (the same standard as
  the one-spread `centered_image`, which uses a nine-point grid). Otherwise `centered_by_balls`: the function of real
  balls (the special-function natives bound ball arguments rigorously); otherwise the call stays unevaluated.
  `beta(centered_interval(2, 1/10), centered_interval(3, 1/10))` is now the hull of beta(2.1, 3.1) and beta(1.9, 2.9);
  `bessel_j` of two spreads (no closed-form derivative in the order, no ball value) declines.
- The complex `centered_interval` sums, products, powers and `exp` give the tight box (sums, products and `exp`
  exactly the hull of the parts; powers an enclosure, checked against a 21^4 sample of the true image).

## Round 37: precision and accuracy goals

- **r37.1** `n(e, d)` takes a real number of digits: a positive finite machine real is rounded up (`n(x, 23.4)` is
  `n(x, 24)`, `n(x, machine_precision)` is `n(x, 16)`), as `set_precision` does. A ball's precision is binary, so
  the reported `precision` is that of the digits used, not 23.4 (semantics 2.3).
- **r37.1** `n(e, [p, a])`: precision goal p digits or accuracy goal a digits (absolute error at most 10^-a),
  whichever is reached first; either may be `inf`, not both. A result that meets the accuracy goal is rounded to
  the bits that accuracy needs (its binary exponent plus a log2 10 and the guard bits); a ball containing zero
  within 10^-a is the zero ball with that radius, so `n(sqrt(2) + sqrt(3) - sqrt(5 + 2 sqrt(6)), [inf, 100])` is
  0 to 100 digits after the point. The doubling of the working precision and its cap are those of `n(e, d)` (L.7);
  complex results need the goal on both parts. Tests: `mc-engine/tests/r37_f37_general.rs`.

## Round 38: values at infinity

- At `complex_infinity` (a point at infinity in no direction) exp, sin, cos, tan, their hyperbolic forms, atan and
  expm1 have no limit: `indeterminate` (exp tends to 0 along -inf and to inf along inf; the circular functions
  oscillate along the real axis). The reciprocal functions and `sinc` follow. `abs` is inf; `re`, `im_part`, `sign`
  are indeterminate; `conjugate` keeps the point; `arg` may be any angle: `interval([-pi, pi])`.
- At a real infinity sin and cos have no limit; their values accumulate on [-1, 1], given as `interval([-1, 1])`
  (tan: the real line; `fractional_part`: [0, 1] at inf, [-1, 0] at -inf). `exp(i inf)` runs round the unit circle:
  indeterminate. An interval argument at least one period long, or unbounded, gives sin and cos the range [-1, 1]
  and tan the real line (`uncertain::periodic_cover`); shorter ones go to the sampler as before.
- `conjugate(c y) = conj(c) conjugate(y)` and `sign(c y) = (c/|c|) sign(y)` for a number c (both are
  multiplicative), so `conjugate(-z)` is `-conjugate(z)`.
- Left: limits in non-real directions (`direction: im`, `complexes`). The series of `sign`, `abs` and `arg` use the
  real-variable derivative (sign' = 0), so a limit along i t would give wrong values for exactly these functions:
  `series(sign(1 - i t), t, 0, 2)` is `1 + O(t^3)`, while for real t the value is 1 - i t + ... . Complex
  centered intervals of the elementary functions are left too (no tight box enclosure for them yet).
  Tests: `mc-engine/tests/round38_infinity.rs`.
- With the multiplicative `conjugate` and `sign`, the other parts take a numeric factor c out of a product c y too,
  so that the results stay in step (`abs(c y) sign(c y)` is `c abs(y) sign(y)`, and `abs(u) sign(u) = u` still
  applies): `abs(c y) = |c| abs(y)`; `re(c y) = re(c) re(y) - im(c) im(y)` and `im_part(c y) = re(c) im(y) +
  im(c) re(y)` (real-linear); `arg(c y) = arg(y)` for c > 0 only (a negative or complex c turns the point and the
  sum of the angles may leave (-pi, pi]). Tests: `mc-engine/tests/round38_integration.rs`.
