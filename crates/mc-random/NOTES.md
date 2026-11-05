# `mc-random`: the session random-number generator

Status: branch `random`, first round. The session generator and seven functions work, with tests: known-answer
vectors, reproducibility, independence of the thread count under real speculation, no caching, statistics.
Dependencies: `mc-api`, `mc-arith`, `mc-core`, `mc-eval`; dev `mc-engine`, `mc-bench`. No new external crates.
Outside the crate there are only registration lines in `mc-engine` and `mc-lsp`; `mc-eval` is unchanged.

## What was built

| module | contents |
|---|---|
| `philox` | `philox4x32_10` (the block function), `mix64` (SplitMix64's finaliser), `Stream` (key, counter, spare word; `next_u64`, `split`, `fork`) |
| `draw` | conversions on `Stream`: `uniform`, `below` (Lemire), `up_to` / `integer` (any size), `normal_pair` / `normals` (Box–Muller) |
| `lib` | `FUNCTIONS`, `Generator` (seed and stream), `Rng` (the shared session state), `key_of`, `os_seed`, `install` |
| `natives` | argument checks, then the effect, then the draws |

## Functions

| call | value |
|---|---|
| `seed(n)` | `null`; resets the generator to integer seed `n` (any size, negative too) |
| `seed()` | `null`; a fresh seed from the operating system |
| `random_seed()` | the seed in use: `seed(random_seed())` at the start of a run reproduces it |
| `random_real()`, `random_real(b)`, `random_real([a, b])` | a uniform machine real in [0, 1), [0, b), [a, b) |
| `random_integer(n)`, `random_integer([a, b])` | a uniform integer in [0, n], [a, b] (any size) |
| `random_normal()`, `random_normal(mu, sigma)` | a normal machine real |
| `random_choice(xs)` | a uniformly chosen element of the list `xs` |
| `random_sample(xs, k)`, `random_sample(xs)` | `k` elements at distinct positions, in random order; a permutation |

`random_real`, `random_integer`, `random_normal` (as third argument) and `random_choice` take a count as a further
argument: `n` gives a list of n, `[n1, ..., nk]` nested lists of that shape (row-major draw order). Range bounds may
be exact numbers or closed expressions (`[0, pi]`, made numeric by `n`).

```
seed(11); random_real()                -> 0.17525075840744309
random_integer([1, 6], 10)             -> [4, 6, 5, 6, 5, 6, 3, 3, 2, 1]     (after the three draws before it)
random_integer([10^30, 10^30 + 5], 2)  -> [1000000000000000000000000000003, 1000000000000000000000000000000]
random_normal(10, 2, [2, 3])           -> [[9.53..., 10.91..., 12.24...], [8.31..., 9.40..., 12.32...]]
random_sample(range(10))               -> [1, 8, 7, 6, 9, 5, 3, 10, 4, 2]
```

Bad arguments give `failure(argument, f, "message")`: ranges with a > b or non-numbers, `random_integer` of a
non-integer, negative counts or more than 2^32 elements, a negative standard deviation, an empty list for
`random_choice`, more elements than the list has for `random_sample`, a non-integer seed. A failed check draws
nothing. Argument counts not listed above leave the call unevaluated (decline).

## Decisions

- **RNG.1** The generator is Philox4x32-10 (Salmon, Moraes, Dror, Shaw, SC 2011), written in the crate (40 lines).
  - It is counter-based: a block of 128 bits is the pure function `philox(key, counter)`. Streams for parallel
    elements are then just other keys; no state has to be jumped ahead or copied.
  - Its period per key is 2^130 bits. It passes BigCrush (the paper, section 5), and it is the generator of cuRAND,
    numpy's `Philox` and Random123.
  - The three `philox4x32 10` known-answer vectors of Random123 (`kat_vectors`) are unit tests.
- **RNG.2** The session state is one `Generator`: the seed it was last reset to and a `Stream`. A stream is a
  64-bit key, a 128-bit counter and the unused second word of the last block (each block gives two 64-bit words,
  low first).
  - The state is an `Arc<Mutex<Generator>>` (`Rng`), shared by the natives of one kernel. `install` creates it,
    so every engine has its own generator, and it lives as long as the engine's kernel, across evaluations.
  - The lock is never contended: only the frontier draws (RNG.7).
- **RNG.3** Split streams, for the explicit parallel constructs of semantics 18.2 (5), which are not built yet.
  - `Stream::split(j)` is the stream with key `mix64(key ^ mix64(j + c))`. It depends only on the key and the
    index: not on the position, the order in which elements run, or the thread count. Splits nest
    (`s.split(i).split(j)`).
  - `Stream::fork()` (and `Rng::fork()`) draws one word from the session stream and returns the stream keyed by it.
    A construct forks once, on the frontier, and gives element `j` the stream `base.split(j)`. Two constructs in
    one session thus get different element streams, and a re-seeded session repeats them.
  - Distinct keys are distinct Philox permutations. Two keys collide with probability about n^2 / 2^65 for n
    streams.
- **RNG.4** Seeding.
  - `seed(n)` sets the key to `key_of(n)`: the sign, the limb count and each 64-bit limb, folded through `mix64`.
    Every integer gives a key, and seeds 1, 2, 3 give unrelated keys. The counter restarts at 0.
  - Unseeded sessions, and `seed()`, use `os_seed()`, which needs no dependency. A fresh
    `std::collections::hash_map::RandomState` holds SipHash keys that the standard library took from the system
    generator (getrandom on Linux, ProcessPrng on Windows, ...). It hashes a process-wide counter, the time in
    nanoseconds, the process id and a stack address. The 64-bit result is used as the integer seed, so
    `random_seed()` returns it and `seed(random_seed())` reproduces the session.
- **RNG.5** Conversions. Each one draws a number of words that depends only on the stream:
  - uniform reals: `(u >> 11) * 2^-53`, every multiple of 2^-53 in [0, 1) equally likely. A range `[a, b]` is
    `a + (b - a) u`, so b itself can come out of rounding when a and b differ widely in magnitude;
  - integers below 2^64 values: Lemire's multiply-and-reject (ACM TOMACS 2019). There is no modulo bias, and an
    extra word is needed with probability below range / 2^64. A range of exactly 2^64 values takes the word as it
    is;
  - larger integer ranges `[a, b]`: draw `bits(b - a)` bits (whole words, the top one masked) and reject values
    above `b - a`, fewer than two tries on average. The arithmetic is `mc_arith::num::{Int, Nat}`. No size limit
    applies beyond the arithmetic's own;
  - choices index with `below(len)`. `random_sample` is a partial Fisher–Yates shuffle over a copy of the list, one
    `below` per element taken.
- **RNG.6** Normal reals use the Box–Muller transform, not Marsaglia's polar method.
  - Each pair of normals takes exactly two words: `u1` in (0, 1] (no `log(0)`) and `u2` in [0, 1); the pair is
    `sqrt(-2 ln u1) (cos 2 pi u2, sin 2 pi u2)`. The polar method rejects about 21% of its draws.
  - A list of n takes ceil(n / 2) pairs, in order. A scalar draw and an odd count drop the last pair's second
    value, so no spare normal is kept in the state.
  - The tails are cut at `sqrt(-2 ln 2^-53)`, about 8.57 standard deviations.
- **RNG.7** Every function is registered with `Attributes { effect: true }` and calls `NativeCx::effect()` after
  its argument checks and before it touches the state. This includes `random_seed`, which reads state that
  `seed` changes. The consequences:
  - a speculative task that reaches a draw aborts with a hole, before drawing, and the frontier evaluates that
    element in sequential order (semantics 18.2 (3), (4)). The draw order is the sequential order for any number of
    threads (18.1);
  - an evaluation that performed an effect is not memoised (`machine.rs`: `cx.effects == effects0`), and neither is
    any evaluation around it. So `[random_real(), random_real()]` gives two values, and so do two calls of
    `fn r() = random_real()` (semantics 4.4, 9);
  - the static cost estimate of an `effect` call is 0, so groups around draws start sequential;
  - a draw inside a condition gives `failure(effect_in_condition)` (A.6).
- **RNG.8** `random_integer(n)` is [0, n] (both ends included). `random_real(b)` is [0, b) to
  match.

## Tests (6 unit + 12 integration + 1 doctest)

- `philox::known_answers`: the Random123 vectors (zero, all ones, the digits of pi).
- `philox::words_follow_the_counter`, `philox::splits_are_deterministic_and_distinct`: splits from any position,
  from threads in reverse order, nested, and forks.
- `draw::*`: [0, 1) bounds, Lemire balance on [0, 3), the full-word range, 100-bit ranges, a range just above 2^64.
- `session.rs`:
  - `seeded_sessions_reproduce`: two fresh engines, and the same session after `seed(11)` again; six seeds,
    including `-11`, `2^64 + 11` and `10^100`, all differ;
  - `unseeded_sessions_differ_and_random_seed_reproduces`;
  - `identical_at_one_and_four_threads`: 22 programs with draws in lists, arguments of pure calls, `map`, `table`
    (one and two iterators), named functions, `for` loops, nested groups, and next to costly unmemoised work
    (`spin`). The results are the same at 1 and 4 threads, under the stress collector, and in three repeated
    4-thread runs. They also equal a plain sequential `for` loop. The evaluator's counters (read with a temporary
    probe that is not committed) show about 1600 splits and 2000 holes over one run of the programs at 4 threads,
    so speculation really reaches the draws. With `cx.effect()` removed from the native, 5 of the 7 session
    tests fail;
  - `draws_are_never_cached`, `no_draws_in_conditions`, `values_and_shapes`, `bad_arguments`.
