# mc-limit: notes

## Round 34

- Split out of `mc-calculus` with no behaviour change: limits and asymptotics (limit, max_limit, min_limit, discrete_limit, asymptotic); bounds.rs (real bounds, signs of constants, substitution) and switches.rs (functions defined by cases, their cut points and branches), moved from the definite integrals because limits use them too. Its functions are listed in `mc_calculus::FUNCTIONS`
  (one table for the area); the history of the code is in `mc-calculus/NOTES.md`.

## Round 36, later fixes

- heaviside_theta is a switch like unit_step for one-sided limits.
- Limits at complex_infinity of rational functions by degrees; other functions not decided (was indeterminate).

## Round 37

- `surd(u, k)` and `cube_root(u)` in the limit walk: an odd root is continuous and increasing on the reals (a finite
  limit maps with its side, an infinite one stays infinite); an even root only at a positive limit.
- An undecided `limit` whose `max_limit` is provably above its `min_limit` is `failure(no_limit, ...)` (sin(1/x) at 0,
  cos(x) at infinity); `max_limit`/`min_limit` that find nothing take the limit when it exists.

## Round 38: more special functions at infinity

- Limits at +-inf from the leading asymptotic terms: Scorer functions (DLMF 9.12.27-9.12.31), the zeros of Airy and
  Bessel functions as their index grows (9.9.6, 10.21.19), the subfactorial, spherical Hankel functions (10.52), Anger
  and Weber functions (11.11.8-11.11.10), parabolic cylinder D (12.9.1), Struve H by its order (11.6.1: 0 for nu < 1,
  2/pi at 1, inf beyond; a symbolic order declines), and the confluent hypergeometric functions at +inf by the sign of
  Gamma(b)/Gamma(a) (13.7.1; a nonpositive integer a is a polynomial, left to the series).
- `limit(abs(u))` where `u` itself has no limit the walk can express (a product tending to infinity off the real
  axis, `8 i / t`) takes the limit of the product of the moduli, `|u1 u2 ...| = |u1| |u2| ...` with `|w^p| = |w|^p`
  for a real number p. Tests: `mc-engine/tests/round38_integration.rs`.

## Round 39: complex arguments of sign and abs, complex directions

- The walk's rule sign(u) = side of u and the Gruntz rewrites abs(u) = s u, sign(u) = s hold for a real u only; an
  argument with an explicit complex number goes to the series (now right for these functions, see mc-poly) or
  declines. `limit((sign(1 + im*t) - 1)/t, t -> 0)` was 0 (it is i), `limit((abs(1 + im*t) - 1)/t, t -> 0)` was i
  (it is 0).
- `direction: d` for a complex number d off the real line (`im`, `-im`, `exp(im*pi/4)`, `1 + im`): the limit along
  the ray x = a - d s, s -> 0 from above, i.e. the one-sided limit in s of f(a - d s) (so `direction: im`
  approaches from below the real axis: `sqrt(-2 + e)` tends to -i sqrt(2)). An infinite point, or no limit found,
  stays unevaluated. `direction: complexes` (every direction at once) stays unevaluated; before, both were
  argument failures.
## Round 39: sequences without a limit

- `discrete_limit` of a sequence whose upper limit is above its lower limit (`discrete_max_limit`,
  `discrete_min_limit`) is `failure(no_limit, discrete_limit, f, ...)`: `sin(n)`, dense in [-1, 1].

## Round 40: one-sided limits onto branch cuts

- The inverse circular and hyperbolic functions whose argument tends to a point of their branch cut are written in
  their logarithm and square-root forms (DLMF 4.23(iv), 4.37(iv); each equals the function off its cuts and has its
  cuts on the same set) before the limit, so the side the path comes from is read from the logarithms and roots as
  for `log` and powers: `limit(atan(2 i - s), s -> 0, direction: from_above) = -pi/2 + i log(3)/2`. Along the cut
  itself the principal value on the cut is kept.
- A logarithm or root reaching its cut with an imaginary part whose sign is not decided (a parameter) or changes now
  gives no limit; before, the value on the cut was substituted, which could be the other side's.
## Round 40: harmonic numbers and the Hurwitz zeta function at infinity

- The limit walk writes `H(n) = euler_gamma + digamma(n + 1)` and `H(n, r) = zeta(r) - zeta(r, n + 1)` (r != 1, r free
  of the variable); `zeta(s, u)` as u -> inf has the expansion `u^(1-s)/(s-1) + u^-s/2 + sum B_2k/(2k)! (s)_(2k-1)
  u^(1-s-2k)` (DLMF 25.11.43). The same expansions are rules of the series helper in `lib/special.mc`
  (`ds_hurwitz_tail`), so `series(harmonic_number(n, 1/2), n, inf, 3)` is `2 n^(1/2) + zeta(1/2) + 1/(2 n^(1/2)) -
  1/(24 n^(3/2)) + O(n^(-7/2))`.

## Round 40: limits from every direction of the plane

- `limit`, `max_limit`, `min_limit` with `direction: complexes` at a finite point a: f(a) when f is continuous on the
  plane at a (`continuous_at`): the variable, finite constants, sums, products, integer powers (a negative power
  only of a base whose value at a is not 0), `c^u` for a constant c != 0, and exp, sin, cos, sinh, cosh, abs, re,
  im_part, conjugate of continuous arguments. `max_limit`/`min_limit` also need a real value. Anything else (poles,
  branch cuts, other functions) still declines.
## Round 40: sums over fixed roots

`limit(root_sum(p, t => g), a -> a0)` (two-sided) with p free of a: the roots do not move, so the limit of the finite
sum is `root_sum(p, t => lim g)` when the summand's own limit is a finite value; otherwise the general method
(`sum of 1/(a r)` tends to 0 because the sum of 1/r over the roots of t^3 + 2 is 0).
## Round 40: real variables in re, im_part, conjugate and arg; limits over the plane

- Along the real line the limit variable is real: before the limit, `re`, `im_part` and `conjugate` of a sum give up
  their terms c x^k (c a number) as re(c) x^k, im_part(c) x^k, conjugate(c) x^k (`split_real_variable`), and
  `arg(u)` with re(u) > 0 at the point is atan(im_part(u)/re(u)) near it. `limit((re(x + h) - re(x))/h, h -> 0)`
  is 1, the im_part quotient 0, `(arg(1 + h) - arg(1))/h` 0.
- `direction: complexes` (round 39 left it unevaluated): the limits along the rays from 1, i, -1, -i (the ray
  limits of round 39, with the ray parameter real as above); two found whose difference is a nonzero number give
  `failure(no_limit, limit, f, "the limits along two directions differ")`. Equal ray limits prove nothing (a
  function may still have no limit along other paths), so the call then stays unevaluated; deciding existence over
  the plane (analyticity at the point) is not done. `abs(x + h) - abs(x)` over h with a symbolic x stays: its ray
  limits are expressions in x whose difference is not decided. Tests `mc-engine/tests/round40_limits.rs`.
