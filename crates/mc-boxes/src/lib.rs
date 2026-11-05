//! Two-dimensional typesetting for the notebook, without UI (design 08, section 4): the box tree, editing in it,
//! its canonical text, typesetting printed output, the alias table, and layout with line breaking over a metrics
//! trait. Painting is the caller's: [`layout::layout`] gives positioned characters, math-font glyphs and rules.
//!
//! ```
//! use mc_boxes::{Editor, Structure};
//! let mut ed = Editor::from_text("1+x");
//! ed.structure(Structure::Fraction);
//! ed.insert_char('2');
//! assert_eq!(ed.text(), "1+((x)/(2))");
//! assert_eq!(ed.linear(), "1+x/2");
//! let out = mc_boxes::typeset_output("x^2/2");
//! assert_eq!(mc_boxes::notation(&out.row), "((x^2)/2)");
//! ```
pub mod aliases;
pub mod canonical;
pub mod edit;
pub mod layout;
pub mod linear;
pub mod ops;
pub mod printed;
mod scan;
pub mod tree;

pub use canonical::{from_text, to_text};
pub use edit::{AliasMark, AliasOutcome, Caret, Editor, Structure};
pub use linear::linear_text;
pub use printed::{typeset_any, typeset_input, typeset_output, Typeset};
pub use tree::{notation, BigOp, Item, Node, Row, RowPath, Step, MAX_NESTING};
