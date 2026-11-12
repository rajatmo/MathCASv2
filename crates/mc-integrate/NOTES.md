# Item 14A: `mc-integrate` (the tower-based integration engine)

Round 17. Roadmap phase 14, item 14A: the differential field of an integrand, the Risch
decision procedure and the parallel Risch–Norman ansatz over it, special functions as monomials of the tower. The
driver is `mc-calculus/src/integrate/portfolio.rs` (the portfolio and the check of every answer). Dependencies
`mc-api`, `mc-arith`, `mc-core`, `mc-eval`, `mc-poly` (polynomials, rational functions, univariate polynomials over
rings, factorisation, functional decomposition), `rustc-hash`.

## What was built (`src/`)

- `field`: the field of one problem, `Q(i)(v_1, ..., v_n)`: `RatFun` in lowest terms, reduced modulo `i^2 + 1` with
  the denominator made free of `i` by its conjugate, so that zero is recognised; a `CoeffRing`, so `mc-poly`'s
  univariate algorithms (gcd, square-free decomposition, subresultant resultants) run over it.
- `tower`: the classification of an integrand (Bronstein, *Symbolic Integration I*, chapters 3 and 5): exponentials
  grouped by rational proportionality of their arguments modulo constants (one monomial per group, the generator
  chosen so every member is an integer multiple), logarithms, algebraic monomials `b^(1/q)` (and `b^a` for a
  symbolic constant `a`), trigonometric pairs `sin(u)`, `cos(u)` (mode `Pair`) or `exp(i u)` (mode `Exp`), the
  hyperbolic functions through `exp`, and the primitives with known derivatives: the inverse trigonometric and
  hyperbolic functions and the special functions `erf`, `erfc`, `erfi`, `exp_integral_ei`, `log_integral`,
  `sin_integral`, `cos_integral`, `sinh_integral`, `cosh_integral`, `fresnel_s`, `fresnel_c`, incomplete `gamma`,
  `elliptic_f`, `elliptic_e`, `elliptic_pi`. Constants are variables of derivative zero. The integrand is rewritten
  over fresh symbols first (so the number of variables is known), then read; derivations computed in order.
- `uni`: polynomials in one monomial over the field: extended Euclid, the diophantine solver, Hermite reduction for
  the tower's derivation (Mack's linear version, Bronstein 5.3).
- `rde`: the Risch differential equation over `C(x)` (Bronstein chapter 6, the base case): normal denominator,
  degree bound, Rothstein's SPDE, the no-cancellation solver; every solution checked by substitution.
- `risch`: the decision procedure for towers of height one (one exponential or one logarithm over `C(x)`): canonical
  splitting, Hermite reduction, the Rothstein–Trager residue criterion (resultant through `mc-poly`'s subresultant
  chain over the field, rational roots by factorisation), the primitive polynomial part by limited integration, the
  hyperexponential Laurent part by the Risch differential equation; the coefficient of `t^0` goes back to the
  caller's rational integrator. Proofs of non-elementarity are returned with the step that gives them.
- `norman`: the parallel ansatz (Norman and Moore 1977; Bronstein 2007): special and normal factors of the
  denominator by `p | Dp`, the candidate denominator, a numerator with unknown coefficients under total and
  per-variable degree bounds, logarithms of the denominator's factors, `atan` of irreducible quadratics with a
  constant discriminant, the special-function candidates; the tower's relations (`sin^2 + cos^2 = 1`, `t^q = b`)
  applied to the equation before its coefficients are compared; one sparse linear system over `Q` (or over the
  field of the constants when there are parameters).
