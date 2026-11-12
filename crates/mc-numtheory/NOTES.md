# Item J: `mc-numtheory`

Status: built. Release tests pass (63
tests across 6 binaries); debug tests pass under the guard (large sizes ignored in debug). Clippy and `cargo doc` clean
with `#![warn(missing_docs)]`. Dependencies: `mc-api`, `mc-arith`, `mc-core`, `mc-eval`, `rayon`; dev: `mc-engine`,
`mc-syntax`, `num-bigint`, `num-traits`.

## What was built

All algorithms are plain functions on `mc-arith`'s `Nat`/`Int`, each taking a `&mut dyn budget::Budget` (inside a
native that is `NativeCx`, so it charges fuel and polls the interrupt); `natives.rs` is a thin layer on top.

- **word**: Montgomery arithmetic on 64-bit words; deterministic Miller–Rabin below 2^64 (bases 2, 7, 61 below 2^32,
  Sinclair's 7 bases above); integer roots, Jacobi symbol, modular inverse, Tonelli–Shanks.
- **sieve**: odd-only segmented sieve with a precomputed pattern for 3 to 13; windows anywhere below 2^64 (survivors
  above 2^40 confirmed by Miller–Rabin); `PiTable` packing bits and counts in one word per entry, sieved in parallel.
- **count**: `prime_pi` by Meissel's formula (phi expanded with a wheel of 30030, truncated by the pi table, cached for
  z < 2^18; top-level subtrees in parallel rounds on the session pool); `nth_prime` inverts Riemann's R(x), counts
  exactly, then sieves a short window.
- **ring**: modular arithmetic for big odd moduli: `Mont<L>` (CIOS on fixed arrays) for 1 to 32 limbs, `Barrett`
  (`mc-arith` multiplication plus a Newton reciprocal) above; generic code through `with_ring`.
- **prime**: Baillie–PSW (trial division below 1000, strong base-2 test, strong Lucas test with Selfridge
  parameters) and `next_prime` (sieved windows).
- **gcd**: subquadratic half-gcd (J.13), with `mc-arith`'s Lehmer gcd below 200 limbs.
- **power**: 2-adic exact roots (J.6), valuations by repeated squaring of the divisor, a remainder tree for many word
  residues, `perfect_power` (J.7).
- **factor**: trial division below 2^16, perfect powers, then splitting by rho (Brent; words and big numbers), ECM
  (Montgomery curves, Suyama's parametrisation, stage 2 with D = 2310) and SIQS (Knuth–Schroeppel multiplier,
  Gray-code polynomials, single large prime, dense GF(2) elimination).
- **modular**: `pow_mod`, extended gcd, inverse, Jacobi for big numbers, smallest primitive root, multiplicative order
  (via Carmichael lambda), `euler_phi`, `divisors`, `divisor_sigma`, `moebius_mu`.
- **combin**: factorial by Luschny's split recursion with product trees (J.8); binomial by an exact small-k loop,
  Kummer prime powers for n ≤ 2^34, or falling factorial over k!.
- **digits**: divide-and-conquer conversion to any base, length in a base, exponent of a base, `from_digits`.

## Public API

- `pub fn install(k: &mut Kernel, h: &Heap)` (J.1).
- `pub const FUNCTIONS` (31): `mod` (optional offset d), `quotient`, `gcd`, `lcm`, `floor`/`ceiling`/`round` (each
  with an optional step), `abs`, `sign`, `numerator`, `denominator`, `factorial`, `binomial`, `integer_digits` (with
  base and length), `integer_length`, `integer_exponent`, `is_prime`, `next_prime` (with k; negative k goes backward),
  `prime`, `prime_pi`, `factor_integer`, `power_mod`, `primitive_root`, `euler_phi`, `divisors`, and `extended_gcd`,
  `from_digits`, `divisor_sigma`, `moebius_mu`, `jacobi_symbol`, `multiplicative_order`.
- Every algorithm module is `pub` (`gcd::gcd`, `factor::factor`, `power::perfect_power`, `count::prime_pi`, ...) for
  use on bare numbers by other crates.

## Tests

- Unit (35): words against trial division and known strong pseudoprimes; sieves and windows near 2^63 and 2^64;
  Meissel against the table and known values; nth prime; rings against num-bigint for 1 to 200 limbs; BPSW on Mersenne
  primes and composites; next prime against a num-bigint Miller–Rabin test; hgcd unimodularity; gcd against Euclid
  and Lehmer up to 12k limbs and on Fibonacci pairs; exact roots, valuations, remainder tree; rho, ECM, SIQS (20 to 46
  digits), the factor pipeline; modular functions against brute force for every modulus below 200; factorials and
  binomials; digits in 11 bases.
- `tests/functions.rs`: about 230 source-text cases (normal values, edge cases, failures, unevaluated calls, list
  threading).
