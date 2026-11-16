# mc-repl

The MathCAS read-eval-print loop and script runner. The `mc-repl` binary runs the REPL, runs script files, and runs
or updates notebooks headlessly.

## Modules
- `lib.rs`: the loop over a pluggable `Engine`: it joins lines while brackets are open (ignoring strings and
  comments), hands complete inputs to the engine and handles `:` commands.
- `cli`: the command line: no arguments starts the REPL, file arguments run the files in order.
- `host`: the real engine (`mc-engine`) as the loop's `Host`: `:load` and `:modules`.
- `interrupt`: forwarding Ctrl+C to the engine as a stop flag instead of ending the process.
- `notebook`: `mc-repl notebook.mcn` and `--update`, the headless notebook runner, which evaluates through `mc-kernel`.
- `main`: the binary entry point.

## Design
- Interrupts are a `StopFlag` shared with the engine: the console handler raises it, the engine stops at its next
  poll, and the loop reports the interruption and continues.
- Line editing is the terminal's own; the loop keeps its own in-memory history for `:history` and `:redo`.
- The loop depends on the `Engine` trait, so it can drive other engines (the benchmark crate uses the same trait);
  file and module commands need a `Host`, which the real engine implements and a plain engine lacks.
- Notebook files are read and written through `mc-nbfile`, and exported through `mc-nbexport`.

## Tests
- Integration tests in `tests/` run the command line, sessions from script files, embedded use, export and the
  notebook runner.
- Run them with `cargo test --release -p mc-repl`.
