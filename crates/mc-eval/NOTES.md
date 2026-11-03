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
