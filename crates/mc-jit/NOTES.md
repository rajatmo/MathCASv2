# Item 1b: `mc-jit`, the numeric compiler

Status: done. `compile(f)`, `@compiled` and `compiled_info(f)`
work; R12c at m = 10^6 runs in 584 ms (benchmark runner median, check passes); fuel and interrupts stop compiled loops at 1 and
4 threads; compiled results are identical to the interpreter's in every test, on both back ends. On the branch:
workspace 458 passed, 0 failed, 6 ignored; clippy clean.

## What was built

- `ir.rs`: spike E's two-level IR, cut to what the front end produces. HIR types `Int` (exact i64), `Real` (machine
  real), `Bool`; straight-line expressions, many of which may *decline*; statements `Set`, `Eval`, `If`, `While`,
  `For` over a range; the type checker and E.3's if-conversion (only non-declining expressions are speculated).
  Lowering gives LIR with checked integer ops, `IAddOv`, `ToReal`, `Floor`, `Math`, `Num`, `Bail`, and a `Poll` on
  every back edge.
- `interp.rs`: the LIR interpreter (the back end compiled code is checked against, and fallback when Cranelift fails).
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
  `Clone`; the interpreter back end stays for tests comparing the two back ends and as fallback.
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
| R12c (benchmark runner, memo off) | 3.96 ms | 51.3 ms | 584 ms (5 reps, check pass) |
| R12, interpreted | 5.10 s | 59.9 s (1 rep) | stops at the time limit |

R12 unchanged plus `compile(collatz_steps)` (the `map` interpreted, 4 threads): about 0.25 s at 10^6 (one REPL run,
including startup). Compile time 0.7-1.8 ms per function; machine code `collatz_steps` 664 B, `longest` 840 B.

## Found outside the crate

`mc-arith`: for a machine real, `Num::pow_int` with a negative exponent takes the reciprocal and negates the exponent
(`2.0^-1` gives `2.0`; `3.0/7.0` gives `21.0`). Compiled code mirrors it through `num`, so a fix at the source applies
to both (handed to item 9).

## Known gaps

- Calls stay interpreted within the statement that compiles the function (1b.5).
- A replaced specialisation stays in `mc-eval`'s append-only registry until the session ends (1a.7), so its machine
  code is not freed.
- Not compiled: complex numbers, lists/arrays, strings, closures, recursion, multi-clause or conditional functions,
  `mod`/`quotient` on reals, global constants (`pi`).
- NaN payload bits may differ between back ends (all NaNs print alike).
- The per-call version check costs one lookup per inlined function.
- No L-class bench cases exist, so none were measured.

## Reconciled with item 1a

- `adapter.rs` is gone: specialisations register with `mc-eval`'s `Kernel::register_compiled`. Registering is not a
  definition (1a.7), so the version forwarding in `Jit::flush` was removed.
- `compiled_info` reports registrations and run counters, which are not definitions, so it calls the new
  `NativeCx::volatile()` and is never memoised (otherwise a recompiled function kept reporting `stale`).
- `Jit::flush` stays; `NativeCx::register_compiled` (an effect) now exists and would let `compile(f)` take effect
  within its own statement (1b.5) if `compile` used it.

## Item J (JIT real arithmetic)

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

## Item N0 (numerics foundation, part a)

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

## Item N4b: compiled random draws, compiled complex numbers, parallel batches

