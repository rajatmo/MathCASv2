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
