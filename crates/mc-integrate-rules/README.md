# mc-integrate-rules

Rule-based indefinite integration: a table of integration rules for the forms the tower-based engine does not reach,
and the interpreter that applies them. It covers powers and products of trigonometric and hyperbolic functions,
products with powers of `x`, inverse trigonometric and hyperbolic functions, binomials `x^m (a + b x^n)^p`, and
logarithms of trigonometric functions.

## Modules
- `syntax`: the notation of the rule files in `rules/*.rules`, and its parser (owned nodes, parsed once per process).
- `table`: the table and its dispatch; a rule is a candidate only when every function its left side names occurs in
  the integrand.
- `run`: the interpreter: patterns lowered to the evaluator's matcher with optional arguments and unit defaults,
  conditions decided natively on the bound terms, right sides built by canonical arithmetic.
- `cond`, `util`: the predicates and term helpers; `methods`: the methods the rule files call.
- `rules/`: the rule files, one per family (trigonometric, hyperbolic, inverse functions, binomial, logarithmic,
  special functions).

## Design
- The rules are translated from Rubi, Albert D. Rich's rule-based integrator (MIT licence; see the repository's
  third-party notices), into this crate's pattern notation. The reduction formulas are classical: Gradshteyn and Ryzhik,
  sections 2.5 to 2.8, and Abramowitz and Stegun 15.3 for hypergeometric forms.
- Recursion through `int` is bounded by depth, steps and time.
- Answers are not checked here: the caller, the integration portfolio of `mc-integrals`, differentiates every answer.
- Depends on `mc-api`, `mc-arith`, `mc-core`, `mc-eval` and `mc-integrate`.

## Tests
- An integration test in `tests/` integrates through an engine session; a documentation example on the crate page.
- Run: `cargo test --release -p mc-integrate-rules`