Status: done. Compiled draws are the
interpreter's bit for bit; compiled machine complex numbers are the interpreter's values bit for bit or decline;
large batches of a compiled expression run split across the session's threads with results identical to
sequential. L9 is new and L10 is a compiled loop; both pass at every size. At completion: full suite 712 passed,
0 failed, 6 ignored; clippy clean for mc-jit, mc-random, mc-numfn. `mc-jit` refuses arithmetic on lists ("a list
where a number is expected"), so the evaluator's list threading of `+`, `*`, `^` cannot disagree with compiled code.

### What was built

1. **Random draws in compiled code** (mc-jit, mc-random): `random_real()`, `random_real(b)`, `random_real([a, b])`,
   `random_normal()`, `random_normal(mu, sigma)`, `random_integer(n)`, `random_integer([a, b])` compile in named
   functions as HIR `E::Draw` / LIR `Op::Draw`, through run-time helpers on the session stream that call the same
   `mc_random::Stream` conversions the natives now use. A count argument and `random_choice`/`random_sample` are
   refused with a reason.
2. **Machine complex numbers** (mc-jit): `complex(x, y)`, `im` and exact complex constants as operands,
   `+ - * /`, integer powers, `abs`, `re`, `im_part`, `conjugate` compiled; other powers, `sqrt` and `exp` ... `tanh`
   on complex arguments through helpers calling the interpreter's own code. Complex values can be locals, branches,
   list leaves and results (`compiled_info` reports `Complex`).
3. **Parallel batches**: `mc-jit/src/batch.rs` (`batch::run`, `Batch`, `pool_threads`) splits one compiled
   expression's calls over the rayon pool; `mc-numfn/src/par.rs`: `Par<'h, I>`, a `NumFn` whose large `eval_flat`
   batches run split, with `I` (normally `Auto`) as the sequential backend; `par_expr(cx, expr, vars, opts)`.
4. Agreement tests extended to draws and complex numbers (IR level and through the engine).

### Decisions

- **N4b.1 Draw order.** Each draw is its own statement where the front end meets the call: arguments first, left
  to right, only in the branch taken (`if`, `and`/`or`, `while` conditions), the interpreter's order. Draws live in
  temporaries, so none is duplicated or dropped (`[r(), 1.0][2]`), and if-conversion never moves one into the
  untaken branch.
- **N4b.2 Drawing code is an effect.** A drawing specialisation calls `cx.effect()` first and declines on a
  speculative task or inside a condition (the interpreter then reports the hole or failure); it holds the generator
  lock for the call and passes the stream in `Env.stream`; a declined or stopped call restores the saved stream
  position, so the interpreter's redo draws the same numbers; a range the native would refuse declines before
  drawing. The generator is attached with `Jit::set_rng`; without it draws are refused; `compile_expr` never draws.
- **N4b.3 Complex numbers are pairs of real HIR expressions** (no new HIR type, `Ty` unchanged). Inline, step for
  step like `mc_arith::Num`: sums and products of two inexact operands without an exact part, or of an exact part and
  one inexact operand (these commute bit for bit); multiplication `(ac - bd) + (ad + bc)i`; integer powers by
  `Num`'s binary powering with the reciprocal `(a, -b)/(a^2 + b^2)` first for negative powers; `abs` as
  `(a*a + b*b)^(1/2)` through the real-power path. Everything else is `S::Pair` (LIR `Op::CNum`/`Op::CMath`) calling
  `Num::sum`/`product`/`pow` or `mc_numeric::Fun::machine`. Exact complex constants compile only as operands, folded
  at compile time; elsewhere they are refused (the interpreter keeps them exact). No `Complex` parameter type (the
  pattern types have none).
- **N4b.4 Additions only.** `Shape::Complex` (two `Real` leaves); `compile_expr` refuses complex results, so
  `mc-numfn` sees reals only.
- **N4b.5 Splitting.** Only compiled `ExprFn`s (pure, immutable) are split. The calling thread and up to
  `threads - 1` pool tasks claim blocks dynamically, each point writing only its outputs. Sequential when the pool
  has one thread, fewer than `PAR_MIN_POINTS` (256) points, or an inlined function was redefined. Declined or
  unfinished points are evaluated in order by the sequential backend on the calling thread: identical to sequential,
  the same at 1 and 4 threads.
- **N4b.6 Fuel and interrupts in split batches.** One fuel unit per point charged first; fuel of loops on other
  threads goes to a shared counter the calling thread charges in its polls and while waiting; a limit or interrupt
  raises the batch's stop word, read by every compiled poll. The calling thread never blocks (it yields; tasks
  starting after the blocks ran out return at once), so nested batches cannot deadlock.

### API additions

- mc-jit `ir`: `Draw` (Real, Normal, Integer), `E::NegF`, `E::Draw`, `S::Pair`, `Pair::{Num, Math}`,
  `Opd::{Complex, CExact}`, `OpdK::{Complex, CExact}`, `Op::{FNeg, Draw, CNum, CMath}`, `ir::draws(&Lir)`.
- mc-jit `rt`: `Env.stream`, `Env::with_stream`, `cnum`, `cnum_words`, `cmath`, `cmath_value`, `draw_value`,
  `draw_real`, `draw_normal`, `draw_int`, `OPD_COMPLEX`, `OPD_CEXACT`.
- mc-jit: `front::Source::draws` (default), `Shape::Complex`, `Shape::has_complex`; `spec::Built::call_with`,
  `Built::draws`, `Jit::set_rng`; `expr::ExprFn::call_detached`, `call_f64_detached`, `draws`; module `batch`.
- mc-random: `Stream::real_in`, `normal_scaled`, `int_in`; `draw::real_range_ok`, `normal_ok` (the natives use them).
- mc-numfn: `par::{Par, ParStats, par_expr, PAR_MIN_POINTS}`.

Edits outside the owned crates: two install lines in `mc-engine` (`mc_random::install` returns the generator,
handed to the JIT with `set_rng`); `mod par`/`pub use` in `mc-numfn/src/lib.rs` and `mc-numfn/tests/par.rs`;
`mc-jit` depends on `mc-random` and `rayon 1.12`; `bench/cases/L/L9.mc` new, `L10.mc` a compiled loop.

### Tests (11 new, 1 extended)

- `mc-jit/tests/agree.rs` (4 new, both back ends): complex pairs against `mc_arith::Num` (over 10,000 results),
  11 complex functions x 2,000 arguments against `mc-numeric`, negation, draws against direct `Stream` calls
  (refused ranges, integer overflow, no stream; stream positions).
- `mc-jit/tests/engine.rs`: 4 complex cases (NaN, infinities, signed zeros) equal to the interpreter, both back ends,
  4 threads.
- `mc-jit/tests/n4b.rs` (4, memo off): 11 drawing functions in 16 seeded programs equal to the interpreter at 1 and 4
  threads (ran compiled; the declining ones declined); the Monte Carlo checksum; draws refused outside compiled
  functions; 150 random complex expressions x 8 points bitwise (over 130 compile).
- `mc-numfn/tests/par.rs` (3): split batches bitwise equal to sequential for 6 expressions at 10, 300 and 5,000 points,
  engine threads 1 and 4; fuel limit and interrupt across a 4-thread batch; speedup measurement.

### Measurements (busy shared machine, release, guard)

| case | interpreted | compiled (benchmark runner median) |
|---|---|---|
| L9 m = 200 | 17.2 s (one REPL run) | 25.9 ms (about 670x) |
| L9 m = 1000 / 3000 | | 637 ms / 5.73 s |
| L10 10^6 | 5.8 s (scalar loop; the old list form 10.3 s) | 38.4 ms (about 150x) |
| L10 10^7 / 10^8 | | 386 ms / 3.89 s |

L9 checksums 1907889, 47555025, 427703486 (equal to the interpreter at m = 40, 200 and to an independent IEEE
reimplementation at 40, 200, 1000). L10 after `seed(2026)`: 786153, 7854611, 78542706 (10^6 equals the interpreted
loop). Parallel split of a Mandelbrot expression, 40,000 points: 1.7x at 2 threads, 2.5x at 3, 3.1x at 4.

### Known gaps

- Draws: expressions (`compile_expr`, `mc-numfn`) never draw and drawing code is never split; per-element streams for
  `pmap`/`ptable` (mc-random gap 1) not built; no fuel per draw (loop polls only).
- Complex: no `Complex` parameter type, no complex values into inlined functions; `compile_expr` refuses complex
  results; exact complex values outside sum/product operands, `arg`, comparisons of complex numbers and `n(z)` refused;
  exact complex constants with rational parts refused; three or more inexact operands and non-integer powers go
  through the helper (correct, slower); NaN sign/payload may differ (J.6).
- Batches: `Par` is opt-in (`compile_expr`/`Auto` do not use it: N1-N3 integrate it by calling `par_expr`); the
  256-point threshold is fixed; the calling thread yields while waiting.
- Bench: `--only` takes one id per guard call (guard splits arguments on commas).

## Round 7 item P: vectors in compiled code

Status: done. At completion: full suite
781 passed, 0 failed, 6 ignored; clippy clean. The mc-random half is described in `crates/mc-random/NOTES.md`
("Round 7 item P").

### What was built

Compiled functions take vectors of run-time length. An **untyped parameter** of a compiled function is a vector of
machine numbers; the function compiles one variant per leaf type (reals, integers), whichever compile. A call reads
a packed payload without unpacking; a short list of all machine reals or all machine integers is copied element by
element. On a vector `v`: `v[i]` with any integer index (negative from the end as `part`; declines outside),
`length(v)`, `sum(v)`, `total(v)`, `for x in v`, passing `v` to an inlined function's untyped parameter,
`let w = v`, `if c then v else w`, and `table(e, n)`, `table(e, k, n)`, `table(e, k, v)` of machine numbers (a new
vector inside the call). A vector result comes back as one packed array (`real_array`/`int_array`). Refused with a
reason: arithmetic on vectors, vectors inside list literals, `var` vectors, `table` of lists or booleans, nested
`table` iterators. New IR: HIR `E::Elem`, `E::VSum`, `S::VNew`, `S::Store`, `S::VOut`; LIR (both back ends) `Load`,
`Store`, `VNew`, `VOut`, `VSum`; run-time `rt::vec_new`, `vec_out`, `vsum_real`, `vsum_int`, `sum_reals`, `sum_ints`,
`MAX_VECTOR`.

### Decisions

- **P.2 Vector parameter = untyped parameter.** mc-eval's pattern types have no list type (`type_of` accepts scalar
  names only; `v: List`, `v: Reals`, `v: List[Real]` are `DefError::UnknownType`), so until that changes an untyped
  parameter of a compiled function is a vector; using it as a number is refused ("a vector where a number is
  expected ..."; the old "has no type (Int or Real)" message is gone, `tests/engine.rs` updated).
- **P.3 One variant per leaf type**: the real-leaf variant compiles first; the integer-leaf variant too when the
  function has vector parameters or the first failed; all vectors of one call share one leaf type, which selects the
  variant (mixed leaf types decline). `compiled_info` shows `params -> ["List[Real]"]`, `vector_leaves -> [Real, Int]`.
- **P.4 `table` follows the library**: computed in order (k = 1..n or over the vector); `table(e, k, n)` with n <= 0
  is `[]`; `table(e, n)` with n < 0 or above 2^26 (`MAX_CELLS`) declines so the interpreter gives the failure.
- **P.5 Non-packed arguments**: a packed rank-1 payload is borrowed for the call; a compound list of at least one
  element, all machine reals or all machine integers, is copied (always under 64 leaves, longer ones are packed);
  everything else declines (empty lists, mixed leaves, big integers, matrices, numbers).
- **P.6 Vector results** are moved out of the call's environment (copied when the result is an argument's own
  payload) and built as one packed array.
- **P.7 `sum`/`total` of a vector** are exactly the library's: integers summed exactly in 128 bits (decline if not
  `i64`); reals sorted ascending then folded from +0.0 (decline on NaN or an empty vector); fuel n/4 charged through
  the host poll first.
- **P.8** Index checks are lowered to existing operations (wrapping add, select, compare, bail) followed by an
  unchecked `Load`; a `Store` is checked against the length too.

### API additions (mc-jit)

`front::{Param, Input, front_vectors}`; `FnDef.params: Vec<(Sym, Param)>` (was `(Sym, Ty)`, used only in mc-jit);
`Front.inputs`, `Shape::Vector(Ty)`, `Info.inputs`; `Built::call_vector`, `Env::take_result`; the IR and run-time
items above; re-exports `Input`, `Param`; `compile_expr` refuses vector results; `compile`'s `FUNCTIONS` text
mentions vector parameters.

### Tests

`mc-jit/tests/packed.rs` (6): 15 functions x 75 calls agree with the interpreter on Cranelift and on the LIR
interpreter (packed and short vectors of reals and integers, -0.0, NaN, near-`i64::MAX` overflow, out-of-range and
`i64::MIN` indices, empty, mixed, matrix and number arguments; all ran compiled, decline counts checked); same at 4
threads and inside `map` on speculative tasks; variants and `compiled_info`; 10 refusals with reasons; the statistics
kernel with seeded draws equals the interpreter; fuel stops vector loops, `table` and `sum` at 1 and 4 threads.

### Measurements (`cargo run -p mc-jit --example moments -- 10000000 3 100000`, memo off)

| case | interpreted | compiled |
|---|---|---|
| `mc(n)`: draw, mean, variance via `table` and `sum`, n = 10^7 | about 50 s (scaled from 10^5) | 0.887 s |
| `mc(10^5)` | 0.511 s | 9.9 ms (52x, equal values) |
| `moments(w)`, indexed two-pass loop, 10^5 packed | 1.018 s | 6.6 ms (154x) |
| `moments(v)`, 10^7 packed | | 1.48 s, of which 1.32 s is the call itself (gap 1); the loop about 0.16 s (8 ns per iteration, `powf` for `^2`) |

### Known gaps

1. **Call overhead in mc-eval**: calling any named function with a 10^7 packed argument costs about 1.3-1.5 s before
   the body runs (interpreted `fn one(x) = 1` 1.5 s; compiled `fn vl(v) = length(v)` 1.4 s; a closure call 0.2 ms;
   `length(v)` 0.2 ms): probably an unpacking `view` on the named-call path. `[v]` repacks the payload into a rank-2
   array (copying it), by the packing rule.
2. Vectors only (rank 1): no matrices, no variable index into a list of compile-time length, no arithmetic on
   vectors, no `var` vectors.
3. A vector argument with mixed integer and real leaves, or an empty list, declines.
4. `x^2` stays a `powf` call to match the interpreter bit for bit (J.1).
5. `sum` on a vector sorts a copy (as the library): O(n log n) and a transient copy.
6. Draws inside compiled code never run in parallel (as N4b).
7. With several vector parameters only the all-real and all-integer variants exist.
8. Needs: mc-eval `type_of` accepting `List[Real]` / `List[Int]` (04-syntax section 9) for typed vector parameters;
   mc-core an owned packed constructor (`real_array_owned(dims, Vec<f64>)`, as `intern_owned`), since `array_of`
   copies the leaves (producer and result peaks about 2x the payload); a safe point must not run inside `cx.tick`
   while compiled code holds payload borrows.

## Round 13: library calls, data and constants, common subexpressions

Compiled code now covers what research computations put in solver functions: special
functions, step functions and `piecewise`, interpolations of every scheme and their derivatives, solutions of
`n_dsolve`, functions given associations, global constants and `pi` beside machine reals. Results stay the
interpreter's bit for bit (declines otherwise).

### What was built

- `callee.rs` (new): a process-wide registry of **library functions** compiled code calls by number (HIR `E::Call`,
  LIR `Op::Call`, helper `mc_jit_call`, both back ends): `special(name, arity)` (every function of mc-special
  through its native's own machine path, and the formula functions of `FORMULA_FUNCTIONS`), `interpolation(xs, ys,
  ms, scheme, order)` (local scheme and derivatives through `mc_numeric::spline`), and `add_resolver` for other
  crates' curried heads (mc-ode registers `interpolating_function(...)` solutions).
- Front end: `piecewise`, `unit_step`, `heaviside_theta`, `sign` (exact integer values, declining where the
  interpreter leaves the call); `derivative(n)(f)` of interpolations and solutions; data arguments of inlined
  functions (`r["f"](x)`, `r["v"][2]` folded at compile time); global numeric constants; closed numeric operands
  beside machine reals; untyped parameters of inlined functions taking numbers, complex numbers and lists; common
  subexpressions.

### Decisions

- **LC.1** One library-call instruction for every function of machine reals outside the elementary ones: up to four
  arguments, integers converted (declining beyond 2^53), rational literals rounded once (`Real::to_f64`, as the
  native converts exact arguments of a machine computation). A special function with exact arguments only is
  refused (the interpreter keeps it exact).
- **LC.2** Callees are registered once per key (name and arity; table content hash; solution key and component) and
  never removed, like `mc_numeric::spline`'s registry.
- **LC.3** A call with a non-finite argument, or whose value is not a finite machine real (complex, NaN, an
  infinity, outside the real domain, outside a table), declines; the interpreter gives its own answer.
- **LC.4** Formula functions of mc-numeric's library are called only when their interpreted value is the library's
  machine value bit for bit, checked by `tests/library.rs` over argument grids: `bessel_k_scaled`, `bessel_i_scaled`,
  `product_log`, `fresnel_s`, `fresnel_c`, `asinh`, `acosh`, `atanh`. Left out (last-bit differences): `dawson_f`,
  `log10`, `log2`, the reciprocal functions.
- **LC.5** Solutions of `n_dsolve` (mc-ode): the resolver finds the solution by its key in any live session's store,
  holds it weakly (the store owns it), and evaluates the component exactly as `Interpolating::at` does for machine
  data; Taylor (ball) solutions are not compiled.
- **LC.6** Common subexpressions: the value of a pure call (arithmetic, comparisons, elementary, special and inlined
  functions; not `if`, blocks, bindings, loops, draws, `table`, `piecewise`, `and`, `or`) is held in a temporary and
  reused for the same term in the same binding environment while its statement list is open; assignments and loops
  forget all of them. Large right-hand sides and Jacobians, whose terms share most subterms, no longer exceed the
  100,000-node limit, compile 5-10 times faster and run faster.
- **LC.7** Data arguments: an untyped parameter of an inlined function given data (a string, an association, an
  interpolation, a solution, a list holding such data, a global holding one) has the data put in the body; `part` by
  constant keys or indices and calls of the data are folded at compile time; the caller's locals the data names are
  carried in under fresh names.
- **LC.8** A global holding a number (machine real, integer, exact rational) is read as a constant, the read
  recorded so that redefining it invalidates the code (1b.6).
- **LC.9** Contagion for constants: a closed numeric operand (`pi`, `pi^-3`, `sqrt(2)`) of a sum or product with a
  machine real among the other operands is `n` of the operand (computed once at compile time by `numericize`), as the
  evaluator's `add`/`mul` natives replace it; without a machine real it stays exact and is refused.
- **LC.10** `piecewise` is the nested `if` the native evaluates; branches of an integer and a machine real are
  still refused (the exact default 0 of `piecewise([[x, c]])` is not a machine real).

### Tests

`tests/library.rs` (8): every special function of mc-special (30 of one, 20 of two, 4 of three arguments and
`hypergeometric_2f1`) over grids with poles, negative arguments, NaN and infinities, compiled on both back ends
equal to the interpreter; exact orders and rational arguments; steps, signs and `piecewise` with signed zeros and
NaN; local interpolations of degrees 1-4 and derivatives of splines and local interpolations (invalidation by
redefining the global); solutions and their derivatives in compiled functions and integrals; right-hand sides with
special functions and steps giving the same solution (same content key) compiled and interpreted; common
subexpressions with loops, assignments and branches; constants, globals (invalidation), data arguments, untyped
parameters. `tests/engine.rs`: the `x*pi` refusal became `n*pi` (exact).

## Round 13: sums and products of up to 64 machine reals inline

`ir::MAX_SORTED` is 64 (was 6): sizes above `SMALL_NETWORKS` (6) sort through `ir::network(n)`, Batcher's
odd-even merge network for the next power of two without the comparators that touch the padding. A sum of 8
machine reals had called the arithmetic (about 2 us); now it is inline (25 times faster). Decision quad-1.11 in
`crates/mc-quad/NOTES.md`.