- `stats.rs` (fixed seeds, tolerances of about five standard errors):
  - uniform: mean and variance of 10^5 draws for three seeds; chi-square on 10 bins (< 27.9, the 99.9% point) and
    on 4×4 bins of successive pairs (< 37.7); a scaled range;
  - normal: mean, variance, the share within one sigma, and the fourth moment of 10^5 draws; `mu`/`sigma`; scalar
    draws through `table`;
  - `random_integer([1, 6], 6*10^4)`: every count within 460 of 10^4, and chi-square below 20.5. Thirds of
    [0, 3·2^62 − 1], which is not a power-of-two range, are balanced. So are the counts of `random_choice`;
  - all 6 permutations of 3 elements are equally frequent over 6000 `random_sample` calls;
  - `speed` (prints its rates).

```
cargo test --release -p mc-random                       # 6 + 7 + 5 + 1 doctest
cargo test --release -p mc-random --test stats speed -- --nocapture
```

## Measurements

Release build, one thread, on a shared machine with other builds running (`stats::speed`):

| what | draws per second |
|---|---|
| `random_real([0, 1], 10^6)` | 2.1·10^6 |
| `random_integer([1, 6], 10^6)` | 1.4·10^7 |
| `random_normal(0, 1, 10^6)` | 5.2·10^6 |
| `table(random_real(), 10^5)` | 1.5·10^6 |
| scalar loop `for k in range(10^5) { x = x + random_real() }` | 3.2·10^5 (the interpreted loop dominates) |
| `Stream::uniform` in Rust (the generator alone) | 1.6·10^8 |

