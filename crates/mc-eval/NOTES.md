# Package F: `mc-eval`

Status: built (branch `core/f-eval`, commits bda5037, 51b451b). `cargo test` passes in debug and release (9 unit, 12
semantics, 1 syntax, 9 workload tests); clippy and `cargo doc` clean with `#![warn(missing_docs)]`. Depends on
`mc-api`, `mc-core`, `rayon`, `rayon-core`, `parking_lot`, `rustc-hash`; not on `mc-arith` or `mc-syntax`. Every
"done when" item is met: I2's cases (R4, R5, R13, R14, S7, R14e native and evaluating condition, MR, MR fuzz) give
results identical to the sequential reference on `mc-core` at 1, 2 and 4 threads, lazy and always-split, also under
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

- **workloads** (I2's cases): the reference against 13 configurations (collect or stress, 1/2/4 threads, lazy or
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
  speculative task). This deviates from semantics 4.2 and the brief: the evaluator recurses per evaluation level.
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


# Package H: language features (added to `mc-eval`)

Status: built and committed (branch `lib/h-lang`, commit 491c98d). Closures, blocks, loops, `match`, `rewrite` and
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

## Package 9 (library and kernel gaps, branch `lib/p9-gaps`, commit 536c775)

- Additions in `defs.rs` only: crate-private `Defs::clear_arg_rules(holder, outer)` and
  `pub fn Kernel::clear_attached(&mut self, holder, outer)` (counts as a definition of both symbols), for 9.6.

## Package 1a (explicit-stack evaluator and evaluator speed, branch `lib/p1a-eval`; merge 502c9bb)

All targets met: R2 at m = 10^5 passes through the harness (1.996 s median, check pass, peak 2.4 GB); a 10^6-deep
linear recursion passes (1 and 2 threads, lazy and always-split); the native-interface additions and
`register_compiled` exist with tests. On the branch: workspace 455 passed; clippy clean.

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
- **`compiled.rs`: the hook for package 1b.** `trait Compiled: Send + Sync { fn call<'h>(&self, cx: &mut
  NativeCx<'_, '_, 'h>, args: &[T<'h>]) -> Option<T<'h>>; }`; `Kernel::register_compiled(&mut self, head: Sym, c:
  Arc<dyn Compiled>)`, `remove_compiled`, `compiled(head)`, and `NativeCx::register_compiled(head, c) -> Res<()>` (an
  effect of the frontier). A slot per symbol in `SymDefs`; specialisations in an append-only table.
- **Native-interface additions (package I's requests):** `NativeCx::compile_pattern(Pat)`,
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
  native whose answer depends on registrations must call `NativeCx::volatile()` (added at the merge; `compiled_info`
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
| R2 m = 10^4 | depth limit | 89 ms (harness) | - |
| R2 m = 10^5, 2 threads | depth limit | 1.58 s, peak 2.4 GB | - |
| DEEP n = 10^6 | depth limit | 1339 ns/level, peak 675 MB | - |

Harness: R1 passes at m = 20, 25, 30 (m = 30 in 626 ms, against 3.7 s before); R2 at 10^3, 10^4, 10^5 (6.9 ms, 89 ms,
2.0 s); R12 at 10^4 (471 ms) and 10^5 (7.3 s), time limit at 10^6 (the compiled R12c handles it).

### Known gaps

- `and`/`or` (`Junction` in `mc-engine`), conditions, `map` elements, `match` and other natives still nest Rust frames
  per callback (bounded by `max_nest`): recursion through `or` stops at 10,000 levels.
- In loops the largest remaining cost is `mc-core`'s young-table hash-consing (about 45% of samples), then
  `mc_numtheory`'s `mod` and the general `Arith::new` path (interns 12 symbols per call).
- Substitution into right sides with binding forms still uses hash maps.
- The engine does not share its stop flag with the kernel yet (1a.8).


## Package N4a

Packed real and integer arrays (store node, contract, evaluator hooks, payload fast paths): see `crates/mc-core/NOTES.md`, section "Package N4a".

## Round 7 package A

The assumption context (`src/assume.rs`): `Cx::assume`, the held form `assuming`, `Attributes::uses_assumptions` (bit 7), the read-set bit of `assuming` as the dependency, memo entries keyed `assume$memo(t, ctx)` for evaluations that depended on a nonempty context, `NativeCx::assumptions()` and `eval_assuming`, the decorator `@uses_assumptions`. Details and decisions A.1-A.8: `crates/mc-simplify/NOTES.md`, section "Round 7 package A".

## Round 7 package G

Scratch scopes: `NativeCx::scratch(f)` runs one evaluation in a private region dropped when it ends; the interpreted `NumFn` backends use it, so long interpreted solver runs stay in bounded memory. Also: calls with packed arguments no longer unpack them (`head_of`, rule index keys). Details, the safety argument G.2 and measurements: `crates/mc-core/NOTES.md`, section "Round 7 package G".

## Round 11 package robust: the panic guard

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
