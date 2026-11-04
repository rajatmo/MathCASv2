# Package 1b: `mc-jit`, the numeric compiler

Status: merged (branch `lib/p1b-jit`, commit 222ec15; merge 342e5dc). `compile(f)`, `@compiled` and `compiled_info(f)`
work; R12c at m = 10^6 runs in 584 ms (harness median, check passes); fuel and interrupts stop compiled loops at 1 and
4 threads; compiled results are identical to the interpreter's in every test, on both back ends. On the branch:
workspace 458 passed, 0 failed, 6 ignored; clippy clean.

## What was built

- `ir.rs`: spike E's two-level IR, cut to what the front end produces. HIR types `Int` (exact i64), `Real` (machine
  real), `Bool`; straight-line expressions, many of which may *decline*; statements `Set`, `Eval`, `If`, `While`,
  `For` over a range; the type checker and E.3's if-conversion (only non-declining expressions are speculated).
  Lowering gives LIR with checked integer ops, `IAddOv`, `ToReal`, `Floor`, `Math`, `Num`, `Bail`, and a `Poll` on
  every back edge.
- `interp.rs`: the LIR interpreter (reference back end, and fallback when Cranelift fails).
- `jit.rs`: Cranelift 0.137.0 at `opt_level=speed`, one JIT module per function (freed with the code), uniform entry
  `fn(*const u64, *mut Env) -> u64`.
- `rt.rs`: run-time shared by both back ends: the `Env` ABI block (exits `Done`, `Decline`, `Stopped`), the poll, an
  exact `ipow`, `num` (sums, products, powers through `mc_arith::Num` itself), C wrappers of `mc_numeric::machine`'s
  elementary functions.
- `front.rs`: MathCAS terms to HIR. Parameters typed `Int` or `Real`; body: arithmetic (incl. `a/b`, `b^-k`),
  comparisons, short-circuit `and`/`or`/`not`, `if`, blocks with `let`/`var`, assignment, `while`, `for` over
  `range(...)`, `exp` ... `tanh`, `sqrt`, `mod`, `quotient`, `floor`, `abs`, `min`, `max`; calls to other compilable
  functions are inlined, recursion refused; anything else refused with a reason.
- `spec.rs`: `Specialisation` (implements `Compiled`), the natives `compile` and `compiled_info`, the `Jit` handle
  (`install`, `flush`, `polls`, `info`).
- Natives: `compile(f)` and `compile(f, back_end: "interpreter")` return `f` or `failure(not_compilable, f, "reason")`;
  `compiled_info(f)` lists `params`, `result`, `hir_nodes`, `lir_instructions`, `code_bytes`, `back_end`,
  `compile_microseconds`, `inlined`, `state` (pending / active / stale), `runs`, `declines` (and `fallback`), or
  `failure(not_compiled, f)`. Both in `FUNCTIONS`.

## Decisions

- **1b.1** Arrays and complex numbers from the spike are dropped (the front end produces neither); LIR `Op` is
  `Clone`; the interpreter back end stays for differential tests and as fallback.
- **1b.2** Types are inferred; a local keeps the type of its first value (assigning another type is refused); both
  branches of a value `if` must agree; a type depending on run-time values (`max(0, x)` with real `x`) is refused;
  declared `let` types are checked as the interpreter does.
- **1b.3 Exactness by declining:** on overflow, an inexact integer quotient, an out-of-domain value (`log` of x < 0,
  `asin` outside [-1, 1]), or a power or sum whose result is not a machine real, the compiled call declines and the
  interpreter evaluates the call. Inline IEEE code only where the interpreter's result is provably one IEEE operation
  (two operands; integer to real only up to 2^53); everything else (sums/products of three or more operands, sorted at
  run time; exact rational parts; all powers) goes through `num`, which calls `mc_arith::Num::sum`, `product`, `pow`
  (identical by construction). `sqrt(x)` mirrors the interpreter's `x^(1/2)` (`powf(x, 0.5)`).
