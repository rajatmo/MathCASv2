# Package G: `mc-engine`, the integration of the core

Status: committed on branch `core/g-glue` (04b3460, 057791a). The REPL runs a 41-entry end-to-end session correctly.
All 263 workspace tests pass in release, with no build, clippy (`--all-targets`) or doc warnings; `mc-engine`,
`mc-repl` and `mc-bench` also pass in debug. Every benchmark case completes or stops cleanly at a limit, so the harness
reports M1 as met; but only R1 produces a correct result, because almost every other case needs library functions or
language features that do not exist yet.

## What was built

- **`crates/mc-engine`** implements `mc_bench::Engine` on the real core: `CoreEngine::new()` /
  `with_config(EngineConfig { threads, heap, eval })`. `eval` reads the whole source with `mc_syntax::read`; if any
  statement has an error nothing runs; otherwise the statements go into `Kernel.roots` and each is evaluated in its own
  `Session::step` (a safe point after every statement). The last value is printed with
  `Printer::with_numbers(h, ArithNumbers)` and fingerprinted with `mc_core::shapes::fingerprint`.
- **`arith::install`** registers `mc-arith` as the arithmetic and comparisons (G.4) and calls `set_order(ArithOrder)`.
- **`format::ArithNumbers`** prints big integers through `Nat::to_decimal` and balls with only correct digits
  (`text::ball_words`).
- **Binaries:** `mc-engine` provides `mc-bench` with the real engine and the counting allocator; the stub harness
  binary is renamed `mc-bench-stub` (used by the harness's own tests); `mc-repl`'s binary uses `CoreEngine`.

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
completes or reports limit cleanly: met"; M2 to M4 unknown (no reference timings). No case fails to parse.

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
| N1 | `n`, `pi` (sizes 10^5 and 10^6 have no reference) |
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

- **G.1 Layout:** the real binaries live in `mc-engine` (`mc-bench`) and `mc-repl`; the stub harness binary is
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
- No engine-side fuel or memory budget (the harness's job cap applies).
- Printer: `add(7, mul(-3, x^2))` prints as `7 + -3*x^2`; complex numbers print as `complex(5, 5)`.
- `mc-arith`'s stop path computes garbage after a stop; other kernels could in principle panic on `Nat::sub`
  underflow (not observed; audit).
- Machine-real comparison follows IEEE: `1/3 == 0.3333333333333333` is `true`.

## Package 9 (library and kernel gaps, branch `lib/p9-gaps`, commit 536c775)

- The install-order comment reflects 9.1 (`gcd`/`lcm` owned by `mc-poly`); after the `reloaded` loop the engine calls
  `k.clear_attached` for every pair in `read.detached` (9.6). Tests: a reload test in `tests/modules.rs`; session cases
  `1.0/4.0`, `2.0^-3`, `3.0/7.0`, `u/v` for reals, rationals, balls in `tests/engine.rs`.
- Harness re-runs (guard, 3 GB): N9 10^5 14.1 ms, 10^6 172 ms, 10^7 2.80 s (was the time limit), all pass; P4 5/10/15
  pass; P12 100 and 1000 pass, 10^4 still stops at its known limit.
