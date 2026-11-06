//! Two-dimensional typesetting without UI (design 08, section 4): the box tree, editing in it, its canonical text,
//! the alias table, and layout with line breaking over a metrics trait. Painting is the caller's: [`layout::layout`]
//! gives positioned characters, math-font glyphs and rules.
//!
//! The notebook's cells use three boxes only, the square root, the fraction and the power (decision NBUI.5): the
//! [`Editor`] makes them ([`Structure`]), [`math_text`] reads printed output into them, [`restrict`] reduces older
//! box trees to them. Plot labels (`mc-graphics`) use the full set through [`typeset_output`].
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
pub mod latex;
pub mod layout;
pub mod linear;
pub mod math;
pub mod ops;
pub mod printed;
mod scan;
pub mod tree;

pub use canonical::{from_text, to_text, to_text_mapped, TextMap};
pub use edit::{AliasMark, AliasOutcome, Caret, Editor, Structure};
pub use latex::to_latex;
pub use linear::linear_text;
pub use math::{has_boxes, math_text, restrict};
pub use printed::{output_input_text, typeset_input, typeset_output, typeset_output_with, OutputOptions, SourceMap, Typeset, MATRIX_MAX};
pub use tree::{notation, BigOp, Item, Node, Row, RowPath, Step, MAX_NESTING};
