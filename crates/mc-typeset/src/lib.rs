//! Output typesetting (design 08, sections 4.5 and 4.6): texts read through the parser into box trees of
//! `mc-boxes`, which lays them out.
//!
//! - [`math_text`]: the notebook's one look (decision NBUI.5), an output's printed form with its square roots,
//!   fractions and powers as boxes and labeled values drawn with their labels; [`label_rows`] finds those labels.
//! - [`typeset_output`]: printed output to the full set of boxes (plot labels in `mc-graphics`, Copy as LaTeX).
//! - [`typeset_input`]: linear input text to boxes (Ctrl+Shift+N); [`output_input_text`] and
//!   [`output_input_boxes`]: a part of an output taken back as input.
//!
//! ```
//! let out = mc_typeset::typeset_output("x^2/2");
//! assert_eq!(mc_boxes::notation(&out.row), "((x^2)/2)");
//! let shown = mc_typeset::math_text("sqrt(2)/2");
//! assert!(mc_boxes::has_boxes(&shown.row));
//! ```
pub mod math;
pub mod printed;

pub use math::{label_rows, math_text, LabelRow};
pub use printed::{
    output_input_boxes, output_input_text, typeset_input, typeset_output, typeset_output_with, OutputOptions, SourceMap,
    Typeset, MATRIX_MAX,
};
