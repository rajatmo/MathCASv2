# MathCAS v2

An independent computer algebra system built around one idea: **a small, fully specified core that combines fast
term rewriting with first-class arbitrary-precision and exact algebra.**

v2 is a new design, not a port. It has its own core syntax, its own evaluation control and its own number model. A
compatibility layer for other languages may come later, as a front end that translates into the core, never as the
core's semantics.

## Goals

- **Simplicity:** few core primitives, every one of them specified; no behaviour that a reader of the spec could not
  predict.
- **Speed:** measured, not claimed, on a fixed benchmark suite (rewriting, polynomial algebra, big numbers, numeric
  loops, structural operations).
- **Predictability:** evaluation that terminates or reports, resource limits (time, memory, fuel) built in from the
  first line of code.

## Status

Design phase. Start with [docs/design/00-plan.md](docs/design/00-plan.md).

Spikes live in `spikes/`; run one with `cargo run --release -p termstore` (add `--quick` for the small sizes).
