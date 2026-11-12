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
