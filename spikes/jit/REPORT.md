# Spike E: numeric compiler

Crate `spikes/jit`. Release build, `cargo clippy --offline --all-targets` and `cargo test` (release and debug) run with
no warnings; 15 tests pass. `--quick` takes about 6 s and `--full` about 21 s under the guard (peak 392 MB); logs in
`results/{quick,full}.txt`. Dependencies: Cranelift 0.137.0 and `hashbrown` 0.15 (for the copied term store); the
hand encoder declares `VirtualAlloc` from kernel32 itself.

## Question

What should v2's numeric compiler be, and what does it gain over interpretation?

## Setup

- **IR (`src/ir.rs`)**, two levels:
  - **HIR**: a typed tree, what a front end produces from a term. Types are i64, f64, complex f64, bool, and packed
    1-D/2-D arrays of i64/f64. It has arithmetic, comparisons, select, `if`, `while`, counted `for`, local variables,
    sin/cos/exp/log/pow, explicit `fma`, `powi` and array load/store. The checker accepts monomorphic machine types
    only and inserts no conversions.
  - **LIR**: scalarised three-address code over virtual registers of three kinds (I, F, B), with labels and branches,
    an explicit bounds `Check` before every array access, and a `Poll` on every loop back edge. Complex numbers become
    pairs of reals and arrays a pointer plus extents. The order of every compound operation (complex multiply and
    divide, `powi`, `abs2`) is defined once, in lowering.
  - An if-conversion pass runs on the HIR.
- **Back ends**, all reading the same LIR: (a) a register-machine interpreter (`interp.rs`); (b) Cranelift 0.137
  (`jit.rs`); (c) native Rust with the same operation order (`native.rs`); (d) a hand-written x86-64 encoder for the
  Mandelbrot escape function (`x64.rs`), 240 bytes, hand register-allocated, with the same poll.
- **Term-level baseline (`term.rs`)**: spike 1's hash-consed arena with machine reals added as interned nodes. Each
  function is converted once to a term (`block(set(..), while(..), ..)`) and run by a strict evaluator that dispatches
  on heads and interns every intermediate value; per-call garbage is removed from the table. This baseline is
  optimistic: no rule lookup, no argument ordering, no stamps. Real symbolic evaluation would be slower.
- **Workloads (`kernels.rs`)**: L1 (a 24-operation expression), L9 Mandelbrot (complex numbers), L3 Lorenz RK4
  (host-driven with a compiled right-hand side, and the whole loop compiled), L5 Newton on x·eˣ = a (host-driven with
  compiled f and f′, and the whole solve compiled), L10 Monte Carlo with a counter-based splitmix64 generator inside
  the compiled code. M1 runs these with rayon at 1, 2 and 4 threads.
- **Checks**: every run compares fingerprints across back ends and aborts on any difference.

## Results (busy shared machine, noisy; full mode)

| Workload, size | term level | LIR interp | JIT | native | JIT vs interp | JIT vs term | JIT / native |
|---|---|---|---|---|---|---|---|
| L1, 10^6 points, ns/point | 873 | 95 (table), 72 (per call) | 14.5 (table), 15.0 (per call) | 11.6 | 6.6× | 60× | 1.25 |
| L9, 1000², ns/iteration | 241 | 43.8 | 1.91 (loop), 1.90 (per pixel) | 2.36 | 23× | 126× | **0.80** |
| L9, hand x86-64 | | | 1.87 ns/iteration, 240 B (Cranelift: 289 B) | | | | 0.79 |
| L3, 10^5 steps, ns/step | 3842 | 254 (rhs) / 294 (whole loop) | 33.1 (rhs called by host) / **10.8 (whole loop)** | 10.7 | 24× (whole loop) | 357× | 3.1 (rhs) / 1.00 (whole loop) |
| L5, 10^5 solves, ns/solve | 1783 | 158 (host) / 311 (whole solve) | 82 (host) / 75 (whole solve) | 64 | 2.1× | 24× | 1.18 |
| L10, 10^7 draws, ns/draw | 1371 | 140 (at 10^6) | 4.78 branch, 2.17 select in source, **2.02 branch + if-conversion** | 2.46 | 70× | 680× | 0.82 |

