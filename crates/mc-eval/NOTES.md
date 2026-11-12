# Item F: `mc-eval`

Status: built. `cargo test` passes in debug and release (9 unit, 12
semantics, 1 syntax, 9 workload tests); clippy and `cargo doc` clean with `#![warn(missing_docs)]`. Depends on
`mc-api`, `mc-core`, `rayon`, `rayon-core`, `parking_lot`, `rustc-hash`; not on `mc-arith` or `mc-syntax`. Every
"done when" item is met: I2's cases (R4, R5, R13, R14, S7, R14e native and evaluating condition, MR, MR fuzz) give
results identical to the sequential run on `mc-core` at 1, 2 and 4 threads, lazy and always-split, also under
`Config::stress()` and with the memo off; an effect inside a condition gives `failure(effect_in_condition)`;
speculative fuel exhaustion alone never changes a result.

## What was built

- **`pat`**: patterns (blank, typed, sequence, literal, compound, alternatives, `named`, condition), compilation with
  the definition-time checks, class order, AC flattening and literal folding (6.1, 6.2), and the sound, incomplete,
  bounded instance check (6.3). Patterns hold their terms as `Raw`.
- **`matcher`**: spike 2's continuation-passing matcher with the pruning AC strategy; conditions run with the task's
  context and may evaluate; checked against the naive enumeration in a unit test.
- **`index`**: discrimination tree and AC key-set filter, updated in place by a single writer (ranks and retirement as
  I2.4, I2.9), keyed by structural hash with handle verification (A.2), traced.
- **`defs`**: per-symbol own values, attributes, native slot, rule sets and argument-head rule sets in a lazily
  segmented per-symbol table.
- **`eval`**: `Kernel`, the session state and a `Trace` root. Speculative groups with holes and read-set validation;
  memo through `Heap::memo_get`/`memo_put` (start epoch plus read set, `Heap::mask` of term and value as the
  normal-form stamp); argument-head rules before the head's rules; failures propagate through strict arguments; AC
  heads canonicalised; heads evaluated (4.1 step 3.1).
- **`ctl`**: the control block `#[repr(C)] Control { fuel: AtomicI64, stop: AtomicU32, status: AtomicU32 }` (offsets
  0, 8, 12 asserted at compile time); `Status` uses spike E's codes plus `Depth = 5`; interrupted →
  `failure(interrupted)`, fuel → `failure(limit, fuel)`, depth → `failure(limit, depth)`.
- **`native`**: the native-function interface (below).
- **`builtins`**: `iadd`, `imul`, `isub`, `iquo`, `imod`, `less`, `less_equal`, `greater`, `greater_equal`, `same`,
  `if` (branches held), `map` (a speculative group), `print`, `define`.
- **`syntax`**: the term shapes of `mc-syntax` (mapping at the end).
- **`order`**: the `TermOrder` hook with a structural stand-in.
- **`workloads`** and the `mc-eval-bench` binary: I2's cases as sessions; `[k/N pct%]` progress; exit status 1 on a
  mismatch.

## Public API

- `session(heap_cfg, threads, EvalConfig) -> Session<Kernel>`; each top-level evaluation is one `Session::step` with
  the safe point after it.
- `Kernel::eval(&mut self, h, t) -> Evaluation { value, status, stats }`, `eval_with(h, t, fuel)`.
- Definitions: `set_own`, `set_attributes(Attributes { hold, ac, effect, handles_failures })`, `define(h, Pat, Rhs)`,
  `define_attached(h, Pat, Rhs, Sym)`, `define_term(h, Pat, body, names, vars)`, `register(sym, Attributes,
  Box<dyn Native>)`, `set_order`, `compile_pattern`.
- Inspection: `output`, `value_of`, `version`, `rule_count`, `rule_order`, `epoch`, `make` (canonical construction),
  `control()` (`Control::interrupt` from any thread), `roots: Roots` for session history.
- `Rhs = Arc<dyn for<'h> Fn(&Binds<'h>, &'h Heap) -> T<'h>>` (captures only symbols).
- Conditions: `cond(p, &[vars], |cx: &mut CondCx| ...)`; `CondCx` offers `one(i)`, `seq(i)`, `value(sym)` (recorded
  read), `eval`, `is_true`.

**Native interface (for `mc-arith`).** A `Native: Send + Sync` implements `call(&self, cx: &mut NativeCx, t, args) ->
Res<Reply>`, where `Reply` is `Done(normal form)`, `Again(replacement, evaluated in the loop)` or `Decline` (rules
tried next); optionally `trace` for terms it keeps. Arguments arrive evaluated per the hold mode, failures already
returned, AC heads canonicalised. `NativeCx` offers `tick(n)` (fuel and interrupt), `eval` (reads recorded), effects
(`effect()`, `emit`, `set_own`, `define`, `define_term`, `set_attributes`; refused speculatively, leaving a hole, and
inside conditions), `rewrite_once`, `matches(&Pattern)`, `group(n, cost, f)` (a speculative parallel group), `make`,
`bool`, `list`, `failure`, `head_cost`, `is_effect`. `mc-arith` plugs in by registering `add`/`mul` (with `ac`),
`pow` and the comparisons, and calling `set_order`; its natives replace the built-in machine-integer ones.

## Tests

- **workloads** (I2's cases): the sequential run against 13 configurations (collect or stress, 1/2/4 threads, lazy or
  always, plus memo off).
- **semantics**: effects in conditions (direct, nested) fail with output and own values unchanged; the condition-scope
  definition error; effects once, in list order, under parallel `map`; a genuine fuel limit fails identically
  everywhere; speculative tasks on stale definitions that loop forever exhaust their shares and leave holes, with an
  identical result and charged fuel within budget; interrupts at 1 and 4 threads; the depth limit (default 20000
  reachable in debug); failure propagation, rule order and replacement, built-ins, hold modes; definitions with young
  literal terms survive stress collections, retired parts are freed.
- **syntax**: `define_fn`/`define_rule`/`set_delayed`/`set`/`define_sym` terms (recursive factorial with typed and
  conditional patterns, sequence and named variables, an AC symbol), a malformed definition; identical at 1 and 4
  threads under stress.
- **unit**: concurrent index (one writer, three readers), handle-verified edges under code collision, pruning equal to
  naive AC matching, lowering scope checks, `conc`, `ctl`.

## Measurements (benchmark sizes, median of 3, one 93 s guarded run, peak 501 MB, busy machine; ms)

| case | ref | seq | lazy 2 | lazy 4 | speed-up at 4 |
|---|---|---|---|---|---|
| R4, 10^4 rules / 10^4 subjects | 206 | 207 | 105 | 55 | 3.78 |
| R5, 5·10^4 rules / 10^6 calls | 370 | 67 | 43 | 29 | 2.31 |
| R13, 10^3 kinds / 10^6 calls | 408 | 394 | 211 | 117 | 3.36 |
| R14, 10^6 | 1248 | 712 | 423 | 246 | 2.90 |
| S7, 10^6 | 130 | 128 | 115 | 76 | 1.69 |
| R14e native, 2·10^5 | 328 | 135 | 79 | 47 | 2.90 |
| R14e evaluating, 2·10^5 | 1895 | 1889 | 1151 | 654 | 2.89 |
| MR, 2·10^5 | 281 | 55 | 67 | 50 | 1.11 (1.5·10^5 elements redone, as in I2) |
| MR fuzz, 20 × 3000 | 88 | 77 | 70 | 56 | 1.38 |

