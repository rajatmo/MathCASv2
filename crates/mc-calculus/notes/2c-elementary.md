# 2C item 3: elementary integration (table, substitution, parts)

Code: `src/integrate/elementary/` (`mod.rs` entry point and helpers, `table.rs`, `trig.rs`, `subst.rs`, `parts.rs`).
Tests: `tests/integrate_elementary.rs`.

## Order of methods (after the dispatcher's linearity, base table and rational method)

0. Normalization: `e^u` -> `exp(u)`, `exp(u)^k` -> `exp(k u)`, exponential factors of a product merged.
1. **Table** (`table::single`), argument `u` linear (slope from `d(u, x)` free of x and nonzero):
   exp, sin, cos, tan, cot, sec, csc, sinh, cosh, tanh, log, atan, asin, acos, asinh, acosh, atanh, `k^u`, `u^n`
   (`n = -1` gives log). A quadratic `p = A x^2 + B x + C` (detected by `p''` free of x) to the powers -1, -1/2, 1/2,
   -3/2, by the signs of `A` and `D = 4AC - B^2`. `exp(u) sin(v)`, `exp(u) cos(v)` in closed form (the cyclic case
   of parts, solved once instead of by recursion).
2. **Trigonometric monomials** (`trig`): `sin(u)^m cos(u)^n` of one linear `u`, with tan/cot/sec/csc rewritten as
   sines and cosines. A pure negative power uses the reduction formula. A negative power times a positive even
   power is rewritten by `cos^2 = 1 - sin^2` and expanded. An odd power uses the substitution `t = cos u` or
   `t = sin u`, picking the one that leaves a polynomial in `t`. Even non-negative powers use `trig_reduce`. Both
   powers negative and even use `1 = sin^2 + cos^2`. Sines and cosines of different arguments, or times a
   polynomial, use `trig_reduce`.
   **Hyperbolic** products of total degree >= 2 are rewritten as exponentials and expanded.
3. **Tabular parts** (`parts::tabular`): a polynomial times exp/k^u/sin/cos/sinh/cosh (linear) or `exp*sin`,
   `exp*cos`, computed as `sum (-1)^k P^(k) G_(k+1)`. It does not recurse, so the degree can reach 200 without
   running into the dispatcher's depth limit of 12.
4. **Derivative-divides** (`subst`): candidates are the subterms depending on x (not products, not f, not x),
   largest first, at most 24. It forms `q = f/u'`, retrying with `u'` factored when it is a sum, then replaces `u`
   by a fresh `t`. Extra identities apply for `u = x^m` (`x^(jm) -> t^j`), `u = sin/cos` (even powers of the other
   function -> `(1 - t^2)^j`), `u = log(x)` (`x^k -> exp(k t)`) and `u = exp(v)` (`exp(kv) -> t^k`). The
   substitution is accepted only if `q` becomes free of x and grows by at most 2 subterms.
   The `log(x)` and `exp(v)` identities are tried in a second pass, after parts (so `x^3 log(x)` goes by parts,
   while `sin(log(x))` and `exp(x) sin(exp(x))` go by substitution).
5. **By parts** (`parts::by_parts`), LIATE: `u` is a log or inverse-function factor (a positive integer power of
   one); otherwise the polynomial factors are differentiated and the rest is integrated.
6. **Expand**: a product with a sum factor (or a power of a sum) is expanded and integrated term by term.

## Decisions

- Results avoid functions that have no derivative rule in `rules.rs` (sec, csc, cot, asinh, acosh, atanh). Instead:
  `∫sec = log((1 + sin u)/cos u)`, `∫csc = log(tan(u/2))`, `∫(1 + x^2)^(-1/2) = log(x + sqrt(1 + x^2))`, and
  `∫cos^-k`, `∫sin^-k` are written with `sin`, `cos` and `tan`. This keeps `d(integrate(f, x), x)` checkable.
  The table entries for asinh/acosh/atanh integrands themselves (`u asinh u - sqrt(1 + u^2)`, ...) do contain those
  functions.
- No `abs` in logarithms, the same as the base table (`log(x)`). No constant of integration.
- Signs of symbolic constants: numeric when possible. Otherwise structural, assuming real nonzero parameters
  (`a^2 > 0`). For example `∫1/(a^2 + x^2) = atan(x (a^2)^(-1/2)) (a^2)^(-1/2)`.
- Budget: each top-level call (depth 0) has 4000 attempts in a thread-local counter, saved and restored around
  nested evaluations. On top of that come the dispatcher's depth limit, `cx.tick` in every loop, the integrand
  size limit (400) for substitution, and exponents limited to 200.
- Fresh variables are symbols named ``integrate`u<depth>``, ``integrate`t<depth>`` and ``integrate`w<depth>``. They
  are always substituted back.

## Known gaps

- Rational sub-integrals are left to `rational::integrate`, which is a stub in this branch. Examples:
  `x*atan(x)` (needs `∫x^2/(1+x^2)`), `sin(x)^-1*cos(x)^-1`, `exp(2x)/(1+exp(x))`.
- Results are not simplified: iterated antiderivatives can be nested, and `sqrt(a^2)` stays as it is.
- No Risch algorithm. No `erf`, `Si` or `Ei` results, so `exp(x^2)` and `sin(x)/x` stay unevaluated.
- Not handled: `sqrt` of a quadratic times other factors (beyond what substitution covers), Euler and
  trigonometric substitutions, `sin(x)^n` with symbolic `n`, and products of hyperbolic functions with negative
  powers.

## Changes wanted outside these files

- `rules.rs`: derivative rules for `sec`, `csc`, `cot`, `asinh`, `acosh`, `atanh` (and `sech`, `csch`, `coth`).
  `d(sec(x), x)` currently gives `sec'(x)`. With these rules the table could return the conventional forms
  (`asinh(x)`, `-cot(x)`), and `check_antiderivative` would work for integrands that contain them.
