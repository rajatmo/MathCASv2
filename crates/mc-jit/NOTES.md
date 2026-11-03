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
- `adapter.rs`: a temporary copy of package 1a's `register_compiled` hook (to be replaced by 1a's registry when it
  merges).
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
  `Built::call`; `Backend`, `Info`; modules `ir`, `front` (`Source`, `FnDef`, `front`), `rt`, `adapter`; `FUNCTIONS`.
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
- With the adapter, a replaced specialisation's native and code stay in the kernel's append-only native list until the
  session ends (1a's registry drops it).
- Not compiled: complex numbers, lists/arrays, strings, closures, recursion, multi-clause or conditional functions,
  `mod`/`quotient` on reals, global constants (`pi`).
- NaN payload bits may differ between back ends (all NaNs print alike).
- The per-call version check costs one lookup per inlined function.
- No L-class bench cases exist, so none were measured.

## Reconciling with package 1a

1. Delete `adapter.rs` and use `mc_eval::{Compiled, ...}` with 1a's `Kernel::register_compiled(head, Arc<dyn
   Compiled>)`; the call in `Jit::flush` reads the same (`NativeCx` has three lifetimes `'a, 'k, 'h`).
2. Keep `Jit::flush` unless 1a offers registration from a native as an effect.
3. `inspect.rs` reads `Eval`/`Cx` internals (`ev.k.defs`, `cx.reads`): adjust if 1a renames them.
