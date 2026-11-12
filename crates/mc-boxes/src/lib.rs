//! Two-dimensional typesetting without UI (design 08, section 4): the box tree, its canonical and linear text,
//! the alias table, and layout with line breaking over a metrics trait. Painting is the caller's: [`layout::layout`]
//! gives positioned characters, math-font glyphs and rules. Texts read through the parser into box trees (printed
//! output, linear input) are `mc-typeset`'s.
//!
//! The notebook's cells use three boxes only, the square root, the fraction and the power (decision NBUI.5): the
//! editor (`mc-edit`) makes them, `mc_typeset::math_text` reads printed output into them, [`restrict`] reduces older
//! box trees to them. Plot labels (`mc-graphics`) use the full set through `mc_typeset::typeset_output`.
//!
//! ```
//! let row = mc_boxes::from_text("1+((x)/(2))");
//! assert_eq!(mc_boxes::notation(&row), "1+(x/2)");
//! assert_eq!(mc_boxes::linear_text(&row), "1+x/2");
//! ```
pub mod aliases;
pub mod canonical;
pub mod latex;
pub mod layout;
pub mod linear;
pub mod ops;
mod scan;
pub mod tree;

pub use canonical::{from_text, to_text, to_text_mapped, TextMap};
pub use latex::to_latex;
pub use linear::{linear_segments, linear_text, restrict, Segment};
pub use scan::{is_one_group, source_text, ESCAPE};
pub use tree::{has_boxes, notation, write_notation, BigOp, Item, Node, Pos, Row, RowPath, Side, Step, MAX_NESTING};