The list functions are bound by building terms; machine reals are heap cells, while small integers are immediate.
The generator costs under 10 ns per word.

## Gaps

1. **`pmap`, `ptable`, `parallel` are not built.** The design for them:
   - the construct calls `Rng::fork()` once, on the frontier (an effect);
   - element `j` draws from `base.split(j)`. Its draws are then private to the element and need not be ordered, so
     they may happen on speculative tasks;
   - this needs one hook in `mc-eval`: a task-local stream slot in `Cx` (set by the construct for each element's
     task and inherited by its sub-tasks, with nested constructs splitting further), visible to natives as
     `NativeCx::stream()`;
   - the random natives would then draw from the task's stream when one is set, and announce the effect as now when
     none is. A draw on a private stream must still keep the result out of the memo (`NativeCx::volatile`).

   `install` returns the `Rng`, so the engine can hand it to these constructs.
2. **Compiled code cannot draw.** The numeric compiler inlines rules and treats unknown calls as not compilable, so
   a function that calls `random_real` stays interpreted (`compile` reports the reason). To support it:
   - make `random_real()` and `random_normal()` (and integers, guarded for range) primitives of `mc-jit`'s front
     end;
   - lower them to calls of new run-time helpers `rt::random_uniform(env) -> f64` and similar, whose `Env` carries
     a pointer to the session `Stream`. The host locks the `Rng` for the duration of the compiled call;
   - the helpers use exactly `Stream::uniform` and `Stream::normal_pair`, so compiled and interpreted draws agree
     (exactness, decision 1b.3);
   - a compiled call that would draw must run on the frontier. The specialisation calls `cx.effect()` before
     entering, or declines on speculative tasks;
   - a compiled loop that draws cannot be parallelised under 18.2 (2) unless its iterations use split streams by
     iteration index, as in gap 1.