- `tests/properties.rs`: random operands against num-bigint (quotient, mod, gcd, lcm, floor, `power_mod`, digits,
  lengths, factorials); factorisations multiply back to primes confirmed by a num-bigint Miller–Rabin test; `next_prime`/`is_prime` agree
  with that test.
- `tests/bench_cases.rs`: N5 (10^4 to 10^6, Legendre and digit-count property), N6 (k = 8, 10, 12), N7 (30, 45, 60
  digits), N9 (10^4, 10^5), N10's property, R12's Collatz loop, `primitive_root(prime(10^6)^(10^6))`.
- `tests/limits.rs`: eight heavy calls each stop with `Status::Fuel` on a 20 000 budget (together in 0.8 s); an
  interrupt from another thread stops a 79-digit factorisation; the session continues.

## Measurements (release, source text, median of 3, guarded run 18 s, peak 177 MB, busy machine)

| case | time |
|---|---|
| N5 factorial 10^4 / 10^5 / 10^6 (with `integer_length`) | 0.001 / 0.021 / 0.40 s |
| N6 `[prime_pi(10^k), prime(10^k)]`, k = 8 / 10 / 12 | 0.006 / 0.048 / 0.33 s |
| N7 `factor_integer` of semiprimes, 30 / 45 / 60 digits | 0.007 / 0.065 / 1.64 s |
| N9 `[quotient(c+5, b), gcd(c, a^2)]`, d = 10^5 / 10^6 | 0.09 / 1.35 s |
| `primitive_root(prime(10^6)^(10^6))` (23.9M-bit input) | 1.87 s (answer 6) |

Also: `prime_pi(2^47)` 0.84 s; SIQS alone at 60 digits 1.54 s; hgcd on 30k limbs 0.59 s (Lehmer 2.8 s); a 23.9M-bit
non-power rejected in 0.9 s.

## Decisions

- **J.1** `install(k, h)`: a `Kernel` alone cannot make symbols (matches `arith::install`).
- **J.2** One fuel unit is about 1000 machine operations, so the interrupt (polled once per 1024 units) is checked
  about every millisecond; parallel phases charge between rounds.
- **J.3** Unsupported argument kinds decline (unevaluated); exact division by zero gives `failure(division_by_zero)`;
  a result beyond 2^28 bits gives `failure(limit, memory, call)`, decided before computing; arithmetic natives are
  listable (`failure(shape_mismatch)` for unequal lengths).
- **J.4** `is_prime` is deterministic below 2^64 and Baillie–PSW above (documented as a probable-prime test).
- **J.5** Edge inputs: unevaluated for `factorial` of a negative integer, `primitive_root` with no root, `power_mod`
  with no inverse, `integer_exponent(0, b)`, `divisors(0)`. Conventions: `next_prime` of n < 2 is 2;
  `factor_integer(1)` is `[[1, 1]]`, `(0)` is `[[0, 1]]`, negative n gets `[-1, 1]`, rationals get negative exponents;
  `power_mod` and `mod` follow the floor convention for a negative modulus; `binomial` extends to negative integers
  (Kronenburg).
- **J.6** Exact k-th roots by 2-adic Newton lifting modulo 2^(bits/k), checked once at full size.
- **J.7** Perfect-power detection by residue filters (moduli q ≡ 1 mod 2k, Jacobi symbols for k = 2) through one
  remainder tree, plus floating-point candidates when the root is under 40 bits; survivors confirmed exactly.
- **J.8** Factorial by Luschny's split recursion.
- **J.9** `prime_pi` is capped at x ≤ 2^47 (the pi table up to x^(2/3) is about 200 MB); beyond, and for `prime(k)`
  past the cap, `failure(limit, memory, call)`.
- **J.10** Factoring pipeline (`factor/mod.rs`): ECM effort before SIQS about 10% of the sieve's expected time, scaled
  with the digit count.
- **J.11** SIQS up to 85 digits only (dense GF(2) elimination grows with the cube of the factor base); parameters tuned
  on the N7 numbers.
- **J.12** ECM on Montgomery curves with Suyama's parametrisation; stage 2 with D = 2310.
- **J.13** The half-gcd only applies unimodular matrices, so correctness never depends on how far top-part quotients
  reach; wrong final quotients are undone from a stored tail of 6, with a division-step fallback.

## Notes

- `NativeCx` has no access to `mc-arith`'s stop flag or `max_bits`, so single big `Nat` operations inside natives are
  not interruptible (loops poll between operations); `DEFAULT_MAX_BITS` is used.
- Natives using `rayon` rely on running inside the session pool.
- Shared names with other crates: `gcd`, `lcm` (also `mc-poly`). The engine installs `mc-poly` after this crate, so
  `mc-poly`'s `gcd`/`lcm` (which also handle integers) win; integer gcd then uses Lehmer, not this half-gcd (follow-up:
  dispatch by argument kind).

## Known gaps

