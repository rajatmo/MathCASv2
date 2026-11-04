//! Evaluating the user's function at numeric points (the shared convention of phase 4A): the expression in named
//! variables becomes the closure `fn([x1, ..., xn], body)`, applied by the evaluator to number terms. A body that is
//! a (nested) list gives its leaves in row-major order, so one closure computes a whole residual vector or Jacobian.
//!
//! The iterations see the function through `NumFn`, which lives in `mc-numfn` (package N0) with its interpreted
//! (`Machine`), compiled and automatic (`Auto`: compiled, the interpreter as the fallback) backends; re-exported
//! here. A value that is not a number of the expected kind is reported as `false`, never as an error.
pub(crate) use mc_numfn::{CentralGradient, Machine, NumFn, TermFn};
