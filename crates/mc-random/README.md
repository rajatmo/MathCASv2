# mc-random

Random numbers: the session generator and the functions that draw from it (`seed`, `random_seed`, `random_real`,
`random_integer`, `random_normal`, `random_choice`, `random_sample`).

## Modules
- `philox`: the counter-based generator Philox4x32-10 (Salmon, Moraes, Dror and Shaw) and `Stream`, with `Stream::split`
  for explicit parallel constructs.
- `draw`: conversions from random words to uniform reals, unbiased integers in any range (big ones too) and normal reals.
- `natives`: argument checks, then the effect, then the draws under one lock of the session state.
- `lib.rs`: `Generator` and `Rng`, the session's state (seed and stream) shared by the natives of one engine.

## Design
- Every draw is an effect: the natives are registered as effects and call the effect hook before touching the state,
  so a draw happens only on the sequential frontier, outside conditions, in the sequential order of evaluation, and no
  result that involved a draw is memoised.
- The generator is counter-based, so a seed fixes the sequence and streams can be split for parallel work.
- The words a conversion consumes depend only on the stream, never on the thread count or timing.
- Depends on `mc-api`, `mc-arith`, `mc-core` and `mc-eval` only.

## Tests
- Unit tests in `src`, a documentation example, integration tests in `tests/` (producers, sessions, statistical
  checks), and an example program in `examples/`.
- Run them with `cargo test --release -p mc-random`.
