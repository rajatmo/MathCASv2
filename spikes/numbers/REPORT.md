# Spike 3: numbers

Code: [`spikes/numbers`](.). Final results: `results/final-{mul,cross,ball,const,pibreak,adapt}.md` (each
`--full <part>` with `--features v1`, on a mostly idle machine). Busy-machine results from development:
`results/full-partial-busy.md`, `results/quick-busy.md`.

## Question

Which big-integer multiplication strategy, and which ball representation (semantics 2.3 to 2.6), should v2's
number layer use? Concretely: an own limb layer with a number-theoretic transform against a library (num-bigint 0.4,
the only one available offline) and against v1's own big numbers; a radius type for balls; whether v1's big numbers
are reusable.

## Setup

- **own**: a new natural-number layer on 64-bit limbs (`nat.rs`, `ntt.rs`).
  - Multiplication: schoolbook, then subtractive Karatsuba on one scratch buffer, then a three-prime NTT. The NTT
    takes the operands' 64-bit limbs directly as coefficients, with three primes c 2^32 + 1 in (2^61, 2^62). Their
    product exceeds 2^183, so the CRT recovers every coefficient (at most n 2^128) for n < 2^55 limbs, and three
    transforms of length N cost less than two of length 2N with 32-bit coefficients. Butterflies follow Harvey:
    lazy residues in [0, 2p) and [0, 4p), Shoup's precomputed twiddle quotients, branch-free conditional
    subtractions. The forward transform is a recursive decimation in frequency and the inverse one in time (no
    bit-reversal pass; the halves become cache-resident), with Montgomery pointwise products and Garner's CRT. The
    primes and their generators are found and checked at first use.
  - Division: Knuth D for short operands, else a Newton reciprocal. The reciprocal is exactly
    floor((2^(2n) - 1) / d), built by recursion on the top half, one Newton step on truncated operands and a bounded
    correction. A divisor much longer than the quotient is truncated first; a `Divisor` caches its reciprocal.
  - Square root: recursion on the top half, one Newton step from above, a bounded correction.
  - Decimal conversion: divide and conquer over cached powers 10^(19 2^k).
- **num-bigint 0.4.8**: Karatsuba and Toom-3 multiplication, Burnikel-Ziegler division, divide-and-conquer
  conversion; no FFT.
- **v1**: v1's `bigint` (u32 limbs, inline i64 small values, Karatsuba from 48 limbs, Knuth division, quadratic
  `to_string`) and `bigfloat` (Chudnovsky pi, `exp` for e, `to_decimal`), behind the cargo feature `v1`. A path
  dependency would not do: cargo resolves optional path dependencies even with the feature off, so the crate could
  not build without the v1 repository (checked). Instead `build.rs` locates v1's `bigint.rs`, `bigfloat.rs` and
  `interrupt.rs` (read-only; `MATHCAS_V1_SRC` overrides the search) and the crate compiles them as modules. v1's own
  19 unit tests pass in this build.
- **Balls** (`ball.rs`): midpoint `Float` = sign, `Nat` mantissa, i64 exponent, rounded to nearest at p bits with an
  upper bound of the rounding error. Radius `Mag` = 32-bit mantissa, i64 exponent, rounded up, with rounded-down
  variants for lower bounds (divisors, square roots). add, sub, mul, div and sqrt use the standard enclosure
  formulas. Comparisons are decided only for disjoint balls or two exact ones.
- **Constants** (`consts.rs`): pi by Chudnovsky with binary splitting (P skipped on the right spine), e by the
  factorial series with binary splitting, generic over a backend trait so that own and num-bigint run the same
  code. Independent checks: Machin's formula for pi (quadratic, to 10^5 digits) and the continued fraction of e
  (matrix products by binary splitting).
- **Correctness**: every product is compared through a representation-independent fingerprint of its 64-bit limbs
  (all implementations, all sizes: all agree). The digit checks are listed under N1 and N2. The enclosure test
  computes exact dyadic or rational values at the end, middle and interior points of random input balls. 13 unit
  tests, among them division, square root and conversion against num-bigint, and a randomised check of the radius
  type's rounding directions.
- **Interruptibility**: binary splitting, chunked multiplication and division, the NTT (between primes), powering,
  conversion and the precision loop poll the stop flag; every correction loop has a proven bound and a cap. A
  watchdog raises this crate's flag and v1's after 60 s (benchmark protocol section 1).
