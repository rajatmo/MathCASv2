# Item G: `mc-engine`, the integration of the core

Status: done. The REPL runs a 41-entry end-to-end session correctly.
All 263 workspace tests pass in release, with no build, clippy (`--all-targets`) or doc warnings; `mc-engine`,
`mc-repl` and `mc-bench` also pass in debug. Every benchmark case completes or stops cleanly at a limit, so the benchmark runner
reports M1 as met; but only R1 produces a correct result, because almost every other case needs library functions or
language features that do not exist yet.

## What was built

- **`crates/mc-engine`** implements `mc_api::Engine` on the real core: `CoreEngine::new()` /
  `with_config(EngineConfig { threads, heap, eval })`. `eval` reads the whole source with `mc_syntax::read`; if any
  statement has an error nothing runs; otherwise the statements go into `Kernel.roots` and each is evaluated in its own
  `Session::step` (a safe point after every statement). The last value is printed with
  `Printer::with_numbers(h, ArithNumbers)` and fingerprinted with `mc_core::shapes::fingerprint`.
- **`arith::install`** registers `mc-arith` as the arithmetic and comparisons (G.4) and calls `set_order(ArithOrder)`.
- **`format::ArithNumbers`** prints big integers through `Nat::to_decimal` and balls with only correct digits
  (`text::ball_words`).