- `special`: the candidates (see the module documentation): `erf`/`erfi` of an exponential's quadratic argument,
  also of an inner polynomial found by functional decomposition (F2's `right_component`), `Ei` of a polynomial
  argument, `Shi`/`Chi`, incomplete gamma with `x^a`, `Si`/`Ci`, Fresnel `S`/`C`, `li` and `Ei(k log x)`,
  `elliptic_e` and `elliptic_f` of `(1 - m sin(phi)^2)^(1/2)`.
- `linsolve`: sparse Gaussian elimination over a field (`CoeffRing`).
- `write`: field elements back to terms (`exp(w)^n` written `exp(n w)`, exponentials of a denominator moved up).
- `budget`: the evaluator's fuel, the interrupt and a per-method deadline as one `mc-poly` `Work`.
- `lib`: `risch(cx, ar, f, x, time) -> Outcome` (`Found { part, rest }`, `NonElementary(reason)`, `Unknown`) and
  `norman(cx, ar, f, x, time, special) -> Option<T>`; the real form of `exp(i u)` answers (`c_n t^n + c_-n t^-n` as
  `cos(n u)` and `sin(n u)` terms).

## Public API

- `mc_integrate::risch`, `mc_integrate::norman`, `mc_integrate::Outcome`; the modules for Rust callers.
- User function (in `mc-calculus`): `risch_integrate(f, x)`.

## Decisions

- **14A.1** One tower, two modes. The decision procedure needs an algebraically independent tower, so it reads
  trigonometric functions as `exp(i u)` with `i` a constant of the field; the ansatz reads them as pairs with the
  Pythagorean relation, so its answers are real without a conversion. Hyperbolic functions go through `exp` in both.
- **14A.2** Constants are indeterminates. `pi`, `2^(1/2)`, `log(3)`, `exp(1)` and symbols are independent variables
  of derivative zero; an identity found with them as indeterminates holds for their values (a polynomial identity
  specialises). Only `i` is reduced. The decision procedure's proofs are generic in symbolic parameters.
- **14A.3** The decision procedure decides towers of height one (`C(x)(t)`, `t` an exponential or a logarithm of a
  rational function); taller towers, algebraic monomials and pairs go to the ansatz. A residue polynomial with an
  irreducible factor of degree 2 or more ends the procedure without a decision (the integral is elementary; the
  other methods find it).
- **14A.4** Special functions are monomials, not table entries: a candidate is a variable with a known derivative
  (`D erf(v) = 2/sqrt(pi) exp(-v^2) Dv`), added to the tower before it is read, and it enters the ansatz linearly
  with a constant coefficient. So `x^2 exp(-x^2)` gives `-x exp(-x^2)/2 + sqrt(pi) erf(x)/4` from the same linear
  system as the elementary part.
- **14A.5** Bounded and interruptible: every loop is metered; the driver gives the decision procedure 1.5 s, the
  ansatz 2.5 s, the elementary rules 4 s (0.6 s after the engine answered); the ansatz has at most 700 unknown
  monomials and total degree 12. A time-out is never a proof.

## Checks and proofs

- Every antiderivative is checked before it is returned (differentiation, numerically to 30 digits at three points,
  then `simplify`).
- Non-elementarity is proven and reported (`risch_integrate`), with the step that proves it, so an unevaluated
  integral is told apart from one that is out of reach.
- `integrate((m/(2 pi f))^(3/2), f)` is answered without assumptions (one algebraic monomial).

## Known gaps

- The decision procedure stops at height one; no algebraic case (14B), no tangent monomial (trigonometric
  integrands go through `exp(i u)`).
- Logarithmic parts with residues of degree 2 or more are not written by the decision procedure (the rational
  integrator writes them for rational functions; for exponential towers the ansatz's `atan` candidates cover
  constant discriminants only).
- The ansatz does not factor over algebraic extensions (Bronstein's pmint second pass over `Q(i)`).
- `elliptic_pi` is a monomial when it occurs in the integrand, not a candidate.

# Item 14B: integrands with roots (`algebraic`, and the portfolio in `mc-calculus`)

Round 18. Roadmap phase 14, item 14B. The exact algebra is here
(`src/algebraic/`); the substitutions, which need the evaluator and the rest of the integrator, are driven from
`mc-calculus/src/integrate/elementary/algebraic/`, called by the portfolio after the engine (one hook in
`portfolio.rs`: the stage, its answer among the candidates, its proof in `integrate` and `risch_integrate`).

## What was built

- `algebraic::binomial`: Chebyshev's theorem on `x^m (a + b x^n)^p`: the case (`p`, `(m + 1)/n`, `(m + 1)/n + p`
  an integer) with its substitution, or the proof.
- `algebraic::hermite`: Hermite reduction (Ostrogradsky's undetermined coefficients) in `Q(x)[y]`, `y^q = g`, `g`
  square-free: `int P/(E g^w) g^s = A/E_red g^(s+1-w) + int B/E_sf g^s`, one linear system, unique solution
  (`linsolve`); poles of order two and more at finite places, above the roots of `g`, and the excess degree at
  infinity are removed. `second_kind` and `residue_at_infinity` for the proof of non-elementarity (q = 2).
- `algebraic::abel`: the continued fraction of `sqrt(P)` in `Q((1/x))`, `P` of even degree with a square leading
  coefficient: the first unit `A + B y` (`A^2 - B^2 P` constant) within 40 partial quotients and degree 64;
  `d log(A + B y) = (A'/B) dx/y`.
- `algebraic::goursat`: Laurent polynomials and rational functions invariant under `x -> c/x` written in
  `u = x + c/x`.
- `algebraic::in_terms_of`: the `h`-adic digits of a polynomial (`p = G(h)`), for derivative-divides with F2's
  `decompose`.

The driver (`mc-calculus`, module documentation there): the table's forms; Chebyshev (substitution, or proof and
the hypergeometric form `x^(m+1)/(m+1) B^p (1 + b x^n/a)^(-p) 2F1(-p, k; k + 1; -b x^n/a)`); rationalising
substitutions `t = L^(1/N)` for every linear or linear-fractional base `L` (nested roots one layer at a time); Hermite
reduction with Abel's logarithmic part and the remainder to the other methods; the trigonometric substitutions
(`sin`, `tan`, `sec` forms, written back through `trig_expand`) and Euler's three substitutions; `u = x^k`,
`u = h(x)` from a decomposition, Goursat's `u = x + c/x`; Legendre's forms `elliptic_f`, `elliptic_e`, `elliptic_pi`
for even quartics and products of two even quadratics, cubics with a rational root by `x = e +- t^2`, quartics with a
rational root by `x = e + 1/u`; radicals of rational functions split `(N/D)^e = N^e D^(-e)`; a sum term by term.
Sub-integrals go to the classic dispatcher (rational, trigonometric) or the portfolio one level deeper (at most 5),
under one deadline of 2 s for the stage.

## Decisions

- **14B.1** Exact algebra in `mc-integrate`, substitutions in `mc-calculus`. The reductions and decisions are pure
  functions on polynomials over `Q` (testable without the evaluator); the substitutions need the integrator itself
  for the sub-integrals, which lives in `mc-calculus`, so the driver is there (in `elementary/`, written for 14B) and
  the dependency direction stays `mc-calculus -> mc-integrate`.
- **14B.2** Every candidate is verified, the smallest kept. A substitution may be valid only on part of the line
  (`x = e + t^2` needs `x > e`; Goursat's `sqrt(P) = x sqrt(Q(u))` needs `x > 0`; splitting `(N/D)^e`) or choose a
  branch; the portfolio's check by differentiation at three points (then `simplify`) drops every wrong one, so
  methods can be tried freely. Cost is `mc-simplify`'s, times a weight of 130% for back-substitutions that divide by a
  power of `x` (Euler's second substitution, Chebyshev's third case): of two answers of about the same size the one
  regular at 0 is kept (`x^2 sqrt(x^2 + 1)` gets `log(x + sqrt(x^2 + 1))`, not `log(sqrt(x^2 + 1)/x - 1)`).
- **14B.3** Proofs are kept only from decisions: Chebyshev's theorem (exact form `K x^m (a + b x^n)^p`), and Hermite
  reduction for `q = 2`, a polynomial `P/E` and `deg g >= 3` when the remainder (after Abel's logarithmic part) has
  no residues: a nonzero differential of the second kind that is not exact (the reduction is complete) has no
  elementary integral by Liouville's theorem. A proof for the remainder of the reduction is one for the integrand.
  A remainder with a residue at infinity and no unit within the bounds stays undecided.
- **14B.4** Proofs do not stop the stage: Legendre's or the hypergeometric form is still sought (`elliptic_f`
  for `1/sqrt(1 - x^4)`, `hypergeometric_2f1` for `(1 + x^2)^(1/3)`); the elementary rules are skipped.
  Legendre's forms run also when only a hypergeometric form was found, and the smaller wins
  (`elliptic_f(asin(x), -1)` for `1/sqrt(1 - x^4)`, `x 2F1(-1/2, 1/3; 4/3; -x^3)` for `sqrt(1 + x^3)`).
- **14B.5** Radicands are tidied (expanded, over one denominator) before the methods read them, and answers after
  the stage, so that `(3 - (-1 + t^2))^(1/3)` is recognised as `(4 - t^2)^(1/3)`.

## Checks and proofs

- Proofs of non-elementarity with the reason (`risch_integrate`): Chebyshev's theorem and the second-kind argument.
- Every answer verified; the smallest of several substitutions kept, not the first one found.
- Chebyshev's pseudo-elliptic integral `x/sqrt(x^4 + 10x^2 - 96x - 71)` by Abel's continued fraction, a decision
  within bounds rather than a search.

## Known gaps

- The logarithmic part with finite poles of higher-degree radicals (Trager's residue criterion with the divisor
  test) is not implemented; such integrands reach Euler or elliptic forms or stay unevaluated. Abel's unit covers
  poles at infinity of square roots only, within the bounds.
- Legendre's reduction of a general quartic or cubic (no rational root, not even) is not implemented; such integrals
  get the hypergeometric form when they are binomials, else stay unevaluated.
- Elliptic answers for radicands with complex roots have complex parameters (`1/sqrt(x^3 + 1)`); a real form by the
  substitutions for conjugate roots is not chosen.
- Rational sub-integrals whose answer is a `root_sum` are not verified (no derivative of `root_sum` in the check), so
  `1/(x^(1/2) + x^(1/3) + x^(1/4))` stays unevaluated.
- A comparison of antiderivatives by `simplify(got - expected)` counts answers equal up to a constant as different
  (for `sqrt(x + sqrt(x))`).