- **Compile time:** the 10 functions take 58–548 µs each at `opt_level=speed`, 2.6 ms in total, roughly 50 µs plus
  3 µs per LIR instruction. `opt_level=none` compiles about 20% faster; run time is the same on L1 and 8% slower on
  L9. Lowering HIR to LIR takes 1–23 µs; building the interpreter's program 0.3–5 µs. Code size 80–1344 bytes.
- **Break-even (L1 scalar):** compiling costs 123 µs. Per call, the term level takes 873 ns, the interpreter 67 ns and
  the JIT 15 ns, so compiling pays for itself after about **143 calls against term evaluation** and about 2400 calls
  against the LIR interpreter.
- **Parallel, 1→2→4 threads** (results identical at every thread count): L1 at 10^7 points 1.86 / 3.32 (efficiency
  0.83 at 4); Mandelbrot 1000² 1.99 / 3.92; Newton 10^5 solves 1.88 / 3.77; Monte Carlo 10^7 draws 1.99 / 3.87; the
  interpreter on L1 1.83 / 3.73.
- **Poll cost:** single-threaded, polling every 1024 iterations against never makes no measurable difference (±3%);
  even every iteration costs 0–8%. At 4 threads, polling every iteration makes Monte Carlo **4.2× slower** (5.26 ms
  against 1.24 ms), because all threads hit the shared fuel counter's cache line. Polling every 64 iterations is
  already free.
- **Interrupt latency:** JIT 3–4 µs on Monte Carlo, 1.7 µs on Mandelbrot 3000²; interpreter 40–110 µs; 4-thread
  parallel 85–95 µs (1.3 s before the fix of finding 6).
- **FMA:** fusing the Horner form of L1 changes 17.5% of the outputs, by up to 635,413 ulp near cancellation, and gives
  no speed-up because the libm calls dominate.

## Findings

1. **Cranelift's code matches LLVM's here.** It is 0.8–1.25× native Rust on every kernel, faster than LLVM's output on
   Mandelbrot, and within 2% of hand-written machine code (1.90 against 1.87 ns per iteration). The remaining gap to
   native on L1 and Newton is the libm-call wrapper and native inlining, not code quality.
2. **The gain over term evaluation is 24–700×; over the LIR interpreter it is only 2–70×.** The interpreter is just
   5–10× faster than the (optimistic) term level, while the JIT is 60–700× faster. An extra interpreter tier between
   the two buys little.
3. **Compile the loop, not only the callback.** A compiled right-hand side called from a Rust RK4 driver is 3.1×
   slower than native because of call and argument overhead; compiling the whole RK4 loop equals native.
4. **Branch misprediction dominates Monte Carlo.** If-converting `if inside { count += 1 }` into a select halves the
   time, and the JIT then beats native.
5. **Bit-identical results are achievable** when all back ends share the operation order (defined in lowering), never
   fuse or reassociate, and call the same elementary functions. A 300-program random cross-check (interp = JIT
   = term level, NaNs treated as one value), together with the workload fingerprints, confirms it.
6. **Parallel drivers must check the control block before starting each task.** Without this, every remaining chunk
   runs to its first poll after a stop: the 1.3 s latency.
7. **Compiled loops parallelise almost perfectly** (efficiency 0.83–0.98 at 4 threads) because the code is pure and
   allocation-free. A counter-based generator makes the Monte Carlo count independent of the chunking.

## Decisions

- **E.1 Back end: Cranelift**, pinned `=0.137.0`, at `opt_level=speed`. No LLVM tier (nothing measured calls for one);
  no hand-written encoder (it gains at most 2%).
- **E.2 IR: the two levels above.** The HIR is the front end's typed tree: monomorphic machine types, no implicit
  conversions, arrays only as (non-assignable) parameters, at most one scalar result (complex or array results go
  through array parameters). The LIR is scalarised three-address code; every back end consumes it, and it is the
  single definition of operation order. Integer arithmetic wraps; integer division by zero fails with a
  division-by-zero status; `MIN / -1` wraps. Real-to-integer conversion truncates and saturates (NaN gives 0), as
  Rust's `as` does. Every array access is bounds-checked and fails with an index status.
- **E.3 Passes before the back end:** always if-convert single-assignment branches whose operands cannot fail (no
  loads, no integer division, no calls); leave common-subexpression elimination to Cranelift; numeric routines (ODE,
  table, plot drivers) compile their whole loop around the user function when it is typed, rather than call it back.
