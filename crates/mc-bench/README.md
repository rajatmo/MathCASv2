# mc-bench

The benchmark runner. It reads case files, runs each measurement in a guarded child process, checks results, writes
result files and prints summaries. It also produces the third-party notices of the shipped programs.

## Modules
- `case`: the case-file format, `bench/cases/<class>/<id>.mc`, written in the concrete syntax.
- `check`: result checkers, run after timing and outside the measured region.
- `child`: one measurement (warm-up, repetitions, peak memory, check) in a child process.
- `guard`: runs a child under a memory cap and a timeout: a job object on Windows, a resource limit on Unix.
- `memory`: peak memory of the measured evaluation above the session's baseline.
- `runner`: the parent loop with `[k/N pct%]` progress, results and trace files.
- `results`, `summary`: the stored results format, class tables, geometric means.
- `cli`: the command line shared by benchmark binaries; a binary supplies its engine and calls `main`.
- `stub`: a tiny integer evaluator for testing the runner itself.
- `notices`: crate licences from `cargo metadata`, SPDX expressions, the licence check.

## Design
- The runner has no parser: the evaluator comes through the `mc_api::Engine` trait.
- Binaries: `mc-bench` (real engine), `mc-bench-stub` (stub engine) and `mc-notices` (writes
  `THIRD-PARTY-NOTICES.txt` and checks dependency licences).
- The notices code reads `cargo metadata` with its own small JSON reader and takes no dependencies of its own.
- Each measurement runs in its own child process under the guard, so a runaway case cannot take the runner down.

## Tests
- Unit tests in `src` for case parsing, checkers, SPDX and JSON reading.
- Integration tests run sample cases from `samples/` through the stub binary and check the notices.
- Run: `cargo test --release -p mc-bench`
