//! The MathCAS v2 core assembled (docs/design/05-assembly.md, integration item G): `mc-syntax` reads and prints,
//! `mc-eval` evaluates in an `mc-core` session, and `mc-arith` is the arithmetic, all behind the
//! [`mc_api::Engine`] trait that the benchmark runner and the REPL drive.
//!
//! - [`arith`]: `mc-arith`'s canonical arithmetic, comparisons and order as natives of the evaluator;
//! - [`format`](mod@format): number text for the printer;
//! - [`CoreEngine`]: the engine (read, evaluate statement by statement with a safe point after each, print).
//!
//! ```
//! use mc_api::{Engine, StopFlag};
//! let mut e = mc_engine::CoreEngine::new();
//! let stop = StopFlag::new();
//! assert_eq!(e.eval("x + 2x", &stop).unwrap().printed, "3*x");
//! assert_eq!(e.eval("2^100", &stop).unwrap().printed, "1267650600228229401496703205376");
//! ```
//!
//! Decisions of this crate are numbered G.1, G.2, ... in the comments where they are applied.
#![warn(missing_docs)]
pub mod arith;
mod attrs;
mod compose;
mod engine;
pub mod format;
mod history;
mod packed;

pub use engine::{
    CoreEngine, EachOptions, EngineConfig, Event, Image, NameKind, Report, RunError, Script, MAPS_LISTS, OPERATOR_FORMS,
};
/// The functions the engine itself adds for notebooks (`out`, `message`), with a one-line description.
pub use history::FUNCTIONS;
/// The panic guard of evaluations (`failure(internal, ...)`), for hosts that guard their own requests too.
pub use mc_eval::guard;
pub use mc_load::{InMemory, LoadError, Module, Origin, SourceProvider};