- **E.4 Floating point:** IEEE binary64, round-to-nearest; no contraction, no reassociation, no fast-math; fused
  multiply-add only when the source writes `fma`. Elementary functions are calls to one shared set of Rust functions,
  used by every back end and by the term evaluator, so results are bitwise identical within a platform build.
- **E.5 Interrupts and fuel:** every loop back edge decrements a register counter; every 1024 iterations it draws 1024
  fuel from the shared budget with an atomic fetch-sub and reads the stop flag. A failure is recorded by
  compare-and-swap into the control block's status, and the function returns 0. Straight-line code does not poll
  (bounded). Parallel drivers skip tasks once the status is set or the stop flag is raised. 1024 because polling every
  iteration contends at 4 threads.
- **E.6 ABI:** the host's native calling convention, with the flattened parameters followed by a pointer to the
  control block (`#[repr(C)] {fuel: AtomicI64, stop: AtomicU32, status: AtomicU32}`). Every function also gets a
  uniform entry `fn(*const u64, *const Ctl) -> u64` for generic callers and cross-check testing.
- **E.7 When to compile:** immediately when a typed function is handed to a numeric routine (table, plot, ODE, root
  finder, sum over an array) or contains a loop (a 0.1–0.5 ms compile is repaid after about 140 calls against term
  evaluation). Otherwise count interpreted time and compile once it exceeds the estimated compile time (about 50 µs
  plus 3 µs per LIR instruction); at worst twice the optimum. **No LIR-interpreter performance tier:** it stays as the
  baseline back end (cross-check tests) and the fallback when compilation fails or Cranelift does not support the
  host.
- **E.8 Parallel compiled loops:** fixed chunks on the work-stealing pool (16,384 elements for tables, one task per
  image row, 2^16 draws); the result is the same for any number of threads; randomness inside compiled code is
  counter-based, from a seed passed as a parameter (semantics 18.2 item 5).

## What it rules out

- An LLVM back end, a hand-written code generator, and any general JIT for this purpose.
- Implicit fused multiply-add or fast-math.
- Polling interrupts and fuel on every iteration against a shared counter.
- An interpreted typed-IR tier as a performance step.
- Compiling only callbacks for ODE and Newton drivers.
- Accepting any type outside the monomorphic machine types.

## Follow-ups

- Cross-platform bitwise identity needs v2's own libm; today results are identical only per platform build.
- The real front end (terms with `x_: machine_real` patterns turned into HIR with type inference) is not built.
- Calls between compiled functions and inlining are not in the IR yet.
- No vectorisation: Cranelift does not auto-vectorise.
- Freeing code memory on redefinition: one JIT module per definition batch, freed when its stamps are invalidated.
- Reconcile the fuel unit with the evaluator's: here 1 fuel = 1 loop iteration in blocks of 1024; decision 4.9 draws
  blocks of 4096.
- The 8- and 16-thread M1 targets could not be measured under the 4-thread cap.
- 2-D arrays are covered by a test but not by any workload.

## What the final assembly needs from this piece

- **One control block shared with the evaluator:** the evaluator's fuel budget, stop flag and failure status must be
  the very atomics compiled code polls (decision 4.9's shared budget); pick one fuel block size.
- **A status-to-failure mapping:** interrupted → `failure(interrupted)`, fuel → `failure(limit, fuel)`, index and
  division by zero → ordinary failures.
- **Packed arrays exposed as pointer plus extents** (contiguous f64/i64, row-major), never moved or freed by the
  collector while a compiled call or a parallel task holds them: an active compiled call pins its arrays.
- **Machine reals converted at the call boundary** between interned f64 nodes (I3) and raw f64.
- **The parallel scheduler running compiled chunks,** checking the stop flag and status before starting each task.
- **The session random generator** passes a seed per compiled call or `ptable` (counter-based inside).
- **Compiled functions keyed by definition** and invalidated by the normal-form stamps / read sets (decision 13.1)
  when a definition changes.
- **Evaluation dispatch** sends a call to compiled code when the head has a compiled specialisation for the argument
  types, and falls back to term evaluation otherwise.

Files: `src/{lib,ir,interp,jit,x64,term,kernels,native,run,main}.rs`, `tests/agree.rs`, `results/{quick,full}.txt`.
