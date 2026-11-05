//! The document model of the MathCAS v2 notebook (design `08-notebook.md`, section 9.5): cells and their styles,
//! automatic and manual grouping, open and closed groups, the selection, the key table, commands, undo, the
//! evaluation queue and clipboard fragments. No user interface and no kernel: the window turns keys, menus and
//! mouse actions into [`Command`]s, and talks to the kernel through a [`KernelLink`].
//!
//! - [`Notebook`]: the document; [`Notebook::apply`] runs a command, [`Notebook::pump`] exchanges messages with the
//!   kernel; [`Notebook::parse`] / [`Notebook::to_text`] read and write `.mcn` text.
//! - [`keys`]: the key table (chord, context, command) shared by the window, the key scripts and the help.
//! - [`script`]: the key-script runner with [`FakeEnv`] and [`FakeKernel`].
//!
//! Decisions of this package are numbered NB-core.1, NB-core.2, ... where they are applied.
mod cell;
mod command;
mod edit;
mod env;
mod eval;
mod fake;
mod group;
mod io;
pub mod keys;
mod notebook;
pub mod script;
mod style;
pub mod text;

pub use cell::{Cell, CellId, EvalMark, Props};
pub use command::{AppAction, Bracket, Command, Effects, InitAnswer, Motion};
pub use edit::{ALIAS_MARK, INDENT};
pub use env::{Clip, Env, FakeEnv};
pub use eval::{KernelLink, KernelMsg, KernelRequest, KernelState};
pub use fake::FakeKernel;
pub use group::{Group, Groups};
pub use notebook::{Notebook, Selection};
pub use style::{FontRole, GroupRule, ReturnKey, Style, StyleInfo};
