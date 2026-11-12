//! The MathCAS v2 benchmark runner (docs/design/02-benchmarks.md sections 1, 2, 4 and 7).
//!
//! - [`case`]: the case-file format, `bench/cases/<class>/<id>.mc`, written in the concrete syntax;
//! - the caller supplies the evaluator through the [`Engine`](mc_api::Engine) trait of `mc-api` (the runner has no
//!   parser); the `mc-bench` binary runs the cases on the real engine (`mc-engine`);
//! - [`guard`]: child-process isolation with a memory cap and a timeout (a job object on Windows, a resource
//!   limit on Unix);
//! - [`child`]: one measurement under the protocol (warm-up, repetitions, peak memory, check);
//! - [`runner`]: the parent loop with `[k/N pct%]` progress, results and trace files;
//! - [`results`], [`summary`]: the results format, class geometric means and milestone status;
//! - [`stub`]: a tiny integer evaluator that stands in for the engine until `mc-eval` exists;
//! - [`cli`]: the command line shared by benchmark binaries;
//! - [`notices`]: the third-party notices of the shipped programs and the licence check (binary `mc-notices`).
pub mod case;
pub mod check;
pub mod child;
pub mod cli;
pub mod guard;
pub mod memory;
pub mod notices;
pub mod results;
pub mod runner;
pub mod stub;
pub mod summary;

