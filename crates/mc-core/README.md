# mc-core

The core term store: a concurrent hash-consed arena with a generational collector and number payloads, implementing
`mc_api::Store`.

## Modules
- `heap`: `Heap`, construction, inspection and the memo, all through `&self` from any number of threads.
- `young`, `old`: the two regions: a concurrent nursery (per-thread id chunks and slabs, one shared lock-free interning
  table) and a dense old region that is immutable between safe points.
- `node`, `handle`, `segs`: the 32-byte node, the bit layout of a 32-bit handle, lazily allocated segment arrays.
- `hash`: structural hashes that depend only on structure and symbol names, never on handles.
- `symbols`: the symbol table; symbols are immediates and never move.
- `gc`: the generational collector, run at safe points only.
- `memo`: the memo of normal forms, dense by term id, weak, lossy, stamped with start epoch and read set.
- `roots`, `session`, `scratch`: root containers, sessions (one evaluation per step, a safe point after each), scratch
  scopes for the garbage of one long evaluation.
- `unpack`, `verify`, `shapes`: packed-array arguments on demand, heap invariant checks, session shapes for stress
  tests and measurements.
- `alloc`: `CachingAlloc`, the global allocator of the binaries that evaluate (`mc-kernel`, `mc-repl`, the docs page
  runner): blocks of up to 1 KB in size classes, freed ones kept in a few per-thread caches (at most 4 MB each) for
  reuse, everything else from the system allocator.

## Design
- Symbols and 30-bit integers are immediates; larger integers, rationals, balls and strings are payload nodes
  interned by value.
- Collection needs `&mut Heap`, so a handle kept across a safe point does not compile; documentation examples check
  this with `compile_fail`.
- Stress mode verifies the heap before and after every collection.
- Depends on `mc-api`, `rayon`, `parking_lot`, `crossbeam-utils` and `rustc-hash`; it has no evaluator.

## Tests
- Unit tests in `src`; integration tests in `tests/` for the store contract, handles, the memo, numbers, packed
  arrays, scratch scopes and collection under stress, and the caching allocator as a test binary's global
  allocator; documentation examples including `compile_fail` ones.
- Run: `cargo test --release -p mc-core`; the binary `mc-core-bench` measures the store on session shapes.
