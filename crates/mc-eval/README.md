# mc-eval

The evaluator: patterns and the matcher, rule indexes, and a speculative parallel evaluator on `mc-core`'s heap and
sessions. It defines the interface through which built-ins and libraries plug in.

## Modules
- `pat`, `matcher`, `subst`: patterns and their compilation, matching by backtracking in continuation-passing style
  with a pruning strategy for associative-commutative heads, and hygienic substitution.
- `index`, `defs`, `conc`: discrimination trees and key-set filters updated by one writer and read without locks;
  definitions of every symbol (own values, attributes, rules).
- `eval`, `machine`, `ctl`: `Kernel`, the evaluator on an explicit heap-allocated frame stack, and `Control`, the single
  block of fuel, stop flag and failure status shared by all tasks.
- `native`, `builtins`, `order`, `syntax`: the native-function interface, the minimal built-ins, the canonical-order
  hook, and the pattern and definition terms produced by `mc-syntax`.
- `lang`, `cells`, `nonlocal`, `constrained`: closures, blocks, `let`/`var`, loops, `match`, `throw`/`catch`,
  `sow`/`reap`, evaluation under time or memory limits.
- `assume`, `packed`, `compiled`, `inspect`, `par`, `guard`, `profile`, `membudget`: assumption contexts, packed arrays,
  compiled specialisations, read access to definitions, parallel portfolios, the panic guard, profiling, memory budget.

## Design
- Evaluation depth is bounded by memory, not the native stack, because frames live on the heap.
- Definition indexes are single-writer, many-reader and keyed by symbols and structural hashes, so collections need no
  re-keying.
- A panic inside an evaluation becomes `failure(internal, ...)` and the session continues.
- Depends on `mc-api`, `mc-core`, `rayon` and `parking_lot`; arithmetic is supplied by `mc-arith` through the native
  interface, not linked here.

## Tests
- Unit tests in `src`; integration tests in `tests/` for semantics, the machine, assumptions, packed calls,
  parallelism and scratch scopes, and workload checks.
- Run: `cargo test --release -p mc-eval`; the binary `mc-eval-bench` measures the workloads.
