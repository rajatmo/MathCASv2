# mc-transforms: notes

## Round 34

- Split out of `mc-calculus` with no behaviour change: the integral transforms and their inverses, discrete_delta and kronecker_delta; Kind (the transforms by name) is mc-calculus's (transform_kind.rs), as the derivative operator reads it. Its functions are listed in `mc_calculus::FUNCTIONS`
  (one table for the area); the history of the code is in `mc-calculus/NOTES.md`.

## Round 38: transforms as inverse Mellin transforms of products

- `hankel_transform` (and its inverse) falls back, when its term rules decline, to the Mellin-Parseval route: with
  `F` the Mellin transform of `f` on `a < re(z) < b`, `M[H](w) = M[bessel_j(nu, t)](w) F(2 - w)` on `max(-re(nu), 2 -
  b) < re(w) < min(3/2, 2 - a)`, inverted on that strip (`calc_hankel_mb` in `lib/calculus.mc`). `mellin_convolve`
  uses `F(z) G(z)` on the intersection of the two strips (`calc_mconv_mb`, called from `nl_mc` in `lib/numerics.mc`
  when the direct integral fails); `generate_conditions: true` returns the parameter conditions of the two
  transforms. Sources: Titchmarsh, *Introduction to the Theory of Fourier Integrals* (1948), sections 1.5 and 2.15;
  Paris and Kaminski, *Asymptotics and Mellin-Barnes Integrals* (2001), sections 3.1 and 3.4.
- The product is written with gamma functions only (sin, cos, csc, sec of `pi u` by the reflection formula). The
  left-closed residue series is the function on all of `x > 0` only if the gamma factors `gamma(a w + c)^e` have
  decay exponent `sum(e |a|) > 0`, or exponent 0 and `sum(e a) > 0` (a Meijer G-function with q > p, entire in its
  argument). The case `sum(e |a|) = sum(e a) = 0` (p = q) gives a function with a jump at the radius of the left
  series (the Hankel transform of `sin(r)/r` is `1/sqrt(1 - s^2)` below `s = 1` and 0 above): it declines, as do
  transforms whose strips do not meet (`erf(r)`, whose Hankel transform exists only as a generalised one).
- Gaps: transforms whose Mellin transform is not a gamma ratio (`cos(r)/(1 + r^2)`, `airy_ai`, `elliptic_k`,
  `log(1 + a^2/r^2)`), and the jump case above (it needs the series closed to the right for the outer part).
- The numeric check of a forward Mellin transform found by the table now samples at points inside the transform's
  strip (`verify::mellin_points`): at the old fixed points 1/2 and 1/3, outside the strip of `exp_integral_e(1,
  x)/x` (`re(s) > 1`), the quadrature of a divergent integral took 5 s; the check now takes milliseconds.

## Round 39: polynomial factors and equations

- `z_transform((n + 1) f(n), n, z)` (a polynomial factor that is a sum, or a power of one, times an undefined
  function) is expanded and taken term by term by the rule `Z[n g] = -z G'`: the product was held as the transform
  of an undefined function and the held call evaluated again without end (the evaluation depth limit).
- In an equation or a rule, a side that is an undefined function of the variable alone keeps its transform held:
  `z_transform(a^n == f(n), n, z)` is `z/(z - a) == z_transform(f(n), n, z)` (it stayed unevaluated as a whole).
- The shift rule of the inverse Laplace transform retries with the shifted term expanded: a completed square under a
  root, `(s^2 + b s + a^2)^(-1/2)`, becomes `(s^2 + a^2 - b^2/4)^(-1/2)` at `s - b/2` (the unexpanded base did not
  match the table): `exp(-b t/2) bessel_j(0, t sqrt(a^2 - b^2/4))`.
