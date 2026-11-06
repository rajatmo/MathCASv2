# Package D: `mc-bench` and `mc-repl`

Status: built and tested on branch `core/d-bench` (commits 08f1430, 7e6114a). The runner executes the sample cases
with the stub engine, each in its own guarded process, and writes a results file, a trace and a summary; the
memory-cap case and both timeout cases are reported as `limit` with no time. The REPL runs with the stub. No build or
clippy warnings; 38 tests pass (mc-bench: 23 unit + 5 end-to-end; mc-repl: 3 unit + 7 scripted sessions). Neither
crate has dependencies.

## What was built

**mc-bench** (`crates/mc-bench/src/`):
- **`engine.rs`**: the `Engine` trait, which the REPL uses too: `name()`, `eval(src, &StopFlag) ->
  Result<Evaluated{printed, fingerprint: Option<u64>}, EngineError>`, `bind_last(name)`; also `StopFlag` (raise /
  clear / is_raised), `EngineError` (Syntax{message, offset} | Stopped | Limit | Other) and
  `EngineFactory = dyn Fn() -> Box<dyn Engine>`.
- **`case.rs`**: the case-file format. A case file `bench/cases/<class>/<id>.mc` is a valid v2 script whose `#@`
  directive lines are comments to the language.
  - Directives: `case`, `class`, `title`, `stresses`, `note`, `sizes: var = s1, s2`,
    `check: exact | exact hash | property | digits d | none`, `expect <size>: value` (or `@file`),
    `limits: time 60s, memory 8GB`, `reps: 5, 2s`, `session: shared|fresh`, `threads`.
  - Sections: `#@ setup`, `#@ measure`, `#@ property`. Errors carry line numbers.
- **`check.rs`**: exact, hash and digits checkers, plus FNV-1a.
- **`guard/`**: the `Guard` trait. `JobGuard` (Windows) uses a job object with a per-process memory limit and
  kill-on-close through hand-written kernel32 declarations, std only. `PortableGuard` enforces only the timeout.
  `platform_guard()` picks one.
- **`child.rs`**: one measurement: setup, warm-up, repetitions, peak memory, check, then one result line.
- **`runner.rs`**: finds the cases (natural order), runs each measurement in a guarded child, prints
  `[k/N pct%] class/id size: status ...` lines, writes results and a trace.
- **`results.rs`**: TSV results with a machine header (CPU, cores, RAM, OS, rustc), and median/min/IQR.
- **`summary.rs`**: class tables, geometric means, overall mean, milestones M1–M4, a flag for memory above 4× the
  reference.
- **`memory.rs`**: `CountingAlloc`, a counting global allocator.
- **`stub.rs`**: `StubEngine`.
- **`cli.rs`**: `list`, `run`, `summary`, and the internal `child` command; binary in `src/bin/mc-bench.rs`.

**Cases:**
- Sample cases for the stub (`crates/mc-bench/samples/`): fib, fact (hash check), digits, wrong (a check that fails on
  purpose), sum (property check, fresh session), timeout, hang, memcap.
- 24 repository cases in the concrete syntax under `bench/cases/` (they cannot run yet): R1, R2, R8, R9, R12; P1, P2,
  P4, P5, P7, P11, P12; N1, N5, N6, N7, N9, N10, N12; S1, S2, S6, S7, S8. Stored values were computed with Python or
  are known results (Collatz maxima 261, 350, 524; resultants 7^n; prime_pi and prime values; S1 sums). The first 10^4
  digits of pi are in `bench/cases/N/ref/N1-pi-1e4.txt`.

**mc-repl** (`crates/mc-repl/src/`): `Repl::new(Box<dyn Engine>, StopFlag)` and
`run(&mut dyn BufRead, &mut dyn Write)`. Input continues while brackets are open (strings and comments skipped).
Commands `:help`, `:quit`, `:time`, `:history`, `:redo [k]`, `:cancel`. Syntax errors show line, column and a caret.
`interrupt::install` registers a Ctrl+C handler on Windows that raises the stop flag.

## Measurements (busy machine, stub only)

- A release run of the 13 sample measurements takes about 3 s.
- Stub evaluations take 0.3–8 µs; `sum(range(10^5))` takes 1.4 ms with a heap peak of 4 MB.
- The memory-cap case stops at its 64 MB cap; the guard saw a process peak of about 50 MB.
- The timeout case stops cooperatively at 300 ms; the hang case is killed at its limit plus the grace period (1.3 s in
  tests, 2.3 s with `--grace 2s`).

## Decisions

- **D.1** The harness has no dependencies; it drives an `Engine` given as text in, printed result out. mc-repl depends
  on mc-bench for that trait and the stub. (05-assembly says mc-bench depends on mc-api; it does not need to.)
- **D.2** Case files are `.mc` v2 scripts with `#@` comment directives. The size is bound by `var = size` before the
  setup. Size variables avoid `n` (the numeric function) and `e`.
- **D.3** The stub evaluates integer arithmetic only: i128 values, lists, comparisons, assignments, and `range`, `sum`,
  `length`, `fact`, `fib`, `busy` (polls the stop flag) and `busy_blind` (does not; tests the hard kill). Its parse
  depth is bounded.