- Machine: AMD Ryzen 7 7700X (16 threads), 31 GB, Windows 11; Rust 1.99.1, release build, single-threaded. In
  `--full`, cheap cases get a warm-up and at least 5 runs (about 2 s), the median is reported. The final numbers
  come from `results/final-*.md`.

## Results

Times in ms unless marked. Final run on a mostly idle machine (spikes 1b and 2 ran alongside on other cores, all
single-threaded): `results/final-{mul,cross,ball,const,pibreak,adapt}.md`, each `--full <part>` with the `v1`
feature.

### Multiplication of two n-digit integers

| n digits | own auto | own Karatsuba only | own NTT only | own schoolbook | num-bigint | v1 |
|---|---|---|---|---|---|---|
| 10^3 | 0.002 | 0.003 | 0.009 | 0.003 | 0.002 | 0.010 |
| 10^4 | 0.070 | 0.069 | 0.163 | 0.192 | 0.068 | 0.248 |
| 10^5 | 1.53 | 2.75 | 1.51 | 19.0 | 2.17 | 9.76 |
| 10^6 | 14.5 | 112 | 14.7 | 1915 | 62.3 | 403 |
| 10^7 | **137** | 4211 | 135 | | 1872 | 14796 |

All results agree. Peak heap at 10^7: 42 to 50 MB for every implementation; the first NTT of that size also builds
its twiddle tables (own auto: 130 MB, about 90 MB more, finding 6).

Crossovers (64-bit limbs; one limb is 19.3 digits):

- **Schoolbook to Karatsuba: 24 limbs.** The total over 64- to 1024-limb products is lowest at 24 limbs
  (300 us), within 1% at 20, within 4% from 40 to 64, 8% worse at 32 (noise), and 1.7 times worse at 8.
- **Karatsuba to NTT: 1536 limbs (about 30,000 digits)**; from there the NTT wins at every measured size. Below it
  the NTT's power-of-two lengths make a staircase: at 1024 limbs (product length exactly 2048) the NTT already
  wins, at 1280 it loses again.

| limbs | 256 | 512 | 1024 | 1280 | 1536 | 2048 | 4096 | 8192 | 16384 | 32768 |
|---|---|---|---|---|---|---|---|---|---|---|
| NTT / Karatsuba | 1.61 | 1.09 | 0.77 | 1.09 | 0.80 | 0.53 | 0.38 | 0.26 | 0.18 | 0.13 |
| num-bigint / own best | 1.03 | 0.92 | 1.14 | 0.87 | 1.02 | 1.56 | 2.10 | 4.26* | 3.64 | 4.94 |

*Outlier (3969 us in the preliminary run, ratio 2.76); the 12288-limb NTT time (3442 us, above the 16384-limb
3136 us at the same transform length) is noise of the same kind.

### Ball arithmetic

**Enclosure: 0 failures in 2,430,444 exact checks.** They cover add, sub, mul, div, sqrt and comparisons on random
balls at 24 to 700 bits with exact, tight and wide radii, plus 3000 chains of 30 operations against exact dyadic
values. 5310 operations were correctly undefined: a divisor that may contain zero, or the square root of a ball
that is not provably positive. Two deliberate mutations (dropping the product's rounding error; dropping the ra rb
term) are caught with 10,704 and 771 failures, so the test discriminates.

Radius bookkeeping, ns per operation, ball against bare (the same rounded midpoint operation):

| digits | add | mul | div | sqrt | v1 BigFloat mul / own ball mul |
|---|---|---|---|---|---|
| 10^2 | 194 / 190 (+3%) | 187 / 155 (+21%) | 398 / 352 (+13%) | 2060 / 2020 (+2%) | 1.6 |
| 10^3 | +4% | +1% | +1% | +1% | 3.8 |
| 10^4 | +1% | 0% | 0% | +1% | 3.6 |
| 10^5 | 0% | 0% | 0% | 0% | 6.7 |

At 10^5 digits own ball div takes 7.8 ms and sqrt 17.7 ms, against 132 ms and 1208 ms for v1's BigFloat (17 and
68 times).

### N1: digits of pi

