//! The input editor of the notebook's code cells (design 11): one document model (a box tree; a cell without boxes is
//! a row of characters), one editor written once over the tree, undo as transactions with the selection before and
//! after, and geometry from the layout through a small trait.
//!
//! ```
//! use mc_edit::{Command, Context, Editor, Structure};
//! let mut ed = Editor::from_text("1+x");
//! let cx = Context::default();
//! ed.apply(&Command::Structure(Structure::Fraction), &cx);
//! let out = ed.apply(&Command::Type("2".into()), &cx);
//! assert_eq!(ed.text(), "1+((x)/(2))");
//! assert_eq!(ed.notation(), "1+(x/2▮)");
//! // the transaction undoes exactly
//! ed.revert(&out.tx.unwrap());
//! assert_eq!(ed.notation(), "1+(x/▮□)");
//! ```
pub mod editor;
pub mod geom;
pub mod pos;
pub mod rules;
pub mod tx;

pub use editor::{AliasMark, Command, Context, Editor, Motion, Outcome, Status, Structure};
pub use geom::{Geometry, Logical};
pub use pos::{Position, Selection};
pub use rules::INDENT;
pub use tx::{History, Kind, Splice, Transaction};
