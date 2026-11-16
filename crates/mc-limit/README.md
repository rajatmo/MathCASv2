# mc-limit

Limits for MathCAS: the native `limit(f, x -> a)` and its Rust entry point `limit`, with the one-sided and
superior/inferior variants.

## Modules
- `walk`: the one-sided limit of a term as its variable tends to a point from one side.
- `gruntz`: Gruntz's algorithm for exp-log functions as the variable tends to `+inf`.
- `asym`: truncated asymptotic expansions of special functions at an infinity of their argument.
- `check`: the numeric plausibility check applied to a limit before it is returned.
- `bounds`, `switches`: real bounds and critical points, and functions defined by cases (`abs`, `sign`, ...); both are
  shared with definite integration.
- `more`: `max_limit` and `min_limit`, the limits superior and inferior.

## Design
- `limit` is two-sided by default; `direction: from_above` and `from_below` give one-sided limits. A two-sided limit is
  the common value of both sides: equal values give that value, provably different ones give `failure(no_limit, ...)`,
  and an undecided side leaves the call unevaluated.
- A limit that is not decided stays as a call; an answer is never guessed. Every value passes the numeric check first.
- Inexact numbers in the input are rationalized first and the limit is given numerically.
- A complex function of a real variable that the other methods leave is split into its real and imaginary parts
  (`complex_expand`); their limits give a value or a directed infinity `c*inf` (semantics 3.1).
- A list as first argument maps elementwise.
- The crate sits on the evaluator (`mc-eval`) and the term builder and derivatives of `mc-calculus`; signs of constants
  come from `mc-decide`, polynomial work from `mc-poly`.

## Tests
- Integration tests in `tests/` evaluate source text on the assembled engine (`mc-engine`, a dev-dependency) and
  cover limits and Gruntz's algorithm.
- Run them with `cargo test --release -p mc-limit`.
