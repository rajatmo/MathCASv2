# mc-api

The interface contract of the core: the `Store` trait for term stores and the `Engine` trait for whole evaluators.
Generic crates are written against these traits, so they do not depend on a particular store or evaluator.

## Modules
- `lib` (crate root): the term handle `T<'h>` and the `Store` trait.
- `conformance`: checks every `Store` implementation must pass; an implementing crate calls `check` from a test.
- `engine`: `Engine`, `EngineFactory`, `Evaluated`, `EngineError`, `StopFlag` and `RaiseHook`: what the benchmark
  runner, the REPL, the kernel and the documentation drive.
- `packed`: packed numeric arrays, a rectangular list of machine numbers held as one node.
- `refstore`: `RefStore`, a simple model store (append-only, one lock, no collection) for testing generic code.
- `walk`: walks over a term as the graph it is.
- `stack`: how much stack the calling thread has left.
- `work`: work done below the evaluator, counted so that it can be charged as fuel.

## Design
- A handle `T<'h>` is 32 bits, `Copy`, and branded with the lifetime of the store borrow that produced it; equal terms
  are the same handle (hash-consing), so `==` is structural identity.
- A handle must not outlive its borrow; terms kept across evaluations live in a store's root containers.
- The only dependency is `parking_lot`; the crate knows no implementation and no evaluator.
- A change here is a change of the contract: every implementation and every user is updated with it.

## Tests
- Unit tests in `src` (packed arrays, walks, work counting, the model store).
- An integration test runs the conformance checks against `RefStore`.
- Run: `cargo test --release -p mc-api`
