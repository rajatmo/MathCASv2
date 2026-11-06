//! The MathCAS v2 evaluator (docs/design/05-assembly.md, package F): integration piece I2's pattern matcher, rule
//! indexes and speculative parallel evaluator, on `mc-core`'s heap and sessions.
//!
//! - [`pat`]: patterns (semantics 6.1), compilation with the definition-time checks, the instance-of relation that
//!   orders rules (6.3).
//! - [`matcher`]: spike 2's continuation-passing matcher with the pruning AC strategy (6.2); conditions may evaluate.
//! - `index`: discrimination trees and AC key-set filters, updated in place by one writer and read without locks,
//!   keyed by symbols and structural hashes (A.2) so collections need no re-keying.
//! - `defs`: own values, attributes, rule sets and argument-head rule sets of every symbol.
//! - [`eval`]: the evaluator ([`Kernel`]), with holes, read-set validation, the memo through `mc-core`, fuel and
//!   interrupts through one [`Control`] block, and pure conditions (A.6).
//! - [`native`]: the interface built-ins (and `mc-arith`) implement; [`builtins`]: the minimal set.
//! - [`order`]: the canonical-order hook and its structural stand-in.
//! - [`syntax`]: the pattern and definition terms `mc-syntax` produces, as patterns and definition natives.
//! - `lang`: closures, blocks, `let`/`var`/assignment, `while`, `for`, `match`, `rewrite` (package H); `cells`: the
//!   mutable locals behind `var`; `subst`: hygienic substitution (semantics 6.6).
//! - `packed`: packed arrays (semantics 11) as normal forms without unpacking, and element-wise `add`/`mul` on their
//!   payloads.
//! - `inspect`: read access to a symbol's version and rules for natives that analyse definitions (the numeric
//!   compiler, package 1b).
//! - [`assume`]: assumption contexts (semantics 13), the held form `assuming` and the memo key of evaluations that
//!   depend on a context.
//! - [`guard`]: a panic inside an evaluation becomes `failure(internal, ...)` instead of ending the session.
//! - [`workloads`]: I2's cases R4, R5, R13, R14, S7, R14e and MR as sessions, for tests and measurements.
//!
//! A session is [`session`]: its state is the [`Kernel`], a root container, and each top-level evaluation is one
//! [`Session::step`](mc_core::Session::step), with a safe point after it.
#![warn(missing_docs)]
pub mod assume;
pub mod builtins;
mod cells;
pub mod compiled;
mod conc;
pub mod ctl;
mod defs;
pub mod eval;
pub mod guard;
mod index;
mod inspect;
mod lang;
mod machine;
mod packed;
pub mod matcher;
mod membudget;
pub mod native;
pub mod order;
pub mod pat;
mod subst;
pub mod syntax;
pub mod workloads;

pub use ctl::{Control, Status};
pub use defs::{Attributes, Hold};
pub use compiled::Compiled;
pub use eval::{session, Abort, CondCx, EvalConfig, EvalStats, Evaluation, Kernel, OperatorForm, Res, Split, DEFAULT_MAX_DEPTH, DEFAULT_MAX_ITERATIONS, DEFAULT_MAX_NEST, default_max_young};
pub use index::Rhs;
pub use inspect::RuleView;
pub use matcher::{Binds, Val};
pub use native::{ElemFn, Native, NativeCx, Pattern, Reply};
pub use pat::{DefError, Pat, Ty};
