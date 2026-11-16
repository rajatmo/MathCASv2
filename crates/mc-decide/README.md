# mc-decide

The decision service: one `Decider` answers sign, zero, nonzero, interval-membership, realness and integrality
questions about an expression under the facts of an assumption context. `refine`, `simplify` under assumptions, the
sign questions of limits and integrals, and the conditions of `solve` all go through it. It also holds conditional
results.

## Modules
- `decide`: `Decider`, the tiers from cheapest to most expensive, and the `Reals` hook for the last tier.
- `facts`: the facts of a context, and the first tiers: structure, bounds, ball refinement of closed parts.
- `basis`: the equational facts as a Groebner basis and the normal form of a candidate against it.
- `quad`: the exact sign of a real polynomial of total degree at most two in several variables.
- `sturm`: exact sign of a univariate rational polynomial on an interval.
- `cond`: `conditional_expression(value, condition)`, its arithmetic and printing, and `Conditions`, the per-call
  accumulator simplified once at the end.
- `build`: `Bld`, the term builder (canonical sums, products and powers through `mc-arith`).

## Design
- Tiers run in order and stop at the first that decides; an undecided question returns no answer rather than a guess.
- The last tier (real quantifier elimination) is supplied from outside through `register_reals`; `mc-cad` provides it,
  so this crate does not depend on it.
- Depends on `mc-api`, `mc-arith`, `mc-core`, `mc-eval`, `mc-numeric` and `mc-poly`.

## Tests
- Unit tests in `src` and an integration test in `tests/` that drives `refine`, `simplify` and conditional results
  through an engine session.
- A documentation example on the crate page.
- Run: `cargo test --release -p mc-decide`
