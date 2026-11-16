# mc-dsolve: notes

## Round 34

- Split out of `mc-calculus` with no behaviour change: d_solve, d_solve_value, asymptotic_d_solve_value, r_solve, r_solve_value. Its functions are listed in `mc_calculus::FUNCTIONS`
  (one table for the area); the history of the code is in `mc-calculus/NOTES.md`.

## Round 40: Mathieu's equation

`y'' + (A + B cos(k x + phi)) y = 0` has the basis `mathieu_c(a, q, t)`,
`mathieu_s(a, q, t)` with t = (k x + phi)/2, a = 4A/k^2, q = -2B/k^2 (DLMF 28.2.1). The algebraic forms of the
equation (x = cos^2 t) are not recognised, nor a sine (the check of the solution needs cos(x - pi/2) = sin(x),
which `simplify` does not give).

## Round 41: Mathieu's equation with a sine; constants in the numeric check

- `B sin(u)` in Mathieu's equation is `B cos(u - pi/2)`: the basis is at t = (u - pi/2)/2.
- The numeric check gave every symbol of the residual a random value, `pi` and `e` included, so a residual with
  `cos(x - pi/2)` against `sin(x)` was compared at a wrong pi and the right basis was rejected. Symbols with a
  numeric value keep it now.

## Round 47: systems of recurrences

- `r_solve(eqs, [a, b, ...], n)` (`rsystem.rs`): linear systems, possibly implicit (`a(n) - 3 b(n + 1) == 5`). The
  equations are solved for the highest shift of every unknown, giving `X(n + 1) = A X(n) + G(n)` on the state of shifted
  values. Constant `A`: Cayley-Hamilton gives every component the scalar recurrence `p(E) x = g` (`p` the
  characteristic polynomial), solved by the scalar `r_solve_value` from the system's own initial values. `A` with
  `(-1)^n`: the two-step system on `n = 2m` and `n = 2m + 1`, joined with `(1 +- (-1)^n)/2`. The state `X(0)` is
  symbolic; initial values fix it by running the system forward exactly, the rest becomes `c(1), c(2), ...`.
- The check substitutes back: `z^(u + w)` (`u` free of `n`) is split into `z^u K` with an inert `K` for `z^w` (valid
  for principal powers), on both parity classes if needed. `simplify` alone does not show `3^((1 + n)/2) =
  3^(1/2) 3^(n/2)`, so `simplify(rewrite(eqns, sol))` on the user's side leaves such recurrences unreduced (an
  `mc-simplify` matter).
- Declined: singular `A` (a zero root of `p`), a top-shift system `solve` cannot solve uniquely, an unknown that is not
  shifted, initial values before the lowest shift, inconsistent initial values, roots given as `root(...)` objects (the
  check cannot prove them), variable coefficients other than `(-1)^n`.
- `r_solve(eq, a(m, n), [m, n])`: two-term partial recurrences `alpha a(x + s1) + beta a(x + s2) == f` with constant
  coefficients and polynomial `f`: `r^t c(1)(I)` with `r = -beta/alpha`, `I` the invariant of the primitive step
  `d = s1 - s2` and `t(d) = 1` (one free value on every line `I = const`), plus a polynomial particular solution
  (one degree more when `alpha + beta = 0`). A non-primitive step (`a(m + 2, n) - a(m, n)`) is declined.
