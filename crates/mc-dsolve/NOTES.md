# mc-dsolve: notes

## Round 34

- Split out of `mc-calculus` with no behaviour change: d_solve, d_solve_value, asymptotic_d_solve_value, r_solve, r_solve_value. Its functions are listed in `mc_calculus::FUNCTIONS`
  (one table for the area); the history of the code is in `mc-calculus/NOTES.md`.

## Round 40: Mathieu's equation

`y'' + (A + B cos(k x + phi)) y = 0` has the basis `mathieu_c(a, q, t)`,
`mathieu_s(a, q, t)` with t = (k x + phi)/2, a = 4A/k^2, q = -2B/k^2 (DLMF 28.2.1). The algebraic forms of the
equation (x = cos^2 t) are not recognised, nor a sine (the check of the solution needs cos(x - pi/2) = sin(x),
which `simplify` does not give).