- **1b.4 ABI:** parameters as words, then `*mut Env` (the evaluator's `Control` pointer, the exit code, the host poll);
  one result word (booleans 0/1).
- **1b.5 Deferred registration:** a native cannot call `&mut Kernel::register`, so `compile` compiles immediately and
  queues the specialisation, and `Jit::flush` (called by the engine after every statement's `k.eval`) registers it.
  So `compile(f); f(x)` in one statement still runs `f` interpreted.
- **1b.6 Invalidation:** every call checks the version counter of the head and of every inlined function through
  `NativeCx::version` (which records the read, so memo entries stay correct); `flush` forwards the version bump of
  registering a native to the specialisations that read the head; a queued specialisation whose definitions changed
  before its flush is dropped.
- **1b.7 Poll:** every 1024 back edges the code reads the shared `Control`'s stop/status word (one atomic 64-bit load)
  and calls the host, which charges 1024 fuel with `cx.tick` (deviates from E.5's direct fetch-sub: F.3 requires a
  speculative task to spend its private share). Loops under 1024 iterations and straight-line code charge no fuel.
- **1b.8** A stopped call declines; the interpreter reproduces the abort (limit, hole, interrupt) at its first poll; no
  new failure path; compiled code never sets `Index`/`DivZero`.
- **1b.9** `Int` parameters run compiled only for machine-word integers; `Real`/`MachineReal`/`Number` only for
  machine reals (an integer for a `Real` parameter declines: the interpreter keeps it exact).
- **1b.10** Every call is inlined; recursion refused; inlining depth 32, size 100,000 nodes; inlined functions have
  exactly one typed clause (plus `fn`'s catch-all), no attributes, no native.
- **1b.11** One JIT module per specialisation, freed when dropped.
- **1b.12** `@compiled`: `define_fn` strips the decorator, defines the function, then evaluates `compile(f)`; the
  definition's value is `null` on success, else the failure (or the unevaluated `compile(f)` without a compiler).
- **1b.13** `compile` is an effect (ordered like a definition, refused speculatively and in conditions) and holds its
  arguments.
- **1b.14** R12c compiles the outer loop too (`longest` inlines `collatz_steps`) and sets `#@ memo: off`.
- **1b.15** If Cranelift fails, the interpreter back end is used and `compiled_info` reports `fallback`.

## Public API and edits outside the crate

- `mc-jit`: `install(k, h) -> Jit`; `Jit::{flush, polls, info}`; `build(&Func, inlined, Backend) -> Built`;
  `Built::call`; `Backend`, `Info`; modules `ir`, `front` (`Source`, `FnDef`, `front`), `rt`; `FUNCTIONS`.
- `mc-eval` (additions; `eval.rs` untouched): `src/inspect.rs` with `NativeCx::version(s)` (read recorded),
  `NativeCx::rules(s) -> Vec<RuleView>` (read recorded), `NativeCx::has_native(s)`, `pub struct RuleView`; `lib.rs`
  `mod inspect` and re-export; `syntax.rs` the `compiled` decorator in `DefineFn::call` and `compile_after`.
- `mc-engine`: depends on `mc-jit`; a `jit` field installed in `with_config`; `jit.flush(k)` after each statement's
  `k.eval`; `mc_jit::FUNCTIONS` among the library names; `CoreEngine::jit()`.
- `bench/cases/R/R12c.mc` (new; `R12.mc` unchanged).

## Tests (11 new)

- `tests/agree.rs` (3): 300 random HIR programs x 12 argument tuples (incl. the ends of i64, +-0, +-inf, NaN) give the
  same result and exit on both back ends, with done and declined both well represented; a table of decline cases for
  every integer op; a stopping poll and a raised stop flag end loops on both back ends.
- `tests/engine.rs` (8): 25 functions x 3-10 argument tuples print identically to the interpreter, with Cranelift and
  with the interpreter back end (overflow, exact division, floor `mod`/`quotient`, powers, three-real sums, rational
  parts, real division, roots, elementary functions and their domain declines, NaN comparisons, `and`/`or`, `for` with
  negative steps and at `i64::MAX`, inlined calls), each verified to run compiled; the same at 4 threads inside `map`;
  R12c against interpreted R12; `@compiled` (also on a function outside the subset); 15 refusals with their reasons;
  invalidation (redefined callee, recompiling, callee compiled after caller, redefined head, module reload); fuel
  (`spin`, `map(spin, ...)`) gives `Limit("fuel")` at 1 and 4 threads; interrupts at 1 and 4 threads stop within 2 s
  and the session continues.

## Measurements (busy machine, release, guard)

| Run | 10^4 | 10^5 | 10^6 |
|---|---|---|---|
| R12c (harness, memo off) | 3.96 ms | 51.3 ms | 584 ms (5 reps, check pass) |
| R12, interpreted | 5.10 s | 59.9 s (1 rep) | stops at the time limit |

R12 unchanged plus `compile(collatz_steps)` (the `map` interpreted, 4 threads): about 0.25 s at 10^6 (one REPL run,
including startup). Compile time 0.7-1.8 ms per function; machine code `collatz_steps` 664 B, `longest` 840 B.

## Found outside the crate

`mc-arith`: for a machine real, `Num::pow_int` with a negative exponent takes the reciprocal and negates the exponent
(`2.0^-1` gives `2.0`; `3.0/7.0` gives `21.0`). Compiled code mirrors it through `num`, so a fix at the source applies
to both (handed to package 9).

## Known gaps

- Calls stay interpreted within the statement that compiles the function (1b.5).
- A replaced specialisation stays in `mc-eval`'s append-only registry until the session ends (1a.7), so its machine
  code is not freed.
- Not compiled: complex numbers, lists/arrays, strings, closures, recursion, multi-clause or conditional functions,
  `mod`/`quotient` on reals, global constants (`pi`).
- NaN payload bits may differ between back ends (all NaNs print alike).
- The per-call version check costs one lookup per inlined function.
- No L-class bench cases exist, so none were measured.

## Reconciled with package 1a (commit e611795)

- `adapter.rs` is gone: specialisations register with `mc-eval`'s `Kernel::register_compiled`. Registering is not a
  definition (1a.7), so the version forwarding in `Jit::flush` was removed.
- `compiled_info` reports registrations and run counters, which are not definitions, so it calls the new
  `NativeCx::volatile()` and is never memoised (otherwise a recompiled function kept reporting `stale`).
- `Jit::flush` stays; `NativeCx::register_compiled` (an effect) now exists and would let `compile(f)` take effect
  within its own statement (1b.5) if `compile` used it.

## Package J (JIT real arithmetic, branch `lib/r4-jit`, commit 47b60c8; merge 9dcb14f)

Compiled code computes powers of machine reals and sums/products of 3+ operands (or with exact parts) inline,
bit-identical to the interpreter except NaN sign/payload. Mandelbrot `grid(300)`: 3.69 s -> 0.015 s. On the branch:
workspace 523 passed; clippy clean.

### What the interpreter computes

- The stored body is canonical (`x*x` is `x^2`; `x*x - y*y + cr` is `add(cr, mul(-1, y^2), x^2)`; `3.0*i/n` is
  `mul(3.0, i, n^-1)`); at run time `add`/`mul` of numbers are `Num::sum`/`Num::product`, `pow` is `Num::pow`.
- `x^k` (machine real, integer k): k = 0 gives exact `1`; otherwise one `powf` with the signed exponent as a real
  (`x.powf(+-(|k| as f64))`): no repeated multiplication, binary powering or reciprocal.
- `x^c` (both machine reals): `powf` when x >= 0, x NaN or infinite, c integral, or c not finite; otherwise complex.
- `x^(n/d)` (constant rational): the exponent converted to a machine real once (correctly rounded), then the same
  rule; an integer-valued n/d takes the integer rule.
- Sums and products (`Num::fold`): exact operands combined exactly; that exact value, if any, converted once to the
  nearest machine real (ties to even; exact zero is +0) starts the fold even when zero; the machine reals sorted by
  `cmp_canonical` (by value, -0.0 before 0.0, NaN last); then a left fold.

### Decisions

- **J.1** Inline code reproduces the arithmetic step for step, never approximates (no unrolling, no `x*x` for
  `powf`); refines 1b.3: `rt::num` remains only for what inline code cannot reproduce.
- **J.2** Powers: real^int bails at k = 0, then `rt::pow_int` (mirrors `pow_int`); real^real: the `pow_machine`
  real-result test (`FTrunc`), bail otherwise, then `rt::pow_real` (`powf`); real^constant rational: converted at
  compile time with `Real::Exact(q).to_f64()`, the test dropped when known; `sqrt` takes the `x^(1/2)` path. Exact
  bases and run-time ratio exponents still use the helper.
- **J.3** Canonical order at run time: each real maps to a total-order integer key (flip the 63 low bits when the
  sign bit is set); a sorting network of `smin`/`smax` (1, 3, 5, 9, 12 comparators for 2-6 values, checked by the 0-1
  principle) sorts the keys; keys map back (the map is its own inverse); equal keys are equal bit patterns, so ties
  never matter. `MAX_SORTED = 6`; beyond it the helper.
- **J.4** Exact parts: integers folded with overflow-flagged ops (`IAddOv`, new `IMulOv`), converted with `IToF`; with
  ratios the result is `N/D`, one IEEE division of `IToF(N)` by `IToF(D)` (correctly rounded for |N|, |D| <= 2^53,
  D != 0; N = 0 gives +0). Overflow, out-of-range values or a zero divisor branch to a slow path that calls the helper
  on the same operands (nothing that computed before now declines). A rational inside a sum still uses the helper.
- **J.5** `b^-1` (integer b) becomes a ratio with denominator b directly.
- **J.6** NaN: only the sign and payload of a NaN result can differ (any NaN operand makes the result NaN); tests treat
  all NaNs as one value.

### API additions

LIR ops `IMulOv`, `IMinMax`, `IToF`, `FTrunc`, `FKey`, `FromKey`, `PowI`, `PowF` (both back ends); `ir::MAX_SORTED`,
`ir::NETWORKS`; `rt::pow_int`, `rt::pow_real`, `rt::order_key`, `rt::from_order_key`; example `examples/mandelbrot.rs`
(`cargo run --release -p mc-jit --example mandelbrot -- [n] [reps]`, memo off, checked against the interpreter). No
edits outside the crate.

### Tests (6 new, 1 extended)

- `tests/agree.rs` (7, 4 new): `sums_and_products_are_the_arithmetics` (142 shapes x 400 random tuples: 1-8 reals,
  0-2 integers, 0-2 ratios shuffled; random bits, +-0, +-inf, +-NaN, subnormals, extremes, cancelling pairs, values one
  bit apart, integers at 2^53+-1 and the i64 ends; bitwise against `mc_arith::Num` except NaN; disabling the sort
  fails it); `powers_are_the_arithmetics` (real^int, real^real, real^ratio, int^real, int^ratio, 20,000 tuples each;
  23 constant exponents x 2000 values); `sorting_networks_sort`; `order_keys_follow_the_canonical_order`.
- `tests/engine.rs` (9, 1 new): cases `mandel`, `poly`, `sum6`, `sum8` (helper), `bigx` (overflow slow path, zero
  divisor), `pw` on both back ends and at 4 threads; `the_mandelbrot_grid_runs_inline` (`grid(300) = 1893529`, no
  declines, under 1 s).

### Measurements (busy machine, release, guard)

| Run | Before | After |
|---|---|---|
| `grid(300)`, memo off | 3.69 s (median of 3) | 0.015 s (median of 9) |
| R12c 10^4 / 10^5 / 10^6 | 3.46 ms / 43.5 ms / 529 ms | 2.14 ms / 27.0 ms / 327 ms (integer code unchanged: noise) |

`grid(300)` is about 1.9 million iterations (about 8 ns each); `escape` is 76 LIR instructions and 960 bytes of code.

### Known gaps

More than 6 machine reals in one sum or product, rationals inside a sum, exact bases, run-time rational exponents, and
exact parts beyond 64 bits (or 2^53 with ratios) use the helper (correct, slower); `powf` stays a call; no common
subexpression elimination of repeated powers; NaN sign/payload as J.6.

## Package N0 (numerics foundation, part a; branch `n0-numfn`)

Global constants, lists of known length, closures, and a Rust API to compile an expression in given variables (what
`mc-numfn`'s compiled backend is built on). Decisions are numbered N0.x, shared with `crates/mc-numfn/NOTES.md`.

### What changed

- `front.rs`: values are scalars or lists of known shape (`Val`); list literals (nested), `let` bindings of lists,
  constant indices `y[k]` (negative from the end), `length(y)`, `if` with lists of one shape; arithmetic on lists is
  refused (the interpreter leaves `[1.0, 2.0] + 1.0` unevaluated; only the elementary functions are listable there,
  and they are not threaded here). A list result goes to the output buffer, leaves row-major (N0.3).
  `front_expr(h, &[(Sym, Arg)], body, src, numeric)` compiles `(p1, ..., pn) => body` with `Arg::{Int, Real,
  Vector(n)}` (N0.4). `n(pi)`, `n(e)`, `n(x)` compile everywhere; bare `pi`/`e` only at the top level of a numeric
  `front_expr` (N0.1). `Front` gained `result: Shape`.
- `ir.rs`: HIR statement `S::Out(k, e)`, LIR `Op::Out(k, r)`, `ir::outputs(&Lir)`. `rt.rs`: `Env::out`/`Env::nout`
  (`ENV_OUT`), `Env::with_out`, `Env::write_out` (bounds-checked; the interpreter back end uses it). `jit.rs`: `Out`
  stores to `env.out + 8k` (the host checks the buffer length first); the host ISA is cached in a `OnceLock` (N0.8).
- `spec.rs`: `Built::call_out(ctl, args, out, poll)`, `Built::outputs()`; `Info::shape`; `compile(f)` of a function
  with a list result returns the list (`compiled_info` shows its shape as a string, e.g. `"[Real, Int]"`).
- `expr.rs` (new): `compile_expr(cx, params, body, numeric, backend) -> Result<ExprFn, String>`,
  `compile_closure(cx, f, &[Arg], numeric, backend)`, `closure_parts(h, f)`, `admits(type_name, Arg)`; `ExprFn::
  {args, inputs, outputs, shape, output_types, info, current, call_words, call_f64}`; `Call::{Done, Declined,
  Stopped}`. An `ExprFn` owns a private `Control`; compiled loops poll through `cx.tick` (1b.7); a call declines if an
  inlined function changed version (1b.6).

### Tests

- `tests/agree.rs` (8, 1 new): `random_vector_programs_agree_on_both_back_ends`: 200 random programs writing 1-6
  output words (early writes overwritten later, writes inside branches) x 12 argument tuples: same words and exit on
  Cranelift and the LIR interpreter; a buffer shorter than the outputs declines.
- `tests/engine.rs` (9, extended): cases `vecs` (lists, `let` of a list, nested indices, negative index, `length`,
  `if` over lists, an integer leaf), `npi` (`n(pi)`, `n(e)`, `n(k)` declining beyond 2^53), `jac` (a nested list
  result) on both back ends and at 4 threads; refusals of bare `pi`, arithmetic on a list, a variable index, an index
  out of range (replacing the old `[n]` refusal: list results now compile).
- The agreement of compiled and interpreted evaluation of random expressions and closures with vectors is in
  `crates/mc-numfn/tests/agree.rs` (through the engine).

### Known gaps

- Lists are compile-time trees: no loops over elements with a variable index, no list parameters of named functions
  (their length is unknown), no packed arrays (N4).
- Compiling a small expression still costs 0.24-0.29 ms (one JIT module per function).