Without memo, charged fuel is identical in every configuration (e.g. MR at 37484): exact accounting.

## Decisions

- **F.1** `Kernel` is the session state and root container; `eval` takes `&mut Kernel` (one frontier per kernel, I2.5
  by types); `Eval` carries separate lifetimes for kernel and heap, so results stay `T<'h>`.
- **F.2** Own values are boxed `Raw` behind an atomic pointer, retired until a safe point. Tree edges are keyed by a
  structural-hash code plus kind and arity; the edge record holds the `Raw` and is compared on lookup; collisions
  chained. The AC filter keys on `(kind, hash)`; its multiset test compares handles.
- **F.3** Fuel follows A.5: only the frontier draws from `Control.fuel`, in blocks of 1024 (E.5) by compare-and-swap
  (never overdraws). A speculative task spends a private share; exhausting it leaves a hole. The frontier completes a
  speculative part in order, charging each kept element, and redoes an element when it is a hole or when a definition
  since the split (including one made while filling an earlier hole) touched its read set. Holes and redone work are
  never charged.
- **F.4** The status-to-failure mapping above; statuses 3 and 4 reserved for compiled code.
- **F.5** Matching charges 64 fuel per 1024 matcher steps, which also polls the control block.
- **F.6** A condition declares the variables it reads (by position); compilation rejects an outer variable (6.1),
  alternatives binding different variables, a variable used as both single and sequence, and sequence variables
  outside argument positions.
- **F.7** Patterns are capped at 10^4 nodes and depth 256; only pattern structure is recursed; every walk over subject
  terms is iterative.
- **F.8** AC subjects with more than 64 arguments match no AC pattern; AC patterns have at most 32 arguments.
- **F.9** A structural stand-in canonical order (numbers by `f64` approximation, then kind, then exact within a kind);
  total. To be replaced by `mc-arith`'s order.
- **F.10** Granularity estimate: the larger of (arity + 1) and the head's learned cost, 0 for heads declared `effect`;
  the hint is scaled from the first 8 elements.
- **F.11** Logical evaluation depth is limited (default 20000), giving `failure(limit, depth)` (a hole on a
  speculative task). This deviates from semantics 4.2 and the plan: the evaluator recurses per evaluation level.
- **F.12** `session()` builds a throwaway kernel against a scratch heap, then replaces it (`Session::new` cannot build
  the state from its own heap).
- **F.13** Minimal built-ins work on machine integers only (checked, declining on overflow).
- **F.14** The `mc-syntax` shapes are supported as patterns plus definition natives; term right sides and term
  conditions are substituted, then evaluated.
- **F.15** An effect reached inside a condition aborts with `EffectInCondition` on any task, becoming the failure value
  at the outermost dispatch or native call; a speculative task holes instead.
- **F.16** Validation uses `last_change[bit]`, the last definition epoch of each of the 64 read-set bits (at most 64
  checks, no epoch limit, conservative).
- **F.17** AC heads are canonicalised on every evaluation; dispatch on symbol heads only.

## Contract changes requested (local workarounds in place)

- `mc-core`: `Raw` `repr(transparent)` with `to_bits`/`from_bits` (traced atomic slots without boxes).
- `mc-core`: a `Session` constructor that builds the state from the heap.
- `mc-api::names`: `depth`, `definition` (the others now exist in `names::failure`).
- Holes use `T::from_raw(0x7FFF_FFFF)` (reserved by `mc-core`): bless it or add a constant.

## Known gaps

- The evaluator is recursive per evaluation level (F.11).
- No time or memory limits yet; failures do not include the `partial` term.
- An interrupt raised before an evaluation begins is cleared by its start.
- Not supported: `optional` and `held` patterns; `block`/`let`/`var`/`fn` closures and `match`; hygienic renaming
  (6.6); rules on compound heads; `complex` in `number`; rule decorators and `define_fn` options; result types are
  not checked.
- Epochs are `u32`; wrap-around is not handled.
- Single-thread cost above the spike's figures (not profiled; candidates: per-App symbol-table lookups, learned-cost
  update).
- The memo is weak in values: results not kept in roots lose their entries at the next collection.

## What integration needs

- `mc-arith`: register natives through `Kernel::register`, call `set_order`.
- Glue / REPL: lower with `mc-syntax`, evaluate lowered terms in `Session::step` (definitions install themselves
  through natives); history in `kernel.roots`; `control().interrupt()` for stop; `eval_with` for per-evaluation fuel;
  show `Evaluation.status`.
- Compiled code (spike E) can share `Kernel::control()`; its fuel draws become the frontier's or follow F.3.

## How the `mc-syntax` shapes map

- `any()` / `any(x)` → `_` / `x_`; `seq1(x)` / `seq0(x)` → `x__` / `x___`.
- `typed(any(x), T)` → `x_ : T` for `integer|Int`, `rational`, `machine_real`, `ball`, `real`, `number`, `string`,
  `symbol` (lower-case or capitalised).
- `named(x, p)` → `x @ p`; `condition(p, c)` → a term condition (reads only variables bound in `p`); `alternatives`
  → `|`.
- `set_delayed(s, e)` → own value `e`; `set_delayed(lhs, e)` → rule `lhs := e`; `set(...)` evaluates its value first.
- `define_rule(rule_delayed | rule(L, R), list())` → `L := R` (`rule` evaluates `R` first); `define_fn(lhs, body,
  list(), result type, list())` → `lhs := body`; `define_sym(s, list(decorators))` accepts `associative`+`commutative`,
  `held`, `effect`.
- `optional`, `held`, and other decorators give `failure(definition, "...")`; sequence variables stand for `list(...)`
  in right sides.


# Item H: language features (added to `mc-eval`)

Status: built. Closures, blocks, loops, `match`, `rewrite` and
the definition features work through `mc-engine`; results are identical under `mc-core` stress collection at 1 and 4
threads, lazy and always-split, memo on and off; R2, R9 and R12 evaluate their language parts correctly (library stubs
for missing functions). The native interface stayed source-compatible (additions only).

Shared files changed: `bench/cases/R/R2.mc` (the rule uses a block, because definitions return `null`:
`rule fib(k_: Int) := { let v = fib(k - 1) + fib(k - 2); fib(k) = v; v }`) and `bench/cases/R/R1.mc`
(`#@ memo: off`, so it measures dispatch).

## What was built

- **`subst.rs`**: hygienic substitution (semantics 6.6), iterative, skipping subterms by mask, sharing work across
  repeated subterms, with a fast path for terms without binding forms. Binding forms: `fn(list(params), body)`;
  `block` with `let`/`var`; `for`; `rule`/`rule_delayed`; `set`/`set_delayed` with a compound left side;
  `define_fn`. A bound name shadows the substitution in its scope; a bound name that would capture a substituted value
  is renamed (H.2). All rule right sides and term conditions go through it.