3. **No distributions as objects** (`random_variate(normal_distribution(...))`, roadmap 3D). The `draw` conversions
   are their building blocks.
4. **Interrupted list draws.** A fuel limit or interrupt in the middle of a long list leaves the stream advanced by
   the elements drawn so far. The value is a limit failure, whose partial content semantics 18.1 already exempts.
5. Only machine reals: there are no ball-valued uniform draws at a given precision yet. They would be 53·k-bit
   mantissas from k words.
6. Draws in `random_real([a, b])` with |a| much larger than b − a lose the low bits of `u` to rounding.

## Package N4b

Compiled draws: `Stream::real_in`, `normal_scaled`, `int_in` and `draw::real_range_ok`, `normal_ok` are the conversions the natives and the compiled helpers share, so compiled and interpreted draws agree bit for bit. Gap 2 above is done; see `crates/mc-jit/NOTES.md`, section "Package N4b".

## Round 7 package P: producers build payloads

`random_real`, `random_normal` and `random_integer` (when the range fits machine integers) with a count `n` or a shape
`[n1, ...]` draw into a plain number buffer (reserved exactly when the allocator allows) and build the result with
`Store::real_array` / `int_array`, packed whenever the packing rule says so. Scalar draws, big-integer ranges and
`random_choice` still build element terms. Nested results go through `mc_api::packed::build_nested` (replacing the
crate's own `nest`, `nest_empty`). Draw order, fuel per element and failures are unchanged.

**P.1** Packing is canonical, so the result is the same handle as the old element-by-element build.

Tests (`tests/producers.rs`, 6, written and passing on the old code first): uniform, normal and integer lists are bit
for bit the session stream at sizes 0-1001 (around the packing threshold) and several ranges (`[0, pi]`, the full i64
range); big ranges still give big integers; shapes are row-major, empty dimensions give nested empty lists; packed
results equal the same draws made one at a time (`table`, `===`); identical at 1 and 4 threads and under the stress
collector.

Measurements (`cargo run -p mc-random --example producers -- <case>`, n = 10^7, one case per process):

| statement | before | after |
|---|---|---|
| `random_real(1.0, n)` | 1551 ms, peak 965 MB | 91 ms, peak 158 MB |
| `random_real([-1, 1], n)` | 1558 ms, 967 MB | 91 ms, 160 MB |
| `random_normal(0, 1, n)` | 1650 ms, 965 MB | 158 ms, 158 MB |
| `random_integer([1, 6], n)` | 114 ms, 147 MB | 99 ms, 160 MB |
| `random_real(1.0, [n/100, 100])` | 1614 ms, 1052 MB | 91 ms, 160 MB |

The peak is the 80 MB payload plus the store's copy (an owned packed constructor in mc-core would halve it). The
compiled-code half of package P is in `crates/mc-jit/NOTES.md`, section "Round 7 package P".
