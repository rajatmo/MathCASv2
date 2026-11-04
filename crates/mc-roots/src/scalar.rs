//! The machine scalars the iterations run on: real (`f64`) and complex (`C64`) machine numbers, behind one small
//! trait so that Newton's method, Broyden's method and the linear algebra are written once for both. Moved to
//! `mc-numfn` (package N0) and re-exported here.
pub(crate) use mc_numfn::{all_finite, norm, norm2, Scalar};