- **Binaries:** `mc-engine` provides `mc-bench` with the real engine and the counting allocator; the stub benchmark
  binary is renamed `mc-bench-stub` (used by mc-bench's own tests); `mc-repl`'s binary uses `CoreEngine`.

## Edits to other crates

- **mc-api:** `names::failure::{DEPTH, DEFINITION}`.
- **mc-core:** `Raw` is `#[repr(transparent)]` with `to_bits`/`from_bits`; `pub const HOLE: u32 = 0x7FFF_FFFF`;
  `Session::with_state(cfg, threads, |h| state)` (`new` delegates to it).
- **mc-eval:** own values are `AtomicU32` slots (`HOLE` = none, no boxes or retired list); `session()` uses
  `with_state` (drops F.12's workaround); uses `mc_core::HOLE` and `names::failure`; `Control::begin` no longer clears
  a pending interrupt, new `Control::clear_interrupt` (G.5); `TermOrder::sort` with a default implementation, used by
  `make` (G.10).
- **mc-arith:** `Arith::ball_from_decimal(text, digits)` (with a unit test) and `text::ball_words(neg, words)`. Bug
  fix: `to_decimal` looped forever, allocating to the memory cap, when a stopped squaring returned zero; it now returns
  early when stopped, and the Newton reciprocal skips its quadratic fallback once stopped (unit test). Integration tests
  run on both `RefStore` and `mc-core` through a runtime-dispatch `TestStore`; `mc-core` is a dev-dependency.
- **mc-bench:** `StopFlag::on_raise(hook) -> RaiseHook` (dropping the guard removes the hook) and `StopFlag::same`;
  stub binary renamed `mc-bench-stub`.
- **mc-repl:** depends on `mc-engine`; `tests/core_sessions.rs` and `tests/sessions/core.txt`.
- **bench/cases:** P1, P2, P11 used the size variable `n` (the numeric function) instead of `m`; fixed.

## Tests

- **`mc-engine/tests/engine.rs`** (7 tests plus a doctest): a 19-input session; identical results under
  `Config::stress()` at 1 and 4 threads; a syntax error evaluates nothing, with the right offset; `bind_last`;
  fingerprints are structural, equal across engines and to a `RefStore` build; the stop flag raised before and during
  an evaluation, after which the session continues; the depth limit gives `EngineError::Limit`.
- **`mc-repl/tests/core_sessions.rs`** runs `tests/sessions/core.txt` through the REPL: exact arithmetic, rationals and
  big integers, `x + 2x`; rule-defined `fib`, recursive `fn fact` with literal and typed patterns, `fn` with `when`;
  `map`, sequence patterns over lists; ball literals and contagion, complex numbers, three-valued comparisons, `and` /
  `not`; `failure(division_by_zero)` and its propagation; `;`-separated statements; a multi-line syntax error.
- **Conformance:** `mc-core/tests/conformance.rs` passes; `mc-arith`'s 16 integration tests pass on `RefStore` and on
  `mc-core`.

## Benchmark report (first run)

`mc-bench run --cases bench/cases --only <class> --mem-ceiling 4GB`, one class per guarded run, at most 4 engine
threads; results in `bench/results/<host>/2026-11-03.tsv` and `.trace`; 70 measurements. Summary: "M1 every case
completes or reports limit cleanly: met"; M2 to M4 unknown (no baseline timings). No case fails to parse.

- **Runs and checks:** R1 at 20, 25, 30, but at about 95 µs: the memo of normal forms makes naive `fib` linear and
  repetitions hit the memo, so R1 does not measure rule dispatch (suggest `session: fresh` plus a memo-off option).
- **Wrong result:** R2 at 10^3 and 10^4 relies on `fib(k) = v` returning `v`, but semantics 6.4 makes definitions
  return `null`; the correct form needs blocks (not supported yet); the case is unchanged. R2 at 10^5 stops at the
  evaluation-depth limit.
- **Stopped at the time limit:** N9 at 10^7 (about 10.6 s per repetition, almost all printing the ~46-million-digit
  result; property unchecked).
- **Needs missing library functions (everything else):**

| case | missing |
|---|---|
| R8 | `expand`, `length`, `binomial`, `rewrite` |
| R9 | `nest`, `depth`, plus block / `let` |
| R12 | `mod`, `range`, `max`, plus `fn` with block / `var` / `while` / assignment |
| N1 | `n`, `pi` (sizes 10^5 and 10^6 have no expected value) |
| N5 | `factorial`, `integer_exponent`, `sum`, `integer_digits`, `integer_length`, `part` |
| N6 | `prime_pi`, `prime` |
| N7 | `floor`, `next_prime`, `factor_integer` |
| N9 | `quotient`, `gcd`, plus block / `let` |
| N10 | `range`, `sum`, `integer_exponent`, `denominator`, `numerator`, `integer_length`, `mod`, plus lambdas |
| N12 | `exp`, `pi`, `sqrt` |
| P1, P2 | `expand`, `length`, `binomial`, `rewrite` |
| P4 | `expand`, `gcd` |
| P5 | `expand`, `factor` |
| P7 | `expand`, `resultant` |
| P11 | `range`, `sum`, `expand`, `length`, `rewrite`, plus lambdas |
| P12 | `normal`, `series`, `exp`, `sin`, `coefficient` |
| S1 | `range`, `select`, `mod`, `sum`, plus lambdas |
| S2 | `range`, `mod`, `string_join`, `to_string`, `sort`, `ordered`, `reverse`, `length`, `part`, plus lambdas |
| S6 | `range`, `flatten`, `transpose`, `length`, `part`, plus lambdas |
| S7 | `range`, `mod`, `cases`, `length`, plus lambdas |
| S8 | `range`, `length`, plus lambdas |

Two language features block most cases and are evaluator work, not library: lambdas / `fn` closures, and blocks with
`let` / `var`, `while` and assignment.

## Measurements (busy machine, release)

- About 95–105 µs per `Engine::eval`, mostly the reserved-stack thread `mc_syntax::read` starts (C.13) plus steps and
  safe points.
- `7^(10^7)` takes 130 ms; printing its 8.45 million digits takes 1.7 s.
- N9: 65 ms at 10^5 and 0.80 s at 10^6, mostly decimal printing.

## Decisions

- **G.1 Layout:** the real binaries live in `mc-engine` (`mc-bench`) and `mc-repl`; the stub benchmark binary is
  `mc-bench-stub`; the `mc-bench` bin has `doc = false`.
- **G.2 Statements:** the whole source is read first; statements rooted in `Kernel.roots` (slot 0 holds the last
  result); one `Session::step` per statement; the last value is the result; empty source gives `null` with no
  fingerprint.
- **G.3 Names:** no resolution beyond `mc-syntax`'s lowering (module and import resolution, semantics 1.2, is a
  follow-up).
- **G.4 Natives:** `add` and `mul` associative and commutative with a small-integer fast path before building an
  `Arith`; `pow`, `complex`, `ball_literal` natives (not folded at read time); `equal`/`unequal` through
  `Arith::equal` for numbers, identity otherwise, distinct strings unequal, else unevaluated; order relations decided
  for reals (NaN false), overlapping balls and non-numbers unevaluated; `not_same`, `not`, `and`/`or` (held, left to
  right, stopping at the first deciding value or failure); `im` is `complex(0, 1)`. These replace `mc-eval`'s
  machine-integer comparisons.
- **G.5 Stop:** a `StopFlag::on_raise` hook calls `Control::interrupt` and raises `mc-arith`'s flag; interrupt requests
  survive the start of an evaluation; before each `eval` the engine clears the old request then checks the flag; a flag
  raised by the end of a statement gives `Stopped`.
- **G.6 Interruptible printing:** printing runs inside `mc-arith`'s stop scope.
- **G.7 Depth limit:** 20000 in optimised builds, 2000 in debug builds (larger frames overflowed the 64 MB pool stacks).
- **G.8 Status mapping:** `Interrupted` → `EngineError::Stopped`; `Fuel` → `Limit("fuel")`; `Depth` →
  `Limit("evaluation depth")`; `Index`, `DivZero` and every failure term are printed as values.
- **G.9 Fingerprint:** `mc_core::shapes::fingerprint`, structural only, no random seed.
- **G.10 Order:** `TermOrder::sort` builds `mc-arith`'s `Order` and name cache once per sort.
- **G.11 bind_last:** `Kernel::set_own` between evaluations; only a fully successful `eval` updates the last result.
- **G.12 Syntax errors:** the first diagnostic's message and byte offset, further diagnostics appended.
- **G.13 Defaults:** `min(4, available)` threads, default heap, default `EvalConfig` with unlimited fuel except the
  depth limit.
- **G.14** `mc-arith` integration tests run on both stores through a runtime-dispatch `TestStore`.

## Known gaps

- The evaluator has no closures, blocks, `let`/`var`, `while`, assignment, `match` or `part`, and no `optional` /
  `held` patterns; these block most cases.
- The memo trivialises R1.
- `add`/`mul` arguments are sorted twice (the evaluator's AC canonicalisation, then `Arith`).
- `print` output (`Kernel::output`) is not shown: the `Engine` trait has no output channel.
- No engine-side fuel or memory budget (the benchmark runner's job cap applies).
- `mc-arith`'s stop path computes garbage after a stop; other kernels could in principle panic on `Nat::sub`
  underflow (not observed; audit).
- Machine-real comparison follows IEEE: `1/3 == 0.3333333333333333` is `true`.

## Item 9 (library and kernel gaps)

- The install-order comment reflects 9.1 (`gcd`/`lcm` owned by `mc-poly`); after the `reloaded` loop the engine calls
  `k.clear_attached` for every pair in `read.detached` (9.6). Tests: a reload test in `tests/modules.rs`; session cases
  `1.0/4.0`, `2.0^-3`, `3.0/7.0`, `u/v` for reals, rationals, balls in `tests/engine.rs`.
- Benchmark re-runs (guard, 3 GB): N9 10^5 14.1 ms, 10^6 172 ms, 10^7 2.80 s (was the time limit), all pass; P4 5/10/15
  pass; P12 100 and 1000 pass, 10^4 still stops at its known limit.

## `timing(e)`

`timing(e)` holds its argument, evaluates it once and gives `[seconds, value]` (wall clock). It is an effect native
(Hold::All), so a repeated timing of the same term is not answered from the memo. Registered in `history.rs`.

## Round 11

- **Guard:** the evaluation of each statement is guarded by the evaluator (`mc-eval/NOTES.md`, "Round 11"); the engine
  also guards reading (`internal error while reading: ...`, nothing runs), the `;` scan, and printing, rendering and
  the compiler's registration of each statement (`failure(internal, ...)` as the statement's failure, with the
  output made before the panic). `mc_engine::guard` re-exports the guard for hosts.
- **Fuzz campaign** (`tests/fuzz.rs`): inputs from the documentation examples, the REPL sessions, the example scripts and
  the `("input", "expected")` pairs of every crate's tests, used as they are, mutated (numbers replaced, functions
  swapped, arguments dropped or doubled, atoms and reader noise inserted, calls wrapped) or as random calls of library
  functions on odd arguments; fuel 2·10^7 per statement, a 3 s stop and a 20 s grace (then `HANG` and exit 3), a
  fresh engine every 400 inputs, plots in the test's temporary folder. Properties: no panic escapes, no internal
  failure, no hang. 300 inputs in the suite; `MC_FUZZ_CASES` / `MC_FUZZ_SEED` for long runs, which print `[k/N pct%]`
  and log each input before it runs (`fuzz-log.txt`: the last entry names the input of a crash).
- **Regressions** (`tests/robust.rs`): the inputs the campaigns found, with their values now.

## Round 20

- `attributes` lists `flat` with `orderless` for associative and commutative heads (`attributes(add)`); clearing
  `flat` clears both. `minus(x)` = -x in the prelude (for `key_sort_by(a, minus)`). Tests: `tests/b20_engine.rs`;
  Q19-07, Q19-22, Q19-24 un-ignored in `tests/q19_quality.rs`.

## Round 25

- `set_attributes(f, flat)` now means associative only (`flat`); `orderless` stays the AC attribute; `attributes`
  reports `flat` for both. See mc-eval NOTES (R22-13). Test `r22_13_flat_only_matching`, `q19_31` un-ignored.

## Round 29

- New natives in `history.rs`: `names(pattern | [patterns])` (sorted symbol names matching `*` and `@`, as strings;
  the hidden and renamed symbols are left out) and `own_values(x)` (`[x :> value]` or `[]`); `echo_timing(e)`,
  `echo_timing(e, label)` in the prelude (prints the seconds, gives the value).
- `extract(x, pos)` evaluates what it returns (`extract(quote(...), positions)` gives `[3, 7, 5, ...]`); the
  three-argument form keeps wrapping before evaluating. `arguments_options` reads held parts with `part` instead.
- `list_interpolation`: `n` of the body turned the integer order into `3.0` (a function of `[x, y]` with four or more
  nodes on an axis then gave no value in `plot3d`, `contour_plot`, `n_integrate`); the order is floored again.
- Small core functions: `up_values`, `down_values`, `sub_values` are NOT reached: a rule keeps only its compiled
  pattern (conditions are closures), so listing definitions needs the source terms kept per rule in mc-eval's
  lock-free rule sets. `defer` is `quote` (no separate display wrapper); there is no `options_pattern` and no
  assignment of option defaults, since options are `name:` arguments; `heads` is the existing `heads:` option of
  `level`/`map`; `failsafe` is not provided. Echo: when the printed line and the value agree, the cell shows the
  printed line once (engine `echoes`).
- By design: `check(0^0, err)` (0^0 is 1), canonical-order differences (`apply(f, a + b + a)`), sparse
  arrays and graphs as non-atoms (in `leaf_count` and `depth`), `unevaluated(...)` passed unwrapped (`is_color`).

## Round 30

- `down_values(f)`, `up_values(g)`, `sub_values(f)` (natives in `history.rs`, arguments held; a name as a string
  works too): the definitions as written, `lhs :> rhs`. mc-eval now keeps each definition's source term with its
  first rule (`RuleRhs::Term::src`, read through `NativeCx::rule_sources`); down values are the symbol's own rule
  set, sub values the rules kept under `call$` for calls with a compound head rooted at `f`, up values the
  argument-head rules attached to `g` (`@on(g)`). Rules without pattern variables are listed first in canonical
  order (they cannot overlap), then the rest in the order they are tried. Built-in
  code, `mc-rules` tables and a `fn`'s catch-all are not listed. Not done: assigning to them (`down_values(f) =
  rules` to reorder rules): by design (rules are ordered by specificity,
  semantics 6.3, not by a user list).
- `string_match_q`, `string_free_q`, `string_count`, `string_position` have operator forms (data first).
- `quantity_array(v, unit)`, `quantity_array(v, [u1, ..., uk])` (library `lib/language.mc`, loaded last):
  the list of quantities, one unit for all entries or one per position of the innermost lists. Decision r30.2:
  arrays are lists in this language, so a quantity array is a plain list of `quantity(...)` terms (no structured
  form); every list and quantity function then applies (`total`, `dot`, `norm`, `dimensions`, plots).
- `subscript(x, i, ...)`: an inert indexed name (a declining native, so it is a known function; no definitions),
  usable as a variable (`solve`, `d`); notebooks typeset it (mc-boxes already did). Decision r30.3: the indices are
  not held from `n` (`n(subscript(x, 1))` gives `subscript(x, 1.0)`).
- Tests: `tests/round30_language.rs`.

## Round 34

- The `Engine` trait and `StopFlag` come from `mc-api` (step 2). The installation of the libraries, the MathCAS
  library sources in load order and the `FUNCTIONS` tables moved to `mc-libraries` (step 4): `with_config` calls
  `mc_libraries::install` between the arithmetic and the engine's own natives, and evaluates `mc_libraries::SOURCES`
  after the prelude; `LIB30_SOURCE` (now `lib/language.mc`) went with them. No behaviour change.

## Round 36

Operator forms added: `merge` (data first), `linear_solve`, `nearest` (data last), `flatten_at` (data first). Tests: `tests/r36_w36_linalg.rs`.

## Round 36, later fixes

- Contagion numericizes root objects with exact coefficients (and accepts complex machine values); comparisons with glaisher/khinchin are decided by their 40-digit values.

## Round 37

- `equal()` and `unequal()` with no arguments are true (vacuously, as with one argument).

## Round 38: vector calculus in orthogonal coordinates, logarithms to a base

- `grad`, `div`, `curl`, `laplacian` with a third argument "Cartesian", "Polar" (r, t), "Cylindrical" (r, t, z) or
  "Spherical" (r, t, p; t from the z axis) are library rules (`lib/calculus.mc`) from the scale factors (Morse and
  Feshbach, Methods of Theoretical Physics, 1953, section 1.4); results simplified. Other systems stay unevaluated.
- `log(b, x)` is `log_base(b, x)` (a rule, so other two-argument calls are left as they are); before, two machine
  arguments stayed unevaluated. Tests: `tests/round38_vectors.rs`, `tests/round38_lists.rs`.
## Round 38: distances and palindromes in the lists library

`edit_distance` (Levenshtein; the row-by-row dynamic programme of Wagner and Fischer, J. ACM 21, 1974),
`chessboard_distance`, `squared_euclidean_distance` and `palindrome_q` (lists, strings, decimal digits of
integers) in `lib/lists.mc`, beside `hamming_distance`. The prelude stays small: the list functions that grew this
round (`reverse_sort_by`, `ordering_by`, `commonest`, `delete_missing`, the distances) moved to `lib/lists.mc`,
and `prime_zeta_p` to `lib/special.mc`.
