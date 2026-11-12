# mc-integrals: notes

## Round 34

- Split out of `mc-calculus` with no behaviour change: the integrate driver, antiderivatives (rational, elementary, algebraic) and definite integrals, with the native of integrate (natives.rs, from mc-calculus) and the notes of item 2C (notes/); definite/split.rs keeps the integration by cases, the switching functions are mc-limit's. Its functions are listed in `mc_calculus::FUNCTIONS`
  (one table for the area); the history of the code is in `mc-calculus/NOTES.md`.

## Round 36, later fixes

- Definite integrals: after the range refinement, square roots `(a + c cos u)^(1/2)` with numbers a > 0, c = -a or
  c = a (after `expand`) are written by the half-angle formulas `(2a)^(1/2) abs(sin(u/2))`, `(2a)^(1/2) abs(cos(u/2))`
  and the integral is tried again (the arc length of the astroid, sqrt(9/8 (1 - cos 4t)) over [0, 2 pi], is 6).