- **`cells.rs`**: the mutable lexical locals behind `var` (H.3–H.5).
- **`lang.rs`**: natives for `fn` (a closure is a normal form), `block`, top-level `let`/`var`, `assign`,
  `local_cell`, `while`, `for`, `match`, `rewrite`, `check_options`, `option_value`; `rule_delayed` and `condition`
  hold their second argument.
- **Evaluator:** closure application when the evaluated head is `fn(list(ps), body)`: arguments evaluated except at
  `held` parameters; `typed` checked; `optional` defaults evaluated, seeing earlier parameters; `rest` takes the
  remaining arguments as a list; the body is substituted and evaluated in the evaluator loop (a tail call); a call the
  parameters do not accept gives `failure(no_match, closure, [args])`. `listable` threads over list arguments
  (`failure(shape_mismatch)` for unequal lengths). Identity elements applied in `make`. Evaluations that read or
  allocate cells are never memoised. `if(c, a)` gives `null` when `c` is false.
- **Definitions:** direct `held(p)` arguments set per-position holding; direct `optional(p, d)` arguments expand into
  rules with and without the argument; `define_fn` options after `;` (including `failure(unknown_option, name)`); `fn`
  installs a catch-all `f(args___) := failure(no_match, f, args)` guarded by `is_ground(args)` (2026-11: a call
  with a symbolic argument stays unevaluated, so `find_root(h(x) - 4, ...)` works for guarded `fn`s); `rule` keeps an open rule set; decorators
  `listable`, `effect`, `held`, `handles_failures`, `associative` + `commutative`, `identity(e)`, `on(s)` on
  `define_rule`, `define_fn` and `define_sym`.
- **`rewrite(e, rules[, strategy: once | fixed_point | innermost])`:** `once` (default) is one pass over all subterms,
  outermost first; `fixed_point` and `innermost` repeat passes, evaluating after each, until nothing changes (fuel
  bounds the passes); every walk is iterative.

## Public API additions

- `Hold::Positions(u32)`, `Hold::mask`, `Hold::from_mask`; `Attributes::listable` (attributes stored as `u64`);
  `Kernel::set_identity`.
- `mc_bench::Evaluated::output: Vec<String>` (struct literals updated across the repo); `Engine::configure(option,
  value)` with a default error; the `#@ memo: on|off` case directive (`child.rs` calls `configure("memo", "off")`; an
  engine without support runs the case with a note).
