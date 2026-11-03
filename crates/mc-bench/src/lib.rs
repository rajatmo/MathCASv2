//! The MathCAS v2 benchmark harness (docs/design/02-benchmarks.md sections 1, 2, 4 and 7).
//!
//! - [`case`]: the case-file format, `bench/cases/<class>/<id>.mc`, written in the concrete syntax;
//! - [`engine`]: the [`Engine`] trait through which the caller supplies the evaluator (the harness has no parser);
//! - [`guard`]: child-process isolation with a memory cap and a timeout ([`guard::JobGuard`] on Windows);
//! - [`child`]: one measurement under the protocol (warm-up, repetitions, peak memory, check);
//! - [`runner`]: the parent loop with `[k/N pct%]` progress, results and trace files;
//! - [`results`], [`summary`]: the results format, class geometric means and milestone status;
//! - [`stub`]: a tiny integer evaluator that stands in for the engine until `mc-eval` exists;
//! - [`cli`]: the command line shared by benchmark binaries.
pub mod case;
pub mod check;
pub mod child;
pub mod cli;
pub mod engine;
pub mod guard;
pub mod memory;
pub mod results;
pub mod runner;
pub mod stub;
pub mod summary;

pub use engine::{Engine, EngineError, EngineFactory, Evaluated, StopFlag};
