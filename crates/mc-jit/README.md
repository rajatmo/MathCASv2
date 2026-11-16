# mc-jit

The numeric compiler: typed MathCAS functions to machine code through Cranelift, run as compiled specialisations of the
evaluator. A call whose arguments have the declared machine types runs compiled; every other call runs as before.

## Modules
- `front`: turns a function's definition (the terms `mc-syntax` lowers it to) into the typed tree of `ir`, inlining the
  functions it calls.
- `ir`: the typed numeric IR `Func`, and the lowering (if-conversion) to three-address code `Lir`.
- `jit`: the Cranelift back end, LIR to machine code; `interp`: the LIR interpreter, the back end compiled code is
  tested against and the fallback when Cranelift fails.
- `rt`: the run-time side shared by both: the environment block every compiled call receives, and helpers.
- `spec`: the specialisation registered with the evaluator's `Kernel::register_compiled`, and the natives `compile`
  and `compiled_info`.
- `expr`: compiling an expression in given variables, or a closure, from Rust (used by `mc-numfn`).
- `batch`: calls of one compiled expression on several threads; `callee`: library calls from compiled code.

## Design
- Exactness: a compiled call gives exactly the interpreter's result, or declines and the interpreter evaluates the
  call. Integer overflow and inexact integer quotients decline; elementary functions are `mc-numeric`'s shared machine
  functions and decline outside their real domain.
- Loops count down on every back edge and, periodically, read the evaluator's control block and charge fuel, so
  compiled code can be interrupted and is budgeted like interpreted code.
- Vector parameters of run-time length are read from a packed array's payload without unpacking.
- Depends on Cranelift, `mc-api`, `mc-arith`, `mc-core`, `mc-eval`, `mc-numeric`, `mc-random`, `mc-special`, `rayon`.

## Tests
- Integration tests in `tests/`: agreement of the compiled back end with the interpreter, library calls, packed
  arrays, batching, and engine-level use; property-style tests compare both back ends on random inputs.
- Run: `cargo test --release -p mc-jit`