| digits | own binary | own total | num-bigint total | v1 total | checks (all pass) |
|---|---|---|---|---|---|
| 10^4 | 2.18 | 2.97 | 2.32 | 8.85 | 50 leading digits; six nines at position 762; agreement with a run at d + 100; Machin (34.6 ms) |
| 10^5 | 50.3 | 89.6* | 96.2 | 734 | the same, Machin (3.3 s) |
| 10^6 | 760 | **1025** | 3459 | > 60 s (limit; probe `v1big`, not run in `--full`) | the same without Machin, plus the published digit counts of the first 10^6 decimals (99959, 99758, ..., 100106) |

"binary" is pi 2^p as an integer, "total" includes the decimal conversion. *The components of the same computation
in the breakdown run sum to 69.7 ms (68.8 in the preliminary run); 89.6 is a disturbed measurement.

Cost breakdown (ms), own (num-bigint):

| digits | series Q, T | sqrt(10005) 2^p | 426880 s Q | / T | to decimal (incl. 10^d product) |
|---|---|---|---|---|---|
| 10^4 | 0.727 (0.601) | 0.755 (1.11) | 0.152 (0.088) | 0.508 (0.198) | 0.816 (0.305) |
| 10^5 | 20.4 (17.9) | 18.0 (58.2) | 1.49 (3.01) | 9.30 (8.32) | 20.5 (8.60) |
| 10^6 | 363 (627) | 229 (2205) | 30.5 (108) | 129 (245) | 274 (262) |

All implementations agree on every digit.

### N2: 20 digits of e after position off

| off | own targeted | own full conversion | num-bigint targeted | v1 (exp(1), full conversion) | digits |
|---|---|---|---|---|---|
| 10^4 | 1.11 | 1.57 | 0.550 | 63.0 | 56743028597460014378 |
| 10^5 | 18.9 | 31.1 | 14.6 | 6222 | 15778308922988924267 |
| 10^6 | **242** | 400 | 407 | not run (see below) | 83747115156239682713 |

All agree with the full conversion and with the continued-fraction route (9.1 ms, 112 ms and 1.3 s); the leading
50 digits are checked. v1 at offset 10^6 is measured only by the opt-in probe `v1big`, which runs pi first: in the
preliminary probe, e had not finished when the probe was stopped at 120 s (from 6.2 s at 10^5 and a quadratic
conversion, minutes are expected).

### N11: n(e, 50) by doubling the precision from 183 bits

| expression | rounds | final bits | result | encloses the exact value | time / one evaluation at the final precision |
|---|---|---|---|---|---|
| E1 sqrt(10^60 + 1) - 10^30 | 3 | 732 | 5.000...e-31 | yes (squared bounds) | 3.0 (0.012 / 0.004 ms) |
| E2 Muller's recurrence, u_40 | 3 | 732 | 6.0009079941545270832407027114923527473913627837936 | yes (exact rational) | 2.5 (0.185 / 0.075 ms) |
| E3 pi minus its 51-digit truncation | 2 | 366 | 5.8209749445923078164062862089986280348253421170680e-51 | yes (pi to 200 digits) | 2.0 (0.014 / 0.007 ms) |
| E4 Rump's polynomial | 1 | 183 | -0.82739605994682136814116509547981629199903311578438 | yes (-54767/66192) | 1.0 |
| E5 sqrt(2)^2 - 2 (exact zero) | 7 | 11712 | failure(precision_not_reached), radius 2^-11709 | yes (contains 0) | 1.6 |

Correct bits per round: E1 0 (the ball contains zero), 165, 531; E2 undefined (division by a ball containing zero),
163, 529; E3 14, 197. Every ball at a too-low precision was wide or undefined, never wrong.

## Findings

