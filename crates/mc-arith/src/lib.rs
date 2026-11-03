//! MathCAS v2 canonical arithmetic (package B of milestone M1, docs/design/05-assembly.md).
//!
//! - [`num`]: the bare number layer (big integers, rationals, balls) from spike 3 and integration piece I3;
//! - [`number`]: numbers as values, with contagion `exact < ball < machine` (semantics 2.2), three-valued
//!   comparisons (2.5) and numeric powers;
//! - [`radical`]: canonical radicals (2.4);
//! - [`Order`]: the canonical order on terms (semantics 8);
//! - [`Arith`]: the canonicaliser of `add`, `mul` and `pow` (semantics 3) and number terms, generic over
//!   [`mc_api::Store`];
//! - [`text`]: decimal views of numbers for printing (04-syntax section 4).
//!
//! Decisions of this package are numbered B.1, B.2, ... in the comments where they are applied.
#![warn(missing_docs)]

mod canon;
pub mod num;
pub mod number;
mod order;
pub mod radical;
mod term;
pub mod text;

pub use canon::{Arith, DEFAULT_MAX_BITS};
pub use number::{Cmp3, Kind, Num, NumError, Real};
pub use order::Order;
pub use term::failure_tags;