- `CoreEngine::configure` for memo, and `CoreEngine::install(f)` to register natives (e.g. a library's `install`).
- The REPL prints output lines before the result.

## Tests

- `crates/mc-engine/tests/language.rs` (4 tests): a 50-entry session covering every feature; the same session at 1
  thread, 4 threads lazy and 4 threads always-split, each under stress collection, memo on and off, all identical;
  cells under speculation (a frontier cell read by many speculative tasks, private cells per element, closures
  escaping from map elements); R2 (m = 1000 release, 200 debug), R9 (10^4 → 10001), R12 (10^4 → 261), with stub
  natives for `mod`, `max`, `range`, `nest`, `depth`.
- `crates/mc-repl/tests/sessions/language.txt`: a 50-input REPL transcript (`core_sessions.rs` runs both transcripts).
- Unit test of hygienic substitution including a 200,000-deep term; mc-bench case-directive tests.

## Measurements (busy machine, release)

| Run | Result |
|---|---|
| R1, memo off | fib(30) 3.7 s, about 2.7 µs per call; fib(20) 30 ms |
| R2 | 10^3 in 8.9 ms, 10^4 in 111 ms, both pass; 10^5 stops at the depth limit |
| Interpreted loop | about 1.3 µs per `while i < n { i = i + 1 }` iteration (general evaluation and arithmetic, not cells) |

## Decisions

- **H.1** Lexical scope by substitution: `let` substitutes its value into later statements (one substitution per
  statement); closures capture by having those values substituted into them.
- **H.2** Renamed binders become `y$k`, with the smallest `k` occurring neither in the substituted values nor in the
  form (deterministic, not writable as a plain identifier).
- **H.3** `var x` stands for `local_cell(id)`; reading gives the stored value (a normal form, not re-evaluated); cells
  are not definitions and carry no version; an evaluation that reads or allocates a cell is volatile (not memoised).
- **H.4** Two id spaces keep printed results deterministic across thread counts: the frontier allocates the smallest
  free positive id (blocks free their cells at the end unless the value or an effect may hold them); speculative tasks
  use negative task-owned ids, and a speculative result containing one becomes a hole.
- **H.5** Only a cell's owner writes it (anyone else leaves a hole); a frontier write to a cell a speculative task may
  be reading bumps the read-set bit of `local_cell`; inside a condition only cells allocated within it may be written.
- **H.6** Holding is a property of the function (the union of all clauses' `held` positions).
- **H.7** Optional parameters (at most 6 per clause) expand into rules with and without the argument; the default is
  bound by `let`, which also checks a declared type.
- **H.8** Options after `;` add a variant with a trailing `named(options$, options(___))` argument; the body is wrapped
  as `block(check_options(...), let(name, option_value(options$, "name", default)), ..., body)`.
- **H.9** In blocks, `while` and `for`, a failure statement ends the construct with that failure; a non-boolean
  `while` condition gives `failure(not_boolean, c)`; a typed `let` mismatch gives `failure(type_mismatch, x, v)`.
- **H.10** `let`/`var` outside a block define the symbol's own value; `assign` on a symbol sets its own value and
  returns the value.
- **H.11** `for` iterates `range(n)`, `range(a, b)`, `range(a, b, step)` with machine-integer bounds directly (inclusive;
  `range(n)` = 1..n) without building the list.
- **H.12** A block whose cells allocated nothing returns its last statement for the evaluator to evaluate (tail calls
  through blocks and `if`: `deep(100000)` works); blocks with cells evaluate the last statement themselves.
- **H.13** `identity(e)` requires `associative` + `commutative`; `on(s)` is not allowed on a `sym` declaration.
- **H.14** Patterns in `match` and `rewrite` are compiled on every call (no cache).

## Known gaps

- The evaluator still recurses once per evaluation level (depth limit): R2 at 10^5 reports it; R9 at large sizes
  relies on the memo.
- Result types of `fn` and parametric parameter types (`List(Int)`) are not checked.
- Cell assignment counts as an effect for purity, so speculative tasks can only write cells they allocated.
- A failure value containing a cell keeps it alive for the session; escaping frontier cells are never reclaimed.
- `replace_at` was not done.

## Item 9 (library and kernel gaps)

- Additions in `defs.rs` only: crate-private `Defs::clear_arg_rules(holder, outer)` and
  `pub fn Kernel::clear_attached(&mut self, holder, outer)` (counts as a definition of both symbols), for 9.6.

## Item 1a (explicit-stack evaluator and evaluator speed)

All targets met: R2 at m = 10^5 passes through the benchmark runner (1.996 s median, check pass, peak 2.4 GB); a 10^6-deep
linear recursion passes (1 and 2 threads, lazy and always-split); the native-interface additions and
`register_compiled` exist with tests. At completion: workspace 455 passed; clippy clean.

### What was built

- **`machine.rs`: evaluation on an explicit, heap-allocated stack.** `Frame::Eval` is one evaluation level (start term
  = memo key, the term being reduced, saved read set, effect, fuel and cell counters, a `Stage`: waiting for its head,
  its next strict argument, a symbol's own value, or a form above it). Argument values go on a per-task value stack
  (`Cx::vals`); the driver (`Eval::eval`) moves values up and aborts down; replacements loop inside one level. Memo
  stamps, read sets, learned costs, holes, validation, effect-in-condition conversion and failure propagation follow
  the old code step for step.
- **`lang.rs`: `block`, `while`, `for` are frames** (`Form`, registered with the internal `Kernel::register_form`), so
  recursion through a non-tail block statement or a loop body adds no Rust frames; a one-statement block that binds
  nothing is that statement (tail call, H.12); block states pooled per task.
- **`compiled.rs`: the hook for item 1b.** `trait Compiled: Send + Sync { fn call<'h>(&self, cx: &mut
  NativeCx<'_, '_, 'h>, args: &[T<'h>]) -> Option<T<'h>>; }`; `Kernel::register_compiled(&mut self, head: Sym, c:
  Arc<dyn Compiled>)`, `remove_compiled`, `compiled(head)`, and `NativeCx::register_compiled(head, c) -> Res<()>` (an
  effect of the frontier). A slot per symbol in `SymDefs`; specialisations in an append-only table.
- **Native-interface additions (item I's requests):** `NativeCx::compile_pattern(Pat)`,
  `NativeCx::match_binds(&Pattern, t, &mut Binds)`; `NativeCx::cmp`/`sort` and `Kernel::cmp`/`sort` (canonical order);
  `Kernel::stop_flag()`, `Kernel::set_stop_flag(Arc<AtomicBool>)`, `NativeCx::stop_flag()`, `NativeCx::stopped()`;
  `Native::reads_term()` (provided, default `true`).
- **Speed:** natives that do not read their term are called before the evaluated call is built; AC canonical forms,
  rule right sides and small substitutions (`subst::plain_small`) on fixed buffers; a pair of arguments that cannot
  split skips the group bookkeeping. `examples/speed.rs` measures R1, R1i, LOOP, LOOPi, R12, R2, DEEP with progress.

### Decisions

- **1a.1** `EvalConfig::max_depth` is a frame limit sized by memory: optimised builds `DEFAULT_STACK_BYTES` (256 MiB)
  over two 72-byte frames per level (about 1.86 million levels), unoptimised 2^17; still `failure(limit, depth)` (a
  hole on a speculative task). A runaway recursion uses about 1.3 GB before it stops. Replaces F.11.
- **1a.2** Native callbacks (`NativeCx::eval`, conditions, split-group elements) still nest Rust frames, bounded by
  `EvalConfig::max_nest` (10,000 optimised, 1,000 unoptimised; 10,000 fit the pool's 64 MB stacks); over it,
  `failure(limit, depth)`.
- **1a.3** `block`, `while`, `for` run as frames; `has_native` stays true for them; attributes unchanged.
- **1a.4** Groups stop splitting at callback nesting `SPLIT_NEST` (48): deep recursions split near the root only.
- **1a.5** A pair of arguments takes the split path only if the lazy rule could split it; a group needs two evaluated
  arguments. This fixed a held-cell waste (`assign(local_cell(-k), ...)` made every speculative element a hole): R12
  at 2 threads had 28,000 holes and 9,000 redos, and is now twice as fast as at 1 thread.
- **1a.6** A native with `reads_term() == false` is called on the evaluated arguments before the call term is built
  (its `t` is the call before argument evaluation); the term is built only if it declines.
- **1a.7** The compiled specialisation is consulted after the arguments and after a native declines, before
  argument-head rules and the head's rules; consulting records the head in the read set; registering is not a
  definition (no epoch or version change); `Kernel::clear` unlinks it; table entries are never freed. Consequence: a
  native whose answer depends on registrations must call `NativeCx::volatile()` (added with the compiler; `compiled_info`
  does).
- **1a.8** The kernel owns a stop flag; the engine still passes its own to the libraries and does not yet share it
  (`set_stop_flag`).

### Edits outside `mc-eval`

- `mc-engine/src/engine.rs`: `EngineConfig::default()` uses `EvalConfig::default()`; `MAX_DEPTH` (G.7) removed.
- `mc-engine/src/arith.rs` (additions): `ArithOrder::cmp`/`sort` order machine integers and small rationals by value
  without building `mc_arith::Order`; `small_ratio`/`small_product` fast path for products coming out as machine
  integers; `reads_term() = false` on Add, Mul, Pow, Complex, BallLiteral, Compare, Equality, NotSame, Not; a unit test.
- `mc-eval/Cargo.toml`: dev-dependencies `mc-engine`, `mc-bench` (a development-only cycle for the example and tests).
- `Kernel::natives` is now `Vec<Impl>` (`Impl::Native { native, reads_term }` or `Impl::Form(Form)`).

### Tests

`tests/machine.rs` (7): 10^6-deep linear recursion (1 thread lazy, 2 lazy, 2 always-split); the frame limit gives
`failure(limit, depth)` and the session continues; callback nesting just under/over the default; compiled
specialisations (rules alone, registration from a native, decline on overflow and non-integers, removal, `clear`) at 1
and 4 threads under stress collection; natives compiling and matching a pattern, sorting and comparing, seeing the stop
flag; recursion to 10^5 through a block statement, a `while` body and a `for` body; R2 at 10^5 with its Cassini
property. Every earlier test passes unchanged.

### Measurements (busy machine, +-20%, release, `-j 2`, medians of 3-5 fresh-engine runs)

| Case | Before | After | Speed-up |
|---|---|---|---|
| R1 `fib(25)`, memo off, 2 threads | 1357 ns/call | 315 ns/call | 4.3x |
| R1 `fib(25)`, memo off, 1 thread | 1815 ns/call | 527-615 ns/call | about 3.2x |
| `while i < n { i = i + 1 }`, n = 10^6, 2 threads | 2082-2290 ns/iter | 492-620 ns/iter | 3.4-4.2x |
| same loop, 1 thread | 1867 ns/iter | 493-548 ns/iter | about 3.6x |
| R12 m = 10^4, 2 threads | 4015 ms | 718-918 ms | 4.4-5.6x |
| R2 m = 5000, 2 threads | 60 ms | 39 ms | 1.5x |
| R2 m = 10^4 | depth limit | 89 ms (benchmark runner) | - |
| R2 m = 10^5, 2 threads | depth limit | 1.58 s, peak 2.4 GB | - |
| DEEP n = 10^6 | depth limit | 1339 ns/level, peak 675 MB | - |

Benchmark runner: R1 passes at m = 20, 25, 30 (m = 30 in 626 ms, against 3.7 s before); R2 at 10^3, 10^4, 10^5 (6.9 ms, 89 ms,
2.0 s); R12 at 10^4 (471 ms) and 10^5 (7.3 s), time limit at 10^6 (the compiled R12c handles it).

### Known gaps

- `and`/`or` (`Junction` in `mc-engine`), conditions, `map` elements, `match` and other natives still nest Rust frames
  per callback (bounded by `max_nest`): recursion through `or` stops at 10,000 levels.
- In loops the largest remaining cost is `mc-core`'s young-table hash-consing (about 45% of samples), then
  `mc_numtheory`'s `mod` and the general `Arith::new` path (interns 12 symbols per call).
- Substitution into right sides with binding forms still uses hash maps.
- The engine does not share its stop flag with the kernel yet (1a.8).


## Item N4a

Packed real and integer arrays (store node, contract, evaluator hooks, payload fast paths): see `crates/mc-core/NOTES.md`, section "Item N4a".

## Round 7 item A

The assumption context (`src/assume.rs`): `Cx::assume`, the held form `assuming`, `Attributes::uses_assumptions` (bit 7), the read-set bit of `assuming` as the dependency, memo entries keyed `assume$memo(t, ctx)` for evaluations that depended on a nonempty context, `NativeCx::assumptions()` and `eval_assuming`, the decorator `@uses_assumptions`. Details and decisions A.1-A.8: `crates/mc-simplify/NOTES.md`, section "Round 7 item A".

## Round 7 item G

Scratch scopes: `NativeCx::scratch(f)` runs one evaluation in a private region dropped when it ends; the interpreted `NumFn` backends use it, so long interpreted solver runs stay in bounded memory. Also: calls with packed arguments no longer unpack them (`head_of`, rule index keys). Details, the safety argument G.2 and measurements: `crates/mc-core/NOTES.md`, section "Round 7 item G".

## Round 11: the panic guard

A panic inside an evaluation no longer ends the session (`src/guard.rs`). `Kernel::eval_with` runs the evaluation
under `catch_unwind`; the value is then `failure(internal, "message (file:line:column)", f)` with `f` the innermost
native the panic unwound through (left out when the panic was in the evaluator itself), and the status `Running`.
Panics of speculative tasks reach the guard through `rayon::join`, which resumes them on the waiting thread.

- **Native scope:** every native call and compiled specialisation holds a `NativeScope` (a symbol and two
  references); its `Drop` checks `thread::panicking()` (one relaxed load on the normal path) and, during unwinding,
  records the native and sets the control block's status to 6 (read as an interrupt), so the other tasks stop at
  their next poll.
- **Hook:** `guard::install_hook` (called by `Kernel::new`, once per process) keeps each panic's message and
  location (per thread, and the last one of any thread); while a guarded evaluation runs, panics on session threads
  (`mc_core::on_session_thread`) are not printed. `MC_PANIC_TRACE` prints them anyway. Other panics go to the
  previous hook.
- **What is reset:** nothing in the heap (no heap structure is half-updated where code can panic; scratch scopes
  close in `Drop`; `parking_lot` locks do not poison; the safe point after the statement collects the unwound
  evaluation's terms). The control block and the cells are reset by the next evaluation's start, the frame stack
  lived in the unwound call. Definitions made before the panic stay, as after any failure; memo entries are only
  those of completed sub-evaluations. The interrupt request is the caller's and is left alone.
- **API:** `mc_eval::guard::{catch, install_hook, payload_text, PanicNote, INTERNAL}` (also re-exported as
  `mc_engine::guard`). `catch(f)` is for hosts that guard their own work (the engine's reading and printing, the
  kernel's requests).

Panic sites: every `unwrap`/`expect`/`panic!`/`unreachable!`/`assert!` left in the library code carries an
`// invariant:` comment saying why no input reaches it, or that the guard reports it.

## Round 15 item 9B: non-local results, runs, held failures, symbol attachment

Built for the coverage of core-language examples (roadmap phase 9, item 9B). Tests:
`crates/mc-lists/tests/nonlocal_runs.rs` (7 tests, on source text), documentation pages `throw`, `catch`, `sow`,
`reap`, `repeated`.

- **C9B.1 Held arguments are not failed arguments.** The evaluator returned the first failure among *all*
  argument values, held ones included, so `if c then failure(...) else x` failed even when `c` was false, and a
  `for` body containing a failure term failed on its first iteration. Now only strict positions count
  (`Eval::first_failure_strict`, used for calls and for closures with `held` parameters). Semantics 10 says a
  failure propagates through *strict* arguments; this was a bug against it.
- **C9B.2 `throw` is a failure value.** `throw(v[, tag])` is `failure(thrown, v[, tag])`; `catch(e[, form[, f]])`
  holds `e`, evaluates it and takes the first thrown failure in the value, depth first and left to right. Because
  failures already end blocks and loops and pass through strict calls (semantics 10), no new control mechanism was
  needed and every native that handles failures handles throws. Where a term keeps failures (lists, `map`), the
  leftmost throw is the first one in evaluation order (arguments left to right, innermost first). Known difference
  from an exception: work after a throw inside such a term still runs (its effects happen) though its result is
  discarded. A throw `catch` does not match is its result, so it reaches an outer `catch`.
- **C9B.3 `sow` and `reap` share a stack of collectors.** Both are `effect` natives (never speculative). A
  `reap` frame records its patterns (term handles of the running evaluation; no safe point falls inside one, so no
  tracing) and pops itself when its body returns or aborts. A `sow` goes to the innermost frame whose pattern
  matches its tag, otherwise outward.
- **C9B.4 `repeated(p, ...)` patterns** (`Pat::Repeat`): a run of `min..=max` arguments each matching `p`,
  variables shared across repetitions. Argument position only (like sequence variables), and only under heads that
  are not associative-commutative (compilation rejects it there: the AC matcher's candidate sets assume single
  patterns). Lengths are tried shortest first, as for sequence variables; with no sequence pattern after it only the
  one length that can work is tried. `instance_of` never proves an instance involving a run (sound). The library
  functions of `mc-lists` now use this matcher (its C9B.10).
- **C9B.5 Argument-head rules attach to symbol arguments too.** `@on(square) rule area(square) = s^2`: the
  dispatch loop now also looks up symbol arguments (`attach_key`), as semantics 6.5 does for the heads of compound
  arguments. Cost: one symbol-table lookup per symbol argument of a call that reaches rule dispatch. Packed
  arguments are tested before viewing (viewing would unpack them).
- **C9B.9 Slot functions ignore extra arguments.** `&(f(&1))(a, b)` is `f(a)`: a closure whose parameters
  are exactly `&1, ..., &n` (what `&(...)` lowers to, `n >= 1`) drops arguments past `&n` instead of giving
  `failure(no_match, ...)`. Slot functions are the shorthand passed to `map_indexed`, `reap`, `catch` and the like,
  which hand over an index or a tag the function may not use. Named lambdas (`x => ...`) keep exact arity.
- **C9B.11 `key_value_pattern([p1, ..., pk])`** (`Pat::KeyValue`): matches an association, or a list whose
  elements are all rules, with `k` distinct entries matching the rule patterns `p1`, ..., `pk` in any order
  (backtracking over the assignments, at most 64 entries). Like `repeated`, its head has a declining native so
  the term stays as written outside matching.
- **C9B.13 `optional(x_, d)` inside patterns** (`Pat::Opt`): among the arguments of a call that is not a
  definition's own left side (those still define one rule per case, H.7), an argument that may be missing; the
  variable is bound to `d` when it is. Present first, then absent. Rejected under associative-commutative heads,
  like runs. The surface `x_ = d` reads as an assignment inside lists and rules, so the explicit call is the way
  to write it there.
- **C9B.14 `x_: Head[f]` in pattern terms.** The type `Head[f]` of 04 section 9 now lowers to `Ty::Head(f)`
  in `syntax::pattern` (it was an unknown type there, so `count(e, _: Head[abs], ...)` failed with
  `failure(argument, count, 2)`).

## Round 16 item 9B part 2: optional arguments, units, typed runs

Tests: `crates/mc-lists/tests/core_round16.rs` (on source text).

- **C9B2.1 Optional parameters with any one-variable pattern.** A definition's `optional(p, d)` took only a
  named blank (possibly typed); now `p` may be `x @ q`, `q when c` or a sequence variable (`syntax::default_binding`).
  When the argument is missing, the variable is bound to `d` without testing it against `q` or `c` (the definition
  says what the argument is when it is missing); a missing run is `[d]` (a sequence variable stands for the list of
  its terms). Inside patterns, `optional(p, d)` likewise takes any `p` with a variable naming the whole argument
  (`whole_var`).
- **C9B2.2 Optional arguments under associative-commutative heads; units.** `compile` no longer rejects
  `optional` under AC heads. `Matcher::m_ac_opt` decides each optional argument present (its pattern in its class
  place: an optional argument has its pattern's class) then absent, and runs the plain AC matcher on each pattern
  list, so the AC search itself never sees an optional argument. `Matcher::m_unit`: a call pattern with optional
  arguments against a term that is not a call of its head matches when all optional defaults are the unit of the
  head at their place (`add` 0, `mul` 1, a declared identity, `pow`'s exponent 1) and one single pattern remains
  (semantics 6.2). `AppInfo::has_opt` marks such patterns; their head is not fixed (`fixed_head`), the rule index
  files them below the top as any term and the AC key filter takes no key from them. `rewrite`'s implicit rest is
  added to left sides with optional arguments too (`a + optional(x_, 0)` rewrites `a + b + c` to `c + f(b)`).
  A default that is not the unit gives no match: matching `x` against `mul(optional(c_, d), v_)` with `c -> d`,
  `v -> x` would be wrong, since `d x` is not `x`.
- **C9B2.3 Typed sequence variables `xs__: T`.** `Pat::Seq` has a type; a run takes only arguments of that
  type (in argument lists: no longer than the typed prefix; under AC heads: subsets of the typed arguments). The
  instance check compares typed runs only outside AC heads (sound: it may not report an instance).
- **C9B2.4 Rules for calls with compound heads (`f(x_)(y_) := ...`).** A pattern whose head is compound or a
  pattern lowers to `call$(head pattern, args...)` (`pat::CALL`, `AppInfo::curried`), and a literal left side with
  a compound head likewise (`pat::curry_lit`); such rules are stored with the innermost head symbol of the head
  pattern, as its argument-head rules under `call$` (so `f` counts as defined, `unset(f)` removes them, and only
  calls rooted at `f` read them); a head pattern without a symbol (`g_(x_)`) puts the rule in `call$`'s own rule
  set. A call whose head is not a symbol is dispatched by trying those rules (root first) on the view
  `call$(head, args...)` (one allocation, only when there are such rules; reads of the root and of `call$`). The matcher takes a curried
  pattern either against that view or against a call, the head pattern against the call's head, so nesting
  (`g(w_)(x_)(y_)(z_)`) and pattern heads (`g_(x_)(y__)`) work; below the top the rule index files curried
  patterns as any term. Semantics 6.4 (the head symbol of a definition) is unchanged for symbol heads.
- **C9B2.5 `replace_list(e, rules[, n])`.** Every match of each rule against the whole of `e`, in the
  enumeration order (`Eval::match_all`, `Matcher::each`); operator form `replace_list(rules)(e)`. Under an AC head
  a single pattern still takes one argument (semantics 6.2), so `x_ + y_` does not match `a + b + c`.

## eval in held arguments (2026-11-08, user decision)

`eval(x)` written directly as an argument in a held position forces that argument, as semantics 5 states: the
argument counts as strict (`Evaluator::held`), the `eval` builtin evaluates `x`, and the function receives the value;
the other held arguments stay unevaluated. Before, `eval` there was the identity and the held function received
`eval(x)` itself (decision p6core.14, superseded). The right side of a `:=` rule is not an argument position, so
`eval` there still waits for the rule to fire.

## Round 17 fixes: `unset`, protection, replaced assumption contexts

Tests: `crates/mc-engine/tests/r17_fixes.rs` (through the whole engine), unit tests in `mc-lists/src/hash.rs`.

- **r17fix.3 `unset` during an evaluation.** The round 16 note was right that `Defs::clear` needs `&mut` (it frees
  rule sets in place). The safe way is the one definitions already use for replaced rules and own values: publish
  the empty state with release stores and *retire* what was there until the next safe point. `SymDefs::rules` is now
  an `AtomicPtr<RuleSet>` (was a `OnceLock<Box<RuleSet>>`, which can be set but never replaced); `Defs::unset(s)`
  swaps the own value, the identity element, the rule set and the argument-head list out and pushes the old rule
  set and list nodes onto `Defs::retired`, freed by `Defs::reclaim` (called by every safe point before tracing, so
  retired sets are never traced). A reader that loaded the old set keeps a valid reference for the rest of the
  evaluation (the same argument as for retired rules, F.2). `Defs::unset_rule(s, lhs)` builds a copy of the rule set
  without the rule whose left side is `lhs` up to renaming (`RuleSet::without`, re-inserting copies in rule order)
  and swaps it in. Both run through `Kernel::defined` (epoch, `last_change` bits: memo entries and speculative
  results that read the symbol are stale) and are frontier effects (`Eval::effect`). The native `unset(x, ...)`
  (held, `effect`) takes symbols, lists of them and left sides `f(p)`; attributes and natives stay (the attributes
  are the symbol's signature, `@listable` and the like; `clear_attributes` removes them).
- **r17fix.4 `protected`.** Attribute bit 8 (`Attributes::protected`). `define_now` (set, set_delayed, define_rule,
  define_fn), `define_sym`, `assign`/top-level `let`, `unset`, `set_attributes`/`clear_attributes` (in `mc-engine`)
  refuse a protected target with `failure(protected, f)` (`DefError::Protected` inside the definition code). The
  target of a rule is its head symbol and the `@on` symbol. `protect(f, ...)`/`unprotect(f, ...)` (held, effect)
  give the names that changed, as strings (so a symbol with an own value is not evaluated in the answer). Built-in
  symbols are not protected: libraries and the prelude define rules for them. An
  assignment to a protected symbol is a failure value, so a block `x = 1; y = 2` stops at it (semantics 10 makes
  errors values).
- **r17fix.5 `NativeCx::eval_replacing(facts, t)`**: `t` evaluated with the context *replaced* by `facts` (the
  `assumptions:` option); `eval_assuming` still extends. The read-set bit of `assuming` is recorded either way
  (conservative: the result may then not be reused under another outer context although it does not depend on it).

## Round 18 item 14C: optional runs for the rule table

Tests: `crates/mc-rules/tests/rules.rs` (`optional_runs`, through the whole engine; the rule table itself uses the
feature in every coefficient pattern).

- **r18c14.1 Optional runs `optional(xs__, d)`.** Inside patterns, `optional(p, d)` now also takes a sequence
  variable (`syntax::pattern`); validation lets the run stand in the argument position of the optional argument.
  Under an associative-commutative head `Matcher::m_ac_opt` already puts the optional argument's own pattern in
  its place when present, so the run takes one or more arguments and stands for the head applied to them
  (semantics 6.2); absent, the variable is bound to `d`, and the unit rule (`m_unit`) applies as for any optional
  argument. Under other heads `m_seq` tries the run first, then the run `[d]` (as a definition's missing run,
  C9B2.1). This is what a rule-based integrator needs from the matcher: `b_. x^n_` in the source notation
  means "the rest of the product, or 1", and `-x^-2/c^2` must match it with `b = -c^-2`; a single variable under
  an associative head takes one argument (semantics 6.2), so the rule table lowers such variables to optional
  runs (`mc-rules`, `table::run_kinds`). Semantics 01, section 6.1 table and 6.2.
- **`CondCx::binding(i)`, `CondCx::seq_head(i)`, `CondCx::run(o, n)`, `Binds::run(o, n)`.** A native condition
  can read a variable that is a run when present and a single default when absent (`CondCx::one` panics on a
  run).
- No other matcher change was needed for the rules ported this round: optional arguments with unit defaults
  (C9B2.2), non-linear variables across factors (`sin(e_. + f_. x)^m_. cos(e_. + f_. x)^n_.`), conditions
  that backtrack into the next match (a native condition wrapping the whole left side) and compiled patterns
  (`NativeCx::compile_pattern`, `match_binds`) were there. Dispatch over thousands of rules is the rule table's
  own (a bit per function, `mc-rules`), since its rules are not stored as definitions of a symbol; the
  definition index still files patterns with optional arguments below the top as any term (C9B2.2), which
  matters only for user rule sets of that shape.

## Round 19

- **Rule order (decision i19.1).** A new rule is placed before the first existing rule that is *strictly* more
  general (or replaces one that is the same up to renaming). Rules equally general but different (the same pattern
  under other conditions, such as `f(x_) := a when c1` and `f(x_) := b when c2`) keep definition order, the new one
  after (so conditional cases are tried in the order written). A `fn g(xs...) when c` clause is as general
  as the `fn` catch-all `g(args___) when is_ground(args)`, so `define_now` takes the catch-all out before such a
  clause and adds it again after: `fn g(xs...) when cond` now matches numeric and Boolean arguments (it never did:
  the catch-all came first).
- **`unevaluated(e)`** (`builtins.rs`, `machine.rs`): a held native that turns the unwrapping on at its first
  evaluation (`Kernel::uneval_seen`, so the hot argument path pays nothing before). `after_args` unwraps strict
  arguments `unevaluated(x)` (not for `eval`, which marks held arguments) and records the written call in
  `EvalFrame::rewrap`; when no definition rewrites the unwrapped call, the written call is the value.
- **`time_constrained`, `memory_constrained`, `pause`** (`constrained.rs`): a watchdog thread raises the control
  block's interrupt at the deadline; the native that started it withdraws the interrupt and the status
  (`Control::clear_status`) and gives the failure value. The memory cap is a second cap beside the session budget,
  polled with the fuel (`Kernel::mem_cap`), plus a check of the young generation's growth and of the value's size
  after the evaluation (the arena grows by chunks).
- **Destructuring** `[a, b] = [x, y]` (`syntax.rs`, `SetDef`): element by element, nested lists too.
- Open: `unevaluated` in closure arguments is not unwrapped (`(x => hold(x))(unevaluated(1 + 2))` is `hold(3)`);
  `flat` attribute matching (Q19-31) and evaluating a `sum` body per term when it has effects (Q19-26) are not done.

## Round 20 (roadmap 16A)

Parallel work of the symbolic engines on the session's pool, with results identical for any number of threads.

- **`par.rs`** (a child module of `eval.rs`, for the task state's private fields): `NativeCx::par_until(n, stop,
  f)` is `f(0), f(1), ...` in order until `stop` holds of a value; `par_map` never stops; `par_first` stops at
  the first `Some` (a portfolio: the first answer **in the fixed order**, never the first to finish); `par_scan`
  adds a stop that sees the values in order (three good points of twelve). `NativeCx::parallel()` says whether
  work would split (results never depend on it).
- **Decision n16a.1 (keeping speculative values).** The caller computes the first block itself; later blocks are
  speculative tasks (a `Cx` each, fuel share, effects refused) claimed through per-element slots
  (`PENDING/RUNNING/DONE/MINE`). The caller walks the elements in order: an element nobody started it computes
  itself; a running one it waits for, blocked on the slot's condition variable (never taking other jobs of the
  pool while waiting: a job stacked above a wait could wait on what is suspended below it; helping with later
  elements of the run was measured and delayed seeing the awaited one); a finished one is kept, its fuel charged
  then (`spend`), unless it left a hole or a definition since the split touched its read set, in which case the
  caller computes it again. Effects therefore happen in order on the caller, and only the caller decides a limit
  failure (semantics 18.4, A.5).
- **Decision n16a.2 (cancellation).** Each run has a pooled `Node` with a cut index; a task's `Cx::cancel` names
  its run and element (inherited by tasks split off it; nested runs chain to their parent's element). When a
  value stops the run, elements above it are cut: a running one leaves a hole at its next fuel refill
  (`refill` checks `cancel.cancelled()`), a block not started is skipped, and `NativeCx::cancelled()` lets long
  loops poll. A cancelled element's value is never used.
- **Plain work** (`Budget`, `Sub`): for Rust work without evaluation (the lifting of a decomposition, integer
  boxes). `NativeCx::budget(flags)` is an owned handle (no borrow of the task, so the native keeps charging its
  meter): the control block (interrupt, time and memory limits), the kernel's and the caller's stop flags, the
  caller's cancellation, and a fuel cap (what the evaluation had left). `Sub::until` runs elements with their own
  `Sub` (charge, halted, settle, nested runs); the caller charges each kept element's fuel to its own meter in
  order and computes again an element that was stopped. Per-element meters are used at every thread count, so
  the fuel of a result does not depend on it.
- **`MC_THREADS`** (one line in `mc-engine`'s `EngineConfig::default`): overrides the pool size, for suites and
  benchmarks at 1 and 8 threads.
- **What stays timing-dependent** (as it already was sequentially): time budgets inside methods (`d_solve`'s 5 s,
  the integration methods' budgets, the search's deadline), and the memo of normal forms, which can make a
  redone element cheaper (fuel, not values).
- Tests: `tests/par.rs` (6: maps, first answer in order with a late early element, cancellation of elements
  that would spin for minutes, definitions by earlier elements seen by later ones, fuel limits failing alike,
  nested runs cancelled with their parent; every one in six configurations: sequential, lazy 1/4/8 threads,
  always-split 1/2 threads), `par::tests::nodes_cut_and_nest`.

### Speed-ups (medians of 5 runs, a fresh `mc-kernel --threads N` per run, machine load 27-29% from other work; identical output at every thread count and to the build before)

| example | 1 thread ms | 2 | 4 | 8 | x2 | x4 | x8 |
|---|---|---|---|---|---|---|---|
| a `minimize` | 2222 | 1531 | 698 | 351 | 1.45 | 3.18 | 6.33 |
| a `maximize` | 2341 | 1558 | 717 | 427 | 1.50 | 3.26 | 5.48 |
| an inequality | 558 | 331 | 205 | 147 | 1.69 | 2.72 | 3.80 |
| a `for_all` | 443 | 259 | 174 | 136 | 1.71 | 2.55 | 3.26 |
| a `resolve` | 361 | 202 | 126 | 84 | 1.79 | 2.87 | 4.30 |
| a `find_instance` | 363 | 341 | 350 | 353 | 1.06 | 1.04 | 1.03 |
| a simplification | 424 | 290 | 148 | 152 | 1.46 | 2.86 | 2.79 |
| a `catalan` example | 810 | 596 | 362 | 343 | 1.36 | 2.24 | 2.36 |
| a `trig_reduce` | 593 | 575 | 564 | 589 | 1.03 | 1.05 | 1.01 |
| a `dsolve` | 149 | 130 | 68 | 61 | 1.15 | 2.19 | 2.44 |
| a Riccati equation in `dsolve` | 636 | 594 | 374 | 308 | 1.07 | 1.70 | 2.06 |
| an algebraic integral | 188 | 188 | 118 | 88 | 1.00 | 1.59 | 2.14 |
| an `asymptotic` | 167 | 159 | 159 | 161 | 1.05 | 1.05 | 1.04 |
| a `sum_convergence` | 177 | 180 | 178 | 179 | 0.98 | 0.99 | 0.99 |
| an integral with conditions | 133 | 137 | 139 | 138 | 0.97 | 0.96 | 0.96 |

Not parallel here: `find_instance` (witness search, sequential by design), `trig_reduce`, `asymptotic`, `sum_convergence`, definite integration (no portfolio to split). One thread against main: within load noise (0 to 5%).

## Round 22

- engine B21-29: `time_constrained`'s watchdog also raises the number kernels' stop flag (`Kernel::watch_flag`,
  registered by the engine), so one huge exact multiplication or gcd stops at the limit; a value returned after the
  watchdog fired is a failure (the stopped kernel may have given junk). The test took 20 s and takes 0.54 s now.
- R20-09 (flat matching) deferred: `flat` is an alias of the AC attribute here (mc-engine `attrs.rs`); matching
  `ff(_)` against `ff(1, 2)` needs a separate associative-only attribute and wrapping in the matcher, which would
  also change rules on built-in heads.

## Round 25

- **R24-05 `threaded` operands:** `Evaluator::thread` (listable calls: `add`, `mul`, user listable heads) spreads
  `threaded(v)` / `threaded(v, n)` over the levels of the array arguments by the rules of `elementwise`
  (`thread_threaded`: trailing levels by default, level `n` from the top for `n > 0`, `r + n + 1` for `n < 0`;
  `failure(shape_mismatch)`, `failure(threaded_level, n)`); with no array argument the result is `threaded(...)`.
- **R22-13 flat-only matching:** new attribute bit `Attributes::flat` (associative only). Calls of a flat head splice
  nested calls of the head in, keeping the order; a pattern `ff(p1, ..., pn)` on a flat head (not AC) has
  `AppInfo::flat` and counts as variable-length (`has_seq`); `m_flat` lets each single argument pattern match one
  argument or, in order, a run wrapped in the head (shortest first): `ff(x_, y_)` on `ff(1, 2, 3)` is `x = 1`,
  `y = ff(2, 3)`. Sequence, optional and repeated patterns fall back to `m_seq`. `set_attributes(f, flat)` sets it
  (before it was an alias of `orderless`); built-in heads keep `ac`. No one-identity handling (`ff(x_)` on `1` fails).
- **R24-07 deferred:** `let x = <failure>` ends the block with the failure by decision H.9. Binding the failure and
  continuing would silently drop an unused failed computation (`let r = fails(); 5` would be 5); the reversal needs a
  decision (an opt-in form such as a typed `let x: any`, or a `try` binding).
- **R24-06:** see mc-syntax `lint.rs` (`LOWERED`), a lint on locals named like lowering targets.

## Round 27

- X26-12: a `threaded(v)` or `threaded(v, n)` operand of a listable call whose value `v` is a scalar (not a list) is
  broadcast like a plain argument; before, `Evaluator::thread_threaded` gave `failure(threaded_level, ...)` for it
  (an array level was required of every threaded value). Test `x26_12` in `tests/bugs_round26_cross.rs`.

## Round 30

- **Definition sources (decision r30.1).** `RuleRhs::Term` keeps `src: Option<Raw>`, the definition as written
  (`rule_delayed(lhs, body)` with the pattern terms before lowering), traced with the rule; `define_now` stores it
  with the first clause of a definition (a definition with optional arguments or options gives several clauses
  and is listed once, as written), rules made internally (`fn` catch-all, `define_term`, natives) keep `None`.
  `NativeCx::rule_sources(s)` returns them per rule set with the set's outer head (none: own rules; `call$`: rules
  of calls with a compound head rooted at `s`; another symbol: argument-head rules attached to `s`), in rule order;
  `SymDefs::arg_outers` lists the outer heads. Used by `down_values`, `up_values`, `sub_values` (mc-engine).
- **Marked runs (decision r30.4).** `longest(r)`, `shortest(r)`, `longest(r, k)`, `shortest(r, k)` on a run
  pattern `r` (`xs__`, `xs___`, `repeated(...)`) lower to a `mark: Option<(bool, i64)>` on `Pat::Seq` /
  `Pat::Repeat` (on a single pattern the marker is dropped: it takes one argument). When an argument list holds
  a marked run (and no optional argument), `m_marked` chooses the run lengths in decision order: the marked runs
  by priority (higher first, ties left to right), each its preferred lengths first, then the other patterns left
  to right as before (runs shortest first), and matches the slices. So `[x__, longest(y__)]` on seven elements
  gives `x = [a]`, `[x__, shortest(y__)]` gives `y = [g]`. Not
  done: marks on optional arguments (`shortest(optional(y_, 0))`, absent first) and under AC heads.

## Round 36

- `NativeCx::eval` returns `Abort::Limit` when the control block is halted: a native that swallowed an earlier abort
  stops at its next nested evaluation (see mc-cad NOTES).