1. **An own layer with an NTT beats the available library by a factor that grows above 10^5 digits, and matches
   it below.** At 10^7 digits own multiplication is 14 times faster than num-bigint (137 ms against 1.9 s) and 108
   times faster than v1 (14.8 s); at 10^6, 4.3 and 28 times. num-bigint's best algorithm is Toom-3, whose exponent
   1.46 loses to the NTT's n log n from about 30,000 digits. Below that the two are within 15% either way
   (num-bigint's Toom-3 wins by up to 13% between 500 and 1300 limbs, where own has only Karatsuba).
2. **The NTT's constant depends on avoiding branches and divisions.** A first version with fully reduced
   Montgomery butterflies and ordinary `if` reductions ran at about 18 cycles per butterfly. Harvey's lazy
   butterflies with Shoup twiddles and branch-free selects made the 10^5-digit product 2.6 times faster (4.08 to
   1.56 ms). Rewriting Karatsuba (which allocated per node) on one scratch buffer in subtractive form made it 1.4
   to 1.7 times faster, level with num-bigint at 10^4 digits.
3. **Fast multiplication reaches the constants only through fast division, square root and conversion.** pi to
   10^6 digits takes 1.0 s own against 3.5 s for num-bigint on the same algorithm; the square root is the largest
   gap (0.23 against 2.2 s). Two fixes gave a factor of 1.6 to 2.5 here: caching the reciprocal per divisor (radix
   conversion divides by each power of ten many times), and not multiplying the low zero limbs of v0 = vh 2^l in
   the Newton step. Even so, division costs about 5 multiplications and square root 10 to 12, against 2 to 3 for
   good implementations. Own conversion is still 2.4 times slower than num-bigint's at 10^5 digits (at par at
   10^6). num-bigint is 1.3 times faster on pi at 10^4 digits and 1.3 to 2 times faster on e's digits at offsets
   10^4 and 10^5, through lower small-number overhead and its mid-size division.
4. **Ball radius bookkeeping is free from 1000 digits and costs at most 21% at 100 digits.** The radius is a
   fixed-size type whose operations are a handful of u64 and u128 instructions. What made small balls slow at first
   was hidden big-number work: reading a mantissa's top bits by shifting a copy, and computing |m| - r exactly,
   which aligns a p-bit mantissa to the radius's exponent. Both now happen in the radius type with directed
   rounding: mul overhead went from +54% to +21%, div from +104% to +13%.
5. **A 32-bit radius mantissa with a 64-bit exponent is the right size.** Range rules out an f64 radius, not
   precision: a 10^6-digit ball has a radius near 2^(-3.3 10^6), and f64 stops at 2^-1074. A u64 mantissa would need
   u128 products in every radius operation, for precision a radius does not use. Deciding "the divisor excludes
   zero" on 32-bit lower bounds is conservative: a ball that excludes zero by less than 2^-31 relative counts as
   undefined, and its quotient would be uselessly wide anyway.
6. **Twiddle tables are the NTT's memory cost.** With Shoup quotients they take 32 bytes per point per prime for
   both directions: 96 MB for transforms of 2^20 points, kept per thread, against 42 MB for the product's own
   working memory.
7. **Doubling precision costs 1.6 to 3 times one evaluation at the final precision.** Sixty digits of
   cancellation (E1) or exponential error growth (E2) need three rounds. At these sizes (a few hundred bits,
   microseconds) an evaluation's cost hardly grows with precision, so k rounds cost close to k evaluations: E1 3.0,
   E2 2.5, E3 2.0. The geometric-sum bound of under 2 applies only once the cost grows at least linearly with
   precision. An exact zero can never meet a relative accuracy target, so the cap of semantics 2.6 is what ends E5.
   Recognising zero is an exact-arithmetic task (N12, zero testing), not a precision one.
8. **Targeted digit extraction is an algorithmic win** (N2). The digits of e at an offset come from
   ((num 10^off) mod den) 10^k / den on the binary-splitting fraction, with 20 guard digits that must not all be 9.
   This is 1.4 to 1.7 times faster than converting all digits, and 1.7 times faster than num-bigint at offset 10^6
   (242 against 407 ms), though num-bigint is faster at 10^4 and 10^5. v1's route (exp(1) in its BigFloat, then a
   quadratic full conversion) takes 6.2 s at offset 10^5 against 19 ms, and did not finish within the probe at 10^6.
9. **v1's big numbers are correct but not reusable as v2's number layer.** Every v1 result agreed exactly with own
   and num-bigint. But v1 has only Karatsuba on u32 limbs (28 to 108 times slower at 10^6 and 10^7 digits),
   schoolbook division, a quadratic `to_string`, and a square root whose Newton steps use that division. At 10^6
   digits pi exceeds the 60 s limit, and e at an offset did not finish within the probe. Worse for an interruptible system, those
   quadratic loops do not poll its interrupt flag: after the watchdog fired, v1 kept the pi case running for up to
   another minute. Its BigFloat is 3.6 to 68 times slower than own balls at 10^3 to 10^5 digits, and its numeric
   type is significance arithmetic, which semantics 2.3 replaces with balls. The design does carry over: inline
   small integers (decision 1.2 already adopts them), Chudnovsky binary splitting, interrupt polling.

