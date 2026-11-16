# mc-numfn

The one trait, `NumFn`, through which every numeric solver (roots, integrals, differential equations, plots) sees a
user's function, generic over the number type.

## Modules
- `term`, `prep`: the user's expression as a closure over its variables, and its preparation (constants such as `pi`
  evaluated once).
- `interp`: the interpreted backend, for any number type (`f64`, complex, `Num` including balls at any precision).
- `compiled`: the compiled backend (`mc-jit`, machine reals) and `Auto`, which falls back to the interpreter.
- `cache`: the process-wide cache of compiled code.
- `par`: parallel batch evaluation (`Par`).
- `monitor`: options that run during a computation.
- `value`: the number types a `NumFn` runs on.

## Design
- `compile_expr` builds an `Auto`: compiled when the compiler accepts the expression, interpreted otherwise, and
  every call the compiled code declines is evaluated by the interpreter, so results never depend on the compiler.
- Bodies that differ only in machine-real constants form a family: compiled once with the constants lifted to
  parameters, so a solver called for many parameter values compiles once.
- `eval_batch` and `eval_flat` evaluate many points per call.
- Depends on `mc-eval`, `mc-arith`, `mc-numeric` and `mc-jit`.

## Tests
- Integration tests in `tests/` check the API, that the backends agree, the compilation cache, parallel evaluation and
  scratch-space handling.
- Run them with `cargo test --release -p mc-numfn`.