- SIQS stops at 85 digits (no block Lanczos); above, only ECM (slow for balanced factors of 35+ digits).
- `prime_pi` capped at 2^47.
- Baillie–PSW on 10^4+-digit numbers takes tens of seconds (interruptible).
- The half-gcd constant is about 3–4 times GMP's.
- For a huge non-power the perfect-power filter costs about 1 s.
- Not supported: gamma-based `factorial`/`binomial` for inexact reals; modular square roots in `power_mod`; Gaussian
  integers in `mod`/`gcd`; symbolic `abs`/`sign` simplification. `mod`/`floor` on balls only when the floor is decided;
  `abs` of a ball containing zero returns a valid but wider enclosure.

## Item 9 (library and kernel gaps)

- `gcd` and `lcm` removed from registration and `FUNCTIONS` (owned by `mc-poly`, 9.1); new public
  `number_gcd_lcm(cx, args, lcm)` (integers and rationals by the half-gcd; declines otherwise). The test helper `Nt`
  also installs `mc-poly` (dev-dependency); `gcd(x, 2)` now gives `1`.

## Round 19 fixes

- **f19.1** `is_square_free` of polynomials (Q19-08): `gcd(p, d(p, x))` free of x for every variable (or the given
  ones), `modulus: m` through `polynomial_gcd`; `x^2 - 3` and `x^2 + 6 x + 6` were `false`. A non-polynomial kernel
  (`sin(x)`) is square free.

## Round 20

- `numerator`/`denominator` of a complex number with rational parts: over the lcm of the parts' denominators
  (`numerator(3/7 + im/11)` = 33 + 7 im, denominator 77); a factor `b^e` whose exponent is a product with a negative
  numeric coefficient goes to the denominator (`denominator(1/(a^b x))` = x a^b).

## Round 31

- `partitions_p` and `partitions_q`: a per-thread table of p(0..n) (pentagonal recurrence, extended on demand), and
  Q(n) = sum over pentagonal numbers of (-1)^k p(n - 2g) (prod (1 + q^k) = prod (1 - q^(2k)) / prod (1 - q^k)), so a
  table of Q(1..1000) takes 0.09 s (the coin table was O(n^2) bigint additions per call, a timeout).
- `multiplicative_order(a, n, [r1, ...])`: the least k >= 1 with a^k congruent to some r_i (a single residue allowed);
  the native is no longer listable in its third argument (lists in a and n still thread). The search walks the cycle of
  a, at most 2^24 steps (larger orders stay unevaluated).
- `number_digit` over a list of numbers is a rule in `mc-combinat/lib/combinat.mc` (two lines).

## Round 32

- `real_digits(x, b, n)` of a long expansion: the first n digits come from one integer division `floor(p b^s / q)` and a
  base conversion (bit groups for powers of two, the decimal converter for 10, word chunks otherwise) instead of long
  division with a map of remainders, which held a 125 KB remainder per digit (10^6 binary digits of pi ran out of
  memory). Rationals with a length use the same path (no 100000-digit cycle limit).
- `is_prime_power(n)`: the largest k with n a perfect k-th power by integer roots, then a primality test of the root
  (it factored n, hopeless for 10^300 + 3).

## Round 33

- `bit_and`, `bit_or`, `bit_xor`, `bit_not` work on integers of any size: sign-extended two's complement over the
  64-bit limbs (Knuth, TAOCP 4.1, bitwise operations on infinite-precision integers). Before, anything beyond 64 bits
  stayed unevaluated, which made `array_plot(integer_digits(nest_list(...bit_xor...)))` spin.
- `divisors` and `divisor_sum` thread over a list; `quotient(a, b, d)` is `floor((a - d)/b)`; `mod` and `quotient` of
  an irrational real constant (`mod(pi, 2)`, `quotient(e^e^e, pi)`) take the floor from the value at 30 digits
  (declined when the quotient lies within 1e-12 of an integer); `prime_pi` accepts and ignores `method:` and
  `progress_reporting:` (one sieve serves every method; `prime_pi(10^15)` and beyond still exceeds the limit).
- Not done: Gaussian-integer `mod`/`quotient`/`divisors`, `divisor_sum(n, ...)` with a symbolic n.

## Round 36

- `real_digits` of a machine real gives 16 digits (53 bits are 15.95 decimal digits); digits asked beyond are
  `indeterminate`. `integer_reverse(n, b, len)` pads to `len` digits and threads over lists.

## Round 36, later fixes

- `continued_fraction` of a machine real: the terms shared by x -+ ulp/2 (plus the last term where the binary value ends); of an exact constant with a count: precision doubled until the convergents stay inside it, beyond 20000 digits unevaluated. Gaussian `quotient`/`mod` (rounded quotient). `from_digits` with trailing indeterminate digits gives an inexact number.

## Round 36, later fixes

- `is_prime_power` of a Gaussian integer with parts below 10^6: the same rules in machine integers (least prime factor of the norm by trial division), without the evaluator; 250,000 values in 1.1 s (4.1 s before). Checked against a brute-force enumeration of the powers of the Gaussian primes in the box [-40, 40]^2 (1,346 values, identical).