## Decisions

| # | Decision | From |
|---|---|---|
| 3.1 | v2 has its **own big-integer layer** on 64-bit limbs, not a library: schoolbook below about 24 limbs, subtractive Karatsuba on a scratch buffer to about 1500 limbs, then the three-prime NTT. The thresholds are constants measured on the benchmark machine. | findings 1, 2 |
| 3.2 | The NTT is the **three-prime, 64-bit-coefficient scheme**: primes c 2^32 + 1 in (2^61, 2^62), Harvey lazy butterflies with Shoup twiddles and branch-free reductions, recursive DIF forward and DIT inverse without bit reversal, Garner CRT. | findings 1, 2 |
| 3.3 | Division, square root and radix conversion are **Newton methods on top of multiplication**, with cached reciprocals for repeated divisors and truncation of divisors longer than the quotient, so that they inherit the NTT's scaling. Reaching 2 to 3 multiplications per division is required work before milestone numbers are taken. | finding 3 |
| 3.4 | **Ball midpoint**: sign, own-layer mantissa, i64 exponent, rounded to nearest at p bits with an explicit rounding-error bound. **Ball radius**: 32-bit mantissa and i64 exponent, rounded up, with rounded-down variants for lower bounds. Radius operations never touch big numbers. | findings 4, 5 |
| 3.5 | The quotient by a ball and the square root of a ball are **undefined unless the divisor is provably non-zero (the operand provably positive), decided on 32-bit radius bounds**. Comparisons are decided only for disjoint balls or two exact ones (semantics 2.5 as written). | findings 5, 7 |
| 3.6 | `n(e, d)` refines by **doubling from d log2(10) + 16 bits up to a cap**. A ball that still contains zero at the cap gives `failure(precision_not_reached, best_so_far)`, never a zero. | finding 7 |
| 3.7 | Digits at an offset (N2, v1's digits-after-the-point case) are computed by **modular reduction of the exact binary-splitting fraction** with guard digits, never by converting all digits. | finding 8 |
| 3.8 | **v1's big-number code is not reused.** Its test vectors and its inline small-integer representation are; its significance arithmetic is replaced by balls. | finding 9 |
| 3.9 | Every number algorithm with input-dependent cost polls the stop flag inside its quadratic or long-running loops. v1 shows that polling only at the top is not enough. | finding 9 |

## What it rules out

- num-bigint, or any library without FFT multiplication, as the number layer: 4 to 14 times slower at 10^6 to
  10^7 digits, and its division and conversion cannot use a faster multiplication plugged in from outside.
- v1's big numbers as a base for v2: no FFT, quadratic division and conversion, loops that ignore interrupts.
- An f64 radius (range), and a u64-mantissa radius (cost without benefit).
- Significance arithmetic (v1's BigReal) for high-precision numbers: balls cost nothing extra from 1000 digits.
- Converting all digits in order to read digits far after the point.

## Follow-ups

- **Division and square root at 2 to 3 multiplications**, instead of about 5 and 11: a middle product or
  wrap-around in the Newton step, Newton on the reciprocal square root, and Burnikel-Ziegler for the mid-size range
  where num-bigint still wins.
- **Smoother NTT lengths**: lengths 3 2^k (primes with 3 | p - 1) or a truncated FFT, to remove the power-of-two
  staircase (up to 2 times at the bad sizes); transform reuse for repeated operands (binary splitting, powering).
- **Twiddle memory**: keep only the largest level's table (the smaller levels are strided views of it) or compute
  the deep levels on the fly, and bound the per-thread cache (finding 6).
- **Toom-3** between Karatsuba and the NTT (num-bigint's 12% band), and lower per-operation overhead for small
  numbers in binary splitting (own is 1.3 times slower than num-bigint at 10^4 digits of pi); immediates
  (decision 1.2) cover part of it.
- Not measured here: elementary and special functions on balls (N3, N4), exact factorials by product trees (N5),
  division and gcd at 10^5 to 10^7 digits (N9), rational binary splitting (N10), comparison refinement (N12).
- **Parallel NTT**: the three primes are independent transforms (semantics 18: three threads for one product).
