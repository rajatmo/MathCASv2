//! Source loading for MathCAS v2 (package F): files, modules, imports, read-time name resolution, the prelude and
//! the standard modules.
//!
//! A [`Loader`] holds a session's reader state: the module table and the session's imports. [`Loader::read`]
//! reads one input (REPL text, `-e` source, a script file): it checks the `#! mathcas 2` header, parses and lowers
//! with `mc-syntax`, loads the module files its imports need (through a [`SourceProvider`]), and resolves every
//! name of every statement before anything is evaluated (semantics 1.2; 04 sections 10 and 11). The result is a
//! list of plain statements on qualified symbols: evaluation never consults the module table.
//!
//! ```
//! use mc_api::RefStore;
//! use mc_load::{InMemory, Loader, Origin};
//! let store = RefStore::new();
//! let mut loader = Loader::new(Box::new(InMemory::new()));
//! let known = |_| false;
//! let read = loader.read(&store, "module M { pub fn f(x) = g(x)\n fn g(x) = x }\nimport M\nf(1)", &Origin::repl(), &known);
//! let printed: Vec<String> = read.unwrap().statements.iter().map(|&t| mc_syntax::Printer::new(&store).statement(t)).collect();
//! assert_eq!(printed, ["fn M.f(x) = M.g(x)", "fn M.g(x) = x", "null", "null", "M.f(1)"]);
//! ```
//!
//! - [`source`](mod@source): [`Origin`], [`SourceProvider`] with [`FileSystem`] and [`InMemory`];
//! - [`Module`]: an entry of the module table;
//! - [`LoadError`]: read-time errors with `file:line:column`, the source line and a caret;
//! - [`stdlib`]: the embedded prelude and standard modules.
//!
//! Decisions of this package are numbered F.1, F.2, ... in the comments where they are applied.
#![warn(missing_docs)]
mod error;
mod loader;
mod modules;
mod resolve;
pub mod source;
pub mod stdlib;

pub use error::LoadError;
pub use loader::{Loader, Read};
pub use modules::Module;
pub use source::{FileSystem, InMemory, Origin, Source, SourceProvider};