- **D.4** Guard handshake: the child waits for one line on stdin, sent only after the process is in the job, so no
  allocation escapes the cap. The guard waits for both pipes to close, with the timeout as deadline (no polling). A
  memory-cap hit is a nonzero exit plus either Rust's "memory allocation of" message or a process peak at the cap
  minus a margin.
- **D.5** Memory per evaluation is peak heap above the baseline (counting allocator in the child), maximised over
  repetitions; the job's process peak is stored as `proc_peak_bytes`.
- **D.6** Time limit in two layers: a watchdog in the child raises the stop flag at the limit (`limit` "time"); the
  guard kills at limit + grace (default 5 s; `limit` "time: killed by the guard", counted as a hang, so M1 is not
  met). Repetitions stop when the next would pass the limit; at most 100 000.
- **D.7** The digits check passes if the printed value differs from the reference by at most one unit in the d-th
  digit; for balls, containment is taken from the printer showing only correct digits (04-syntax section 4).
- **D.8** Results go to `bench/results/<machine>/<date>.tsv`; a partial rerun replaces rows with the same (system,
  case, size). Bytes as integers, times in shortest round-trip form.
- **D.9** The summary's reference is another results file. From M2 on, any measurement without a time and any `fail`
  check means not met. Class M is not scored.
- **D.10** REPL: the console's own line editing, history in memory; the stop flag is cleared before and after each
  evaluation; end of input with the flag raised is treated as Ctrl+C at the prompt.
- **D.11** Each case's memory cap is lowered to `--mem-ceiling`, default 4 GB (shared-machine rule; the protocol
  default is 8 GB).

## Contract request

`exact hash` checks store fingerprints across processes and runs, so the real engine's fingerprint must be the same in
every process and run (no random seed). If `structural_hash` cannot promise that, the engine needs a separate
canonical fingerprint.

## Known gaps

- Library names in the cases are guesses (`nest`, `depth`, `integer_exponent`, `integer_digits`, `integer_length`,
  `factor_integer`, `next_prime`, `quotient`, `string_join`, `to_string`, `ordered`, `select`, `cases`, `rewrite`,
  `coefficient`, `normal`, `series`, `resultant`, `binomial`); the cases were parsed only by the harness, not by
  mc-syntax.
- No repository case uses `exact hash` yet (needs the real fingerprint).
- N1 has a reference only for 10^4 digits; 10^5 and 10^6 are `unchecked`.
- Cases not yet written: R3–R7, R10, R11, R13, R14, P3, P6, P8–P10, P13, N2–N4, N8, N11, S3–S5, S9, S10, classes L and
  M. S10 (startup) needs its own mode.
- Ctrl+C was not tested in a real console; non-Windows code is not compiled here; `threads` is recorded, not enforced.

## What integration needs from this package

1. Binaries shaped like `crates/mc-bench/src/bin/mc-bench.rs` and `crates/mc-repl/src/main.rs` with a factory for the
   real engine.
2. An `Engine` on mc-syntax and mc-eval: parse whole input, evaluate, print with the mc-syntax printer, failure terms
   as ordinary results; map `StopFlag` to the core's stop mechanism (`Stopped`), engine limits to `Limit`; a
   process-independent fingerprint; `bind_last` defines an own value.
3. Fix the case files to the real library names; `mc-bench list` validates the format.

Commands: `mc-bench list`; `mc-bench run [--cases DIR] [--only R1,S] [--results DIR] [--grace 5s] [--mem-ceiling 4GB]
[--reference FILE]`; `mc-bench summary FILE [--reference FILE]`.

## Round 11 (release): Unix guard and third-party notices

- **`guard::RlimitGuard`** (Unix, the platform guard there): the child gets a memory resource limit between `fork` and
  `exec` (`RLIMIT_DATA` on Linux and Android: private writable mappings, heap and `mmap` included, address space
  reserved without access excluded; `RLIMIT_AS` elsewhere), never above the hard limit the parent already has; the
  guard reaps with `wait4` and reports `ru_maxrss` as the peak. `Container` gained `wait` (default `Child::wait`).
  The `memcap` sample now stops at its 64 MB cap on Linux as on Windows. Dependency: `libc` 0.2 on Unix only.
- **`notices`** (binary `mc-notices`, wrapped by `tools/notices.ps1`): `cargo metadata --filter-platform <target>`,
  the normal-dependency graph from the packages that build the shipped binaries (`SHIPPED_BINARIES`), the licence
  check (`spdx::ALLOWED`, per-crate `EXCEPTIONS`), and `THIRD-PARTY-NOTICES.txt`: crate table, each distinct licence
  text once with the crates carrying it, the bundled fonts' licences (each workspace member's `fonts/` folder), and
  `packaging/licenses/embedded-data.txt`. Crates published without licence files get the standard text of the chosen
  licence from `packaging/licenses/`. A small JSON reader (`notices::json`) keeps the crate free of other dependencies.
- Tests: `tests/notices.rs` runs the check and the rendering on the real workspace for the host target (CI fails on a
  disallowed licence or a stale exception) and checks that hayro-interpret's `LAB.icc` is the CC0 profile shipped by
  hayro-jpeg2000.
